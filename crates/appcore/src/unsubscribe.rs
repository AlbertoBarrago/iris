//! Leaving a mailing list lives in `mailrs_sync::unsubscribe`. The app
//! keeps these names at their old path for the assistant's tools, and
//! adds what both front ends decide before asking: which list, which way
//! out, and the address the list knows the person by.

use mailrs_domain::translate::gettext;
use mailrs_domain::{MessageBody, MessageMeta};
use mailrs_store::unsubscribes::How;
pub use mailrs_sync::unsubscribe::{Unsubscribe, choose_with_body};

use crate::unsubscribe_lines::{Way, sent_to};

/// Where the mail asking a list to let go stands once the outbox has
/// taken it. A list hears nothing until the mail leaves, so only `Sent`
/// counts as unsubscribed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RequestSent {
    Sent,
    /// Gmail could not take it yet, and it waits in the Outbox.
    Waiting,
}

/// One list the person can leave from the message on screen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Leaving {
    /// The sender as a person reads it, "Trail Notes", for the question.
    pub sender: String,
    /// The sender's address, which the store keeps as the list left.
    pub email: String,
    pub method: Unsubscribe,
    /// The address the newsletter came to, which a page is typed and a
    /// request mail goes from.
    pub address: String,
}

impl Leaving {
    /// The way out as the question first shows it. A page has still to
    /// be read, so it starts as `Reading`.
    pub fn way(&self) -> Way {
        match &self.method {
            Unsubscribe::OneClick(_) => Way::OneClick,
            Unsubscribe::Email { .. } => Way::Mail {
                from: self.address.clone(),
            },
            Unsubscribe::Page(_) | Unsubscribe::BodyLink(_) => Way::Reading,
        }
    }

    /// How the store remembers the list was left.
    pub fn how(&self) -> How {
        match &self.method {
            Unsubscribe::OneClick(_) => How::OneClick,
            Unsubscribe::Email { .. } => How::Email,
            Unsubscribe::Page(_) | Unsubscribe::BodyLink(_) => How::Page,
        }
    }

    /// The page to read, for a list that lets go through one.
    pub fn page(&self) -> Option<&str> {
        match &self.method {
            Unsubscribe::Page(url) | Unsubscribe::BodyLink(url) => Some(url),
            _ => None,
        }
    }
}

/// The list `meta` comes from and the way out `body` offers, or `None`
/// when it offers none Iris can use. `send_as` holds the addresses the
/// account sends as, and `account` the account's own.
pub fn leaving(meta: &MessageMeta, body: &MessageBody, send_as: &[String], account: &str) -> Option<Leaving> {
    let method = choose_with_body(
        body.list_unsubscribe.as_deref(),
        body.one_click_unsubscribe,
        body.html.as_deref(),
    )?;
    let to_and_cc: Vec<String> = meta.to.iter().chain(&meta.cc).map(|a| a.email.clone()).collect();
    Some(Leaving {
        sender: meta
            .from
            .as_ref()
            .map(|a| a.display().to_string())
            .unwrap_or_else(|| gettext("this list")),
        email: meta.from.as_ref().map(|a| a.email.clone()).unwrap_or_default(),
        method,
        address: sent_to(&to_and_cc, send_as, account),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use mailrs_domain::Address;

    fn meta() -> MessageMeta {
        MessageMeta {
            from: Some(Address {
                name: Some("Trail Notes".into()),
                email: "news@trail.example".into(),
            }),
            to: vec![Address {
                name: None,
                email: "me+news@example.com".into(),
            }],
            account_id: 1,
            id: "m1".into(),
            thread_id: "t1".into(),
            rfc822_msgid: None,
            cc: Vec::new(),
            subject: "Trail notes".into(),
            date: 0,
            snippet: String::new(),
            size: 0,
            has_attachments: false,
            held: Default::default(),
            roles: Vec::new(),
            list_unsubscribe: None,
            one_click: false,
        }
    }

    fn body(header: &str, one_click: bool) -> MessageBody {
        MessageBody {
            list_unsubscribe: Some(header.into()),
            one_click_unsubscribe: one_click,
            ..MessageBody::default()
        }
    }

    #[test]
    fn a_one_click_list_is_left_with_one_request() {
        let found = leaving(&meta(), &body("<https://trail.example/u/1>", true), &[], "me@example.com")
            .expect("a way out");
        assert_eq!(found.sender, "Trail Notes");
        assert_eq!(found.email, "news@trail.example");
        assert!(matches!(found.way(), Way::OneClick));
        assert_eq!(found.how(), How::OneClick);
        assert_eq!(found.page(), None);
    }

    #[test]
    fn a_request_mail_goes_from_the_address_the_list_wrote_to() {
        let mine = ["me+news@example.com".to_string()];
        let found = leaving(&meta(), &body("<mailto:leave@trail.example>", false), &mine, "me@example.com")
            .expect("a way out");
        assert_eq!(found.address, "me+news@example.com");
        assert!(matches!(found.way(), Way::Mail { ref from } if from == "me+news@example.com"));
        assert_eq!(found.how(), How::Email);
    }

    #[test]
    fn a_page_is_read_before_it_is_named() {
        let found = leaving(&meta(), &body("<https://trail.example/u/1>", false), &[], "me@example.com")
            .expect("a way out");
        assert!(matches!(found.way(), Way::Reading));
        assert_eq!(found.page(), Some("https://trail.example/u/1"));
        assert_eq!(found.address, "me@example.com");
    }

    #[test]
    fn a_message_with_no_way_out_offers_nothing() {
        assert_eq!(leaving(&meta(), &MessageBody::default(), &[], "me@example.com"), None);
    }
}
