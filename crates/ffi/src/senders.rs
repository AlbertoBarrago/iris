//! What Iris does about the person who wrote a conversation, for the
//! Swift front end: sort their mail into a category, or block them, each
//! with a rule on the server for the mail still to come, as the GTK
//! window's sender menu did.

use std::sync::Arc;

use mailrs_appcore::images;
use mailrs_appcore::offered::{categorize_choices, sender_actions};
use mailrs_domain::translate::{fill, gettext};
use mailrs_domain::{Address, Category, Target};
use mailrs_store::messages;
use mailrs_sync::{History, MailAction, MovedFrom, Permitted, TriageAction};

use crate::CoreError;
use crate::mail::Mail;
use crate::prefs::ChoiceItem;

/// The sender menu for one conversation.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct SenderMenu {
    /// The person the menu acts on, as the menu names them.
    pub sender: String,
    pub email: String,
    /// The domain Always Load Images may let in as a whole; `None` when
    /// the address has none the list would take.
    pub image_domain: Option<String>,
    /// Categorize Sender's categories; empty when the account cannot
    /// sort senders.
    pub categories: Vec<ChoiceItem>,
    pub can_block: bool,
    /// The question before blocking.
    pub block_heading: String,
    pub block_body: String,
}

#[uniffi::export]
impl Mail {
    /// What the sender menu offers for conversation `thread_id`: its
    /// newest sender who is not the person, and what the account can do
    /// about them. `None` when only the person wrote in it.
    pub fn sender_menu(&self, account_id: i64, thread_id: String) -> Result<Option<SenderMenu>, CoreError> {
        let Some(sender) = self.other_sender(account_id, thread_id)? else { return Ok(None) };
        let offers = self.offers(account_id);
        let [(_, can_block), (_, can_sort)] = sender_actions(offers);
        let name = sender.display().to_string();
        Ok(Some(SenderMenu {
            categories: match can_sort {
                true => categorize_choices(offers)
                    .into_iter()
                    .map(|c| ChoiceItem { key: c.key().into(), label: c.name() })
                    .collect(),
                false => Vec::new(),
            },
            can_block,
            block_heading: fill(&gettext("Block {sender}?"), &[("sender", &name)]),
            block_body: fill(
                &gettext(
                    "New mail from {email} goes straight to the Trash. This conversation \
                     moves there now. Remove the rule under Rules to unblock.",
                ),
                &[("email", &sender.email)],
            ),
            image_domain: images::entry_for(&sender.email, true).map(|(domain, _)| domain),
            sender: name,
            email: sender.email,
        }))
    }

    /// Moves the sender's stored conversations, this one included, into
    /// the category `key` names, and adds the rule that sorts their mail
    /// there from now on. Answers the toast.
    pub fn categorize_sender(&self, account_id: i64, thread_id: String, key: String) -> Result<String, CoreError> {
        let category = Category::from_key(&key).ok_or_else(|| CoreError::Store(format!("not a category: {key}")))?;
        let sender = self
            .other_sender(account_id, thread_id.clone())?
            .ok_or_else(|| CoreError::Store(gettext("Open a message from the sender first")))?;
        let (actions, email) = (Arc::clone(&self.actions), sender.email.clone());
        let done = self.run(async move { actions.categorize_sender(account_id, &email, Some(&thread_id), category).await });
        self.mailboxes.forget_remote();
        let name = category.name();
        let who = sender.display().to_string();
        let values = [("sender", who.as_str()), ("category", name.as_str())];
        if let Some(error) = done.moved.failed.first() {
            return Err(CoreError::Store(error.error.clone()));
        }
        Ok(match done.sorted {
            Ok(Permitted::Done(())) => fill(&gettext("Mail from {sender} now goes to {category}"), &values),
            Ok(Permitted::NeedsPermission) => fill(&gettext("Moved mail from {sender} to {category}"), &values),
            Err(err) => fill(
                &gettext("Moved mail from {sender} to {category}, but could not sort new mail: {reason}"),
                &[("sender", who.as_str()), ("category", name.as_str()), ("reason", &err.to_string())],
            ),
        })
    }

    /// Sends the sender's future mail to the Trash with a rule, and moves
    /// this conversation there now. Answers the toast.
    pub fn block_sender(&self, account_id: i64, thread_id: String) -> Result<String, CoreError> {
        let account = self.account_row(account_id)?;
        let sender = self
            .other_sender(account_id, thread_id.clone())?
            .ok_or_else(|| CoreError::Store(gettext("Open a message from the sender first")))?;
        let (settings, email) = (self.account_settings(), sender.email.clone());
        let blocked = self
            .run(async move { settings.block_sender(account_id, &email).await })
            .map_err(|err| CoreError::Store(fill(&gettext("Could not block the sender: {reason}"), &[("reason", &err.to_string())])))?;
        if let Permitted::NeedsPermission = blocked {
            return Err(CoreError::Store(mailrs_appcore::permission::settings_needed(account.provider)));
        }
        let actions = Arc::clone(&self.actions);
        let target = Target {
            account_id,
            thread_id,
            message_id: None,
        };
        self.run(async move {
            actions
                .run_from(&[target], MailAction::Triage(TriageAction::Trash), History::Record, &MovedFrom::nowhere())
                .await
        });
        self.mailboxes.forget_remote();
        Ok(fill(&gettext("Blocked {sender}"), &[("sender", &sender.email)]))
    }
}

impl Mail {
    /// The newest sender of the thread who is not one of the person's
    /// own addresses.
    fn other_sender(&self, account_id: i64, thread_id: String) -> Result<Option<Address>, CoreError> {
        let mine: Vec<String> = self.senders()?.into_iter().map(|s| s.email).collect();
        let db = self.db.clone();
        let metas = self.run(async move {
            db.read(move |c| messages::thread_messages(c, account_id, &thread_id)).await
        })?;
        Ok(metas
            .into_iter()
            .rev()
            .filter_map(|m| m.from)
            .find(|a| !mine.iter().any(|own| own.eq_ignore_ascii_case(&a.email))))
    }
}
