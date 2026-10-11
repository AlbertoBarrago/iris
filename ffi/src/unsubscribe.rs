//! Leaving the mailing list of the open conversation, for the Swift front
//! end: the banner's offer and the one question before leaving, worded by
//! `mailrs_appcore::unsubscribe_lines`, then the way out through sync's
//! `MailActions`, as the GTK window's Unsubscribe takes it.
//!
//! A list that lets go through a page has the page opened in the
//! person's browser, as the assistant in this app does: reading and
//! pressing a page out of sight needs a browser behind the window, which
//! comes later.

use std::sync::Arc;

use mailrs_appcore::compose;
use mailrs_appcore::settings::Settings;
use mailrs_appcore::unsubscribe::{Leaving, RequestSent, leaving};
use mailrs_appcore::unsubscribe_lines::{ListLine, Way, heading, line_text, summary};
use mailrs_appcore::unsubscribe_page::{Outcome, Prepared, Step};
use mailrs_domain::translate::{fill, gettext};
use mailrs_domain::{AccountId, Role};
use mailrs_store::{accounts, bodies, messages, unsubscribes};
use mailrs_sync::{Leave, Outbox, Posted};

use crate::CoreError;
use crate::compose::queued;
use crate::mail::Mail;

/// The banner's offer to leave the list the conversation comes from.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct ListOffer {
    /// "Unsubscribe from Trail Notes?"
    pub heading: String,
    /// What leaving will do: "ask the sender to take you off the list".
    pub line: String,
}

/// How leaving went.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct UnsubscribeDone {
    /// What the window says, as a toast.
    pub words: String,
    /// A page the person finishes in their browser, or empty.
    pub open_url: String,
    /// Whether the list let go, so the banner goes.
    pub left: bool,
}

#[uniffi::export]
impl Mail {
    /// The offer to leave the list of conversation `thread_id`, or `None`
    /// when its newest message offers no way out, or the person already
    /// left that sender's list.
    pub fn list_offer(&self, account_id: i64, thread_id: String) -> Result<Option<ListOffer>, CoreError> {
        let Some(leaving) = self.leaving(account_id, thread_id)? else {
            return Ok(None);
        };
        let way = shown_way(&leaving);
        let lines = [ListLine {
            name: leaving.sender.clone(),
            way,
        }];
        Ok(Some(ListOffer {
            heading: heading(&lines),
            line: line_text(&lines[0].way),
        }))
    }

    /// Leaves the list of conversation `thread_id`, once the person said
    /// yes to the offer.
    pub fn unsubscribe(&self, account_id: i64, thread_id: String) -> Result<UnsubscribeDone, CoreError> {
        let Some(leaving) = self.leaving(account_id, thread_id)? else {
            return Err(CoreError::Store(gettext("This message has no unsubscribe link")));
        };
        let Leaving {
            sender,
            email,
            method,
            address,
        } = leaving.clone();
        let actions = Arc::clone(&self.actions);
        let leave = self.run(async move { actions.unsubscribe(account_id, method).await });
        let outcome = match leave {
            Ok(Leave::Done) => Outcome::Done,
            Ok(Leave::Open(url)) => Outcome::OpenInBrowser(url),
            Ok(Leave::Send { to, subject, body }) => match self.send_request(account_id, &address, &to, subject, body) {
                Ok(RequestSent::Sent) => Outcome::Done,
                // The list hears nothing until the mail leaves, so this is
                // not "Unsubscribed" yet.
                Ok(RequestSent::Waiting) => {
                    return Ok(UnsubscribeDone {
                        words: fill(
                            &gettext("The request to leave {sender} waits in the Outbox. It goes out as soon as it can."),
                            &[("sender", &sender)],
                        ),
                        open_url: String::new(),
                        left: false,
                    });
                }
                Err(why) => Outcome::Failed(why),
            },
            Err(err) => Outcome::Failed(err.to_string()),
        };
        let left = outcome == Outcome::Done;
        if left && !email.is_empty() {
            let (actions, how) = (Arc::clone(&self.actions), leaving.how());
            // The list let go either way, so a store that would not take
            // the note goes to the log.
            if let Err(err) = self.run(async move { actions.left(account_id, &email, how).await }) {
                tracing::warn!(error = %err, "could not keep the list the person left");
            }
            self.changed("mail");
        }
        let open_url = match &outcome {
            Outcome::OpenInBrowser(url) | Outcome::Unclear(url) => url.clone(),
            _ => String::new(),
        };
        Ok(UnsubscribeDone {
            words: summary(&sender, &outcome),
            open_url,
            left,
        })
    }
}

