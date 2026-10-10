//! Labels and folders for the mail picked: the picker's rows, what a row
//! does, a new label from the picker, and mail dropped on a mailbox in the
//! sidebar. appcore words the picker (`Filing`) and decides what a drop
//! does (`moving`), as it does for the GTK app.

use std::collections::{BTreeMap, HashSet};
use std::sync::Arc;

use mailrs_appcore::moving::{self, moved_to};
use mailrs_appcore::label_colors::{LABEL_COLORS, label_color_name};
use mailrs_appcore::offered::{Filing, offers_for};
use mailrs_domain::translate::{fill, gettext};
use mailrs_domain::{AccountId, LabelKind, Target};
use mailrs_store::{labels, messages};
use mailrs_sync::{Accounts, History, MailAction, MovedFrom, Offers, TriageAction};

use mailrs_view::tree::{self, Placement};

use crate::CoreError;
use crate::mail::{ActionDone, Mail, ThreadRef};

/// The label picker for the mail picked.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct FilingMenu {
    /// The account files in folders: a row moves the mail.
    pub folders: bool,
    /// The picker's heading, "Choose Labels" or "Choose a Folder".
    pub title: String,
    /// "Labels" or "Move to folder", for the button and the submenu.
    pub name: String,
    /// The button's tooltip, with its key.
    pub tooltip: String,
    /// The new label dialog's heading and its field's prompt.
    pub new_heading: String,
    pub new_prompt: String,
    /// "New Label…" or "New Folder…", or empty when the picker cannot
    /// make one, for mail from several accounts.
    pub new_item: String,
    /// What the picker says in place of rows: nothing picked, mail from
    /// several folder accounts, or no labels yet.
    pub said: Option<String>,
    pub rows: Vec<FilingRow>,
}

/// One label or folder in the picker.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct FilingRow {
    /// The label's server id, or its name for mail from several accounts.
    pub id: String,
    /// `Work › Tax` for `Work/Tax`.
    pub name: String,
    pub applied: bool,
    /// The row for a screen reader, which hears whether it is applied.
    pub described: String,
}

/// What a label's menu in the sidebar needs: its words, its colours and
/// where it can move.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct LabelLook {
    pub account_id: i64,
    /// Its whole name, `Work/Tax`, as Rename starts from.
    pub name: String,
    pub rename_heading: String,
    pub rename_body: String,
    pub new_inside_item: String,
    pub new_heading: String,
    pub new_prompt: String,
    pub delete_title: String,
    pub delete_body: String,
    pub colors: Vec<LabelColorChoice>,
    pub can_move_up: bool,
    pub can_move_down: bool,
}

/// One colour of Gmail's label palette.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct LabelColorChoice {
    pub name: String,
    /// `#rrggbb`.
    pub hex: String,
}

#[uniffi::export]
impl Mail {
    /// The menu of the label behind the sidebar's `key`.
    pub fn label_look(&self, key: String) -> Result<LabelLook, CoreError> {
        let (account_id, label_id) = self.label_of(&key)?;
        let all = self.all_labels(account_id)?;
        let order = self.label_positions(account_id)?;
        let rows = tree::label_rows(&all, &order);
        let moves = tree::moves(&rows, &label_id);
        let name = all.iter().find(|l| l.id == label_id).map(|l| l.name.clone()).unwrap_or_default();
        let filing = Filing::of([self.offers(account_id)]);
        Ok(LabelLook {
            account_id,
            delete_title: fill(&gettext("Delete “{name}”?"), &[("name", &shown_name(&name))]),
            name,
            rename_heading: filing.rename_heading(),
            rename_body: filing.rename_body(),
            new_inside_item: filing.new_inside_item(),
            new_heading: filing.new_heading(),
            new_prompt: gettext("Name, or Parent/Name to nest it"),
            delete_body: filing.delete_body(),
            colors: LABEL_COLORS
                .iter()
                .enumerate()
                .map(|(index, (hex, _))| LabelColorChoice {
                    name: label_color_name(index),
                    hex: hex.to_string(),
                })
                .collect(),
            can_move_up: moves.up,
            can_move_down: moves.down,
        })
    }

