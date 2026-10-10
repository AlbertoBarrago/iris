//! The sidebar's account menu: the name and the colour an account shows
//! under, and its place among the others. Each is a `Change` to
//! Preferences, the same the GTK app makes, so both apps show an account
//! alike.

use std::collections::HashMap;

use mailrs_appcore::settings::{Change, Settings};
use mailrs_domain::{Account, AccountId};
use mailrs_render::format;

use crate::CoreError;
use crate::mail::Mail;

/// What the account menu needs to know about one account.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct AccountLook {
    pub email: String,
    /// The name the sidebar shows, or empty for the address.
    pub name: String,
    /// The colour chosen, as an index into `colors`.
    pub color: u32,
    pub colors: Vec<AccountColorChoice>,
    pub can_move_up: bool,
    pub can_move_down: bool,
}

/// One colour of the account palette.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct AccountColorChoice {
    pub name: String,
    /// `#rrggbb`.
    pub hex: String,
}

#[uniffi::export]
impl Mail {
    /// The account menu's state for `account_id`.
    pub fn account_look(&self, account_id: i64) -> Result<AccountLook, CoreError> {
        let account = self.account_row(account_id)?;
        let settings = Settings::load(&Settings::default_path());
        let all = self.all_accounts()?;
        let order = ordered(&settings, all);
        let at = order.iter().position(|a| a.id == account_id).unwrap_or(0);
        Ok(AccountLook {
            name: settings.account_names.get(&account.email).cloned().unwrap_or_default(),
            color: color_index(&settings, &account) as u32,
            colors: palette(),
            can_move_up: at > 0,
            can_move_down: at + 1 < order.len(),
            email: account.email,
        })
    }

    /// The colours the account menu offers.
    pub fn account_palette(&self) -> Vec<AccountColorChoice> {
        palette()
    }

    /// Shows `account_id` under `name` in the sidebar; empty goes back to
    /// the address.
    pub fn rename_account(&self, account_id: i64, name: String) -> Result<(), CoreError> {
        let email = self.account_row(account_id)?.email;
        self.change_accounts(Change::AccountName {
            email,
            name: name.trim().to_string(),
        })
    }

    /// Gives `account_id` the palette's colour at `index`.
    pub fn color_account(&self, account_id: i64, index: u32) -> Result<(), CoreError> {
        let email = self.account_row(account_id)?.email;
        let index = (index as usize).min(format::PALETTE.len() - 1);
        self.change_accounts(Change::AccountColor { email, index })
    }

    /// Moves `account_id` one place up (`-1`) or down (`1`) in the
    /// sidebar.
    pub fn move_account(&self, account_id: i64, step: i32) -> Result<(), CoreError> {
        let email = self.account_row(account_id)?.email;
        let emails = self.all_accounts()?.into_iter().map(|a| a.email).collect();
        self.change_accounts(Change::MoveAccount {
            emails,
            email,
            step: step.signum() as isize,
        })
    }
}

impl Mail {
    fn all_accounts(&self) -> Result<Vec<Account>, CoreError> {
        let db = self.db.clone();
        Ok(self.run(async move { db.read(mailrs_store::accounts::list_accounts).await })?)
    }

    /// Saves `change` and tells the window, which reads the sidebar and
    /// the list again for the new name, colour or order.
    fn change_accounts(&self, change: Change) -> Result<(), CoreError> {
        let path = Settings::default_path();
        let mut settings = Settings::load(&path);
        change.apply_to(&mut settings);
        settings.save(&path).map_err(|err| CoreError::Store(err.to_string()))?;
        self.changed("accounts");
        Ok(())
    }
}

fn palette() -> Vec<AccountColorChoice> {
    (0..format::PALETTE.len())
        .map(|index| AccountColorChoice {
            name: format::palette_name(index),
            hex: format::PALETTE[index].into(),
        })
        .collect()
}

/// `accounts` in the order the person chose in the account menu.
pub(crate) fn ordered(settings: &Settings, mut accounts: Vec<Account>) -> Vec<Account> {
    let emails: Vec<&str> = accounts.iter().map(|a| a.email.as_str()).collect();
    let order: Vec<String> = settings.ordered(&emails).iter().map(|e| e.to_string()).collect();
    accounts.sort_by_key(|a| order.iter().position(|e| *e == a.email));
    accounts
}

/// The palette index `account` shows in: the one chosen, else the one its
/// id gives, as the GTK app falls back to.
pub(crate) fn color_index(settings: &Settings, account: &Account) -> usize {
    settings
        .account_colors
        .get(&account.email)
        .copied()
        .filter(|i| *i < format::PALETTE.len())
        .unwrap_or((account.id.max(1) - 1) as usize % format::PALETTE.len())
}

/// Every account's colour, for the list's rows.
pub(crate) fn colors(settings: &Settings, accounts: &[Account]) -> HashMap<AccountId, String> {
    accounts
        .iter()
        .map(|a| (a.id, format::PALETTE[color_index(settings, a)].to_string()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use mailrs_domain::{AccountState, Provider};

    fn account(id: AccountId, email: &str) -> Account {
        Account {
            id,
            email: email.into(),
            state: AccountState::Ok,
            provider: Provider::Imap,
            provider_name: None,
        }
    }

    #[test]
    fn a_chosen_order_and_colour_win_over_the_ids() {
        let mut settings = Settings::default();
        let all = vec![account(1, "a@x.com"), account(2, "b@x.com")];
        Change::MoveAccount {
            emails: vec!["a@x.com".into(), "b@x.com".into()],
            email: "b@x.com".into(),
            step: -1,
        }
        .apply_to(&mut settings);
        Change::AccountColor {
            email: "a@x.com".into(),
            index: 5,
        }
        .apply_to(&mut settings);
        let order: Vec<AccountId> = ordered(&settings, all.clone()).iter().map(|a| a.id).collect();
        assert_eq!(order, [2, 1]);
        assert_eq!(color_index(&settings, &all[0]), 5);
        assert_eq!(color_index(&settings, &all[1]), 1);
    }
}
