//! An account's rules, for the Swift front end: the list in plain words,
//! the form behind each rule, and saving or deleting one, through
//! `mailrs_sync::AccountSettings` as the GTK Rules dialog does. The
//! words and the form's reading of a rule are `mailrs_appcore::rules`.
//!
//! Rules that run on this computer, for IMAP and POP3 accounts without a
//! rules server, run here too: over new mail before it notifies, and
//! every minute, as the GTK app runs them.

use std::sync::Arc;

use mailrs_appcore::offered::Filing;
use mailrs_appcore::permission::settings_needed;
use mailrs_appcore::rules::{RuleForm, describe_action, describe_criteria, offers_never_spam, place_line, waiting_line};
use mailrs_domain::translate::{fill, gettext};
use mailrs_domain::{AccountId, Filter, Provider};
use mailrs_sync::rules::RulesEngine;
use mailrs_sync::{Accounts, Permitted, Replaced, RulesPlace, SyncEngine};

use crate::CoreError;
use crate::mail::{Mail, Running};

/// What the rule form shows, field by field.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct RuleFields {
    pub from: String,
    pub to: String,
    pub subject: String,
    pub has_words: String,
    pub not_words: String,
    pub has_attachment: bool,
    pub skip_inbox: bool,
    pub mark_read: bool,
    pub star: bool,
    /// The label or folder it files into, by id.
    pub label: Option<String>,
    pub never_spam: bool,
    pub trash: bool,
}

/// One rule in the list.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct RuleRow {
    pub id: String,
    /// "From ann@example.com, subject has “invoice”".
    pub criteria: String,
    /// "Skip the Inbox, apply Travel".
    pub action: String,
    /// Made elsewhere in a shape the form cannot say: listed, never
    /// opened or deleted here.
    pub read_only: bool,
    pub fields: RuleFields,
}

/// A label or folder a rule can file into.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct RuleLabel {
    pub id: String,
    pub name: String,
}

/// The Rules sheet.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct RulesPage {
    pub rules: Vec<RuleRow>,
    /// Blocks the server holds that nobody here wrote.
    pub elsewhere: Vec<String>,
    /// Where the rules run.
    pub place: String,
    /// The server is not answering, and changes wait; empty when it is.
    pub waiting: String,
    pub labels: Vec<RuleLabel>,
    /// Whether the account files into folders rather than labels.
    pub folders: bool,
    /// Whether the form offers Never Send to Spam.
    pub never_spam: bool,
}

#[derive(Debug, Clone, PartialEq, uniffi::Enum)]
pub enum RulesRead {
    Page { page: RulesPage },
    /// The sign-in left the account's settings out.
    NeedsPermission { reason: String },
}

#[uniffi::export]
impl Mail {
    /// Account `account_id`'s rules, as the server or this computer
    /// holds them.
    pub fn rules(&self, account_id: i64) -> Result<RulesRead, CoreError> {
        let account = self.account_row(account_id)?;
        if self.running.services(account_id).is_none() {
            return Err(CoreError::Store(gettext("This account is not syncing yet.")));
        }
        let settings = self.account_settings();
        let read = self
            .run(async move { settings.rule_list(account_id).await })
            .map_err(|err| CoreError::Store(err.to_string()))?;
        let list = match read {
            Permitted::Done(list) => list,
            Permitted::NeedsPermission => {
                return Ok(RulesRead::NeedsPermission {
                    reason: settings_needed(account.provider),
                });
            }
        };
        let labels = self.user_labels(account_id)?;
        let name_of = |id: &str| labels.iter().find(|l| l.id == id).map(|l| l.name.clone());
        let offered = |id: &str| labels.iter().any(|l| l.id == id);
        let filing = Filing::of([self.offers(account_id)]);
        let provider = mailrs_discover::resolved_provider_name(account.provider_name());
        Ok(RulesRead::Page {
            page: RulesPage {
                rules: list
                    .rules
                    .iter()
                    .map(|filter| RuleRow {
                        id: filter.id.clone().unwrap_or_default(),
                        criteria: describe_criteria(&filter.criteria),
                        action: describe_action(&filter.action, name_of),
                        read_only: filter.read_only,
                        fields: fields_of(&RuleForm::read(filter, offered).0),
                    })
                    .collect(),
                elsewhere: list.elsewhere,
                place: place_line(list.place, &provider),
                waiting: match list.waiting {
                    true => waiting_line(&provider),
                    false => String::new(),
                },
                labels: labels.iter().map(|l| RuleLabel { id: l.id.clone(), name: l.name.clone() }).collect(),
                folders: filing == Filing::Folders,
                never_spam: offers_never_spam(filing),
            },
        })
    }

