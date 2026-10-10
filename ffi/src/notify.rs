//! New-mail notifications and the Dock badge for the Swift front end.
//! appcore words the notices and decides whether a button still has work
//! to do, as it does for the GTK app; the Mac's notification centre shows
//! them, with the buttons Preferences picks.

use std::sync::Arc;

use mailrs_appcore::dock::badge_label;
use mailrs_appcore::notify::{self, Button};
use mailrs_appcore::settings::Settings;
use mailrs_domain::{MailSet, MessageMeta, Role, Target};
use mailrs_store::{Db, accounts, messages, threads};
use mailrs_sync::{History, MovedFrom};

use crate::CoreError;
use crate::mail::{ActionDone, Mail, MailListener};

/// One notification to show.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct MailNotice {
    pub title: String,
    pub body: String,
    /// The message a click opens, when the notice names one.
    pub account_id: Option<i64>,
    pub thread_id: Option<String>,
    pub message_id: Option<String>,
}

/// A button a new-mail notification carries.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct NoticeButton {
    /// `archive`, `mark-read`, `delete` or `reply`.
    pub key: String,
    pub label: String,
}

/// What a button on a notification led to.
#[derive(Debug, Clone, PartialEq, uniffi::Enum)]
pub enum NoticeAnswer {
    /// The mail moved on since, so the button had nothing left to do.
    Nothing,
    /// Open a reply to the message.
    Reply,
    Done { done: ActionDone },
}

#[uniffi::export]
impl Mail {
    /// The buttons new-mail notifications carry, as Preferences picks
    /// them.
    pub fn notice_buttons(&self) -> Vec<NoticeButton> {
        Settings::load(&Settings::default_path())
            .notification_buttons
            .into_iter()
            .map(|button| NoticeButton {
                key: button.key().into(),
                label: button.label(),
            })
            .collect()
    }

    /// What the Dock's badge reads: the unread conversations in every
    /// inbox, or `None` for none.
    pub fn dock_badge(&self) -> Option<String> {
        let db = self.db.clone();
        let unread = self.run(async move {
            db.read(|c| {
                let mut unread = 0;
                for account in accounts::list_accounts(c)? {
                    let inbox = threads::ThreadFilter::account(account.id, MailSet::Role(Role::Inbox));
                    unread += threads::unread_threads(c, &inbox)?;
                }
                Ok(unread)
            })
            .await
        });
        match unread {
            Ok(unread) => {
                tracing::debug!(unread, "Dock badge");
                badge_label(unread)
            }
            Err(err) => {
                tracing::warn!(error = %err, "could not count the unread mail for the Dock");
                None
            }
        }
    }

    /// Does what `button` on a notification for one message asks. The mail
    /// may have been read, archived or trashed since it arrived, so the
    /// store decides whether the button still has work to do.
    pub fn notice_chose(
        &self,
        account_id: i64,
        thread_id: String,
        message_id: String,
        button: String,
    ) -> Result<NoticeAnswer, CoreError> {
        let Some(button) = Button::ALL.into_iter().find(|b| b.key() == button) else {
            return Ok(NoticeAnswer::Nothing);
        };
        let db = self.db.clone();
        let id = message_id.clone();
        let message = self.run(async move { db.read(move |c| Ok(messages::by_ids(c, account_id, &[id])?.pop())).await })?;
        // Mail the store no longer holds leaves nothing to act on.
        let Some(message) = message else {
            return Ok(NoticeAnswer::Nothing);
        };
        if !notify::still_applies(button, &message) {
            return Ok(NoticeAnswer::Nothing);
        }
        let Some(action) = button.action() else {
            return Ok(NoticeAnswer::Reply);
        };
        let target = Target {
            account_id,
            thread_id,
            message_id: Some(message_id),
        };
        let (actions, given) = (Arc::clone(&self.actions), action.clone());
        let outcome = self.run(async move {
            actions
                .run_from(&[target], given, History::Record, &MovedFrom::nowhere())
                .await
        });
        self.mailboxes.forget_remote();
        Ok(NoticeAnswer::Done {
            done: ActionDone {
                words: mailrs_view::done::done_message(&action, outcome.done.len(), true).unwrap_or_default(),
                done: outcome.done.len() as u32,
                failed: outcome.failed.first().map(|f| f.error.clone()),
            },
        })
    }
}

/// Tells `listener` about new mail in `account_id`, when Preferences asks
/// for notifications: the mail still unread in the inbox, from a VIP when
/// only VIPs notify. Local rules do not run here yet, so mail a rule would
/// file away still notifies.
pub(crate) async fn announce(db: Db, account_id: i64, ids: Vec<String>, listener: Arc<dyn MailListener>) {
    let settings = Settings::load(&Settings::default_path());
    if !settings.notifications {
        tracing::info!(account = account_id, "new mail, but notifications are off");
        return;
    }
    let found = db.read(move |c| messages::by_ids(c, account_id, &ids)).await;
    let found = match found {
        Ok(found) => found,
        Err(err) => {
            tracing::warn!(error = %err, "could not read new mail to announce");
            return;
        }
    };
    let news = worth_announcing(&settings, found);
    tracing::info!(account = account_id, news = news.len(), "new mail");
    tell(&news, settings.notification_previews, &listener);
}

/// Announces conversations a reminder brought back, each by its newest
/// message, as the GTK app does.
pub(crate) fn announce_returned(newest: Vec<MessageMeta>, listener: &Arc<dyn MailListener>) {
    let settings = Settings::load(&Settings::default_path());
    if settings.notifications {
        for message in newest {
            tell(std::slice::from_ref(&message), settings.notification_previews, listener);
        }
    }
}

fn tell(messages: &[MessageMeta], previews: bool, listener: &Arc<dyn MailListener>) {
    let notices = notify::notices(messages, previews);
    if notices.is_empty() {
        return;
    }
    listener.new_mail(
        notices
            .into_iter()
            .map(|notice| MailNotice {
                title: notice.title,
                body: notice.body,
                account_id: notice.target.as_ref().map(|t| t.account_id),
                thread_id: notice.target.as_ref().map(|t| t.thread_id.clone()),
                message_id: notice.target.and_then(|t| t.message_id),
            })
            .collect(),
    );
}

/// The new mail a notification is for: still unread in the inbox, and from
/// a VIP when only VIPs notify.
fn worth_announcing(settings: &Settings, found: Vec<MessageMeta>) -> Vec<MessageMeta> {
    found
        .into_iter()
        .filter(|m| m.in_role(Role::Inbox) && m.is_unread())
        .filter(|m| !settings.notify_vips_only || m.from.as_ref().is_some_and(|a| settings.is_vip(&a.email)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use mailrs_sync::fake::meta;

    #[test]
    fn only_unread_inbox_mail_is_news() {
        let settings = Settings::default();
        let found = vec![
            meta("new", "t1", 0, &["INBOX", "UNREAD"]),
            meta("read", "t2", 0, &["INBOX"]),
            meta("filed", "t3", 0, &["Label_1", "UNREAD"]),
        ];
        let news: Vec<String> = worth_announcing(&settings, found).into_iter().map(|m| m.id).collect();
        assert_eq!(news, ["new"]);
    }

    #[test]
    fn vips_only_drops_everyone_else() {
        let settings = Settings {
            notify_vips_only: true,
            ..Settings::default()
        };
        let found = vec![meta("new", "t1", 0, &["INBOX", "UNREAD"])];
        assert!(worth_announcing(&settings, found).is_empty());
    }
}
