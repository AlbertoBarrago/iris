//! The mail as the window shows it: the sidebar with its counts, and a
//! mailbox's conversations, through `mailrs_sync::Mailboxes`, the same
//! model the GTK window and the assistant read mail through.
//!
//! `Mail::start_sync` runs the sync engine the GTK app runs, with the
//! same lock on the store, so only one of the two syncs at a time.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use mailrs_domain::{AccountId, Category, FlagColor, ThreadSummary};
use mailrs_render::format;
use mailrs_store::{Db, accounts, labels};
use mailrs_domain::{Account, AccountState, ChangeEvent};
use mailrs_sync::config::{Config, config_path};
use mailrs_sync::lock::{LockError, SyncLock};
use mailrs_sync::mailbox::{Mailboxes, Scope, View};
use mailrs_sync::passwords::Secrets;
use mailrs_sync::starting::{self, Connected, Starting};
use mailrs_sync::{AccountServices, Clients, Connector, SyncEngine};
use mailrs_domain::Target;
use mailrs_sync::OneClick;
use mailrs_sync::{Accounts, History, MailAction, MailActions, Mailbox, MovedFrom, TriageAction};
use mailrs_view::sidebar::{self, Entry, Icon};

use crate::CoreError;

pub(crate) use mailrs_appcore::engine::RunningEngine as Running;

/// Who hears the engine's news: the window, which reads the sidebar and
/// the list again.
#[uniffi::export(with_foreign)]
pub trait MailListener: Send + Sync {
    /// Something on screen changed: `what` is `accounts`, `labels`,
    /// `threads` or `mail`.
    fn changed(&self, what: String);
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
    /// The account a line or a mailbox belongs to; `None` for the unified
    /// ones and the headings.
    pub account_id: Option<i64>,
    /// An account's state, for its line: `ok`, `bootstrapping`,
    /// `needs_reauth`, `backing_off`, `offline`, and so on.
    pub state: String,
}

/// A conversation an action is taken on.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct ThreadRef {
    pub account_id: i64,
    pub thread_id: String,
}

/// What the person asked to do with the conversations picked.
#[derive(Debug, Clone, PartialEq, uniffi::Enum)]
pub enum Action {
    Archive,
    Trash,
    Untrash,
    Junk,
    NotJunk,
    MarkRead,
    MarkUnread,
    /// A flag of this color, `red`, `orange`, `yellow`, `green`, `blue`,
    /// `purple` or `gray`; `None` takes the flag off.
    Flag { color: Option<String> },
    Mute,
    Unmute,
}

/// What came of an action, for the toast that offers Undo.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct ActionDone {
    /// What the toast says, such as "Archived"; empty for none.
    pub words: String,
    pub done: u32,
    /// The first failure's words, when something failed.
    pub failed: Option<String>,
}

/// A file of a message: an attachment, or a picture its body shows.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct FilePart {
    pub filename: String,
    pub mime_type: String,
    pub data: Vec<u8>,
}

/// An attachment as the page lists it, for saving them all.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct AttachmentInfo {
    /// Its place among the message's attachments, as the page's links
    /// name it.
    pub index: u32,
    pub filename: String,
    pub size: i64,
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

/// A search offered while one is typed: what the list shows, and the
/// whole search to run when it is picked.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct SearchSuggestion {
    pub label: String,
    pub query: String,
}

/// The mail store, opened as the app opens it, with the mailboxes over it.
#[derive(uniffi::Object)]
pub struct Mail {
    pub(crate) runtime: tokio::runtime::Runtime,
    pub(crate) db: Db,
    /// The folder the store sits in, which the sync lock goes in too.
    dir: PathBuf,
    pub(crate) running: Arc<Running>,
    /// Held while this front end syncs, so the GTK app cannot.
    lock: Mutex<Option<SyncLock>>,
    pub(crate) mailboxes: Arc<Mailboxes<Running>>,
    pub(crate) actions: Arc<MailActions<Running>>,
    /// The mailbox behind each key the last sidebar handed out.
    keys: Mutex<HashMap<String, Mailbox>>,
    /// Everyone a recipient field can offer, read once.
    pub(crate) people: Mutex<Option<Arc<Vec<mailrs_store::contacts::Suggestion>>>>,
    /// Who hears the engine, kept to start it again after a change.
    listener: Mutex<Option<Arc<dyn MailListener>>>,
}

