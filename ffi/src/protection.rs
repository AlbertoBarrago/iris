//! Signed and encrypted mail, for the Swift front end: each protected
//! message of a conversation is opened with gpg or gpgsm, as the GTK
//! engine run opens it, through `mailrs_appcore::protection`, `pgp` and
//! `smime`. The card says what the engine found; a message it decrypted
//! is drawn from what it opened, kept for the run and never stored.

use std::collections::HashMap;
use std::sync::{LazyLock, Mutex, OnceLock};

use mailrs_appcore::compose::{self, Built, Draft};
use mailrs_appcore::protection::{Addressees, Engine, Held, Mark, Read, Standard, Tone, encrypting, encrypting_with, engine, signing};
use mailrs_appcore::{pgp, smime};
use mailrs_domain::translate::{fill, gettext};
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

/// What the composer can do with the message as it is addressed now.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct ProtectionState {
    /// Whether gpg or gpgsm is on this Mac at all, so Sign can work.
    pub can_sign: bool,
    /// Whether every recipient has a key or certificate here.
    pub can_encrypt: bool,
    /// What Encrypt would do, or why it cannot.
    pub encrypt_note: String,
    /// Preferences say to encrypt whenever every recipient can read it.
    pub encrypt_when_possible: bool,
}

/// Settings, Writing, Signing and Encryption: what gpg and gpgsm hold
/// for the person's own addresses, and the two switches. `None` from
/// `protection_settings` when neither program is on this Mac.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct ProtectionSettings {
    pub title: String,
    pub description: String,
    /// Empty when gpg is missing; else "Your OpenPGP Keys" and what gpg
    /// holds of the person's addresses.
    pub keys_title: String,
    pub keys_line: String,
    pub certificates_title: String,
    pub certificates_line: String,
    pub import_title: String,
    pub sign_title: String,
    pub sign_subtitle: String,
    pub sign_by_default: bool,
    pub encrypt_title: String,
    pub encrypt_subtitle: String,
    pub encrypt_when_possible: bool,
}

/// What an import brought in, for the dialog after it.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct ImportReport {
    pub heading: String,
    pub body: String,
    /// "Ann Lee · ann@example.com: New".
    pub lines: Vec<String>,
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

#[uniffi::export]
impl Mail {
    /// The Signing and Encryption group of Settings, Writing.
    pub fn protection_settings(&self) -> Option<ProtectionSettings> {
        let (gpg, gpgsm) = engines();
        if gpg.is_none() && gpgsm.is_none() {
            return None;
        }
        let settings = crate::prefs::load();
        let mut addresses: Vec<String> = self.senders().unwrap_or_default().into_iter().map(|s| s.email.to_lowercase()).collect();
        addresses.sort();
        addresses.dedup();
        let (keys_title, keys_line) = match gpg {
            Some(gpg) => (
                gettext("Your OpenPGP Keys"),
                match gpg.keys_for(&addresses) {
                    Ok(held) => pgp::own_keys(&held),
                    Err(err) => fill(&gettext("gpg could not be asked: {reason}"), &[("reason", &pgp::explain(&err))]),
                },
            ),
            None => (String::new(), String::new()),
        };
        let (certificates_title, certificates_line) = match gpgsm {
            Some(gpgsm) => (
                gettext("Your S/MIME Certificates"),
                match gpgsm.signing_certificates(&addresses) {
                    Ok(held) => smime::own_certificates(&held),
                    Err(err) => fill(&gettext("gpgsm could not be asked: {reason}"), &[("reason", &smime::explain(&err))]),
                },
            ),
            None => (String::new(), String::new()),
        };
        Some(ProtectionSettings {
            title: gettext("Signing and Encryption"),
            description: gettext(
                "Iris signs and encrypts through GnuPG, which holds your keys and \
                 asks for your passphrase itself.",
            ),
            keys_title,
            keys_line,
            certificates_title,
            certificates_line,
            import_title: gettext("Import…"),
            sign_title: gettext("Sign My Messages by Default"),
            sign_subtitle: gettext("New messages open with Sign turned on"),
            sign_by_default: settings.sign_by_default,
            encrypt_title: gettext("Encrypt When I Can"),
            encrypt_subtitle: gettext("Turn Encrypt on as soon as every recipient has a key or a certificate"),
            encrypt_when_possible: settings.encrypt_when_possible,
        })
    }

