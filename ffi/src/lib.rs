//! The Rust core of Iris as the SwiftUI front end calls it. UniFFI turns
//! what is exported here into Swift: records into structs, `Store` into a
//! class, errors into a thrown enum.
//!
//! This first slice reads the mail the GTK app syncs, without writing:
//! the accounts, a page of an inbox, and one conversation with each
//! message's HTML already cleaned, through the same `mailrs_render`
//! cleaner the GTK app shows mail with.

use std::path::PathBuf;
use std::sync::Mutex;

use mailrs_domain::{MailSet, Role};
use mailrs_store::threads::{self, ThreadFilter};
use mailrs_store::{accounts, bodies, messages};

mod about;
mod account_menu;
mod account_settings;
mod account_setup;
mod ai_prefs;
mod assistant;
mod calendar;
mod compose;
mod contacts;
mod erase;
mod event_editor;
mod filing;
mod hidden;
mod invitation_answer;
mod mail_invitation;
mod mail;
mod notify;
mod prefs;
mod protection;
mod rules;
mod senders;
mod smart;
mod source;
mod templates;
mod translation;
mod unsubscribe;

pub use prefs::{ChoiceItem, PreferenceChoices, Preferences, SignatureSetting, import_signature};

pub use compose::{ComposeDraft, OutgoingFile, ReplyMode, RichBlock, RichSpan, Sender, Suggestion};

pub use mail::{Action, ActionDone, AttachmentInfo, FilePart, ThreadRef, CategoryTab, ListRow, Mail, MailListener, MailboxListing, SidebarItem, bind_language, start_logging, translate};

uniffi::setup_scaffolding!();

/// What can go wrong reading the store, in words for the person.
#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum CoreError {
    #[error("{0}")]
    Store(String),
}

impl From<mailrs_store::StoreError> for CoreError {
    fn from(err: mailrs_store::StoreError) -> Self {
        CoreError::Store(err.to_string())
    }
}

/// One account, as the sidebar lists it.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct AccountRow {
    pub id: i64,
    pub email: String,
    pub provider: String,
}

/// One conversation, as the mail list shows it.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct ThreadRow {
    pub account_id: i64,
    pub id: String,
    pub subject: String,
    pub from: String,
    pub snippet: String,
    /// Milliseconds since the Unix epoch.
    pub date: i64,
    pub message_count: i64,
    pub unread: bool,
    pub starred: bool,
    pub has_attachments: bool,
}

/// One message of an open conversation.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct MessageItem {
    pub id: String,
    pub from: String,
    pub from_address: String,
    pub to: String,
    pub subject: String,
    /// Milliseconds since the Unix epoch.
    pub date: i64,
    /// A whole page for a web view: the body cleaned, or the plain text
    /// escaped, under a policy that loads nothing from the network.
    /// `None` until the GTK app has fetched the body.
    pub page: Option<String>,
}

/// The colors a conversation page is drawn in: the window's appearance
/// and the system accent, as CSS colors.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct PageTheme {
    pub dark: bool,
    pub accent: String,
    /// The accent where it colors text, which needs more contrast.
    pub accent_text: String,
}

/// A body's cleaned HTML, whether it picks its own colors, and where its
/// quoted history starts and ends.
type Cleaned = (String, bool, Option<std::ops::Range<usize>>);

/// A conversation's page, and whether its mail names pictures on the web
/// that the page left out.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct ConversationPage {
    pub html: String,
    pub remote_hidden: bool,
    /// Some message's body has not been fetched yet, and shows as loading.
    pub bodies_missing: bool,
}

/// The mail store, opened to read.
#[derive(uniffi::Object)]
pub struct Store {
    conn: Mutex<rusqlite::Connection>,
}

#[uniffi::export]
impl Store {
    /// Opens the store at `path`.
    #[uniffi::constructor]
    pub fn open(path: String) -> Result<Store, CoreError> {
        let conn = mailrs_store::open_read_only(&PathBuf::from(path))?;
        Ok(Store {
            conn: Mutex::new(conn),
        })
    }

    pub fn accounts(&self) -> Result<Vec<AccountRow>, CoreError> {
        let conn = self.conn();
        Ok(accounts::list_accounts(&conn)?
            .into_iter()
            .map(|account| AccountRow {
                id: account.id,
                email: account.email,
                provider: format!("{:?}", account.provider),
            })
            .collect())
    }

    /// A page of `account_id`'s inbox, newest first.
    pub fn inbox(&self, account_id: i64, offset: i64, limit: i64) -> Result<Vec<ThreadRow>, CoreError> {
        let conn = self.conn();
        let filter = ThreadFilter::account(account_id, MailSet::Role(Role::Inbox));
        Ok(threads::list_threads(&conn, &filter, offset, limit)?
            .into_iter()
            .map(|t| ThreadRow {
                account_id: t.account_id,
                id: t.id,
                subject: t.subject,
                from: t.from,
                snippet: t.snippet,
                date: t.last_message_at,
                message_count: t.message_count,
                unread: t.unread,
                starred: t.starred,
                has_attachments: t.has_attachments,
            })
            .collect())
    }

