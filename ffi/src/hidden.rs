//! Hide My Email for the Swift front end: plus addresses of the person's
//! own, each with the server rules that label its mail, kept in the
//! settings file as the GTK app keeps them. `mailrs_sync`'s
//! `AccountSettings` makes and drops the rules.

use mailrs_appcore::settings::Change;
use mailrs_domain::translate::{fill, gettext};
use mailrs_domain::{AccountId, EpochMillis};
use mailrs_store::accounts;
use mailrs_sync::Permitted;
use mailrs_sync::hidden::{self, HiddenAddress};

use crate::CoreError;
use crate::mail::Mail;
use crate::prefs;

/// One hidden address as the sheet lists it.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct HiddenRow {
    pub address: String,
    /// "Shopping site · Created 3 Oct", and "Off: mail goes to the
    /// Trash" for one turned off.
    pub details: String,
    pub active: bool,
}

/// The Hide My Email sheet's words and what it lists.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct HiddenPage {
    pub about: String,
    pub new_about: String,
    pub rows: Vec<HiddenRow>,
    /// The accounts a new address can belong to: those with rules.
    pub accounts: Vec<String>,
}

#[uniffi::export]
impl Mail {
    pub fn hidden_addresses(&self) -> Result<HiddenPage, CoreError> {
        let kept = prefs::load().hidden_addresses;
        let now = chrono::Local::now().date_naive();
        let db = self.db.clone();
        let listed = self.run(async move { db.read(accounts::list_accounts).await })?;
        Ok(HiddenPage {
            about: gettext(
                "Gmail delivers mail sent to these addresses to your inbox. Each one shows \
                 which site shared your address, and you can turn one off when the spam \
                 starts. Your real address is part of each one, so anyone who removes the tag \
                 can still reach you, and your replies come from your main address.",
            ),
            new_about: gettext(
                "You get a new address that ends in your own, such as \
                 name+kite.fern482@gmail.com. Give it to one site only.",
            ),
            rows: kept
                .iter()
                .map(|h| {
                    let mut details = Vec::new();
                    if !h.note.trim().is_empty() {
                        details.push(h.note.trim().to_string());
                    }
                    details.push(created_on(h.created, now));
                    if !h.active {
                        details.push(gettext("Off: mail goes to the Trash"));
                    }
                    HiddenRow {
                        address: h.address.clone(),
                        details: details.join(" · "),
                        active: h.active,
                    }
                })
                .collect(),
            accounts: listed
                .into_iter()
                .filter(|a| self.offers(a.id).rules)
                .map(|a| a.email)
                .collect(),
        })
    }

    /// Makes a new hidden address of the account `email`, noted as given
    /// to `note`, and keeps it. Answers the address.
    pub fn create_hidden_address(&self, email: String, note: String) -> Result<String, CoreError> {
        let account = self.account_by_email(&email)?;
        let (settings, taken) = (self.account_settings(), prefs::load().hidden_addresses);
        let made = self
            .run(async move { settings.create_hidden_address(account, &email, &note, &taken).await })
            .map_err(|err| failed("Could not create the address: {reason}", &err))?;
        match made {
            Permitted::Done(hidden) => {
                let address = hidden.address.clone();
                prefs::save(Change::SaveHiddenAddress(hidden))?;
                Ok(address)
            }
            Permitted::NeedsPermission => Err(CoreError::Store(gettext("Allow Hide My Email"))),
        }
    }

    /// Turns `address` on or off; off sends its mail to the Trash. Says
    /// which.
    pub fn set_hidden_address_active(&self, address: String, active: bool) -> Result<String, CoreError> {
        let (hidden, account) = self.hidden(&address)?;
        let settings = self.account_settings();
        let changed = self
            .run(async move { settings.set_hidden_address_active(account, &hidden, active).await })
            .map_err(|err| failed("Could not change the address: {reason}", &err))?;
        match changed {
            Permitted::Done(changed) => {
                prefs::save(Change::SaveHiddenAddress(changed))?;
                Ok(match active {
                    true => gettext("Mail to this address reaches you again"),
                    false => gettext("Mail to this address now goes to the Trash"),
                })
            }
            Permitted::NeedsPermission => Err(CoreError::Store(gettext("Allow Hide My Email"))),
        }
    }

    pub fn delete_hidden_address(&self, address: String) -> Result<String, CoreError> {
        let (hidden, account) = self.hidden(&address)?;
        let settings = self.account_settings();
        let gone = hidden.address.clone();
        let dropped = self
            .run(async move { settings.delete_hidden_address(account, &hidden).await })
            .map_err(|err| failed("Could not delete the address: {reason}", &err))?;
        match dropped {
            Permitted::Done(()) => {
                prefs::save(Change::ForgetHiddenAddress(gone))?;
                Ok(gettext("Address deleted"))
            }
            Permitted::NeedsPermission => Err(CoreError::Store(gettext("Allow Hide My Email"))),
        }
    }

    /// The question before deleting `address`.
    pub fn hidden_delete_words(&self, address: String) -> String {
        fill(
            &gettext(
                "Mail sent to {address} arrives in your inbox again, without the \
                 Hide My Email label. To stop that mail, turn the address off instead.",
            ),
            &[("address", &address)],
        )
    }
}

impl Mail {
    fn account_by_email(&self, email: &str) -> Result<AccountId, CoreError> {
        let db = self.db.clone();
        self.run(async move { db.read(accounts::list_accounts).await })?
            .into_iter()
            .find(|a| a.email.eq_ignore_ascii_case(email))
            .map(|a| a.id)
            .ok_or_else(|| CoreError::Store(gettext("That account is not connected")))
    }

    /// The kept address `address`, and the account it belongs to.
    fn hidden(&self, address: &str) -> Result<(HiddenAddress, AccountId), CoreError> {
        let kept = prefs::load().hidden_addresses;
        let hidden = hidden::find(&kept, address)
            .cloned()
            .ok_or_else(|| CoreError::Store(gettext("That account is not connected")))?;
        let account = self.account_by_email(&hidden.account)?;
        Ok((hidden, account))
    }
}

fn created_on(at: EpochMillis, today: chrono::NaiveDate) -> String {
    mailrs_appcore::format::local(at)
        .map(|when| {
            fill(
                &gettext("Created {date}"),
                &[("date", &mailrs_appcore::format::short_date(when.date_naive(), today))],
            )
        })
        .unwrap_or_default()
}

fn failed(pattern: &str, err: &impl std::fmt::Display) -> CoreError {
    CoreError::Store(fill(&gettext(pattern), &[("reason", &err.to_string())]))
}
