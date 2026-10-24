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

use mailrs_domain::translate::{fill, gettext};

use crate::CoreError;
use crate::account_menu;
use mailrs_appcore::settings::Settings;

pub(crate) use mailrs_appcore::engine::RunningEngine as Running;

/// The sample accounts' in-memory servers, in a demo run.
#[cfg(any(feature = "demo", test))]
use crate::demo::DemoMail;

/// A build without the `demo` feature never holds a demo, so its stand-in
/// has no values.
#[cfg(not(any(feature = "demo", test)))]
pub(crate) enum DemoMail {}

#[cfg(not(any(feature = "demo", test)))]
impl DemoMail {
    fn services(&self, _account_id: AccountId) -> Option<AccountServices> {
        match *self {}
    }
}

/// Who hears the engine's news: the window, which reads the sidebar and
/// the list again.
#[uniffi::export(with_foreign)]
pub trait MailListener: Send + Sync {
    /// Something on screen changed: `what` is `accounts`, `labels`,
    /// `threads` or `mail`.
    fn changed(&self, what: String);
    /// New mail worth a notification arrived, worded as Preferences asks.
    fn new_mail(&self, notices: Vec<crate::notify::MailNotice>);
    /// An account started (`busy`) or finished a check for new mail the
    /// window shows: its first after starting, or Check for Mail.
    fn checking(&self, account_id: i64, busy: bool);
}

/// One line of the sidebar.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct SidebarItem {
    /// `heading`, `account` or `mailbox`.
    pub kind: String,
    /// The smart mailbox a row stands for, for its Edit and Delete.
    pub smart_id: Option<String>,
    /// What selecting it asks `Mail::list` for; empty for a heading.
    pub key: String,
    pub title: String,
    /// What the icon stands for: `inbox`, `unread`, `flag`, `sent`, `drafts`,
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
    /// Archive now and bring back, unread, at `at` (milliseconds).
    Remind { at: i64 },
    /// Drop the reminder and put the conversation back in the inbox now.
    CancelReminder,
    DismissFollowUp,
}

/// When something happens later, with the words a menu shows for it.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct TimeChoice {
    pub label: String,
    /// Milliseconds since the epoch.
    pub at: i64,
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
    /// The sender's contact photo on disk, or empty for initials.
    pub photo: String,
    /// The account's colour, or `None` when it shows in none.
    pub account_color: Option<String>,
}

/// One message's metadata beneath an expanded conversation in the list.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct MessageSummary {
    pub id: String,
    pub sender: String,
    pub date: String,
    pub snippet: String,
    pub unread: bool,
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
    /// Whether the list can show only unread mail.
    pub filters_unread: bool,
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
    pub(crate) people: Arc<Mutex<Option<Arc<Vec<mailrs_store::contacts::Suggestion>>>>>,
    pub(crate) directory: Arc<Mutex<crate::contacts::DirectoryCache>>,
    /// Who hears the engine, kept to start it again after a change.
    listener: Mutex<Option<Arc<dyn MailListener>>>,
    /// The local copy of every account's calendars.
    pub(crate) calendar: Arc<mailrs_sync::calendar_copy::CalendarCopy<Running>>,
    /// Each account's colour, as the last sidebar read it from
    /// Preferences, for the list's rows.
    account_colors: Mutex<HashMap<AccountId, Option<String>>>,
    /// What the sample accounts sync with, in a demo run. Their accounts
    /// connect to these instead of going through the Keychain and the
    /// network.
    demo: Option<Arc<DemoMail>>,
}

