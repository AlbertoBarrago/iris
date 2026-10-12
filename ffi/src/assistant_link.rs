//! Where a link in the assistant's answer leads: a conversation or an event
//! it named, which the window opens itself, or a page for the browser. The
//! link is read by `mailrs_appcore::assistant::links`; here it is checked
//! against the store, so a link to mail deleted since the answer says so
//! rather than opening nothing.

use mailrs_appcore::assistant::links::{self, Link};
use mailrs_domain::calendar::split_occurrence_id;
use mailrs_domain::translate::gettext;
use mailrs_domain::{AccountId, EpochMillis};
use mailrs_store::{accounts, calendar, messages};
use rusqlite::Connection;

use crate::mail::Mail;
use crate::mail_invitation::EventSpot;

/// Where a link in an answer leads.
#[derive(Debug, Clone, PartialEq, uniffi::Enum)]
pub enum LinkGoal {
    Conversation { account_id: i64, thread_id: String },
    Event { spot: EventSpot },
    Web { url: String },
    /// Nothing to open, and the words that say why.
    Nowhere { words: String },
}

#[uniffi::export]
impl Mail {
    /// Where `url`, a link in the assistant's answer, leads.
    pub fn assistant_link(&self, url: String) -> LinkGoal {
        let db = self.db.clone();
        match links::read(&url) {
            Link::Web(url) => LinkGoal::Web { url },
            Link::Unknown => LinkGoal::Nowhere {
                words: gettext("Iris cannot open that link."),
            },
            Link::Conversation { account, thread } => {
                let found = self.run(async move { db.read(move |c| conversation(c, &account, &thread)).await });
                match found {
                    Ok(Some((account_id, thread_id))) => LinkGoal::Conversation { account_id, thread_id },
                    _ => LinkGoal::Nowhere {
                        words: gettext("That conversation is no longer in Iris."),
                    },
                }
            }
            Link::Event { account, id } => {
                let found = self.run(async move { db.read(move |c| event(c, &account, &id)).await });
                match found {
                    Ok(Some(spot)) => LinkGoal::Event { spot },
                    _ => LinkGoal::Nowhere {
                        words: gettext("That event is no longer on the calendar."),
                    },
                }
            }
        }
    }
}

fn account_id(conn: &Connection, email: &str) -> mailrs_store::Result<Option<AccountId>> {
    Ok(accounts::account_by_email(conn, email)?.map(|a| a.id))
}

/// The conversation, when the store still holds mail of it.
fn conversation(conn: &Connection, email: &str, thread: &str) -> mailrs_store::Result<Option<(AccountId, String)>> {
    let Some(id) = account_id(conn, email)? else { return Ok(None) };
    let held = messages::thread_messages(conn, id, thread)?;
    Ok((!held.is_empty()).then(|| (id, thread.to_string())))
}

/// Where the event `list_events` named `id` sits: an event by its own id,
/// or one occurrence of a series by the series id and its start.
fn event(conn: &Connection, email: &str, id: &str) -> mailrs_store::Result<Option<EventSpot>> {
    let Some(account_id) = account_id(conn, email)? else { return Ok(None) };
    let spot = |calendar: String, id: String, start: EpochMillis| EventSpot { account_id, calendar, id, start };
    if let Some(found) = calendar::find_event(conn, account_id, id)? {
        return Ok(Some(spot(found.calendar, found.id, found.start)));
    }
    let Some((series, start)) = split_occurrence_id(id) else { return Ok(None) };
    Ok(calendar::find_event(conn, account_id, series)?.map(|found| spot(found.calendar, found.id, start)))
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use mailrs_appcore::assistant::links as written;

    use super::*;

    fn mail() -> (tempfile::TempDir, Arc<Mail>) {
        let dir = tempfile::tempdir().unwrap();
        let mail = Arc::new(Mail::open(dir.path().join("mailrs.db").display().to_string()).unwrap());
        (dir, mail)
    }

    #[test]
    fn a_link_to_mail_not_in_the_store_says_so() {
        let (_dir, mail) = mail();
        let goal = mail.assistant_link(written::conversation("nobody@example.com", "t1"));
        assert!(matches!(goal, LinkGoal::Nowhere { .. }));
        let goal = mail.assistant_link(written::event("nobody@example.com", "e1_20261022T090000Z"));
        assert!(matches!(goal, LinkGoal::Nowhere { .. }));
    }

    #[test]
    fn a_web_link_goes_to_the_browser_and_a_strange_one_nowhere() {
        let (_dir, mail) = mail();
        assert_eq!(
            mail.assistant_link("https://example.com".into()),
            LinkGoal::Web { url: "https://example.com".into() }
        );
        assert!(matches!(mail.assistant_link("file:///etc/hosts".into()), LinkGoal::Nowhere { .. }));
    }

    #[test]
    fn a_stored_conversation_opens() {
        let (_dir, mail) = mail();
        let db = mail.db.clone();
        mail.run(async move {
            db.write(|c| {
                let id = accounts::insert_account(c, "me@example.com", 0)?;
                c.execute(
                    "INSERT INTO messages (account_id, id, thread_id, to_addrs, cc_addrs, subject, date, snippet, size, \
                     has_attachments, sync_gen) VALUES (?1, 'm1', 't1', '[]', '[]', 'Hi', 1, '', 1, 0, 1)",
                    [id],
                )?;
                Ok(())
            })
            .await
        })
        .unwrap();
        assert!(matches!(
            mail.assistant_link(written::conversation("me@example.com", "t1")),
            LinkGoal::Conversation { ref thread_id, .. } if thread_id == "t1"
        ));
    }
}
