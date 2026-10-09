//! The sidebar's rows in order: the headings, the unified mailboxes under
//! them, then each account with its own mailboxes, folders, labels and
//! tags. A front end lays these out; it decides nothing about which rows
//! there are or where they go.

use std::collections::HashMap;

use mailrs_domain::{Account, AccountId, FlagColor, Folder, Label};
use mailrs_sync::Mailbox;
use mailrs_sync::mailbox::Standard;

use crate::sections::{self, Place, Section};
use crate::tree;

/// What a row's icon stands for. A front end picks its own picture for
/// each: GTK a symbolic icon, SwiftUI an SF Symbol.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Icon {
    Inbox,
    Flag(FlagColor),
    Sent,
    Drafts,
    Muted,
    Outbox,
    SendLater,
    RemindMe,
    FollowUp,
    Archive,
    Junk,
    Trash,
    AllMail,
    /// A label, or a folder that holds mail.
    Label,
    /// A folder that only holds other folders.
    Group,
    /// An Outlook category.
    Tag,
}

/// One line of the sidebar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Entry {
    Heading(Section),
    /// An account's own line, which its mailboxes sit under.
    Account(AccountId),
    Mailbox(Row),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    pub mailbox: Mailbox,
    pub title: String,
    pub icon: Icon,
    /// 0 for a unified mailbox, 1 under an account, one more for each
    /// level a label nests.
    pub depth: u32,
    /// A label's own color, as `#rrggbb`.
    pub color: Option<String>,
    /// Shown only while it holds mail: the flag colors.
    pub hidden_until_used: bool,
}

impl Row {
    fn new(mailbox: Mailbox, title: String, icon: Icon, depth: u32) -> Row {
        Row {
            hidden_until_used: sections::hidden_until_used(&mailbox),
            mailbox,
            title,
            icon,
            depth,
            color: None,
        }
    }
}

/// The sidebar for `accounts`, each with its labels, in the order the
/// person put each account's labels in (`label_order`, by label id).
pub fn entries(
    accounts: &[(Account, Vec<Label>)],
    label_order: &HashMap<AccountId, HashMap<String, i64>>,
) -> Vec<Entry> {
    let mut out = Vec::new();
    for (section, places) in sections::LAYOUT {
        out.push(Entry::Heading(section));
        for &place in places {
            unified(place, &mut out);
        }
    }
    if !accounts.is_empty() {
        out.push(Entry::Heading(Section::Accounts));
    }
    for (account, labels) in accounts {
        out.push(Entry::Account(account.id));
        for which in Standard::ALL {
            let mailbox = Mailbox::Standard {
                account_id: account.id,
                which,
            };
            out.push(Entry::Mailbox(Row::new(mailbox, which.name(), standard_icon(which), 1)));
        }
        for folder in Folder::ALL {
            let mailbox = Mailbox::Folder {
                account_id: Some(account.id),
                folder,
            };
            out.push(Entry::Mailbox(Row::new(mailbox, mailrs_sync::mailbox::folder_name(folder), folder_icon(folder), 1)));
        }
        let order = label_order.get(&account.id).cloned().unwrap_or_default();
        for entry in tree::label_rows(labels, &order) {
            let label = entry.label;
            let mailbox = Mailbox::Label {
                account_id: account.id,
                label_id: label.id.clone(),
                name: label.name.replace('/', " › "),
            };
            let icon = if entry.opens { Icon::Label } else { Icon::Group };
            let mut row = Row::new(mailbox, entry.leaf.to_string(), icon, entry.depth);
            row.color = label.color.clone();
            out.push(Entry::Mailbox(row));
        }
        for tag in tree::tag_rows(labels) {
            let mailbox = Mailbox::Label {
                account_id: account.id,
                label_id: tag.id.clone(),
                name: tag.name.clone(),
            };
            let mut row = Row::new(mailbox, tag.name.clone(), Icon::Tag, 1);
            row.color = tag.color.clone();
            out.push(Entry::Mailbox(row));
        }
    }
    out
}

