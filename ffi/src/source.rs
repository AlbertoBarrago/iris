//! View Source and exporting mail, for the Swift front end: a message as
//! it arrived, headers and all, from the account's server or the cache
//! the sync keeps, and conversations saved as mbox through
//! `mailrs_sync::export`, as the GTK window saved them.

use std::sync::Arc;

use mailrs_domain::translate::{fill, gettext};
use mailrs_domain::{AccountId, MessageMeta, Role, Target};
use mailrs_store::messages;
use mailrs_sync::Accounts;
use mailrs_sync::export::file_name;

use crate::CoreError;
use crate::mail::{Mail, ThreadRef};

/// One message as it arrived.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct MessageSource {
    pub subject: String,
    /// The message as text, line ends as the Mac writes them.
    pub text: String,
    /// The bytes as they arrived, for Save As.
    pub raw: Vec<u8>,
    /// "Lunch plans 2026-01-05.eml".
    pub file_name: String,
}

/// Mail ready to save to a file the person picks.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct ExportedMail {
    pub data: Vec<u8>,
    pub file_name: String,
}

#[uniffi::export]
impl Mail {
    /// The newest message of conversation `thread_id` as it arrived.
    pub fn message_source(&self, account_id: i64, thread_id: String) -> Result<MessageSource, CoreError> {
        let meta = self
            .newest_message(account_id, thread_id)?
            .ok_or_else(|| CoreError::Store(gettext("Open a conversation first")))?;
        let raw = self.raw(account_id, &meta.id)
            .map_err(|err| CoreError::Store(fill(&gettext("Could not load the source: {reason}"), &[("reason", &err)])))?;
        Ok(MessageSource {
            subject: meta.subject.clone(),
            text: String::from_utf8_lossy(&raw).replace("\r\n", "\n"),
            file_name: file_name(&meta.subject, meta.date, "eml"),
            raw,
        })
    }

    /// `threads` as one mbox file, named after the first one's subject.
    pub fn export_mbox(&self, threads: Vec<ThreadRef>) -> Result<ExportedMail, CoreError> {
        let first = threads.first().cloned().ok_or_else(|| CoreError::Store(gettext("Open a conversation first")))?;
        let account_id = first.account_id;
        let targets: Vec<Target> = threads
            .iter()
            .map(|t| Target {
                account_id: t.account_id,
                thread_id: t.thread_id.clone(),
                message_id: None,
            })
            .collect();
        let name = match self.newest_message(account_id, first.thread_id.clone())? {
            Some(meta) => file_name(&meta.subject, meta.date, "mbox"),
            None => "mail.mbox".into(),
        };
        let actions = Arc::clone(&self.actions);
        let data = self
            .run(async move { actions.export_mbox(&targets).await })
            .map_err(|err| CoreError::Store(fill(&gettext("Could not export the mail: {reason}"), &[("reason", &err.to_string())])))?;
        Ok(ExportedMail { data, file_name: name })
    }
}

impl Mail {
    /// The newest message of the thread that is not a draft.
    fn newest_message(&self, account_id: AccountId, thread_id: String) -> Result<Option<MessageMeta>, CoreError> {
        let db = self.db.clone();
        Ok(self.run(async move {
            db.read(move |c| {
                Ok(messages::thread_messages(c, account_id, &thread_id)?
                    .into_iter()
                    .rev()
                    .find(|m| !m.in_role(Role::Drafts)))
            })
            .await
        })?)
    }

    fn raw(&self, account_id: AccountId, message_id: &str) -> Result<Vec<u8>, String> {
        let sync = self
            .running
            .account(account_id)
            .ok_or_else(|| gettext("That account is not connected"))?;
        let id = message_id.to_string();
        self.run(async move { sync.raw_message(&id).await }).map_err(|err| err.to_string())
    }
}