impl Mail {
    /// The list the newest message of the thread comes from, when it
    /// offers a way out and the person has not left it already.
    fn leaving(&self, account_id: AccountId, thread_id: String) -> Result<Option<Leaving>, CoreError> {
        let db = self.db.clone();
        let found = self.run(async move {
            db.read(move |c| {
                let metas = messages::thread_messages(c, account_id, &thread_id)?;
                // The message a reply would answer: the newest that is
                // not a draft of the person's own.
                let Some(meta) = metas.into_iter().rev().find(|m| !m.in_role(Role::Drafts)) else {
                    return Ok(None);
                };
                if let Some(from) = &meta.from
                    && unsubscribes::left(c, account_id, &from.email)?.is_some()
                {
                    return Ok(None);
                }
                let Some(body) = bodies::peek_body(c, account_id, &meta.id)? else {
                    return Ok(None);
                };
                let account = accounts::list_accounts(c)?
                    .into_iter()
                    .find(|a| a.id == account_id)
                    .map(|a| a.email)
                    .unwrap_or_default();
                Ok(Some((meta, body, account)))
            })
            .await
        })?;
        let Some((meta, body, account)) = found else {
            return Ok(None);
        };
        let settings = Settings::load(&Settings::default_path());
        let mine: Vec<String> = settings.senders(&account).into_iter().map(|a| a.email).collect();
        Ok(leaving(&meta, &body, &mine, &account))
    }

    /// Sends the mail that asks a list to let go, from `from`, the
    /// address the list writes to, under the name the person gave it.
    fn send_request(
        &self,
        account_id: AccountId,
        from: &str,
        to: &str,
        subject: String,
        body: String,
    ) -> Result<RequestSent, String> {
        let mut draft = self.blank(account_id).map_err(|err| err.to_string())?;
        if let Ok(senders) = self.senders()
            && let Some(alias) = senders
                .into_iter()
                .find(|s| s.account_id == account_id && s.email.eq_ignore_ascii_case(from))
        {
            draft.from.email = alias.email;
            draft.from.name = alias.name;
        }
        draft.to = compose::parse_recipients(to);
        draft.subject = subject;
        draft.markdown = body;
        let message_id = compose::new_message_id(&draft.from.email);
        let raw = compose::build_mime(&draft, mailrs_sync::now_millis() / 1000, &message_id)
            .map_err(|err| fill(&gettext("Could not build the message: {reason}"), &[("reason", &err)]))?;
        let message = queued(&draft, raw);
        let outbox = Arc::new(Outbox::new(Arc::clone(&self.running), self.db.clone()));
        match self.run(async move { outbox.post(message).await }).map_err(|err| err.to_string())? {
            Posted::Sent(_) => {
                self.check_account(account_id);
                Ok(RequestSent::Sent)
            }
            Posted::Waiting(_) => Ok(RequestSent::Waiting),
            Posted::Refused(problem) => Err(problem),
        }
    }
}

/// The way out as the question names it. A page is not read out of sight
/// here, so it is the one the person finishes in their browser.
fn shown_way(leaving: &Leaving) -> Way {
    match leaving.page() {
        Some(url) => Way::Page(Prepared {
            url: url.to_string(),
            address: leaving.address.clone(),
            button: String::new(),
            step: Step::Browser(url.to_string()),
        }),
        None => leaving.way(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mailrs_appcore::unsubscribe::Unsubscribe;

    fn leaving(method: Unsubscribe) -> Leaving {
        Leaving {
            sender: "Trail Notes".into(),
            email: "news@trail.example".into(),
            method,
            address: "me@example.com".into(),
        }
    }

    #[test]
    fn a_page_is_named_as_one_to_finish_in_the_browser() {
        let way = shown_way(&leaving(Unsubscribe::Page("https://trail.example/u".into())));
        assert_eq!(line_text(&way), "open the page in the browser");
    }

    #[test]
    fn one_click_is_named_as_a_request() {
        let way = shown_way(&leaving(Unsubscribe::OneClick("https://trail.example/u".into())));
        assert_eq!(line_text(&way), "ask the sender to take you off the list");
    }
}
