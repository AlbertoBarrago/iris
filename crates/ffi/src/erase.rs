//! Delete Forever, for the Swift front end: Delete in the Trash erases
//! the mail after one question, as the GTK window did, through
//! `MailActions::erase`. Erased mail cannot come back, so no Undo keeps
//! it.

use std::sync::Arc;

use mailrs_appcore::erase::{deleted_forever_message, erase_question};
use mailrs_domain::translate::{fill, gettext};
use mailrs_domain::{AccountId, Folder, Target};
use mailrs_store::accounts;
use mailrs_sync::Permitted;

use crate::CoreError;
use crate::mail::{ActionDone, Mail, ThreadRef};

/// What the question before Delete Forever says.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct EraseQuestion {
    pub heading: String,
    pub body: String,
    /// The button's word, "Delete Forever".
    pub verb: String,
}

#[uniffi::export]
impl Mail {
    /// The question before erasing `threads`, when the mailbox `from`
    /// names is a Trash, where Delete has nowhere further to move mail.
    /// `None` elsewhere, and when no account of theirs can erase.
    pub fn erase_question(&self, threads: Vec<ThreadRef>, from: Option<String>) -> Result<Option<EraseQuestion>, CoreError> {
        let in_trash = from.and_then(|key| self.mailbox(&key)).and_then(|m| m.folder()) == Some(Folder::Trash);
        if !in_trash || threads.is_empty() {
            return Ok(None);
        }
        let db = self.db.clone();
        let listed = self.run(async move { db.read(accounts::list_accounts).await })?;
        let mut ids: Vec<AccountId> = Vec::new();
        for thread in &threads {
            if !ids.contains(&thread.account_id) {
                ids.push(thread.account_id);
            }
        }
        let mut erasers: Vec<String> = Vec::new();
        let mut kept: Vec<String> = Vec::new();
        for id in ids {
            let Some(account) = listed.iter().find(|a| a.id == id) else { continue };
            if self.offers(id).delete_forever {
                let provider = mailrs_discover::resolved_provider_name(account.provider_name()).to_string();
                if !provider.is_empty() && !erasers.contains(&provider) {
                    erasers.push(provider);
                }
            } else {
                kept.push(account.email.clone());
            }
        }
        if erasers.is_empty() {
            return Ok(None);
        }
        let count = threads.iter().filter(|t| self.offers(t.account_id).delete_forever).count();
        let threaded = crate::prefs::load().threading;
        let erasers: Vec<&str> = erasers.iter().map(String::as_str).collect();
        let kept: Vec<&str> = kept.iter().map(String::as_str).collect();
        let question = erase_question(count, threaded, &erasers, &kept);
        Ok(Some(EraseQuestion {
            heading: question.heading,
            body: question.body,
            verb: question.verb,
        }))
    }

    /// Erases `threads` for good, leaving the ones whose server cannot in
    /// the Trash. Answers the toast.
    pub fn erase(&self, threads: Vec<ThreadRef>) -> ActionDone {
        let targets: Vec<Target> = threads
            .into_iter()
            .filter(|t| self.offers(t.account_id).delete_forever)
            .map(|t| Target {
                account_id: t.account_id,
                thread_id: t.thread_id,
                message_id: None,
            })
            .collect();
        if targets.is_empty() {
            return ActionDone {
                words: String::new(),
                done: 0,
                failed: None,
            };
        }
        let actions = Arc::clone(&self.actions);
        let erased = self.run(async move { actions.erase(&targets).await });
        self.mailboxes.forget_remote();
        match erased {
            Ok(Permitted::Done(outcome)) => ActionDone {
                words: deleted_forever_message(outcome.done.len(), crate::prefs::load().threading),
                done: outcome.done.len() as u32,
                failed: outcome.failed.first().map(|f| f.error.clone()),
            },
            Ok(Permitted::NeedsPermission) => ActionDone {
                words: String::new(),
                done: 0,
                failed: Some(gettext("Allow Iris to Delete Mail")),
            },
            Err(err) => ActionDone {
                words: String::new(),
                done: 0,
                failed: Some(fill(&gettext("Could not delete the mail: {reason}"), &[("reason", &err.to_string())])),
            },
        }
    }
}