    /// Saves `fields` as a new rule, or in place of rule `id`, keeping
    /// what the form has no field for. Answers the toast.
    pub fn save_rule(&self, account_id: i64, id: Option<String>, fields: RuleFields) -> Result<String, CoreError> {
        let account = self.account_row(account_id)?;
        let form = form_of(&fields);
        let settings = self.account_settings();
        let old = match &id {
            Some(id) => Some(self.rule(account_id, id)?),
            None => None,
        };
        let labels = self.user_labels(account_id)?;
        let unshown = old
            .as_ref()
            .map(|old| RuleForm::read(old, |id| labels.iter().any(|l| l.id == id)).1)
            .unwrap_or_default();
        let filter = form
            .filter_keeping(&unshown)
            .map_err(|missing| CoreError::Store(gettext(missing)))?;
        match old {
            None => {
                let added = self
                    .run(async move { settings.add_rule(account_id, filter).await })
                    .map_err(|err| failed("Could not add the rule: {reason}", &err))?;
                match added {
                    Permitted::Done(_) => Ok(gettext("Rule added")),
                    Permitted::NeedsPermission => Err(CoreError::Store(settings_needed(account.provider))),
                }
            }
            Some(old) => {
                let replaced = self
                    .run(async move { settings.replace_rule(account_id, &old, filter).await })
                    .map_err(|err| failed("Could not save the rule: {reason}", &err))?;
                match replaced {
                    Permitted::Done(Replaced::Swapped(_)) => Ok(gettext("Rule saved")),
                    Permitted::Done(Replaced::BothRun { error, .. }) => Err(failed(
                        "Saved, but Gmail kept the old rule too, so both run now: {reason}",
                        &error,
                    )),
                    Permitted::NeedsPermission => Err(CoreError::Store(settings_needed(account.provider))),
                }
            }
        }
    }

    pub fn delete_rule(&self, account_id: i64, id: String) -> Result<String, CoreError> {
        let account = self.account_row(account_id)?;
        let settings = self.account_settings();
        let deleted = self
            .run(async move { settings.delete_rule(account_id, &id).await })
            .map_err(|err| failed("Could not delete the rule: {reason}", &err))?;
        match deleted {
            Permitted::Done(()) => Ok(gettext("Rule deleted")),
            Permitted::NeedsPermission => Err(CoreError::Store(settings_needed(account.provider))),
        }
    }
}

impl Mail {
    /// Rule `id` as the server holds it now.
    fn rule(&self, account_id: AccountId, id: &str) -> Result<Filter, CoreError> {
        let settings = self.account_settings();
        let read = self
            .run(async move { settings.rule_list(account_id).await })
            .map_err(|err| CoreError::Store(err.to_string()))?;
        match read {
            Permitted::Done(list) => list
                .rules
                .into_iter()
                .find(|f| f.id.as_deref() == Some(id))
                .ok_or_else(|| CoreError::Store(gettext("Could Not Load the Rules"))),
            Permitted::NeedsPermission => Err(CoreError::Store(gettext("Could Not Load the Rules"))),
        }
    }

    pub(crate) fn rules_engine(&self) -> Arc<RulesEngine<Running>> {
        Arc::new(RulesEngine::new(Arc::clone(&self.running), self.db.clone(), Arc::clone(&self.actions)))
    }
}