    /// Renames the label behind `key`; a slash nests it under another.
    pub fn rename_label(&self, key: String, name: String) -> Result<(), CoreError> {
        let (account_id, label_id) = self.label_of(&key)?;
        let name = name.trim().to_string();
        if name.is_empty() {
            return Ok(());
        }
        let sync = self.connected(account_id)?;
        let filing = Filing::of([self.offers(account_id)]);
        self.run(async move { sync.rename_label(&label_id, &name).await })
            .map_err(|err| failed(&filing.rename_failed(), &err))
    }

    /// Deletes the label behind `key`, and says so.
    pub fn delete_label(&self, key: String) -> Result<String, CoreError> {
        let (account_id, label_id) = self.label_of(&key)?;
        let name = self.all_labels(account_id)?.into_iter().find(|l| l.id == label_id).map(|l| l.name).unwrap_or_default();
        let sync = self.connected(account_id)?;
        let filing = Filing::of([self.offers(account_id)]);
        self.run(async move { sync.delete_label(&label_id).await })
            .map_err(|err| failed(&filing.delete_failed(), &err))?;
        Ok(fill(&gettext("Deleted “{name}”"), &[("name", &shown_name(&name))]))
    }

    /// Gives the label behind `key` colour `index` of Gmail's palette.
    pub fn color_label(&self, key: String, index: u32) -> Result<(), CoreError> {
        let (account_id, label_id) = self.label_of(&key)?;
        let Some((background, text)) = LABEL_COLORS.get(index as usize) else {
            return Ok(());
        };
        let color = mailrs_gmail::LabelColor {
            background_color: background.to_string(),
            text_color: text.to_string(),
        };
        let sync = self.connected(account_id)?;
        self.run(async move { sync.set_label_color(&label_id, color).await })
            .map_err(|err| failed(&gettext("Could not change the color: {reason}"), &err))
    }

    /// Moves the label behind `key` one place up (`-1`) or down (`1`)
    /// among its siblings.
    pub fn move_label(&self, key: String, step: i32) -> Result<(), CoreError> {
        let (account_id, label_id) = self.label_of(&key)?;
        let all = self.all_labels(account_id)?;
        let order = self.label_positions(account_id)?;
        let rows = tree::label_rows(&all, &order);
        let Some(Placement { name, order }) = tree::step(&rows, &label_id, step.signum()) else {
            return Ok(());
        };
        let sync = self.connected(account_id)?;
        let filing = Filing::of([self.offers(account_id)]);
        self.run(async move { sync.place_label(&label_id, &name, &order).await })
            .map_err(|err| failed(&filing.rename_failed(), &err))
    }

    /// Makes a label on `account_id` from the sidebar, with no mail to
    /// file under it.
    pub fn new_label(&self, account_id: i64, name: String) -> Result<(), CoreError> {
        let name = name.trim().to_string();
        if name.is_empty() {
            return Ok(());
        }
        self.create_label(account_id, name).map(|_| ())
    }

    /// The picker for `threads`: the account's labels with those already
    /// on a single conversation ticked, or the label names every account
    /// holds for mail from several.
    pub fn filing_menu(&self, threads: Vec<ThreadRef>) -> Result<FilingMenu, CoreError> {
        let accounts: Vec<AccountId> = distinct(threads.iter().map(|t| t.account_id));
        let filing = Filing::picker(&accounts, &[], |id| self.offers(id));
        let mut menu = FilingMenu {
            folders: filing == Filing::Folders,
            title: match filing {
                Filing::Labels => gettext("Choose Labels"),
                Filing::Folders => gettext("Choose a Folder"),
            },
            name: filing.shortcut_line(),
            tooltip: filing.tooltip(),
            new_heading: filing.new_heading(),
            new_prompt: gettext("Name, or Parent/Name to nest it"),
            new_item: String::new(),
            said: None,
            rows: Vec::new(),
        };
        match accounts.as_slice() {
            [] => menu.said = Some(filing.nothing_picked()),
            [account] => {
                let account = *account;
                // A message sits in one folder, so no folder shows ticked.
                let applied = match (filing, threads.as_slice()) {
                    (Filing::Labels, [one]) => self.applied(one)?,
                    _ => HashSet::new(),
                };
                menu.rows = self
                    .user_labels(account)?
                    .into_iter()
                    .map(|label| row(&label.id, &label.name, applied.contains(&label.id)))
                    .collect();
                menu.new_item = filing.new_item();
                if menu.rows.is_empty() {
                    menu.said = Some(filing.none_yet());
                }
            }
            many => match Filing::across(many.iter().map(|id| self.offers(*id))) {
                // Each account has its own label behind a name, so mail
                // from several takes labels by name.
                Filing::Labels => {
                    let mut names = BTreeMap::new();
                    for account in many {
                        for label in self.user_labels(*account)? {
                            names.entry(label.name.to_lowercase()).or_insert(label.name);
                        }
                    }
                    menu.rows = names.into_values().map(|name| row(&name, &name, false)).collect();
                }
                Filing::Folders => menu.said = Some(Filing::Folders.one_account_only()),
            },
        }
        Ok(menu)
    }

