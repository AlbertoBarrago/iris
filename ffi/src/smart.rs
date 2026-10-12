//! Smart mailboxes for the Swift front end: saved conditions over the
//! mail, for every account or one, kept in the settings file as the GTK
//! app kept them. `mailrs_sync` lists what each one finds.

use mailrs_appcore::settings::Change;
use mailrs_domain::SmartMailbox;
use mailrs_domain::smart::{Condition, Field};
use mailrs_domain::translate::gettext;
use mailrs_store::accounts;

use crate::CoreError;
use crate::mail::Mail;
use crate::prefs::{self, key, parse};

/// A condition a smart mailbox can hold.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct SmartField {
    pub key: String,
    /// "From contains".
    pub label: String,
    /// Whether it takes text beside it.
    pub takes_value: bool,
}

#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct SmartCondition {
    pub field: String,
    pub value: String,
}

/// A smart mailbox as its editor shows it.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct SmartForm {
    /// Empty for a new one.
    pub id: String,
    pub name: String,
    /// One account's address, or empty for every account.
    pub account: String,
    pub match_all: bool,
    pub conditions: Vec<SmartCondition>,
}

#[uniffi::export]
impl Mail {
    /// The conditions the editor offers, in its order.
    pub fn smart_fields(&self) -> Vec<SmartField> {
        Field::ALL
            .into_iter()
            .map(|field| SmartField {
                key: key(&field),
                label: field.label(),
                takes_value: field.takes_value(),
            })
            .collect()
    }

    /// Every account's address, for the editor's account choice.
    pub fn smart_accounts(&self) -> Result<Vec<String>, CoreError> {
        let db = self.db.clone();
        Ok(self
            .run(async move { db.read(accounts::list_accounts).await })?
            .into_iter()
            .map(|a| a.email)
            .collect())
    }

    /// Smart mailbox `id` as the editor shows it.
    pub fn smart_mailbox(&self, id: String) -> Option<SmartForm> {
        prefs::load().smart_mailboxes.into_iter().find(|m| m.id == id).map(|m| SmartForm {
            id: m.id,
            name: m.name,
            account: m.account.unwrap_or_default(),
            match_all: m.match_all,
            conditions: m
                .conditions
                .into_iter()
                .map(|c| SmartCondition {
                    field: key(&c.field),
                    value: c.value,
                })
                .collect(),
        })
    }

    /// Saves `form`, a new smart mailbox when its id is empty, or says
    /// what it still needs.
    pub fn save_smart_mailbox(&self, form: SmartForm) -> Result<(), CoreError> {
        let mailbox = smart_of(form)?;
        prefs::save(Change::SaveSmartMailbox(Box::new(mailbox)))?;
        self.changed("sidebar");
        Ok(())
    }

    pub fn delete_smart_mailbox(&self, id: String) -> Result<(), CoreError> {
        prefs::save(Change::DeleteSmartMailbox(id))?;
        self.changed("sidebar");
        Ok(())
    }
}

/// The smart mailbox `form` describes, or what it still needs: a name,
/// and a condition the mail can be searched for.
fn smart_of(form: SmartForm) -> Result<SmartMailbox, CoreError> {
    let name = form.name.trim().to_string();
    if name.is_empty() {
        return Err(CoreError::Store(gettext("Give the mailbox a name")));
    }
    let conditions = form
        .conditions
        .into_iter()
        .map(|c| Ok(Condition { field: parse(&c.field)?, value: c.value.trim().to_string() }))
        .collect::<Result<Vec<_>, CoreError>>()?;
    let mailbox = SmartMailbox {
        id: match form.id.is_empty() {
            true => format!("smart-{}", mailrs_gmail::random_token(6)),
            false => form.id,
        },
        name,
        account: Some(form.account).filter(|a| !a.is_empty()),
        match_all: form.match_all,
        conditions,
    };
    if mailbox.query().is_none() {
        return Err(CoreError::Store(gettext("Add a condition with a value")));
    }
    Ok(mailbox)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn form(name: &str, field: &str, value: &str) -> SmartForm {
        SmartForm {
            id: String::new(),
            name: name.into(),
            account: String::new(),
            match_all: true,
            conditions: vec![SmartCondition { field: field.into(), value: value.into() }],
        }
    }

    #[test]
    fn a_new_mailbox_gets_an_id_of_its_own() {
        let made = smart_of(form(" Receipts ", "subject", "receipt")).expect("a mailbox");
        assert!(made.id.starts_with("smart-"));
        assert_eq!(made.name, "Receipts");
        assert_eq!(made.account, None);
    }

    #[test]
    fn a_mailbox_needs_a_name_and_a_usable_condition() {
        assert!(smart_of(form("", "subject", "receipt")).is_err());
        assert!(smart_of(form("Receipts", "subject", "  ")).is_err());
        assert!(smart_of(form("Unread", "unread", "")).is_ok());
    }
}
