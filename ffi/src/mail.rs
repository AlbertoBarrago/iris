//! The mail as the window shows it: the sidebar with its counts, and a
//! mailbox's conversations, through `mailrs_sync::Mailboxes`, the same
//! model the GTK window and the assistant read mail through.
//!
//! This front end does not sync yet, so the accounts are offline to it:
//! a mailbox that only Gmail can list, such as Archive, comes back empty.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use mailrs_domain::{AccountId, Category, FlagColor, ThreadSummary};
use mailrs_render::format;
use mailrs_store::{Db, accounts, labels};
use mailrs_sync::AccountSync;
use mailrs_sync::mailbox::{Mailboxes, Scope, View};
use mailrs_sync::{Accounts, Mailbox};
use mailrs_view::sidebar::{self, Entry, Icon};

use crate::CoreError;

/// No account syncs in this front end yet.
struct Offline;

impl Accounts for Offline {
    fn account(&self, _: AccountId) -> Option<Arc<AccountSync>> {
        None
    }
}

/// One line of the sidebar.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct SidebarItem {
    /// `heading`, `account` or `mailbox`.
    pub kind: String,
    /// What selecting it asks `Mail::list` for; empty for a heading.
    pub key: String,
    pub title: String,
    /// What the icon stands for: `inbox`, `flag`, `sent`, `drafts`,
    /// `muted`, `outbox`, `send-later`, `remind-me`, `follow-up`,
    /// `archive`, `junk`, `trash`, `all-mail`, `label`, `group` or `tag`.
    pub icon: String,
    /// A flag's or a label's color, `#rrggbb`.
    pub color: Option<String>,
    pub depth: u32,
    /// Unread conversations, or for Drafts and the Outbox how many there are.
    pub count: i64,
    /// The account's own color, for an account's line.
    pub account_color: Option<String>,
}

/// One category tab over an inbox.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct CategoryTab {
    pub key: String,
    pub name: String,
    pub unread: i64,
}

/// One conversation in the list, ready to draw.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct ListRow {
    pub account_id: i64,
    pub thread_id: String,
    pub sender: String,
    pub sender_email: String,
    pub subject: String,
    pub snippet: String,
    /// The date the way the GTK list shows it: the time today, then
    /// "Yesterday", the weekday, the day and month.
    pub date: String,
    pub unread: bool,
    pub flag: Option<String>,
    pub message_count: i64,
    pub has_attachments: bool,
    pub muted: bool,
    pub invitation: bool,
    pub initials: String,
    pub avatar_color: String,
    pub account_color: String,
}

/// What the list shows for one mailbox.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct MailboxListing {
    pub title: String,
    pub subtitle: String,
    pub rows: Vec<ListRow>,
    pub categories: Vec<CategoryTab>,
    /// What an empty list says.
    pub empty: String,
}

/// The mail store, opened as the app opens it, with the mailboxes over it.
#[derive(uniffi::Object)]
pub struct Mail {
    runtime: tokio::runtime::Runtime,
    db: Db,
    mailboxes: Mailboxes<Offline>,
    /// The mailbox behind each key the last sidebar handed out.
    keys: Mutex<HashMap<String, Mailbox>>,
}

#[uniffi::export]
impl Mail {
    #[uniffi::constructor]
    pub fn open(path: String) -> Result<Mail, CoreError> {
        let db = Db::open(&PathBuf::from(path))?;
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .map_err(|err| CoreError::Store(err.to_string()))?;
        Ok(Mail {
            mailboxes: Mailboxes::new(Arc::new(Offline), db.clone()),
            runtime,
            db,
            keys: Mutex::new(HashMap::new()),
        })
    }

    /// The sidebar, with each row's count.
    pub fn sidebar(&self) -> Result<Vec<SidebarItem>, CoreError> {
        let accounts_and_labels = self.runtime.block_on(self.db.read(|c| {
            let mut out = Vec::new();
            for account in accounts::list_accounts(c)? {
                let labels = labels::list_labels(c, account.id)?;
                out.push((account, labels));
            }
            Ok(out)
        }))?;
        let entries = sidebar::entries(&accounts_and_labels, &HashMap::new());
        let counted = sidebar::counted(&entries);
        let view = view(None);
        let counts = self
            .runtime
            .block_on(self.mailboxes.counts(&counted, &Mailbox::Unified(mailrs_sync::mailbox::Standard::Inbox), &view))
            .map_err(|err| CoreError::Store(err.to_string()))?;
        let emails: HashMap<AccountId, String> =
            accounts_and_labels.iter().map(|(a, _)| (a.id, a.email.clone())).collect();
        let mut keys = HashMap::new();
        let mut items = Vec::new();
        for entry in entries {
            match entry {
                Entry::Heading(section) => items.push(SidebarItem {
                    kind: "heading".into(),
                    key: String::new(),
                    title: section.title(),
                    icon: String::new(),
                    color: None,
                    depth: 0,
                    count: 0,
                    account_color: None,
                }),
                Entry::Account(id) => items.push(SidebarItem {
                    kind: "account".into(),
                    key: String::new(),
                    title: emails.get(&id).cloned().unwrap_or_default(),
                    icon: String::new(),
                    color: None,
                    depth: 0,
                    count: 0,
                    account_color: Some(format::PALETTE[format::account_color_index(id)].to_string()),
                }),
                Entry::Mailbox(row) => {
                    let count = counts.mailboxes.get(&row.mailbox).copied().unwrap_or(0);
                    if row.hidden_until_used && count == 0 {
                        continue;
                    }
                    let key = format!("{:?}", row.mailbox);
                    keys.insert(key.clone(), row.mailbox.clone());
                    let (icon, flag) = icon_name(row.icon);
                    items.push(SidebarItem {
                        kind: "mailbox".into(),
                        key,
                        title: row.title,
                        icon: icon.into(),
                        color: row.color.or(flag),
                        depth: row.depth,
                        count,
                        account_color: None,
                    });
                }
            }
        }
        *self.keys.lock().unwrap_or_else(|p| p.into_inner()) = keys;
        Ok(items)
    }

