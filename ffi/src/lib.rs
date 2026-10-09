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