#[uniffi::export]
impl Mail {
    #[uniffi::constructor]
    pub fn open(path: String) -> Result<Mail, CoreError> {
        let path = PathBuf::from(path);
        // On a Mac Iris has never run on, the folder the store lives in is
        // not there yet, and SQLite cannot make a file in a missing folder.
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|err| CoreError::Store(err.to_string()))?;
        }
        let db = Db::open(&path)?;
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .thread_name("iris-sync")
            .thread_stack_size(mailrs_sync::WORKER_STACK)
            .enable_all()
            .build()
            .map_err(|err| CoreError::Store(err.to_string()))?;
        let running = Arc::new(Running::default());
        let calendar = Arc::new(mailrs_sync::calendar_copy::CalendarCopy::new(Arc::clone(&running), db.clone()));
        // A calendar change still waiting on its Undo when the app last
        // stopped has nothing left to close over it, so it is queued before
        // anything reads or sends its account, as the GTK app does.
        if let Err(err) = runtime.block_on(calendar.recover_holds()) {
            tracing::warn!(%err, "could not queue the calendar changes left waiting");
        }
        Ok(Mail {
            calendar,
            mailboxes: Arc::new(Mailboxes::new(Arc::clone(&running), db.clone())),
            actions: Arc::new(MailActions::new(Arc::clone(&running), db.clone(), OneClick::Web)),
            dir: path.parent().map(PathBuf::from).unwrap_or_default(),
            running,
            lock: Mutex::new(None),
            runtime,
            db,
            keys: Mutex::new(HashMap::new()),
            people: Arc::new(Mutex::new(None)),
            directory: Arc::new(Mutex::new(crate::contacts::DirectoryCache::default())),
            account_colors: Mutex::new(HashMap::new()),
            listener: Mutex::new(None),
            demo: None,
        })
    }

    /// Makes a new store at `path`, fills it with the sample accounts, and
    /// opens it the way `open` does. Their mail, calendars and contacts come
    /// from in-memory servers, so nothing reaches the network or the
    /// Keychain. For screenshots (`scripts/macos-screenshots.sh`); only a
    /// build with the `demo` feature, as debug builds are, has it.
    #[uniffi::constructor]
    pub fn open_demo(path: String) -> Result<Mail, CoreError> {
        #[cfg(any(feature = "demo", test))]
        {
            // Seeding a store that already holds mail would mix the samples
            // into someone's real accounts.
            if std::path::Path::new(&path).exists() {
                return Err(CoreError::Store(format!("The demo makes a new store, and {path} already exists.")));
            }
            let mut mail = Mail::open(path)?;
            let now = mailrs_sync::now_millis();
            // Seeding runs on a worker, whose stack is `WORKER_STACK`: in a
            // debug build its future overflows the caller's, such as a
            // Swift task's thread.
            let db = mail.db.clone();
            let seeding = mail.runtime.spawn(async move { crate::demo::seed(&db, now).await });
            let demo = mail
                .runtime
                .block_on(seeding)
                .map_err(|err| CoreError::Store(err.to_string()))?
                .map_err(|err| CoreError::Store(err.to_string()))?;
            let photos = mail.dir.join("photos");
            mail.runtime.block_on(mail.db.write(move |c| crate::demo::seed_contacts(c, &photos)))?;
            // The samples' newsletters name lists nobody runs, so their
            // one-click unsubscribe posts nowhere.
            let one_click = OneClick::Fake(Arc::new(mailrs_sync::fake::FakeOneClick::default()));
            mail.actions = Arc::new(MailActions::new(Arc::clone(&mail.running), mail.db.clone(), one_click));
            mail.demo = Some(Arc::new(demo));
            Ok(mail)
        }
        #[cfg(not(any(feature = "demo", test)))]
        {
            let _ = path;
            Err(CoreError::Store("This build of Iris has no demo: build it with the demo feature.".into()))
        }
    }

    /// Starts syncing every account, as the GTK app does when it opens,
    /// and tells `listener` what changes. Fails while another copy of Iris
    /// syncs this store.
    pub fn start_sync(&self, listener: Arc<dyn MailListener>) -> Result<(), CoreError> {
        let mut held = self.lock.lock().unwrap_or_else(|p| p.into_inner());
        if held.is_some() {
            return Ok(());
        }
        // A copy just quit, as when the app restarts, can hold the lock for
        // a moment more, so the take is tried for a few seconds before it
        // says another copy has it.
        let mut tries = 0;
        let lock = loop {
            match SyncLock::take(&self.dir) {
                Ok(lock) => break lock,
                Err(LockError::Held) if tries < 20 => {
                    tries += 1;
                    std::thread::sleep(std::time::Duration::from_millis(250));
                }
                Err(LockError::Held) => {
                    return Err(CoreError::Store(gettext(
                        "Another copy of Iris is syncing your mail. Quit it, then try again.",
                    )));
                }
                Err(other) => return Err(CoreError::Store(other.to_string())),
            }
        };
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
        let db = self.db.clone();
        let (rules, running) = (self.rules_engine(), Arc::clone(&self.running));
        self.runtime.spawn(async move {
            while let Ok(event) = events.recv().await {
                // A check going on changes nothing on screen but its
                // spinner, so it skips the reading again a change asks for.
                if let ChangeEvent::Checking { account_id, busy } = event {
                    heard.checking(account_id, busy);
                    continue;
                }
                heard.changed(what_changed(&event).into());
                if let ChangeEvent::NewMail { account_id, message_ids } = &event {
                    let (db, heard, rules, running) = (db.clone(), Arc::clone(&heard), Arc::clone(&rules), Arc::clone(&running));
                    let (account_id, message_ids) = (*account_id, message_ids.clone());
                    tokio::spawn(async move {
                        // Local rules file new mail first, so mail they move
                        // out of the inbox or mark read does not notify.
                        if crate::rules::rules_here(&running, account_id)
                            && let Err(err) = rules.run_due(account_id).await
                        {
                            tracing::warn!(account = account_id, %err, "local rules did not run");
                        }
                        crate::notify::announce(db, account_id, message_ids, heard).await;
                    });
                }
            }
        });
        self.runtime.spawn(crate::rules::keep_running(
            self.rules_engine(),
            self.account_settings(),
            Arc::clone(&self.running),
            Arc::clone(&engine),
            self.db.clone(),
        ));
        self.runtime.spawn(send_due(
            Arc::clone(&self.running),
            Arc::clone(&engine),
            self.db.clone(),
            Arc::clone(&self.actions),
            Arc::clone(&listener),
        ));
        self.runtime.spawn(crate::contacts::keep_fresh(
            self.contact_book(),
            Arc::clone(&self.running),
            Arc::clone(&engine),
            self.db.clone(),
            Arc::clone(&listener),
            Arc::clone(&self.people),
        ));
        self.runtime.spawn(crate::calendar::keep_fresh(
            Arc::clone(&self.calendar),
            Arc::clone(&self.running),
            Arc::clone(&engine),
            self.db.clone(),
            Arc::clone(&listener),
        ));
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
        let demo = self.demo.clone();
        self.runtime.spawn(async move {
            let Ok(all) = db.read(accounts::list_accounts).await else { return };
            let connect = move |account: Account| {
                let (connector, demo) = (Arc::clone(&connector), demo.clone());
                async move {
                    if let Some(demo) = demo {
                        let services = demo
                            .services(account.id)
                            .ok_or(mailrs_sync::SyncError::UnknownAccount(account.id))?;
                        return Ok(Connected::Ready(services));
                    }
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

    /// The conversation to select after the removed rows leave the list.
    pub fn next_after_removal(&self, rows: Vec<ThreadRef>, removed: Vec<ThreadRef>) -> Option<ThreadRef> {
        mailrs_appcore::moving::next_after_removal(&rows, &removed).cloned()
    }

    /// Runs `action` on `threads`, taken from the mailbox `from` names, and
    /// puts it on the undo stack.
    pub fn act(&self, threads: Vec<ThreadRef>, action: Action, from: Option<String>) -> ActionDone {
        let from = from.and_then(|key| self.mailbox(&key));
        // Delete in Send Later stops the message going out, and in Remind
        // Me drops the reminder, as the GTK app's Trash does there.
        let action = match (action, &from) {
            (Action::Trash, Some(Mailbox::Scheduled)) => return self.cancel_send(threads),
            // An Outbox row is a message waiting here, not mail on the
            // server: Delete drops it, as the GTK app's Outbox does.
            (Action::Trash, Some(Mailbox::Outbox)) => return self.drop_waiting(threads),
            (Action::Trash, Some(Mailbox::Reminders)) => Action::CancelReminder,
            (action, _) => action,
        };
        let said = match &action {
            Action::Remind { at } => Some(fill(
                &gettext("Will remind you {when}"),
                &[("when", &format::future_date(*at, chrono::Local::now()))],
            )),
            Action::CancelReminder => Some(gettext("Back in the Inbox")),
            _ => None,
        };
        let action = mail_action(action);
        let moved = from.map(|m| m.moved_from()).unwrap_or_else(MovedFrom::nowhere);
        let targets: Vec<Target> = threads.into_iter().map(target).collect();
        let (actions, given) = (Arc::clone(&self.actions), action.clone());
        let outcome = self.run(async move { actions.run_from(&targets, given, History::Record, &moved).await });
        self.mailboxes.forget_remote();
        let words = match said {
            Some(said) if !outcome.done.is_empty() => said,
            _ => mailrs_view::done::done_message(&action, outcome.done.len(), true).unwrap_or_default(),
        };
        ActionDone {
            words,
            done: outcome.done.len() as u32,
            failed: outcome.failed.first().map(|f| f.error.clone()),
        }
    }

    /// Stops the Send Later messages `threads` stand for: each goes back
    /// to Drafts, or stays waiting when the server is out of reach.
    fn cancel_send(&self, threads: Vec<ThreadRef>) -> ActionDone {
        let targets: Vec<Target> = threads.into_iter().map(target).collect();
        let outbox = mailrs_sync::outbox::Outbox::new(Arc::clone(&self.running), self.db.clone());
        match self.run(async move { outbox.cancel_scheduled(&targets).await }) {
            Ok(cancelled) => {
                self.mailboxes.forget_remote();
                ActionDone {
                    words: mailrs_appcore::send_later::cancelled_line(cancelled.in_drafts, 0, cancelled.unsaved.len()),
                    done: cancelled.in_drafts as u32,
                    failed: None,
                }
            }
            Err(err) => ActionDone {
                words: String::new(),
                done: 0,
                failed: Some(fill(&gettext("Could not cancel: {reason}"), &[("reason", &err.to_string())])),
            },
        }
    }

    /// Drops the Outbox messages `threads` stand for: nothing goes out and
    /// nothing is kept.
    fn drop_waiting(&self, threads: Vec<ThreadRef>) -> ActionDone {
        let targets: Vec<Target> = threads.into_iter().map(target).collect();
        let outbox = mailrs_sync::outbox::Outbox::new(Arc::clone(&self.running), self.db.clone());
        let dropped = self.run(async move {
            let waiting = outbox.named(&targets).await?;
            for item in &waiting {
                outbox.drop_one(item.id).await?;
            }
            Ok::<usize, mailrs_sync::SyncError>(waiting.len())
        });
        match dropped {
            Ok(count) => {
                self.mailboxes.forget_remote();
                self.changed("threads");
                ActionDone {
                    words: match count {
                        0 => String::new(),
                        1 => gettext("Deleted. It will not be sent."),
                        _ => gettext("Deleted. They will not be sent."),
                    },
                    done: count as u32,
                    failed: None,
                }
            }
            Err(err) => ActionDone {
                words: String::new(),
                done: 0,
                failed: Some(fill(&gettext("Could not delete: {reason}"), &[("reason", &err.to_string())])),
            },
        }
    }

    /// The times Remind Me offers: an hour from now, tonight, tomorrow
    /// morning, next Monday.
    pub fn remind_times(&self) -> Vec<TimeChoice> {
        times(format::remind_presets(chrono::Local::now()))
    }

    /// The times Send Later offers: tonight, tomorrow morning, next
    /// Monday.
    pub fn send_later_times(&self) -> Vec<TimeChoice> {
        times(format::send_later_presets(chrono::Local::now()))
    }

    /// When `at` falls, as a toast says it: "tomorrow at 08:00".
    pub fn when_words(&self, at: i64) -> String {
        format::future_date(at, chrono::Local::now())
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

    /// Asks one account to look for new mail now, announced.
    pub fn check_account(&self, account_id: i64) {
        if let Some(engine) = self.running.current() {
            engine.check(account_id);
        }
    }

    /// Asks every account to look for new mail now, announced.
    pub fn check_now(&self) {
        if let Some(engine) = self.running.current() {
            engine.check_all();
        }
    }

    /// The line under the message list: "Checking for mail…" while
    /// `checking`, else when `account_id`'s mail, or every account's when
    /// it names none, was last brought up to date. With several accounts
    /// the one that checked longest ago speaks for them all.
    pub fn checked_line(&self, checking: bool, account_id: Option<i64>) -> String {
        let last = self.running.current().and_then(|engine| match account_id {
            Some(id) => engine.checked_at(id),
            None => engine.oldest_check(),
        });
        format::checked_line(checking, last, chrono::Local::now())
    }

    /// The sidebar, with each row's count.
    pub fn sidebar(&self) -> Result<Vec<SidebarItem>, CoreError> {
        let db = self.db.clone();
        let settings = Settings::load(&Settings::default_path());
        let listed = self.run(async move { db.read(accounts::list_accounts).await })?;
        let listed = account_menu::ordered(&settings, listed);
        *self.account_colors.lock().unwrap_or_else(|p| p.into_inner()) = account_menu::colors(&settings, &listed);
        let db = self.db.clone();
        let accounts_and_labels = self.run(async move { db.read(move |c| {
            let mut out = Vec::new();
            for account in listed {
                let labels = labels::list_labels(c, account.id)?;
                out.push((account, labels));
            }
            Ok(out)
        }).await })?;
        // Each account's labels in the order the person put them in.
        let mut order = HashMap::new();
        for (account, _) in &accounts_and_labels {
            order.insert(account.id, self.label_positions(account.id).unwrap_or_default());
        }
        let vips: Vec<(String, String)> = settings.vips.iter().map(|(e, n)| (e.clone(), n.clone())).collect();
        let entries = sidebar::entries(&accounts_and_labels, &order, &vips, &settings.smart_mailboxes);
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
            .map(|(a, _)| {
                let shown = settings.account_names.get(&a.email).cloned().unwrap_or_else(|| a.email.clone());
                (a.id, (shown, a.state))
            })
            .collect();
        let colors = self.account_colors.lock().unwrap_or_else(|p| p.into_inner()).clone();
        let mut keys = HashMap::new();
        let mut items = Vec::new();
        for entry in entries {
            match entry {
                Entry::Heading(section) => items.push(SidebarItem {
                    kind: "heading".into(),
                    smart_id: None,
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
                    smart_id: None,
                    key: format!("account-{id}"),
                    title: emails.get(&id).map(|(email, _)| email.clone()).unwrap_or_default(),
                    icon: String::new(),
                    color: None,
                    depth: 0,
                    count: 0,
                    account_color: colors.get(&id).cloned().flatten(),
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
                        smart_id: match &row.mailbox {
                            Mailbox::Smart(smart) => Some(smart.id.clone()),
                            _ => None,
                        },
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
    /// category `category` names when it is an inbox's, and to unread
    /// mail with `unread_only`. `keep` is the conversation the reader has
    /// open, which stays listed after reading it while it is open.
    pub fn list(
        &self,
        key: String,
        category: Option<String>,
        unread_only: bool,
        keep: Option<ThreadRef>,
    ) -> Result<MailboxListing, CoreError> {
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
        let keep = keep.map(|t| (t.account_id, t.thread_id));
        self.listing(mailbox, category, unread_only, keep)
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
            let found = self.listing(mailbox, None, false, None);
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
        let colors = self.account_colors.lock().unwrap_or_else(|p| p.into_inner()).clone();
        let photos = self.photo_paths();
        Ok(MailboxListing {
            title: listing.title,
            subtitle: listing.subtitle,
            rows: listing.rows.iter().map(|t| row(t, now, &colors, &photos)).collect(),
            categories: Vec::new(),
            empty: listing.empty.title,
            filters_unread: false,
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
    fn listing(
        &self,
        mailbox: Mailbox,
        category: Option<String>,
        unread_only: bool,
        keep: Option<(AccountId, String)>,
    ) -> Result<MailboxListing, CoreError> {
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
        let filters_unread = mailbox.filters_unread();
        let view = View {
            threading: settings.threading,
            follow_ups: settings.suggest_follow_ups,
            unread_only: unread_only && filters_unread,
            keep,
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
        let colors = self.account_colors.lock().unwrap_or_else(|p| p.into_inner()).clone();
        let photos = self.photo_paths();
        Ok(MailboxListing {
            title: listing.title,
            subtitle: listing.subtitle,
            rows: listing.rows.iter().map(|t| row(t, now, &colors, &photos)).collect(),
            categories,
            empty: listing.empty.title,
            filters_unread,
        })
    }
}

impl Mail {
    /// Every contact photo on disk, by lower-case address, from the
    /// people the store knows.
    pub(crate) fn photo_paths(&self) -> HashMap<String, std::path::PathBuf> {
        let files: Vec<(String, String)> = self
            .known_people()
            .iter()
            .filter_map(|p| Some((p.email.clone(), p.photo_file.clone()?)))
            .collect();
        mailrs_appcore::photos::on_disk(crate::contacts::photo_dir(), files)
    }

    /// Tells the window that `what` changed, as the engine's news does.
    /// Sends `account`'s queued calendar changes now rather than at the
    /// next refresh, as the GTK app's `push_calendar` does, then says the
    /// calendar changed so the waiting marks clear. Offline, the queue
    /// waits for the next try.
    pub(crate) fn send_calendar(&self, account: AccountId) {
        let copy = Arc::clone(&self.calendar);
        let listener = self.listener.lock().unwrap_or_else(|p| p.into_inner()).clone();
        self.runtime.spawn(async move {
            match copy.send(account).await {
                Ok(_) => {
                    if let Some(listener) = listener {
                        listener.changed("calendar".into());
                    }
                }
                Err(err) => tracing::info!(%err, "calendar changes wait for the next try"),
            }
        });
    }

    pub(crate) fn changed(&self, what: &str) {
        let listener = self.listener.lock().unwrap_or_else(|p| p.into_inner()).clone();
        if let Some(listener) = listener {
            listener.changed(what.to_string());
        }
    }

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
    /// an action overflowed its stack and took the app down. The wait goes
    /// through `futures` rather than tokio's `block_on`, which panics on a
    /// thread that already drives a runtime, as the assistant's does.
    pub(crate) fn run<T: Send + 'static>(&self, work: impl std::future::Future<Output = T> + Send + 'static) -> T {
        futures::executor::block_on(self.runtime.spawn(work))
            .unwrap_or_else(|err| std::panic::resume_unwind(err.into_panic()))
    }

    /// The mailbox behind a key the last sidebar handed out.
    pub(crate) fn mailbox(&self, key: &str) -> Option<Mailbox> {
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
fn times(presets: Vec<(String, i64)>) -> Vec<TimeChoice> {
    presets.into_iter().map(|(label, at)| TimeChoice { label, at }).collect()
}

/// How often the Outbox is tried, as the GTK app's scheduler does.
const SEND_DUE_EVERY: std::time::Duration = std::time::Duration::from_secs(30);

/// Sends what waits in the Outbox, every half minute while `engine` runs:
/// a message that met an account still starting, or one Send Later holds
/// until its hour. Brings back the conversations whose reminder is due,
/// unread, with a notification. Ends once the engine is stopped or
/// replaced.
async fn send_due(
    running: Arc<Running>,
    engine: Arc<SyncEngine>,
    db: Db,
    actions: Arc<MailActions<Running>>,
    listener: Arc<dyn MailListener>,
) {
    let outbox = mailrs_sync::outbox::Outbox::new(Arc::clone(&running), db);
    let mut every = tokio::time::interval(SEND_DUE_EVERY);
    loop {
        every.tick().await;
        if !running.current().is_some_and(|now| Arc::ptr_eq(&now, &engine)) {
            return;
        }
        match outbox.send_due(mailrs_sync::now_millis()).await {
            Ok(drained) => {
                for message in &drained.sent {
                    tracing::info!(account = message.account_id, "sent a message that waited in the Outbox");
                    engine.poke(message.account_id);
                }
                for message in &drained.stuck {
                    tracing::warn!(account = message.account_id, "a message stays in the Outbox");
                }
                if drained.changed {
                    listener.changed("threads".into());
                }
            }
            Err(err) => tracing::warn!(error = %err, "the outbox could not be emptied; will retry"),
        }
        match actions.return_due(mailrs_sync::now_millis()).await {
            Ok(returned) if !returned.is_empty() => {
                tracing::info!(returned = returned.len(), "reminders brought conversations back");
                crate::notify::announce_returned(returned.into_iter().filter_map(|r| r.newest).collect(), &listener);
                listener.changed("threads".into());
            }
            Ok(_) => {}
            Err(err) => tracing::warn!(error = %err, "could not read the reminders that are due"),
        }
    }
}

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
        Action::Remind { at } => MailAction::Remind { at },
        Action::CancelReminder => MailAction::CancelReminder,
        Action::DismissFollowUp => MailAction::DismissFollowUp,
    }
}

fn view(category: Option<Category>) -> View {
    View {
        category,
        now: mailrs_sync::now_millis(),
        ..View::default()
    }
}

fn row(
    thread: &ThreadSummary,
    now: chrono::DateTime<chrono::Local>,
    colors: &HashMap<AccountId, Option<String>>,
    photos: &HashMap<String, std::path::PathBuf>,
) -> ListRow {
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
        photo: photos
            .get(&thread.from_email.trim().to_lowercase())
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_default(),
        account_color: colors
            .get(&thread.account_id)
            .cloned()
            .unwrap_or_else(|| Some(format::PALETTE[format::account_color_index(thread.account_id)].to_string())),
        sender,
        sender_email: thread.from_email.clone(),
        // A draft not given a subject yet would show a blank line, as the
        // GTK list does not.
        subject: if thread.subject.trim().is_empty() {
            mailrs_domain::translate::gettext("(no subject)")
        } else {
            thread.subject.clone()
        },
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
        Icon::Unread => "unread",
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
        Icon::Vips => "vips",
        Icon::Person => "person",
        Icon::Smart => "smart",
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

/// Where the interface's catalogs are, once `bind_language` has run.
pub(crate) static LOCALE_DIR: std::sync::OnceLock<std::path::PathBuf> = std::sync::OnceLock::new();

/// Binds the interface's words to the catalogs in `locale_dir`, in
/// `language` (such as `it_IT`), or in the system's when it is empty.
#[uniffi::export]
pub fn bind_language(locale_dir: String, language: String) {
    // Called once, first thing, before any other thread reads the
    // environment or the locale. gpg and gpgsm live where an app opened
    // from the Dock does not look.
    mailrs_appcore::tool_paths::add_tool_paths();
    unsafe {
        if !language.is_empty() {
            std::env::set_var("LANGUAGE", &language);
            std::env::set_var("LANG", format!("{language}.UTF-8"));
        }
        gettextrs::setlocale(gettextrs::LocaleCategory::LcAll, "");
    }
    let _ = LOCALE_DIR.set(std::path::PathBuf::from(&locale_dir));
    if gettextrs::bindtextdomain("iris", locale_dir).is_err() {
        return;
    }
    let _ = gettextrs::bind_textdomain_codeset("iris", "UTF-8");
    let _ = gettextrs::textdomain("iris");
}

#[cfg(test)]
mod first_run {
    use super::Mail;

    #[test]
    fn the_store_opens_on_a_mac_iris_never_ran_on() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("Application Support/iris/mailrs.db");
        assert!(!path.parent().unwrap().exists());
        Mail::open(path.display().to_string()).expect("the store opens");
        assert!(path.exists());
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::Mail;

    #[test]
    fn dismissing_a_follow_up_survives_reopening_without_changing_the_email() {
        use super::{Action, ThreadRef};
        use mailrs_store::{follow_ups, messages};
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("mailrs.db").display().to_string();
        let mail = Mail::open_demo(path.clone()).unwrap();
        let now = mailrs_sync::now_millis();
        let db = mail.db.clone();
        let waiting = mail.run(async move { db.read(move |c| follow_ups::waiting(c, now)).await }).unwrap();
        let first = waiting.first().expect("demo has a pending follow-up").clone();
        let account = first.account_id;
        let thread = first.thread_id.clone();
        let db = mail.db.clone();
        let before = mail.run(async move { db.read(move |c| messages::thread_messages(c, account, &thread)).await }).unwrap();
        let target = ThreadRef { account_id: account, thread_id: first.thread_id.clone() };
        let done = mail.act(vec![target.clone()], Action::DismissFollowUp, None);
        assert_eq!(done.done, 1);
        assert!(done.failed.is_none());
        let id = first.thread_id.clone();
        let db = mail.db.clone();
        let after = mail.run(async move { db.read(move |c| {
            assert!(!follow_ups::waiting(c, now)?.iter().any(|f| f.account_id == account && f.thread_id == id));
            messages::thread_messages(c, account, &id)
        }).await }).unwrap();
        assert_eq!(before, after, "dismissal changes no message or mailbox");
        assert_eq!(mail.undo().expect("dismissal can be undone").done, 1);
        let db = mail.db.clone();
        let id = first.thread_id.clone();
        assert!(mail.run(async move { db.read(move |c| follow_ups::waiting(c, now)).await }).unwrap().iter().any(|f| f.account_id == account && f.thread_id == id));
        assert_eq!(mail.act(vec![target], Action::DismissFollowUp, None).done, 1);
        drop(mail);
        let mail = Mail::open(path).unwrap();
        let id = first.thread_id;
        let db = mail.db.clone();
        assert!(!mail.run(async move { db.read(move |c| follow_ups::waiting(c, now)).await }).unwrap().iter().any(|f| f.account_id == account && f.thread_id == id));
    }

    /// A reaction answers the message it names, in the conversation it
    /// sits in, with both reaction formats; a word or a list's mail is
    /// refused before anything is queued.
    #[test]
    fn a_reaction_goes_out_answering_its_message() {
        use mailrs_store::{accounts, messages, outbox};
        let dir = tempfile::tempdir().unwrap();
        let mail = Mail::open_demo(dir.path().join("mailrs.db").display().to_string()).unwrap();
        let db = mail.db.clone();
        let (target, listed) = mail
            .run(async move {
                db.read(move |c| {
                    let me: Vec<String> = accounts::list_accounts(c)?.into_iter().map(|a| a.email).collect();
                    let mut target = None;
                    let mut listed = None;
                    let threads: Vec<(i64, String)> = c
                        .prepare("SELECT DISTINCT account_id, thread_id FROM messages ORDER BY account_id, thread_id")?
                        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
                        .collect::<rusqlite::Result<_>>()?;
                    for (account, thread) in threads {
                        for meta in messages::thread_messages(c, account, &thread)? {
                            if mailrs_render::conversation::may_react(&meta, &me) {
                                target.get_or_insert(meta);
                            } else if meta.list_unsubscribe.is_some() {
                                listed.get_or_insert(meta);
                            }
                        }
                    }
                    Ok((target, listed))
                })
                .await
            })
            .unwrap();
        let target = target.expect("demo has mail sent to the account");
        let listed = listed.expect("demo has mail from a list");
        let refused = |meta: &mailrs_domain::MessageMeta, emoji: &str| {
            mail.react(meta.account_id, meta.thread_id.clone(), meta.id.clone(), emoji.into()).is_err()
        };
        assert!(refused(&target, "thanks"));
        assert!(refused(&listed, "👍"));
        mail.react(target.account_id, target.thread_id.clone(), target.id.clone(), "👍".into()).unwrap();
        let db = mail.db.clone();
        let queued = mail.run(async move { db.read(outbox::list).await }).unwrap();
        // The demo's Outbox already holds mail of its own.
        let reactions: Vec<_> = queued
            .iter()
            .filter_map(|q| Some((q, mailrs_mime::read(q.raw.as_deref()?).reaction?)))
            .collect();
        let [(message, reaction)] = reactions.as_slice() else { panic!("one reaction queued, not {}", reactions.len()) };
        assert_eq!(message.thread_id.as_deref(), Some(target.thread_id.as_str()));
        let reaction = reaction.clone();
        assert_eq!(Some(reaction.to), target.rfc822_msgid);
        assert_eq!(reaction.emoji, "👍");
    }

    /// The assistant's tools run on a thread that drives a current-thread
    /// runtime, and they start, sign and send drafts through `Mail`. With
    /// tokio's `block_on` there, every one of them panicked and the model
    /// heard "The mail window closed".
    #[test]
    fn the_assistant_thread_can_reach_the_store() {
        let dir = tempfile::tempdir().unwrap();
        let mail = Arc::new(Mail::open(dir.path().join("mailrs.db").display().to_string()).unwrap());
        let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
        let local = tokio::task::LocalSet::new();
        let senders = local.block_on(&runtime, async { mail.senders() });
        assert!(senders.unwrap().is_empty());
    }

    /// A demo run opens on the sample accounts, each with mail in its
    /// inbox, and never seeds over a store that is already there.
    #[test]
    fn the_demo_fills_a_new_store_and_leaves_an_old_one_alone() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("mailrs.db").display().to_string();
        let mail = Mail::open_demo(path.clone()).unwrap();
        assert!(!mail.has_no_accounts());
        let accounts = mail.sidebar().unwrap().into_iter().filter(|item| item.kind == "account").count();
        assert_eq!(accounts, 6);
        let inbox = mail.sidebar().unwrap().into_iter().find(|item| item.kind == "mailbox").unwrap();
        assert!(!mail.list(inbox.key, None, false, None).unwrap().rows.is_empty());
        drop(mail);
        assert!(Mail::open_demo(path).is_err());
    }
}
