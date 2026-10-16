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
    /// The colour it shows in, as an index into `colors`, or `None` when it
    /// shows in none.
    pub color: Option<u32>,
    pub colors: Vec<AccountColorChoice>,
    pub can_move_up: bool,
    pub can_move_down: bool,
    /// "New Label…", or "New Folder…" on an account that files in
    /// folders, with its dialog's heading and prompt.
    pub new_label_item: String,
    pub new_label_heading: String,
    pub new_label_prompt: String,
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
        let alone = all.len() == 1;
        let order = ordered(&settings, all);
        let at = order.iter().position(|a| a.id == account_id).unwrap_or(0);
        let filing = mailrs_appcore::offered::Filing::of([self.offers(account_id)]);
        Ok(AccountLook {
            new_label_item: filing.new_item(),
            new_label_heading: filing.new_heading(),
            new_label_prompt: mailrs_domain::translate::gettext("Name, or Parent/Name to nest it"),
            name: settings.account_names.get(&account.email).cloned().unwrap_or_default(),
            color: color_index(&settings, &account, alone).map(|i| i as u32),
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
        self.change_accounts(Change::AccountColor {
            email,
            index: Some(index),
        })
    }

    /// Shows `account_id` with no colour, in the sidebar and on its rows.
    pub fn uncolor_account(&self, account_id: i64) -> Result<(), CoreError> {
        let email = self.account_row(account_id)?.email;
        self.change_accounts(Change::AccountColor { email, index: None })
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

/// The palette index `account` shows in, or `None` for no colour: the one
/// chosen, else none when the person took the colour away, else none when
/// it is the only account, since a colour tells accounts apart and there
/// is nothing to tell it from. Otherwise the one its id gives, as the GTK
/// app falls back to.
pub(crate) fn color_index(settings: &Settings, account: &Account, alone: bool) -> Option<usize> {
    if let Some(index) = settings
        .account_colors
        .get(&account.email)
        .copied()
        .filter(|i| *i < format::PALETTE.len())
    {
        return Some(index);
    }
    if alone || settings.account_colorless.contains(&account.email) {
        return None;
    }
    Some((account.id.max(1) - 1) as usize % format::PALETTE.len())
}

/// Every account's colour, for the list's rows; `None` for an account
/// shown in none.
pub(crate) fn colors(settings: &Settings, accounts: &[Account]) -> HashMap<AccountId, Option<String>> {
    let alone = accounts.len() == 1;
    accounts
        .iter()
        .map(|a| {
            let color = color_index(settings, a, alone).map(|i| format::PALETTE[i].to_string());
            (a.id, color)
        })
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
            index: Some(5),
        }
        .apply_to(&mut settings);
        let order: Vec<AccountId> = ordered(&settings, all.clone()).iter().map(|a| a.id).collect();
        assert_eq!(order, [2, 1]);
        assert_eq!(color_index(&settings, &all[0], false), Some(5));
        assert_eq!(color_index(&settings, &all[1], false), Some(1));
    }

    #[test]
    fn an_account_taken_out_of_colour_shows_none() {
        let mut settings = Settings::default();
        let all = vec![account(1, "a@x.com"), account(2, "b@x.com")];
        Change::AccountColor {
            email: "b@x.com".into(),
            index: None,
        }
        .apply_to(&mut settings);
        let shown = colors(&settings, &all);
        assert_eq!(shown[&1].as_deref(), Some(format::PALETTE[0]));
        assert_eq!(shown[&2], None);
    }

    #[test]
    fn a_lone_account_shows_no_colour_unless_one_was_chosen() {
        let mut settings = Settings::default();
        let all = vec![account(1, "a@x.com")];
        assert_eq!(colors(&settings, &all)[&1], None);
        Change::AccountColor {
            email: "a@x.com".into(),
            index: Some(4),
        }
        .apply_to(&mut settings);
        assert_eq!(colors(&settings, &all)[&1].as_deref(), Some(format::PALETTE[4]));
    }
}
