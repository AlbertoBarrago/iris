//! An account's settings on its server, for the Swift front end: the
//! automatic reply, through `mailrs_sync::AccountSettings` as the GTK
//! dialog reads and stores it.

use std::sync::Arc;

use chrono::{Days, Local, NaiveTime, TimeZone};
use mailrs_appcore::auto_reply::{away_subtitle, subject_to_save};
use mailrs_appcore::offered::offers_for;
use mailrs_appcore::permission::settings_needed;
use mailrs_domain::translate::{fill, gettext};
use mailrs_domain::EpochMillis;
use mailrs_sync::{Accounts, AccountSettings, AutomaticReply, Permitted};

use crate::CoreError;
use crate::mail::{Mail, Running};

/// The automatic reply as its sheet shows it.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct AutoReplyForm {
    pub enabled: bool,
    /// Who answers while the person is away, under the switch.
    pub subtitle: String,
    /// Whether the reply runs only between `first_day` and `last_day`.
    pub dated: bool,
    /// Local midnight of the first and the last day; a week from today
    /// when the reply names none.
    pub first_day: i64,
    pub last_day: i64,
    pub subject: String,
    pub body: String,
    pub contacts_only: bool,
    /// Whether the server keeps a subject. Outlook does not, and the
    /// sheet says the same reply goes to everyone instead.
    pub keeps_subject: bool,
    /// Whether the server can answer only the person's contacts.
    pub can_limit_to_contacts: bool,
}

/// What reading the automatic reply found.
#[derive(Debug, Clone, PartialEq, uniffi::Enum)]
pub enum AutoReplyRead {
    Form { form: AutoReplyForm },
    /// The sign-in left the account's settings out: the sheet offers
    /// Grant Access.
    NeedsPermission { reason: String },
}

#[uniffi::export]
impl Mail {
    /// The automatic reply of account `account_id`, as the server holds it.
    pub fn automatic_reply(&self, account_id: i64) -> Result<AutoReplyRead, CoreError> {
        let account = self.account_row(account_id)?;
        let services = self.running.services(account_id);
        if services.is_none() {
            return Err(CoreError::Store(gettext("This account is not syncing yet.")));
        }
        let offers = offers_for(services.as_ref());
        let settings = self.account_settings();
        let read = self
            .run(async move { settings.automatic_reply(account_id).await })
            .map_err(|err| CoreError::Store(err.to_string()))?;
        let reply = match read {
            Permitted::Done(reply) => reply,
            Permitted::NeedsPermission => {
                return Ok(AutoReplyRead::NeedsPermission {
                    reason: settings_needed(account.provider),
                });
            }
        };
        let today = midnight(mailrs_sync::now_millis());
        let week = Local
            .timestamp_millis_opt(today)
            .single()
            .and_then(|day| day.checked_add_days(Days::new(6)))
            .map_or(today, |day| day.timestamp_millis());
        Ok(AutoReplyRead::Form {
            form: AutoReplyForm {
                enabled: reply.enabled,
                subtitle: away_subtitle(account.provider_name()),
                dated: reply.first_day.is_some() || reply.last_day.is_some(),
                first_day: reply.first_day.map_or(today, midnight),
                last_day: reply.last_day.map_or(week, midnight),
                subject: reply.subject,
                body: reply.body,
                contacts_only: reply.contacts_only,
                keeps_subject: offers.auto_reply_subject,
                can_limit_to_contacts: offers.auto_reply_contacts_only,
            },
        })
    }

    /// Stores `form` as account `account_id`'s automatic reply, keeping
    /// what the sheet does not show as the server has it, and says whether
    /// it is now on or off.
    pub fn set_automatic_reply(&self, account_id: i64, form: AutoReplyForm) -> Result<String, CoreError> {
        let account = self.account_row(account_id)?;
        let settings = self.account_settings();
        let read = Arc::clone(&settings);
        let base = match self
            .run(async move { read.automatic_reply(account_id).await })
            .map_err(|err| CoreError::Store(err.to_string()))?
        {
            Permitted::Done(reply) => reply,
            Permitted::NeedsPermission => return Err(CoreError::Store(settings_needed(account.provider))),
        };
        let wanted = reply_of(&form, &base);
        let enabled = wanted.enabled;
        let stored = self
            .run(async move { settings.set_automatic_reply(account_id, &wanted).await })
            .map_err(|err| CoreError::Store(fill(&gettext("Could not save: {reason}"), &[("reason", &err.to_string())])))?;
        match stored {
            Permitted::Done(()) => Ok(match enabled {
                true => gettext("Automatic reply is on"),
                false => gettext("Automatic reply is off"),
            }),
            Permitted::NeedsPermission => Err(CoreError::Store(settings_needed(account.provider))),
        }
    }
}

impl Mail {
    pub(crate) fn account_settings(&self) -> Arc<AccountSettings<Running>> {
        Arc::new(AccountSettings::new(Arc::clone(&self.running), self.db.clone()))
    }
}

/// `base` with the sheet's values; what the sheet does not show stays.
fn reply_of(form: &AutoReplyForm, base: &AutomaticReply) -> AutomaticReply {
    let first = midnight(form.first_day);
    let last = midnight(form.last_day).max(first);
    AutomaticReply {
        enabled: form.enabled,
        subject: subject_to_save(form.keeps_subject, form.subject.trim(), &base.subject),
        body: form.body.trim_end().to_string(),
        contacts_only: form.contacts_only,
        first_day: form.dated.then_some(first),
        last_day: form.dated.then_some(last),
        ..base.clone()
    }
}

/// Local midnight at the start of the day `millis` falls on.
fn midnight(millis: EpochMillis) -> EpochMillis {
    Local
        .timestamp_millis_opt(millis)
        .single()
        .and_then(|at| Local.from_local_datetime(&at.date_naive().and_time(NaiveTime::MIN)).earliest())
        .map_or(millis, |day| day.timestamp_millis())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn form() -> AutoReplyForm {
        AutoReplyForm {
            enabled: true,
            subtitle: String::new(),
            dated: true,
            first_day: midnight(1_760_000_000_000),
            last_day: midnight(1_760_000_000_000) - 86_400_000,
            subject: " Away ".into(),
            body: "Back Monday.\n\n".into(),
            contacts_only: false,
            keeps_subject: true,
            can_limit_to_contacts: true,
        }
    }

    #[test]
    fn the_sheet_keeps_what_it_does_not_show() {
        let base = AutomaticReply {
            domain_only: true,
            subject: "Old".into(),
            ..AutomaticReply::default()
        };
        let reply = reply_of(&form(), &base);
        assert!(reply.domain_only);
        assert_eq!(reply.subject, "Away");
        assert_eq!(reply.body, "Back Monday.");
        // A last day before the first runs one day, not none.
        assert_eq!(reply.first_day, reply.last_day);
    }

    #[test]
    fn a_hidden_subject_is_left_as_the_server_has_it() {
        let hidden = AutoReplyForm {
            keeps_subject: false,
            dated: false,
            ..form()
        };
        let base = AutomaticReply {
            subject: "Old".into(),
            ..AutomaticReply::default()
        };
        let reply = reply_of(&hidden, &base);
        assert_eq!(reply.subject, "Old");
        assert_eq!((reply.first_day, reply.last_day), (None, None));
    }
}