    /// One conversation as a whole page, drawn as the GTK app draws it:
    /// the subject, a header and a body for each message, the unread ones
    /// and the newest open, quoted history folded away. The page loads
    /// nothing from the network.
    ///
    /// With `allow_remote`, pictures and styles on the web load too, as the
    /// GTK app's Load Images does. The messages `toggled` names are drawn
    /// the other way round: closed where they would open, open where they
    /// would stay closed.
    pub fn conversation_page(
        &self,
        account_id: i64,
        thread_id: String,
        theme: PageTheme,
        allow_remote: bool,
        toggled: Vec<String>,
        open_all: bool,
    ) -> Result<ConversationPage, CoreError> {
        // Cleaning a deeply nested newsletter recurses far, more than a
        // Swift task's thread holds, so the page is drawn on a thread with
        // the sync code's stack.
        std::thread::scope(|scope| {
            std::thread::Builder::new()
                .stack_size(16 * 1024 * 1024)
                .spawn_scoped(scope, || self.draw(account_id, &thread_id, theme, allow_remote, &toggled, open_all))
                .map_err(|err| CoreError::Store(err.to_string()))?
                .join()
                .unwrap_or_else(|panic| std::panic::resume_unwind(panic))
        })
    }

}

impl Store {
    fn draw(
        &self,
        account_id: i64,
        thread_id: &str,
        theme: PageTheme,
        allow_remote: bool,
        toggled: &[String],
        open_all: bool,
    ) -> Result<ConversationPage, CoreError> {
        use mailrs_render::conversation::{
            self as page, BodyState, Head, MessageView, Sanitized, Theme,
        };
        let conn = self.conn();
        let metas = messages::thread_messages(&conn, account_id, thread_id)?;
        // A message translated in this run draws its translation while
        // the person has it shown, and one the engine decrypted draws what
        // it opened.
        let bodies = metas
            .iter()
            .map(|meta| match translation::shown_body(account_id, &meta.id)
                .or_else(|| protection::opened_body(account_id, &meta.id))
            {
                Some(body) => Ok(Some(body)),
                None => bodies::peek_body(&conn, account_id, &meta.id),
            })
            .collect::<Result<Vec<_>, _>>()?;
        let me: Vec<String> = accounts::list_accounts(&conn)?
            .into_iter()
            .map(|account| account.email)
            .collect();
        // Remote pictures load without asking when Preferences say always,
        // or when every sender in the conversation is on the list of
        // senders who may load them, as in the GTK app.
        let senders: Vec<Option<&str>> = metas.iter().map(|m| m.from.as_ref().map(|a| a.email.as_str())).collect();
        let listed = mailrs_store::image_senders::list(&conn)?;
        let allow_remote = allow_remote
            || prefs::loads_remote_images()
            || (!senders.is_empty() && senders.iter().all(|from| mailrs_appcore::images::allowed(&listed, *from)));
        drop(conn);
        // Cleaned once, kept for the borrow each article takes.
        let cleaned: Vec<Option<Cleaned>> = bodies
            .iter()
            .zip(&metas)
            .map(|(body, meta)| {
                let html = body.as_ref()?.html.as_deref()?;
                // Pictures the body names by cid: come through the page's
                // own scheme, which the app answers from the message.
                let pictures = format!("mailrs-cid:{account_id}/{}/0/", mailrs_render::escape_cid(&meta.id));
                let clean = mailrs_render::sanitize::sanitize_html(html, Some(&pictures));
                let lower = clean.to_ascii_lowercase();
                let history = mailrs_render::quoted::history_in_html(&clean);
                Some((clean, page::paints_itself(&lower), history))
            })
            .collect();
        let theme = Theme {
            dark: theme.dark,
            accent: theme.accent,
            accent_text: theme.accent_text,
            summarize: false,
            font: "system-ui".into(),
        };
        let subject = metas.first().map(|m| m.subject.clone()).unwrap_or_default();
        let mut html = page::head(
            &Head {
                subject: &subject,
                count: metas.len(),
                allow_remote,
            },
            &theme,
        );
        let remote_hidden = !allow_remote
            && cleaned
                .iter()
                .flatten()
                .any(|(html, _, _)| page::loads_remote(&html.to_ascii_lowercase()));
        let thumbnails = std::collections::HashMap::new();
        let photos = sender_photos(&self.conn(), &metas);
        let last = metas.len().saturating_sub(1);
        for (at, meta) in metas.iter().enumerate() {
            let body = match &bodies[at] {
                Some(body) => BodyState::Loaded(body),
                None => BodyState::Loading,
            };
            let view = MessageView {
                meta,
                body,
                // Find in the conversation opens every message, so it
                // finds what a closed one holds, as the GTK app's does.
                expanded: open_all || (at == last || meta.is_unread()) != toggled.contains(&meta.id),
                thumbnails: &thumbnails,
                sanitized: cleaned[at].as_ref().map(|(html, paints, history)| Sanitized {
                    html,
                    paints: *paints,
                    history: history.clone(),
                }),
                event_slot: false,
            };
            html.push_str(&page::article(&view, &me, &photos));
        }
        html.push_str(page::TAIL);
        let bodies_missing = bodies.iter().any(Option::is_none);
        Ok(ConversationPage {
            html,
            remote_hidden,
            bodies_missing,
        })
    }
}