#[uniffi::export]
impl Mail {
    #[uniffi::constructor]
    pub fn open(path: String) -> Result<Mail, CoreError> {
        let path = PathBuf::from(path);
        let db = Db::open(&path)?;
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .thread_name("iris-sync")
            .thread_stack_size(mailrs_sync::WORKER_STACK)
            .enable_all()
            .build()
            .map_err(|err| CoreError::Store(err.to_string()))?;
        let running = Arc::new(Running::default());
        Ok(Mail {
            mailboxes: Arc::new(Mailboxes::new(Arc::clone(&running), db.clone())),
            actions: Arc::new(MailActions::new(Arc::clone(&running), db.clone(), OneClick::Web)),
            dir: path.parent().map(PathBuf::from).unwrap_or_default(),
            running,
            lock: Mutex::new(None),
            runtime,
            db,
            keys: Mutex::new(HashMap::new()),
            people: Mutex::new(None),
            listener: Mutex::new(None),
        })
    }

    /// Starts syncing every account, as the GTK app does when it opens,
    /// and tells `listener` what changes. Fails while another copy of Iris
    /// syncs this store.
    pub fn start_sync(&self, listener: Arc<dyn MailListener>) -> Result<(), CoreError> {
        let mut held = self.lock.lock().unwrap_or_else(|p| p.into_inner());
        if held.is_some() {
            return Ok(());
        }
        let lock = SyncLock::take(&self.dir).map_err(|err| match err {
            LockError::Held => CoreError::Store(
                "Another copy of Iris is syncing your mail. Quit it, then try again.".into(),
            ),
            other => CoreError::Store(other.to_string()),
        })?;
        *self.listener.lock().unwrap_or_else(|p| p.into_inner()) = Some(Arc::clone(&listener));
        let config = match config_path().map(|path| Config::load(&path)) {
            Ok(Ok(config)) => config,
            Ok(Err(err)) if err.is_missing() => Config::default(),
            Ok(Err(err)) => return Err(CoreError::Store(err.to_string())),
            Err(err) => return Err(CoreError::Store(err.to_string())),
        };
        let _entered = self.runtime.enter();
        let (engine, events) = SyncEngine::new(self.db.clone(), config.engine_config());
        let engine = Arc::new(engine);
        engine.set_network(true);
        engine.set_window_open(true);
        self.running.replace(Some(Arc::clone(&engine)));
        let heard = Arc::clone(&listener);
        self.runtime.spawn(async move {
            while let Ok(event) = events.recv().await {
                heard.changed(what_changed(&event).into());
            }
        });
        let connector = Arc::new(Connector {
            db: self.db.clone(),
            config,
            clients: Clients::built_in(),
            secrets: Secrets::keyring(),
        });
        let port = Arc::new(Port {
            engine,
            db: self.db.clone(),
            listener,
        });
        let db = self.db.clone();
        self.runtime.spawn(async move {
            let Ok(all) = db.read(accounts::list_accounts).await else { return };
            let connect = move |account: Account| {
                let connector = Arc::clone(&connector);
                async move {
                    Ok(match connector.connect(&account).await? {
                        mailrs_sync::Connected::Ready(services) => Connected::Ready(services),
                        mailrs_sync::Connected::NeedsSignIn(_) => Connected::NeedsSignIn,
                    })
                }
            };
            starting::start_each(all, connect, port, starting::Waits::APP).await;
        });
        *held = Some(lock);
        Ok(())
    }

    /// Runs `action` on `threads`, taken from the mailbox `from` names, and
    /// puts it on the undo stack.
    pub fn act(&self, threads: Vec<ThreadRef>, action: Action, from: Option<String>) -> ActionDone {
        let action = mail_action(action);
        let moved = from
            .and_then(|key| self.mailbox(&key))
            .map(|m| m.moved_from())
            .unwrap_or_else(MovedFrom::nowhere);
        let targets: Vec<Target> = threads.into_iter().map(target).collect();
        let (actions, given) = (Arc::clone(&self.actions), action.clone());
        let outcome = self.run(async move { actions.run_from(&targets, given, History::Record, &moved).await });
        self.mailboxes.forget_remote();
        ActionDone {
            words: mailrs_view::done::done_message(&action, outcome.done.len(), true).unwrap_or_default(),
            done: outcome.done.len() as u32,
            failed: outcome.failed.first().map(|f| f.error.clone()),
        }
    }