    /// Files `threads` under the picker's row `id`: moves them on a folder
    /// account, puts the label on, or takes it off when `applied`. For
    /// mail from several accounts `id` is a label name.
    pub fn file(&self, threads: Vec<ThreadRef>, id: String, name: String, applied: bool, from: Option<String>) -> ActionDone {
        let accounts = distinct(threads.iter().map(|t| t.account_id));
        let (action, words) = match accounts.as_slice() {
            [account] => match Filing::of([self.offers(*account)]) {
                Filing::Folders => (MailAction::Triage(TriageAction::MoveTo(id)), Some(moved_to(&name))),
                Filing::Labels if applied => (MailAction::Triage(TriageAction::RemoveLabel(id)), None),
                Filing::Labels => (MailAction::Triage(TriageAction::AddLabel(id)), None),
            },
            _ => (
                MailAction::Label {
                    add: vec![id],
                    remove: Vec::new(),
                    create: false,
                },
                None,
            ),
        };
        let moved = from.and_then(|key| self.mailbox(&key)).map(|m| m.moved_from()).unwrap_or_else(MovedFrom::nowhere);
        self.run_filing(threads, action, words, moved)
    }

    /// Makes a label, or a folder on a folder account, called `name` on
    /// `account_id`, and answers its row.
    pub fn create_label(&self, account_id: i64, name: String) -> Result<FilingRow, CoreError> {
        let sync = self.connected(account_id)?;
        let label = self
            .run(async move { sync.create_label(&name).await })
            .map_err(|err| CoreError::Store(err.to_string()))?;
        self.changed("labels");
        Ok(row(&label.id, &label.name, false))
    }

    /// Drops `threads`, listed in the sidebar's `from`, on its `to`, as
    /// dragging them there does.
    pub fn drop_on(&self, threads: Vec<ThreadRef>, from: Option<String>, to: String) -> Result<ActionDone, CoreError> {
        let to = self.mailbox(&to).ok_or_else(|| CoreError::Store("That mailbox is no longer in the sidebar.".into()))?;
        let from = from.and_then(|key| self.mailbox(&key));
        let targets: Vec<Target> = threads.iter().cloned().map(target).collect();
        let source = from.clone().unwrap_or(mailrs_sync::Mailbox::Unified(mailrs_sync::mailbox::Standard::Inbox));
        let moves = |id| !self.offers(id).labels;
        let (action, words) = moving::dropped(&targets, &source, &to, moves).map_err(CoreError::Store)?;
        let moved = from.map(|m| m.moved_from()).unwrap_or_else(MovedFrom::nowhere);
        Ok(self.run_filing(threads, action, words, moved))
    }
}

impl Mail {
    /// The account and the label id behind a sidebar key.
    fn label_of(&self, key: &str) -> Result<(AccountId, String), CoreError> {
        match self.mailbox(key) {
            Some(mailrs_sync::Mailbox::Label { account_id, label_id, .. }) => Ok((account_id, label_id)),
            _ => Err(CoreError::Store("That label is no longer in the sidebar.".into())),
        }
    }

    fn connected(&self, account_id: AccountId) -> Result<Arc<mailrs_sync::AccountSync>, CoreError> {
        self.running
            .account(account_id)
            .ok_or_else(|| CoreError::Store(gettext("That account is not connected")))
    }

    fn all_labels(&self, account_id: AccountId) -> Result<Vec<mailrs_domain::Label>, CoreError> {
        let db = self.db.clone();
        Ok(self.run(async move { db.read(move |c| labels::list_labels(c, account_id)).await })?)
    }