#[uniffi::export]
impl Store {

    /// The messages of one conversation, oldest first.
    pub fn conversation(&self, account_id: i64, thread_id: String) -> Result<Vec<MessageItem>, CoreError> {
        let conn = self.conn();
        let mut items = Vec::new();
        for meta in messages::thread_messages(&conn, account_id, &thread_id)? {
            let body = bodies::peek_body(&conn, account_id, &meta.id)?;
            let to = meta
                .to
                .iter()
                .map(|a| a.display().to_string())
                .collect::<Vec<_>>()
                .join(", ");
            items.push(MessageItem {
                from: meta.from.as_ref().map(|a| a.display().to_string()).unwrap_or_default(),
                from_address: meta.from.as_ref().map(|a| a.email.clone()).unwrap_or_default(),
                to,
                subject: meta.subject,
                date: meta.date,
                page: body.map(|b| page(b.html.as_deref(), b.text.as_deref())),
                id: meta.id,
            });
        }
        Ok(items)
    }
}

impl Store {
    fn conn(&self) -> std::sync::MutexGuard<'_, rusqlite::Connection> {
        // A panic while the lock was held leaves a connection that is
        // still fine to read from.
        self.conn.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

/// A message body as a page of its own. Nothing loads from the network:
/// the policy allows inline styles and pictures carried in the page.
fn page(html: Option<&str>, text: Option<&str>) -> String {
    let body = match (html, text) {
        (Some(html), _) => mailrs_render::sanitize::sanitize_html(html, None),
        (None, Some(text)) => format!("<pre>{}</pre>", escape(text)),
        (None, None) => String::new(),
    };
    format!(
        "<!doctype html><html><head><meta charset=\"utf-8\">\
         <meta http-equiv=\"Content-Security-Policy\" content=\"default-src 'none'; \
         style-src 'unsafe-inline'; img-src data:; font-src data:\">\
         <meta name=\"color-scheme\" content=\"light\">\
         <style>body{{margin:16px 20px;font:13px -apple-system,sans-serif;color:#1d1d1f;\
         background:#fff;overflow-wrap:anywhere}}pre{{white-space:pre-wrap;font:inherit}}\
         img{{max-width:100%;height:auto}}</style></head><body>{body}</body></html>"
    )
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

/// The contact photos of the senders in `metas`, as `data:` URIs by
/// lower-case address: the page loads nothing from disk, so a photo
/// travels inline or not at all.
fn sender_photos(conn: &rusqlite::Connection, metas: &[mailrs_domain::MessageMeta]) -> std::collections::HashMap<String, String> {
    let dir = crate::contacts::photo_dir();
    let mut files = Vec::new();
    for from in metas.iter().filter_map(|m| m.from.as_ref()) {
        let key = from.email.trim().to_lowercase();
        if files.iter().any(|(k, _)| *k == key) {
            continue;
        }
        if let Ok(Some(contact)) = mailrs_store::address_book::find(conn, &key)
            && let Some(file) = contact.photo_file
        {
            files.push((key, dir.join(file)));
        }
    }
    mailrs_appcore::photos::read(files)
        .into_iter()
        .filter_map(|(key, data)| Some((key, data?)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_store_written_by_the_app_reads_back_through_the_bridge() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("mailrs.db");
        let conn = mailrs_store::open_connection(&path).unwrap();
        drop(conn);
        let store = Store::open(path.display().to_string()).unwrap();
        assert!(store.accounts().unwrap().is_empty());
        assert!(store.inbox(1, 0, 50).unwrap().is_empty());
        assert!(store.conversation(1, "t1".into()).unwrap().is_empty());
    }

    #[test]
    fn a_body_becomes_a_page_that_loads_nothing_and_runs_nothing() {
        let shown = page(Some("<p>Hi <script>x()</script><img src=\"https://t.example/p.gif\"></p>"), None);
        assert!(shown.contains("default-src 'none'"));
        assert!(!shown.contains("<script"));
        assert!(page(None, Some("a < b")).contains("<pre>a &lt; b</pre>"));
    }
}