    /// Marks a conversation the person opened as read, with no place on
    /// the undo stack, as opening it in the GTK app does.
    pub fn mark_read(&self, thread: ThreadRef) {
        let targets = vec![target(thread)];
        let action = MailAction::Triage(TriageAction::MarkRead);
        let actions = Arc::clone(&self.actions);
        self.run(async move {
            actions.run_from(&targets, action, History::Skip, &MovedFrom::nowhere()).await
        });
    }

    /// Reverses the newest action on the undo stack; `None` when there is
    /// none.
    pub fn undo(&self) -> Option<ActionDone> {
        let actions = Arc::clone(&self.actions);
        let undone = self.run(async move { actions.undo().await })?;
        self.mailboxes.forget_remote();
        Some(ActionDone {
            words: undone.words,
            done: undone.outcome.done.len() as u32,
            failed: undone.outcome.failed.first().map(|f| f.error.clone()),
        })
    }

    /// Fetches the bodies of `thread`'s messages that are not in the store
    /// yet, from the cache or the server, as opening it in the GTK app
    /// does. They land in the store, where the next page reads them.
    /// Returns how many came.
    pub fn fetch_bodies(&self, thread: ThreadRef) -> Result<u32, CoreError> {
        let Some(sync) = self.running.account(thread.account_id) else {
            tracing::warn!(account = thread.account_id, "no sync running for the account; bodies stay missing");
            return Ok(0);
        };
        let (db, account_id, thread_id) = (self.db.clone(), thread.account_id, thread.thread_id);
        self.run(async move {
            let missing: Vec<String> = db
                .read(move |c| {
                    let mut missing = Vec::new();
                    for meta in mailrs_store::messages::thread_messages(c, account_id, &thread_id)? {
                        if mailrs_store::bodies::peek_body(c, account_id, &meta.id)?.is_none() {
                            missing.push(meta.id);
                        }
                    }
                    Ok(missing)
                })
                .await?;
            tracing::info!(account = account_id, missing = missing.len(), "fetching bodies");
            let mut came = 0;
            for id in missing {
                match sync.body(&id).await {
                    Ok(_) => came += 1,
                    Err(err) => tracing::warn!(error = %err, "could not fetch a message body"),
                }
            }
            Ok(came)
        })
        .map_err(|err: mailrs_store::StoreError| CoreError::Store(err.to_string()))
    }

    /// The attachment at `index` of a message, fetched from the cache or
    /// the server.
    pub fn attachment(&self, account_id: i64, message_id: String, index: u32) -> Result<FilePart, CoreError> {
        let body = self.stored_body(account_id, &message_id)?;
        let part = body
            .attachments
            .get(index as usize)
            .cloned()
            .ok_or_else(|| CoreError::Store("That attachment is not in the message.".into()))?;
        let data = self.fetch_part(account_id, message_id, handle(&part))?;
        Ok(FilePart {
            filename: part.filename,
            mime_type: part.mime_type,
            data,
        })
    }

    /// The picture a message's body names by `cid`, or `None` when the
    /// message has no such picture.
    pub fn picture(&self, account_id: i64, message_id: String, cid: String) -> Result<Option<FilePart>, CoreError> {
        let body = self.stored_body(account_id, &message_id)?;
        let Some(part) = body
            .attachments
            .into_iter()
            .find(|a| a.content_id.as_deref() == Some(cid.as_str()))
        else {
            return Ok(None);
        };
        let data = self.fetch_part(account_id, message_id, handle(&part))?;
        Ok(Some(FilePart {
            filename: part.filename,
            mime_type: part.mime_type,
            data,
        }))
    }

    /// The attachments the page lists for a message: every file but the
    /// pictures its body shows.
    pub fn attachments(&self, account_id: i64, message_id: String) -> Result<Vec<AttachmentInfo>, CoreError> {
        let body = self.stored_body(account_id, &message_id)?;
        Ok(body
            .attachments
            .iter()
            .enumerate()
            .filter(|(_, a)| !mailrs_render::conversation::shown_in_body(a, &body))
            .map(|(index, a)| AttachmentInfo {
                index: index as u32,
                filename: a.filename.clone(),
                size: a.size,
            })
            .collect())
    }

    /// Asks one account to look for new mail now.
    pub fn check_account(&self, account_id: i64) {
        if let Some(engine) = self.running.current() {
            engine.poke(account_id);
        }
    }