/// Whether account `account_id`'s rules run on this computer.
pub(crate) fn rules_here(running: &Running, account_id: AccountId) -> bool {
    running
        .services(account_id)
        .and_then(|services| services.rules.as_ref().map(|r| r.place()))
        == Some(RulesPlace::ThisComputer)
}

/// Every minute while `engine` runs: the local rules of IMAP and POP3
/// accounts over mail no new-mail event named, and rule changes that
/// waited for a ManageSieve server. Ten seconds after start first.
pub(crate) async fn keep_running(
    rules: Arc<RulesEngine<Running>>,
    settings: Arc<mailrs_sync::AccountSettings<Running>>,
    running: Arc<Running>,
    engine: Arc<SyncEngine>,
    db: mailrs_store::Db,
) {
    let start = tokio::time::Instant::now() + std::time::Duration::from_secs(10);
    let mut every = tokio::time::interval_at(start, std::time::Duration::from_secs(60));
    loop {
        every.tick().await;
        if !running.current().is_some_and(|now| Arc::ptr_eq(&now, &engine)) {
            return;
        }
        let Ok(listed) = db.read(mailrs_store::accounts::list_accounts).await else { continue };
        for account in listed.iter().filter(|a| matches!(a.provider, Provider::Imap | Provider::Pop3)) {
            if rules_here(&running, account.id) {
                if let Err(err) = rules.run_due(account.id).await {
                    tracing::warn!(account = account.id, %err, "local rules did not run");
                }
                continue;
            }
            match settings.send_rule_changes(account.id).await {
                Ok(sent) => {
                    for words in sent.refused {
                        tracing::warn!(account = account.id, %words, "a rule that waited was refused");
                    }
                }
                Err(err) => tracing::info!(account = account.id, %err, "rule changes still wait"),
            }
        }
    }
}

fn failed(pattern: &str, err: &impl std::fmt::Display) -> CoreError {
    CoreError::Store(fill(&gettext(pattern), &[("reason", &err.to_string())]))
}

fn fields_of(form: &RuleForm) -> RuleFields {
    RuleFields {
        from: form.from.clone(),
        to: form.to.clone(),
        subject: form.subject.clone(),
        has_words: form.has_words.clone(),
        not_words: form.not_words.clone(),
        has_attachment: form.has_attachment,
        skip_inbox: form.skip_inbox,
        mark_read: form.mark_read,
        star: form.star,
        label: form.label.clone(),
        never_spam: form.never_spam,
        trash: form.trash,
    }
}

fn form_of(fields: &RuleFields) -> RuleForm {
    RuleForm {
        from: fields.from.clone(),
        to: fields.to.clone(),
        subject: fields.subject.clone(),
        has_words: fields.has_words.clone(),
        not_words: fields.not_words.clone(),
        has_attachment: fields.has_attachment,
        skip_inbox: fields.skip_inbox,
        mark_read: fields.mark_read,
        star: fields.star,
        label: fields.label.clone().filter(|l| !l.is_empty()),
        never_spam: fields.never_spam,
        trash: fields.trash,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_form_reads_back_what_it_wrote() {
        let fields = RuleFields {
            from: "ann@example.com".into(),
            to: String::new(),
            subject: "invoice".into(),
            has_words: String::new(),
            not_words: String::new(),
            has_attachment: true,
            skip_inbox: true,
            mark_read: true,
            star: false,
            label: Some("Label_1".into()),
            never_spam: false,
            trash: false,
        };
        let filter = form_of(&fields).filter().expect("a rule");
        let (read, unshown) = RuleForm::read(&filter, |id| id == "Label_1");
        assert!(unshown.is_empty());
        assert_eq!(fields_of(&read), fields);
    }

    #[test]
    fn an_empty_label_is_no_label() {
        let fields = RuleFields {
            from: "a@b.c".into(),
            to: String::new(),
            subject: String::new(),
            has_words: String::new(),
            not_words: String::new(),
            has_attachment: false,
            skip_inbox: true,
            mark_read: false,
            star: false,
            label: Some(String::new()),
            never_spam: false,
            trash: false,
        };
        assert_eq!(form_of(&fields).label, None);
    }
}