    /// Where the person put each label among its siblings, by label id.
    pub(crate) fn label_positions(&self, account_id: AccountId) -> Result<std::collections::HashMap<String, i64>, CoreError> {
        let db = self.db.clone();
        Ok(self.run(async move { db.read(move |c| labels::positions(c, account_id)).await })?)
    }

    /// What `account_id` offers, or everything while it has not started.
    pub(crate) fn offers(&self, account_id: AccountId) -> Offers {
        let running = self.running.account(account_id);
        offers_for(running.as_ref().map(|sync| sync.services()))
    }

    /// The account's own labels, sorted as the picker lists them.
    fn user_labels(&self, account_id: AccountId) -> Result<Vec<mailrs_domain::Label>, CoreError> {
        let db = self.db.clone();
        let mut found: Vec<_> = self
            .run(async move { db.read(move |c| labels::list_labels(c, account_id)).await })?
            .into_iter()
            .filter(|l| l.kind == LabelKind::User)
            .collect();
        found.sort_by_key(|l| l.name.to_lowercase());
        Ok(found)
    }

    /// Every mailbox some message of `thread` is in.
    fn applied(&self, thread: &ThreadRef) -> Result<HashSet<String>, CoreError> {
        let (db, account, id) = (self.db.clone(), thread.account_id, thread.thread_id.clone());
        let found = self.run(async move { db.read(move |c| messages::thread_messages(c, account, &id)).await })?;
        Ok(found.into_iter().flat_map(|m| m.held.mailboxes).collect())
    }

    fn run_filing(&self, threads: Vec<ThreadRef>, action: MailAction, words: Option<String>, moved: MovedFrom) -> ActionDone {
        let targets: Vec<Target> = threads.into_iter().map(target).collect();
        let (actions, given) = (Arc::clone(&self.actions), action.clone());
        let outcome = self.run(async move { actions.run_from(&targets, given, History::Record, &moved).await });
        self.mailboxes.forget_remote();
        let words = words.unwrap_or_else(|| {
            mailrs_view::done::done_message(&action, outcome.done.len(), true).unwrap_or_default()
        });
        ActionDone {
            words: if outcome.done.is_empty() { String::new() } else { words },
            done: outcome.done.len() as u32,
            failed: outcome.failed.first().map(|f| f.error.clone()),
        }
    }
}

/// `words` with the server's `reason` in it.
fn failed(words: &str, reason: &impl std::fmt::Display) -> CoreError {
    CoreError::Store(fill(words, &[("reason", &reason.to_string())]))
}

fn target(thread: ThreadRef) -> Target {
    Target {
        account_id: thread.account_id,
        thread_id: thread.thread_id,
        message_id: None,
    }
}

fn distinct(ids: impl Iterator<Item = AccountId>) -> Vec<AccountId> {
    let mut ids: Vec<AccountId> = ids.collect();
    ids.sort_unstable();
    ids.dedup();
    ids
}

/// A picker row. The tick beside the name is the only sign a label is on
/// the mail, so the name a screen reader hears carries it too.
fn row(id: &str, name: &str, applied: bool) -> FilingRow {
    let shown = shown_name(name);
    FilingRow {
        id: id.to_string(),
        described: match applied {
            true => fill(
                &gettext("{label}, on this mail"),
                &[("label", &shown)],
            ),
            false => shown.clone(),
        },
        name: shown,
        applied,
    }
}

/// A nested label's name as the picker shows it, `Work › Tax` for
/// `Work/Tax`.
fn shown_name(label: &str) -> String {
    label.replace('/', " › ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_nested_label_reads_with_an_arrow_and_says_when_it_is_on() {
        let on = row("Label_1", "Work/Tax", true);
        assert_eq!(on.name, "Work › Tax");
        assert_ne!(on.described, on.name, "a ticked row says so out loud");
        assert_eq!(row("Label_2", "Home", false).described, "Home");
    }

    #[test]
    fn nothing_picked_says_so() {
        let dir = tempfile::tempdir().unwrap();
        let mail = Mail::open(dir.path().join("mailrs.db").display().to_string()).unwrap();
        let menu = mail.filing_menu(Vec::new()).unwrap();
        assert!(menu.said.is_some());
        assert!(menu.rows.is_empty());
    }
}