    /// Asks every account to look for new mail now.
    pub fn check_now(&self) {
        if let Some(engine) = self.running.current() {
            engine.poke_all();
        }
    }

    /// The sidebar, with each row's count.
    pub fn sidebar(&self) -> Result<Vec<SidebarItem>, CoreError> {
        let db = self.db.clone();
        let accounts_and_labels = self.run(async move { db.read(|c| {
            let mut out = Vec::new();
            for account in accounts::list_accounts(c)? {
                let labels = labels::list_labels(c, account.id)?;
                out.push((account, labels));
            }
            Ok(out)
        }).await })?;
        let entries = sidebar::entries(&accounts_and_labels, &HashMap::new());
        let counted = sidebar::counted(&entries);
        let view = view(None);
        let mailboxes = Arc::clone(&self.mailboxes);
        let counts = self
            .run(async move {
                mailboxes
                    .counts(&counted, &Mailbox::Unified(mailrs_sync::mailbox::Standard::Inbox), &view)
                    .await
            })
            .map_err(|err| CoreError::Store(err.to_string()))?;
        let emails: HashMap<AccountId, (String, AccountState)> = accounts_and_labels
            .iter()
            .map(|(a, _)| (a.id, (a.email.clone(), a.state)))
            .collect();
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
                    account_id: None,
                    state: String::new(),
                }),
                Entry::Account(id) => items.push(SidebarItem {
                    kind: "account".into(),
                    key: format!("account-{id}"),
                    title: emails.get(&id).map(|(email, _)| email.clone()).unwrap_or_default(),
                    icon: String::new(),
                    color: None,
                    depth: 0,
                    count: 0,
                    account_color: Some(format::PALETTE[format::account_color_index(id)].to_string()),
                    account_id: Some(id),
                    state: emails.get(&id).map(|(_, state)| state.as_str().to_string()).unwrap_or_default(),
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
                        account_id: row.mailbox.account(),
                        title: row.title,
                        icon: icon.into(),
                        color: row.color.or(flag),
                        depth: row.depth,
                        count,
                        account_color: None,
                        state: String::new(),
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
        // The sidebar may be in the middle of being read again, as it is
        // when the engine's first news comes in right at start; reading it
        // once more settles which mailbox the key names.
        let mailbox = match self.mailbox(&key) {
            Some(mailbox) => mailbox,
            None => {
                self.sidebar()?;
                self.mailbox(&key)
                    .ok_or_else(|| CoreError::Store("That mailbox is no longer in the sidebar.".into()))?
            }
        };
        self.listing(mailbox, category)
    }

    /// The conversations that `query` finds, in Gmail's search language,
    /// in the account of the mailbox `scope_key` names, or in every
    /// account when that mailbox spans them. With `servers` the servers
    /// answer; without, the mail kept on this Mac does, at once and
    /// whether or not the accounts are connected. The GTK app shows the
    /// second while it waits for the first.
    pub fn search(&self, query: String, scope_key: Option<String>, servers: bool) -> Result<MailboxListing, CoreError> {
        let account_id = scope_key.as_deref().and_then(|key| self.mailbox(key)).and_then(|m| m.account());
        let mailbox = Mailbox::Search {
            query: query.trim().to_string(),
            account_id,
        };
        if servers {
            let found = self.listing(mailbox, None);
            match &found {
                Ok(listing) => tracing::info!(rows = listing.rows.len(), ?account_id, "searched the servers"),
                Err(err) => tracing::warn!(error = %err, "could not search the servers"),
            }
            return found;
        }
        let settings = mailrs_appcore::settings::Settings::load(&mailrs_appcore::settings::Settings::default_path());
        let view = View {
            threading: settings.threading,
            ..view(None)
        };
        let db = self.db.clone();
        let accounts = self.run(async move { db.read(accounts::list_accounts).await })?;
        let mailboxes = Arc::clone(&self.mailboxes);
        let query = query.trim().to_string();
        let listing = self
            .run(async move { mailboxes.stored_search(&query, account_id, &Scope::over(accounts), &view).await })
            .map_err(|err| {
                tracing::warn!(error = %err, "could not search the stored mail");
                CoreError::Store(err.to_string())
            })?;
        tracing::info!(rows = listing.rows.len(), ?account_id, "searched the stored mail");
        let now = chrono::Local::now();
        Ok(MailboxListing {
            title: listing.title,
            subtitle: listing.subtitle,
            rows: listing.rows.iter().map(|t| row(t, now)).collect(),
            categories: Vec::new(),
            empty: listing.empty.title,
        })
    }

    /// Searches to offer while `text` is typed: the subject, people it
    /// could be from or to, and labels it could name.
    pub fn search_suggestions(&self, text: String) -> Vec<SearchSuggestion> {
        let labels: Vec<String> = self
            .keys
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .values()
            .filter_map(|mailbox| match mailbox {
                Mailbox::Label { name, .. } => Some(name.clone()),
                _ => None,
            })
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect();
        mailrs_appcore::search::suggestions(&text, &self.known_people(), &labels)
            .into_iter()
            .map(|s| SearchSuggestion { label: s.label, query: s.query })
            .collect()
    }
}