    /// The conversations of the mailbox `key` names, narrowed to the
    /// category `category` names when it is an inbox's.
    pub fn list(&self, key: String, category: Option<String>) -> Result<MailboxListing, CoreError> {
        let mailbox = self
            .keys
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .get(&key)
            .cloned()
            .ok_or_else(|| CoreError::Store("That mailbox is no longer in the sidebar.".into()))?;
        let category = category
            .as_deref()
            .and_then(Category::from_key)
            .filter(|_| mailbox.takes_categories());
        let accounts = self.runtime.block_on(self.db.read(accounts::list_accounts))?;
        let view = view(category);
        let listing = self
            .runtime
            .block_on(self.mailboxes.list(&mailbox, &Scope::over(accounts), &view, mailrs_sync::mailbox::Loaded::nothing()))
            .map_err(|err| CoreError::Store(err.to_string()))?;
        let categories = match mailbox.takes_categories() {
            true => {
                // The tabs are counted over the inbox as a whole, which a
                // category in the view asks for.
                let whole = self::view(Some(Category::All));
                let counts = self
                    .runtime
                    .block_on(self.mailboxes.counts(&[], &mailbox, &whole))
                    .map_err(|err| CoreError::Store(err.to_string()))?;
                Category::ALL
                    .into_iter()
                    .map(|c| CategoryTab {
                        key: c.key().into(),
                        name: c.name(),
                        unread: counts.categories.get(&c).copied().unwrap_or(0),
                    })
                    .collect()
            }
            false => Vec::new(),
        };
        let now = chrono::Local::now();
        Ok(MailboxListing {
            title: listing.title,
            subtitle: listing.subtitle,
            rows: listing.rows.iter().map(|t| row(t, now)).collect(),
            categories,
            empty: listing.empty.title,
        })
    }
}

fn view(category: Option<Category>) -> View {
    View {
        category,
        now: mailrs_sync::now_millis(),
        ..View::default()
    }
}

fn row(thread: &ThreadSummary, now: chrono::DateTime<chrono::Local>) -> ListRow {
    let sender = if thread.from.trim().is_empty() {
        thread.from_email.clone()
    } else {
        thread.from.clone()
    };
    ListRow {
        account_id: thread.account_id,
        thread_id: thread.id.clone(),
        initials: format::initials(&sender),
        avatar_color: format::PALETTE[format::avatar_hue(&sender, &thread.from_email)].to_string(),
        account_color: format::PALETTE[format::account_color_index(thread.account_id)].to_string(),
        sender,
        sender_email: thread.from_email.clone(),
        subject: thread.subject.clone(),
        snippet: thread.snippet.clone(),
        date: format::relative_date(thread.last_message_at, now),
        unread: thread.unread,
        flag: thread
            .starred
            .then(|| flag_hex(thread.flag_color.unwrap_or(FlagColor::Red)).to_string()),
        message_count: thread.message_count,
        has_attachments: thread.has_attachments,
        muted: thread.muted,
        invitation: thread.invitation,
    }
}

/// The name Swift picks an SF Symbol by, and a flag's color.
fn icon_name(icon: Icon) -> (&'static str, Option<String>) {
    let name = match icon {
        Icon::Inbox => "inbox",
        Icon::Flag(color) => return ("flag", Some(flag_hex(color).to_string())),
        Icon::Sent => "sent",
        Icon::Drafts => "drafts",
        Icon::Muted => "muted",
        Icon::Outbox => "outbox",
        Icon::SendLater => "send-later",
        Icon::RemindMe => "remind-me",
        Icon::FollowUp => "follow-up",
        Icon::Archive => "archive",
        Icon::Junk => "junk",
        Icon::Trash => "trash",
        Icon::AllMail => "all-mail",
        Icon::Label => "label",
        Icon::Group => "group",
        Icon::Tag => "tag",
    };
    (name, None)
}

/// The colors Apple Mail gives its flags.
fn flag_hex(color: FlagColor) -> &'static str {
    match color {
        FlagColor::Red => "#ff3b30",
        FlagColor::Orange => "#ff9500",
        FlagColor::Yellow => "#ffcc00",
        FlagColor::Green => "#34c759",
        FlagColor::Blue => "#007aff",
        FlagColor::Purple => "#af52de",
        FlagColor::Gray => "#8e8e93",
    }
}

/// Binds the interface's words to the catalogs in `locale_dir`, in
/// `language` (such as `it_IT`), or in the system's when it is empty.
#[uniffi::export]
pub fn bind_language(locale_dir: String, language: String) {
    // Called once, first thing, before any other thread reads the
    // environment or the locale.
    unsafe {
        if !language.is_empty() {
            std::env::set_var("LANGUAGE", &language);
            std::env::set_var("LANG", format!("{language}.UTF-8"));
        }
        gettextrs::setlocale(gettextrs::LocaleCategory::LcAll, "");
    }
    if gettextrs::bindtextdomain("iris", locale_dir).is_err() {
        return;
    }
    let _ = gettextrs::bind_textdomain_codeset("iris", "UTF-8");
    let _ = gettextrs::textdomain("iris");
}