    /// Imports the OpenPGP keys (`pgp`) or S/MIME certificates (`smime`)
    /// in `file` into the keyring gpg or gpgsm already uses, and says
    /// what came in. Either may ask for a passphrase in its own window.
    pub fn import_keys(&self, kind: String, file: Vec<u8>) -> ImportReport {
        use mailrs_appcore::key_import::{Failure, Kind, more_text, report};
        let (gpg, gpgsm) = engines();
        let (kind, outcome) = match kind.as_str() {
            "smime" => (
                Kind::Smime,
                match gpgsm {
                    Some(gpgsm) => gpgsm.import(&file).map_err(|err| match err {
                        mailrs_smime::SmimeError::WrongPassphrase | mailrs_smime::SmimeError::NoPassphrase => {
                            Failure::Passphrase(smime::explain(&err))
                        }
                        other => Failure::Other(smime::explain(&other)),
                    }),
                    None => Err(Failure::Other(smime::explain(&mailrs_smime::SmimeError::NoGpgsm))),
                },
            ),
            _ => (
                Kind::Pgp,
                match gpg {
                    Some(gpg) => gpg.import(&file).map_err(|err| Failure::Other(pgp::explain(&err))),
                    None => Err(Failure::Other(pgp::explain(&mailrs_pgp::PgpError::NoGpg))),
                },
            ),
        };
        let said = report(kind, outcome.as_deref().map_err(Clone::clone));
        let mut lines: Vec<String> = said
            .lines
            .iter()
            .map(|line| {
                let who = match &line.subtitle {
                    Some(address) => format!("{} · {address}", line.title),
                    None => line.title.clone(),
                };
                format!("{who}: {}", line.status)
            })
            .collect();
        if said.more > 0 {
            lines.push(more_text(said.more));
        }
        ImportReport {
            heading: said.heading,
            body: said.body,
            lines,
        }
    }

    /// What Sign and Encrypt can do for a message to `to`, `cc` and `bcc`
    /// (the recipient fields' text), from what gpg and gpgsm hold here.
    pub fn protection_state(&self, to: String, cc: String, bcc: String) -> ProtectionState {
        let parse = |field: &str| compose::parse_recipients(field).into_iter().map(|a| a.email).collect::<Vec<_>>();
        let (to, cc, bcc) = (parse(&to), parse(&cc), parse(&bcc));
        let everyone: Vec<String> = to.iter().chain(&cc).chain(&bcc).cloned().collect();
        let (gpg, gpgsm) = engines();
        let encrypt = match everyone.is_empty() {
            true => Err(gettext("Add a recipient this computer can encrypt to.")),
            false => encrypting(&held(&everyone), !bcc.is_empty()),
        };
        ProtectionState {
            can_sign: gpg.is_some() || gpgsm.is_some(),
            can_encrypt: encrypt.is_ok(),
            encrypt_note: match encrypt {
                Ok(standard) => encrypting_with(standard),
                Err(reason) => reason,
            },
            encrypt_when_possible: crate::prefs::load().encrypt_when_possible,
        }
    }
}

impl Mail {
    /// The bytes to send for `draft`: the message as written, or what gpg
    /// or gpgsm made of it when it is to go out signed or encrypted, as
    /// the GTK app's `raw_for` builds it. The standard is decided here,
    /// from what the engines hold now.
    pub(crate) fn raw_for(&self, draft: &mut Draft) -> Result<Vec<u8>, CoreError> {
        let message_id = compose::new_message_id(&draft.from.email);
        let date = mailrs_sync::now_millis() / 1000;
        let could_not = |what: &str| {
            CoreError::Store(fill(&gettext("Could not build the message: {reason}"), &[("reason", what)]))
        };
        if !draft.sign && !draft.encrypt {
            return compose::build_mime(draft, date, &message_id).map_err(|err| could_not(&err));
        }
        let addressees = Addressees::of(draft);
        let everyone: Vec<String> = draft.to.iter().chain(&draft.cc).chain(&draft.bcc).map(|a| a.email.clone()).collect();
        let not_sent = |reason: &str| {
            CoreError::Store(match draft.encrypt {
                true => fill(&gettext("Not encrypted, so not sent: {reason}"), &[("reason", reason)]),
                false => fill(&gettext("Not signed, so not sent: {reason}"), &[("reason", reason)]),
            })
        };
        draft.standard = match draft.encrypt {
            true => encrypting(&held(&everyone), addressees.has_blind_copy()).map_err(|reason| not_sent(&reason))?,
            false => signing(&own_held(&draft.from.email)),
        };
        let part = match compose::build(draft, date, &message_id).map_err(|err| could_not(&err))? {
            Built::Message(raw) => return Ok(raw),
            Built::Body(part) => part,
        };
        let from = draft.from.email.clone();
        let (sign, encrypt) = (draft.sign, draft.encrypt);
        let entity = match (draft.standard, engines()) {
            (Standard::Pgp, (Some(gpg), _)) => {
                if encrypt {
                    // The sender reads their own copy in Sent only if gpg
                    // encrypts to them as well, which it can when it holds
                    // a key of theirs.
                    let own = gpg
                        .keys_for(std::slice::from_ref(&from))
                        .is_ok_and(|held| held.iter().any(|r| r.key.is_some()));
                    gpg.encrypt(&part, &addressees.readers(own), sign.then_some(from.as_str()))
                } else {
                    gpg.sign(&part, &from)
                }
                .map_err(|err| pgp::explain(&err))
            }
            (Standard::Smime, (_, Some(gpgsm))) => {
                if encrypt {
                    let own = gpgsm
                        .certificates_for(std::slice::from_ref(&from))
                        .is_ok_and(|held| held.iter().any(|r| r.certificate.is_some()));
                    gpgsm.encrypt(&part, &addressees.certificates(own), sign.then_some(from.as_str()))
                } else {
                    gpgsm.sign(&part, &from)
                }
                .map_err(|err| smime::explain(&err))
            }
            (Standard::Pgp, _) => Err(pgp::explain(&mailrs_pgp::PgpError::NoGpg)),
            (Standard::Smime, _) => Err(smime::explain(&mailrs_smime::SmimeError::NoGpgsm)),
        }
        .map_err(|reason| not_sent(&reason))?;
        compose::build_protected(draft, date, &message_id, entity).map_err(|err| could_not(&err))
    }