impl Mail {
    /// The list `mailbox` shows, as Preferences say it reads.
    fn listing(&self, mailbox: Mailbox, category: Option<String>) -> Result<MailboxListing, CoreError> {
        // Preferences decide how the list reads: conversations or single
        // messages, an inbox in categories or whole, follow-ups or not.
        let settings = mailrs_appcore::settings::Settings::load(&mailrs_appcore::settings::Settings::default_path());
        let categories_on = settings.inbox_categories && mailbox.takes_categories();
        let category = category
            .as_deref()
            .and_then(Category::from_key)
            .filter(|_| categories_on);
        let db = self.db.clone();
        let accounts = self.run(async move { db.read(accounts::list_accounts).await })?;
        let view = View {
            threading: settings.threading,
            follow_ups: settings.suggest_follow_ups,
            ..view(category)
        };
        let (mailboxes, shown, asked) = (Arc::clone(&self.mailboxes), mailbox.clone(), view.clone());
        let listing = self
            .run(async move {
                mailboxes
                    .list(&shown, &Scope::over(accounts), &asked, mailrs_sync::mailbox::Loaded::nothing())
                    .await
            })
            .map_err(|err| CoreError::Store(err.to_string()))?;
        let categories = match categories_on {
            true => {
                // The tabs are counted over the inbox as a whole, which a
                // category in the view asks for.
                let whole = self::view(Some(Category::All));
                let (mailboxes, shown) = (Arc::clone(&self.mailboxes), mailbox.clone());
                let counts = self
                    .run(async move { mailboxes.counts(&[], &shown, &whole).await })
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

impl Mail {
    /// Stops the engine and starts it again, so it reads `config.toml`
    /// anew. Does nothing while this copy does not sync.
    pub(crate) fn restart_sync(&self) -> Result<(), CoreError> {
        let Some(listener) = self.listener.lock().unwrap_or_else(|p| p.into_inner()).clone() else {
            return Ok(());
        };
        if self.lock.lock().unwrap_or_else(|p| p.into_inner()).is_none() {
            return Ok(());
        }
        if let Some(engine) = self.running.replace(None) {
            engine.shutdown();
        }
        *self.lock.lock().unwrap_or_else(|p| p.into_inner()) = None;
        self.start_sync(listener)
    }

    /// The body the store holds for a message, which names its files.
    pub(crate) fn stored_body(&self, account_id: i64, message_id: &str) -> Result<mailrs_domain::MessageBody, CoreError> {
        let (db, id) = (self.db.clone(), message_id.to_string());
        self.run(async move { db.read(move |c| mailrs_store::bodies::peek_body(c, account_id, &id)).await })?
            .ok_or_else(|| CoreError::Store("The message has not been fetched yet.".into()))
    }

    /// One part of a message, through the account's sync.
    pub(crate) fn fetch_part(&self, account_id: i64, message_id: String, part_path: String) -> Result<Vec<u8>, CoreError> {
        let sync = self
            .running
            .account(account_id)
            .ok_or_else(|| CoreError::Store("The account is not connected.".into()))?;
        self.run(async move { sync.attachment(&message_id, &part_path).await })
            .map_err(|err| CoreError::Store(err.to_string()))
    }

    /// Runs `work` on the runtime's own threads and waits for it. The sync
    /// code needs the large stacks those threads get
    /// (`mailrs_sync::WORKER_STACK`): run on the Swift thread that called,
    /// an action overflowed its stack and took the app down.
    pub(crate) fn run<T: Send + 'static>(&self, work: impl std::future::Future<Output = T> + Send + 'static) -> T {
        self.runtime
            .block_on(self.runtime.spawn(work))
            .unwrap_or_else(|err| std::panic::resume_unwind(err.into_panic()))
    }

    /// The mailbox behind a key the last sidebar handed out.
    fn mailbox(&self, key: &str) -> Option<Mailbox> {
        self.keys.lock().unwrap_or_else(|p| p.into_inner()).get(key).cloned()
    }
}

/// Where starting an account lands: the engine, the store, and the window.
struct Port {
    engine: Arc<SyncEngine>,
    db: Db,
    listener: Arc<dyn MailListener>,
}

impl Starting<AccountServices> for Port {
    fn start(&self, account: AccountId, services: AccountServices) {
        self.engine.start_account(account, services);
    }

    async fn report(&self, account_id: AccountId, state: AccountState) {
        let marked = self.db.write(move |c| accounts::set_state(c, account_id, state)).await;
        if let Err(err) = marked {
            tracing::warn!(account = account_id, %err, "could not record the account's state");
        }
        self.listener.changed("accounts".into());
    }

    async fn wanted(&self, account_id: AccountId) -> bool {
        matches!(self.db.read(move |c| accounts::account(c, account_id)).await, Ok(Some(_)))
    }
}

/// The word the window hears for an engine event.
fn what_changed(event: &ChangeEvent) -> &'static str {
    match event {
        ChangeEvent::AccountStateChanged { .. } => "accounts",
        ChangeEvent::LabelsChanged { .. } => "labels",
        ChangeEvent::NewMail { .. } => "mail",
        _ => "threads",
    }
}

/// What the server fetches an attachment by: its own handle where it gives
/// one, as the GTK app asks, else the part's path.
fn handle(part: &mailrs_domain::Attachment) -> String {
    part.attachment_id.clone().unwrap_or_else(|| part.part_id.clone())
}

fn target(thread: ThreadRef) -> Target {
    Target {
        account_id: thread.account_id,
        thread_id: thread.thread_id,
        message_id: None,
    }
}

fn mail_action(action: Action) -> MailAction {
    let triage = |t| MailAction::Triage(t);
    match action {
        Action::Archive => triage(TriageAction::Archive),
        Action::Trash => triage(TriageAction::Trash),
        Action::Untrash => triage(TriageAction::Untrash),
        Action::Junk => triage(TriageAction::Junk),
        Action::NotJunk => triage(TriageAction::NotJunk),
        Action::MarkRead => triage(TriageAction::MarkRead),
        Action::MarkUnread => triage(TriageAction::MarkUnread),
        Action::Flag { color } => MailAction::Flag(color.map(|name| {
            FlagColor::ALL
                .into_iter()
                .find(|c| c.as_str() == name)
                .unwrap_or(FlagColor::Red)
        })),
        Action::Mute => MailAction::Mute { muted: true },
        Action::Unmute => MailAction::Mute { muted: false },
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

/// Sends the core's log lines to `~/Library/Logs/Iris/iris-next.log`,
/// where Console.app finds them: an app opened from the Dock has nowhere
/// to send standard error. Called once, at start.
#[uniffi::export]
pub fn start_logging() {
    let Some(home) = std::env::var_os("HOME") else { return };
    let folder = PathBuf::from(home).join("Library/Logs/Iris");
    if std::fs::create_dir_all(&folder).is_err() {
        return;
    }
    let path = folder.join("iris-next.log");
    if std::fs::metadata(&path).is_ok_and(|meta| meta.len() > 5 * 1024 * 1024) {
        let _ = std::fs::rename(&path, folder.join("iris-next.log.1"));
    }
    let Ok(file) = std::fs::OpenOptions::new().create(true).append(true).open(&path) else {
        return;
    };
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));
    let _ = tracing_subscriber::fmt()
        .with_ansi(false)
        .with_env_filter(filter)
        .with_writer(Mutex::new(file))
        .try_init();
}

/// `text` in the interface's language, from the same catalogs the GTK app
/// reads. The SwiftUI views word their buttons through this, so a word
/// already translated for the GTK app needs no second translation.
#[uniffi::export]
pub fn translate(text: String) -> String {
    gettextrs::gettext(text)
}

/// `one` or `many` in the interface's language, as `count` asks.
#[uniffi::export]
pub fn translate_plural(one: String, many: String, count: u32) -> String {
    gettextrs::ngettext(one, many, count)
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