/// The rows one place in the layout stands for.
fn unified(place: Place, out: &mut Vec<Entry>) {
    let row = |mailbox: Mailbox, icon| {
        let title = mailbox.title();
        Entry::Mailbox(Row::new(mailbox, title, icon, 0))
    };
    match place {
        Place::Unified(which) => out.push(row(Mailbox::Unified(which), standard_icon(which))),
        // The VIPs come from Preferences, which this front end does not
        // read yet.
        Place::Vips => {}
        Place::FlagColors => {
            for color in FlagColor::ALL {
                let mut flag = Row::new(Mailbox::Flag(color), color.name(), Icon::Flag(color), 1);
                flag.hidden_until_used = true;
                out.push(Entry::Mailbox(flag));
            }
        }
        Place::Outbox => out.push(row(Mailbox::Outbox, Icon::Outbox)),
        Place::Scheduled => out.push(row(Mailbox::Scheduled, Icon::SendLater)),
        Place::Reminders => out.push(row(Mailbox::Reminders, Icon::RemindMe)),
        Place::FollowUp => out.push(row(Mailbox::FollowUp, Icon::FollowUp)),
        Place::Folder(folder) => out.push(row(
            Mailbox::Folder {
                account_id: None,
                folder,
            },
            folder_icon(folder),
        )),
    }
}

fn standard_icon(which: Standard) -> Icon {
    match which {
        Standard::Inbox => Icon::Inbox,
        Standard::Flagged => Icon::Flag(FlagColor::Red),
        Standard::Sent => Icon::Sent,
        Standard::Drafts => Icon::Drafts,
        Standard::Muted => Icon::Muted,
    }
}

fn folder_icon(folder: Folder) -> Icon {
    match folder {
        Folder::Archive => Icon::Archive,
        Folder::Junk => Icon::Junk,
        Folder::Trash => Icon::Trash,
        Folder::AllMail => Icon::AllMail,
    }
}

/// The rows a count is asked for: every mailbox row.
pub fn counted(entries: &[Entry]) -> Vec<Mailbox> {
    entries
        .iter()
        .filter_map(|entry| match entry {
            Entry::Mailbox(row) => Some(row.mailbox.clone()),
            _ => None,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use mailrs_domain::{AccountState, LabelKind, Provider};

    use super::*;

    fn account(id: AccountId, email: &str) -> Account {
        Account {
            id,
            email: email.into(),
            state: AccountState::Ok,
            provider: Provider::Gmail,
            provider_name: None,
        }
    }

    fn label(account_id: AccountId, id: &str, name: &str) -> Label {
        Label {
            account_id,
            id: id.into(),
            name: name.into(),
            kind: LabelKind::User,
            color: None,
        }
    }

    #[test]
    fn unified_mailboxes_come_first_then_each_account_with_its_labels() {
        let accounts = vec![(account(1, "ann@example.com"), vec![label(1, "L1", "Work/Clients"), label(1, "L2", "Work")])];
        let entries = entries(&accounts, &HashMap::new());
        assert_eq!(entries[0], Entry::Heading(Section::Favorites));
        let first = entries.iter().find_map(|e| match e {
            Entry::Mailbox(row) => Some(row),
            _ => None,
        });
        assert_eq!(first.map(|r| r.mailbox.clone()), Some(Mailbox::Unified(Standard::Inbox)));
        let account_at = entries.iter().position(|e| *e == Entry::Account(1)).unwrap();
        let rows: Vec<&Row> = entries[account_at..]
            .iter()
            .filter_map(|e| match e {
                Entry::Mailbox(row) => Some(row),
                _ => None,
            })
            .collect();
        assert_eq!(rows[0].mailbox, Mailbox::Standard { account_id: 1, which: Standard::Inbox });
        let work = rows.iter().position(|r| r.title == "Work").unwrap();
        assert_eq!(rows[work + 1].title, "Clients");
        assert_eq!(rows[work + 1].depth, rows[work].depth + 1);
    }

    #[test]
    fn flag_colors_wait_until_they_are_used() {
        let entries = entries(&[], &HashMap::new());
        let flags: Vec<&Row> = entries
            .iter()
            .filter_map(|e| match e {
                Entry::Mailbox(row) if matches!(row.mailbox, Mailbox::Flag(_)) => Some(row),
                _ => None,
            })
            .collect();
        assert_eq!(flags.len(), FlagColor::ALL.len());
        assert!(flags.iter().all(|r| r.hidden_until_used));
        assert!(!entries.contains(&Entry::Heading(Section::Accounts)));
    }
}