    /// What the server keeps for `draft` while it is to go out encrypted:
    /// the body encrypted to the writer's own key, never readable on the
    /// server. OpenPGP first, then S/MIME, as the GTK app saves one.
    pub(crate) fn sealed_draft(&self, draft: &Draft, date: i64, message_id: &str) -> Result<Vec<u8>, CoreError> {
        use mailrs_appcore::protection::draft::{build, for_writer_pgp, for_writer_smime};
        let failed = |reason: &str| CoreError::Store(fill(&gettext("Draft not saved: {reason}"), &[("reason", reason)]));
        let part = compose::build_saved_body_part(draft).map_err(|err| failed(&err))?;
        let from = draft.from.email.clone();
        let (gpg, gpgsm) = engines();
        let mut kept = draft.clone();
        kept.encrypt = true;
        let entity = match gpg.as_ref().map(|gpg| for_writer_pgp(gpg, &part, &from)) {
            Some(Ok(Some(entity))) => Some(entity),
            Some(Err(err)) => return Err(failed(&pgp::explain(&err))),
            _ => match gpgsm.as_ref().map(|gpgsm| for_writer_smime(gpgsm, &part, &from)) {
                Some(Ok(Some(entity))) => Some(entity),
                Some(Err(err)) => return Err(failed(&smime::explain(&err))),
                _ => None,
            },
        };
        let Some(entity) = entity else {
            return Err(CoreError::Store(fill(
                &gettext(
                    "Draft not saved. An encrypted message waits in Drafts encrypted to your own key, and \
                     this computer holds no key or certificate for {address}.",
                ),
                &[("address", &draft.from.email)],
            )));
        };
        build(&kept, date, message_id, entity).map_err(|err| failed(&err))
    }

    /// Fills `draft` from the encrypted draft `raw`, opened with the
    /// engine `standard` names. The error is what to tell the writer.
    pub(crate) fn reopen_sealed(&self, raw: &[u8], standard: Standard, draft: &mut Draft) -> Result<(), CoreError> {
        let unopened = |reason: &str| CoreError::Store(fill(&gettext("Could not open the draft: {reason}"), &[("reason", reason)]));
        let read = match (standard, engines()) {
            (Standard::Pgp, (Some(gpg), _)) => pgp::read(gpg, pgp::Opening::Decrypt, raw, &MessageBody::default()),
            (Standard::Smime, (_, Some(gpgsm))) => smime::read(gpgsm, smime::Opening::Decrypt, raw, &MessageBody::default()),
            (Standard::Pgp, _) => return Err(unopened(&pgp::explain(&mailrs_pgp::PgpError::NoGpg))),
            (Standard::Smime, _) => return Err(unopened(&smime::explain(&mailrs_smime::SmimeError::NoGpgsm))),
        };
        mailrs_appcore::protection::draft::reopen(raw, standard, read, draft).map_err(|reason| unopened(&reason))
    }

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

/// What each engine holds for `addresses`. An engine this Mac lacks, or
/// one that would not answer, is left out, since a question nobody
/// answered is not a missing key.
pub(crate) fn held(addresses: &[String]) -> Held {
    let (gpg, gpgsm) = engines();
    Held {
        pgp: gpg.as_ref().and_then(|gpg| gpg.keys_for(addresses).ok()),
        smime: gpgsm.as_ref().and_then(|gpgsm| gpgsm.certificates_for(addresses).ok()),
    }
}

/// What the engines hold of the sender's own, for which signs.
pub(crate) fn own_held(from: &str) -> Held {
    let from = vec![from.trim().to_lowercase()];
    let (gpg, gpgsm) = engines();
    Held {
        pgp: gpg.as_ref().and_then(|gpg| gpg.keys_for(&from).ok()),
        smime: gpgsm.as_ref().and_then(|gpgsm| gpgsm.signing_certificates(&from).ok()),
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
