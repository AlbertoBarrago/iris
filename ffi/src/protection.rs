//! Signed and encrypted mail, for the Swift front end: each protected
//! message of a conversation is opened with gpg or gpgsm, as the GTK
//! engine run opens it, through `mailrs_appcore::protection`, `pgp` and
//! `smime`. The card says what the engine found; a message it decrypted
//! is drawn from what it opened, kept for the run and never stored.

use std::collections::HashMap;
use std::sync::{LazyLock, Mutex, OnceLock};

use mailrs_appcore::protection::{Engine, Mark, Read, Tone, engine};
use mailrs_appcore::{pgp, smime};
use mailrs_domain::{AccountId, MessageBody, MessageMeta, Role};
use mailrs_pgp::Pgp;
use mailrs_smime::Smime;
use mailrs_store::{bodies, messages};
use mailrs_sync::Accounts;

use crate::CoreError;
use crate::mail::Mail;

/// What the card says about one protected message.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct ProtectionMark {
    pub message_id: String,
    pub title: String,
    pub detail: String,
    /// `good`, `bad` or `unchecked`.
    pub tone: String,
}

/// What the engine made of one message: the card's words, and the body
/// it opened when the message arrived encrypted.
struct Opened {
    mark: Mark,
    body: Option<MessageBody>,
}

/// Every message opened since the app started, by account and message.
static OPENED: LazyLock<Mutex<HashMap<(AccountId, String), Opened>>> = LazyLock::new(Default::default);

/// gpg and gpgsm as this Mac has them, looked for once.
static ENGINES: OnceLock<(Option<Pgp>, Option<Smime>)> = OnceLock::new();

fn engines() -> &'static (Option<Pgp>, Option<Smime>) {
    ENGINES.get_or_init(|| (Pgp::find().ok(), Smime::find().ok()))
}

#[uniffi::export]
impl Mail {
    /// The cards for conversation `thread_id`'s protected messages, oldest
    /// first, opening each one the engine has not opened yet. Opening
    /// asks the server for the message as it arrived, so this runs off
    /// the main thread, and the page is drawn again after it.
    pub fn protection_marks(&self, account_id: i64, thread_id: String) -> Result<Vec<ProtectionMark>, CoreError> {
        let db = self.db.clone();
        let protected: Vec<(MessageMeta, MessageBody, Engine)> = self.run(async move {
            db.read(move |c| {
                let mut out = Vec::new();
                for meta in messages::thread_messages(c, account_id, &thread_id)? {
                    if meta.in_role(Role::Drafts) {
                        continue;
                    }
                    if let Some(body) = bodies::peek_body(c, account_id, &meta.id)?
                        && let Some(opening) = engine(&body)
                    {
                        out.push((meta, body, opening));
                    }
                }
                Ok(out)
            })
            .await
        })?;
        let mut marks = Vec::new();
        for (meta, body, opening) in protected {
            let key = (account_id, meta.id.clone());
            if !opened().contains_key(&key) {
                let read = self.open_protected(account_id, &meta.id, body, opening);
                opened().insert(key.clone(), read);
            }
            if let Some(found) = opened().get(&key) {
                marks.push(ProtectionMark {
                    message_id: meta.id.clone(),
                    title: found.mark.title.clone(),
                    detail: found.mark.detail.clone().unwrap_or_default(),
                    tone: match found.mark.tone {
                        Tone::Good => "good",
                        Tone::Bad => "bad",
                        Tone::Unchecked => "unchecked",
                    }
                    .into(),
                });
            }
        }
        Ok(marks)
    }
}

impl Mail {
    /// Opens one message with its engine. A message the server will not
    /// hand over, or an engine this Mac lacks, is said on the card.
    fn open_protected(&self, account_id: AccountId, message_id: &str, body: MessageBody, opening: Engine) -> Opened {
        let raw = match self.raw_message(account_id, message_id) {
            Ok(raw) => raw,
            Err(reason) => return unchecked(reason),
        };
        let read: Read = match (opening, engines()) {
            (Engine::Pgp(opening), (Some(gpg), _)) => pgp::read(gpg, opening, &raw, &body),
            (Engine::Smime(opening), (_, Some(gpgsm))) => smime::read(gpgsm, opening, &raw, &body),
            (Engine::Pgp(_), _) => return unchecked(pgp::explain(&mailrs_pgp::PgpError::NoGpg)),
            (Engine::Smime(_), _) => return unchecked(smime::explain(&mailrs_smime::SmimeError::NoGpgsm)),
        };
        Opened {
            mark: read.mark,
            body: read.body,
        }
    }

    fn raw_message(&self, account_id: AccountId, message_id: &str) -> Result<Vec<u8>, String> {
        let sync = self
            .running
            .account(account_id)
            .ok_or_else(|| mailrs_domain::translate::gettext("This account is not syncing yet."))?;
        let id = message_id.to_string();
        self.run(async move { sync.raw_message(&id).await }).map_err(|err| err.to_string())
    }
}

/// The card for a message nothing here could check, saying why.
fn unchecked(reason: String) -> Opened {
    Opened {
        mark: Mark {
            title: reason,
            detail: None,
            tone: Tone::Unchecked,
        },
        body: None,
    }
}

/// The body the page draws for `message_id`: what the engine opened, or
/// `None` for the body as it arrived.
pub(crate) fn opened_body(account_id: AccountId, message_id: &str) -> Option<MessageBody> {
    opened().get(&(account_id, message_id.to_string())).and_then(|o| o.body.clone())
}

fn opened() -> std::sync::MutexGuard<'static, HashMap<(AccountId, String), Opened>> {
    OPENED.lock().unwrap_or_else(|p| p.into_inner())
}
