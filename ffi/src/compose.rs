//! Writing mail from Iris Next: who it can come from, a new message, a
//! reply or a forward started the way the GTK app starts them, and sending
//! through the engine's outbox. The drafting itself is
//! `mailrs_appcore::compose`, which the GTK composer uses too.

use std::sync::Arc;

use mailrs_appcore::compose::{self, Draft, ReplyKind};
use mailrs_appcore::richtext::{Block, BlockKind, RichBody, Span, Style};
use mailrs_appcore::settings::Settings;
use mailrs_domain::translate::{fill, gettext};
use mailrs_domain::{Address, MessageMeta};
use mailrs_store::outbox::Queued;
use mailrs_store::{accounts, messages};
use mailrs_sync::{Accounts, Outbox, Posted};

use crate::CoreError;
use crate::mail::Mail;

/// An address mail can come from, with the name and signature it sends with.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct Sender {
    pub account_id: i64,
    pub email: String,
    pub name: Option<String>,
}

/// What the composer shows and edits. `draft` carries the rest of the
/// message as the core keeps it (what it answers, its thread, the
/// forwarded original), so a reply goes out threaded.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct ComposeDraft {
    pub draft: String,
    pub account_id: i64,
    pub from: String,
    pub to: String,
    pub cc: String,
    pub bcc: String,
    pub subject: String,
    /// What the person writes, as Markdown, the signature's lines under it
    /// when the signature is not a formatted one.
    pub body: String,
    /// The quote a reply folds under the words, as Markdown.
    pub quoted: Option<String>,
    /// A formatted signature, shown under the words and sent as it is.
    pub signature_html: Option<String>,
    /// The original a forward carries, as HTML, shown read-only.
    pub forwarded_html: Option<String>,
    /// The words as the rich editor holds them. When set they decide what
    /// goes out, and `body` is only their plain text.
    pub rich: Option<Vec<RichBlock>>,
    /// Files going out with the message.
    pub attachments: Vec<OutgoingFile>,
}

/// One line of the rich editor: `paragraph`, `heading`, `bullet`,
/// `numbered`, `quote` or `code`.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct RichBlock {
    pub kind: String,
    /// A heading's level, 1 to 3; 0 otherwise.
    pub level: u8,
    pub spans: Vec<RichSpan>,
}

/// A run of text sharing one style, one link or one picture.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct RichSpan {
    pub text: String,
    pub bold: bool,
    pub italic: bool,
    pub strike: bool,
    pub code: bool,
    pub link: Option<String>,
    /// A picture's source, such as `cid:…`; `text` is then its alt text.
    pub image: Option<String>,
}

/// A file the writer attached. With a `content_id` it is a picture the
/// words show.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct OutgoingFile {
    pub filename: String,
    pub mime_type: String,
    pub data: Vec<u8>,
    pub content_id: Option<String>,
}

/// A person a recipient field can offer.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct Suggestion {
    pub name: Option<String>,
    pub email: String,
    /// What the field holds once this one is picked: the earlier
    /// recipients, this person, and a comma to go on with.
    pub completed: String,
}

/// How a composer starts from a message.
#[derive(Debug, Clone, Copy, PartialEq, uniffi::Enum)]
pub enum ReplyMode {
    Reply,
    ReplyAll,
    Forward,
}

#[uniffi::export]
impl Mail {
    /// Every address the accounts send as, Gmail's aliases included.
    pub fn senders(&self) -> Result<Vec<Sender>, CoreError> {
        let settings = Settings::load(&Settings::default_path());
        let db = self.db.clone();
        let accounts = self.run(async move { db.read(accounts::list_accounts).await })?;
        let mut out = Vec::new();
        for account in accounts {
            for sender in settings.senders(&account.email) {
                let name = sender
                    .name
                    .clone()
                    .or_else(|| sender.email.eq_ignore_ascii_case(&account.email).then(|| settings.display_name(&account.email)).flatten());
                out.push(Sender {
                    account_id: account.id,
                    email: sender.email,
                    name,
                });
            }
        }
        Ok(out)
    }

    /// A new message from `account_id`'s own address, or the first
    /// account's, signed.
    pub fn new_draft(&self, account_id: Option<i64>) -> Result<ComposeDraft, CoreError> {
        let senders = self.senders()?;
        let sender = account_id
            .and_then(|id| senders.iter().find(|s| s.account_id == id))
            .or_else(|| senders.first())
            .cloned()
            .ok_or_else(|| CoreError::Store(gettext("Add an account to write mail.")))?;
        let draft = Draft::new(sender.account_id, address(&sender));
        Ok(self.shown(self.signed(draft)))
    }

    /// A reply, a reply to all, or a forward of the newest message of a
    /// conversation, from the address it was written to, signed.
    pub fn reply_draft(&self, account_id: i64, thread_id: String, mode: ReplyMode) -> Result<ComposeDraft, CoreError> {
        let db = self.db.clone();
        let (thread, id) = (thread_id.clone(), account_id);
        let metas: Vec<MessageMeta> = self.run(async move { db.read(move |c| messages::thread_messages(c, id, &thread)).await })?;
        let original = metas
            .last()
            .cloned()
            .ok_or_else(|| CoreError::Store("The conversation is empty.".into()))?;
        let body = match self.stored_body(account_id, &original.id) {
            Ok(body) => Some(body),
            Err(_) => {
                // A message never opened has no body yet; fetch it.
                if let Some(sync) = self.running.account(account_id) {
                    let message = original.id.clone();
                    self.run(async move { sync.body(&message).await }).ok()
                } else {
                    None
                }
            }
        };
        let text = body
            .as_ref()
            .map(|b| b.text.clone().unwrap_or_else(|| b.html.as_deref().map(mailrs_mime::html::html_to_text).unwrap_or_default()))
            .unwrap_or_else(|| original.snippet.clone());
        let html = body.as_ref().and_then(|b| b.html.clone());
        let mine: Vec<Address> = self
            .senders()?
            .iter()
            .filter(|s| s.account_id == account_id)
            .map(address)
            .collect();
        let kind = match mode {
            ReplyMode::Reply => ReplyKind::Reply,
            ReplyMode::ReplyAll => ReplyKind::ReplyAll,
            ReplyMode::Forward => ReplyKind::Forward,
        };
        let draft = compose::respond(kind, account_id, &mine, &original, &text, html.as_deref(), &metas);
        Ok(self.shown(self.signed(draft)))
    }

    /// Keeps what the composer holds as a draft on the account's server,
    /// replacing the copy saved before, so it can be finished later on any
    /// device. Hands the composer back with the draft's id.
    pub fn save_draft(&self, composed: ComposeDraft) -> Result<ComposeDraft, CoreError> {
        let mut draft = self.draft_from(composed)?;
        let sync = self
            .running
            .account(draft.account_id)
            .ok_or_else(|| CoreError::Store("The account is not connected.".into()))?;
        let message_id = compose::new_message_id(&draft.from.email);
        let date = mailrs_sync::now_millis() / 1000;
        let raw = compose::build_saved_draft(&draft, date, &message_id).map_err(|err| {
            CoreError::Store(fill(&gettext("Could not build the message: {reason}"), &[("reason", &err)]))
        })?;
        let (thread, old) = (draft.thread_id.clone(), draft.draft_id.clone());
        let saved = self
            .run(async move { sync.save_draft(raw, thread, old).await })
            .map_err(|err| CoreError::Store(err.to_string()))?;
        draft.draft_id = Some(saved.draft_id);
        Ok(self.shown(draft))
    }

    /// Deletes the draft the composer saved before, when it was discarded.
    pub fn delete_draft(&self, composed: ComposeDraft) {
        let Ok(draft) = serde_json::from_str::<Draft>(&composed.draft) else { return };
        let (Some(id), Some(sync)) = (draft.draft_id, self.running.account(composed.account_id)) else {
            return;
        };
        let deleted = self.run(async move { sync.delete_draft(&id).await });
        if let Err(err) = deleted {
            tracing::warn!(error = %err, "could not delete a discarded draft");
        }
    }

    /// The people `field` could mean by what is being typed after its last
    /// comma, best first, the contacts of `account_id` before the rest,
    /// leaving out those already in the field.
    pub fn suggest(&self, field: String, account_id: Option<i64>) -> Vec<Suggestion> {
        let (start, query) = mailrs_appcore::contacts::current_token(&field);
        if query.is_empty() {
            return Vec::new();
        }
        let entered: Vec<String> = compose::parse_recipients(&field[..start])
            .into_iter()
            .map(|a| a.email)
            .collect();
        let known = self.known_people();
        mailrs_appcore::contacts::suggest(&known, query, &entered, 8, account_id)
            .into_iter()
            .map(|person| {
                let picked = Address {
                    name: person.name.clone(),
                    email: person.email.clone(),
                };
                let before = field[..start].trim_end();
                let joiner = if before.is_empty() { "" } else { " " };
                Suggestion {
                    name: person.name.clone(),
                    email: person.email.clone(),
                    completed: format!("{before}{joiner}{}, ", compose::format_recipients(&[picked])),
                }
            })
            .collect()
    }

    /// Sends what the composer holds through the engine's outbox, which
    /// keeps it and tries again while the network is away. Says what came
    /// of it, or why it was refused.
    pub fn send(&self, composed: ComposeDraft) -> Result<String, CoreError> {
        let draft = self.draft_from(composed)?;
        if let Some(problem) = draft.problem() {
            return Err(CoreError::Store(problem));
        }
        let message_id = compose::new_message_id(&draft.from.email);
        let date = mailrs_sync::now_millis() / 1000;
        let raw = compose::build_mime(&draft, date, &message_id).map_err(|err| {
            CoreError::Store(fill(&gettext("Could not build the message: {reason}"), &[("reason", &err)]))
        })?;
        let queued = queued(&draft, raw);
        let outbox = Arc::new(Outbox::new(Arc::clone(&self.running), self.db.clone()));
        let posted = self
            .run(async move { outbox.post(queued).await })
            .map_err(|err| CoreError::Store(fill(&gettext("Not sent: {reason}"), &[("reason", &err.to_string())])))?;
        match posted {
            Posted::Sent(_) => {
                self.check_account(draft.account_id);
                Ok(gettext("Message sent"))
            }
            Posted::Waiting(_) => Ok(gettext("Waiting in the Outbox. It goes out as soon as it can.")),
            Posted::Refused(problem) => Err(CoreError::Store(fill(&gettext("Not sent: {reason}"), &[("reason", &problem)]))),
        }
    }
}

impl Mail {
    /// The core's draft with what the composer edited laid over it.
    fn draft_from(&self, composed: ComposeDraft) -> Result<Draft, CoreError> {
        let mut draft: Draft = serde_json::from_str(&composed.draft)
            .map_err(|err| CoreError::Store(err.to_string()))?;
        let name = self
            .senders()?
            .into_iter()
            .find(|s| s.account_id == composed.account_id && s.email.eq_ignore_ascii_case(&composed.from))
            .and_then(|s| s.name);
        draft.account_id = composed.account_id;
        draft.from = Address {
            name,
            email: composed.from.clone(),
        };
        draft.to = compose::parse_recipients(&composed.to);
        draft.cc = compose::parse_recipients(&composed.cc);
        draft.bcc = compose::parse_recipients(&composed.bcc);
        draft.subject = composed.subject.trim().to_string();
        draft.rich = composed.rich.map(rich_body);
        draft.markdown = match &draft.rich {
            Some(rich) => rich.to_markdown(),
            None => composed.body,
        };
        draft.attachments = composed
            .attachments
            .into_iter()
            .map(|file| compose::OutgoingAttachment {
                filename: file.filename,
                mime_type: file.mime_type,
                data: file.data,
                content_id: file.content_id,
            })
            .collect();
        draft.quoted = composed.quoted;
        draft.signature = composed.signature_html;
        Ok(draft)
    }

    /// Everyone the store knows to write to, read once and kept: the
    /// accounts' contacts and the addresses mail turned up.
    pub(crate) fn known_people(&self) -> Arc<Vec<mailrs_store::contacts::Suggestion>> {
        let mut held = self.people.lock().unwrap_or_else(|p| p.into_inner());
        if let Some(people) = held.as_ref() {
            return Arc::clone(people);
        }
        let db = self.db.clone();
        let people = Arc::new(
            self.run(async move { db.read(mailrs_store::contacts::suggestions).await })
                .unwrap_or_default(),
        );
        *held = Some(Arc::clone(&people));
        people
    }

    /// A blank message from `account_id`'s own address, unsigned.
    pub(crate) fn blank(&self, account_id: i64) -> Result<Draft, CoreError> {
        let senders = self.senders()?;
        let sender = senders
            .iter()
            .find(|s| s.account_id == account_id)
            .cloned()
            .ok_or_else(|| CoreError::Store(gettext("That account is not connected.")))?;
        Ok(Draft::new(sender.account_id, address(&sender)))
    }

    /// `draft` with the signature of the address it comes from, as the GTK
    /// app signs a message it starts: Markdown lines under the words, or a
    /// formatted signature beside them.
    pub(crate) fn signed(&self, mut draft: Draft) -> Draft {
        let settings = Settings::load(&Settings::default_path());
        let (db, id) = (self.db.clone(), draft.account_id);
        let account = self
            .run(async move { db.read(move |c| accounts::account(c, id)).await })
            .ok()
            .flatten()
            .map(|a| a.email)
            .unwrap_or_else(|| draft.from.email.clone());
        let signature = settings.signature_for(&account, &draft.from.email);
        draft.markdown = compose::with_signature(&draft.markdown, signature.markdown());
        draft.signature = signature.html().map(str::to_string);
        draft
    }

    /// The draft as the composer shows it.
    pub(crate) fn shown(&self, draft: Draft) -> ComposeDraft {
        ComposeDraft {
            account_id: draft.account_id,
            from: draft.from.email.clone(),
            to: compose::format_recipients(&draft.to),
            cc: compose::format_recipients(&draft.cc),
            bcc: compose::format_recipients(&draft.bcc),
            subject: draft.subject.clone(),
            body: draft.markdown.clone(),
            quoted: draft.quoted.clone(),
            signature_html: draft.signature.clone(),
            forwarded_html: draft.forwarded.as_ref().map(|f| f.to_html()),
            rich: Some(rich_blocks(
                draft
                    .rich
                    .clone()
                    .unwrap_or_else(|| RichBody::from_markdown(&draft.markdown)),
            )),
            attachments: draft
                .attachments
                .iter()
                .map(|a| OutgoingFile {
                    filename: a.filename.clone(),
                    mime_type: a.mime_type.clone(),
                    data: a.data.clone(),
                    content_id: a.content_id.clone(),
                })
                .collect(),
            draft: serde_json::to_string(&draft).unwrap_or_default(),
        }
    }
}


/// The editor's lines as the core's rich body.
fn rich_body(blocks: Vec<RichBlock>) -> RichBody {
    RichBody {
        blocks: blocks
            .into_iter()
            .map(|block| Block {
                kind: match block.kind.as_str() {
                    "heading" => BlockKind::Heading(block.level.clamp(1, 3)),
                    "bullet" => BlockKind::Bullet,
                    "numbered" => BlockKind::Numbered,
                    "quote" => BlockKind::Quote,
                    "code" => BlockKind::Code,
                    _ => BlockKind::Paragraph,
                },
                spans: block
                    .spans
                    .into_iter()
                    .map(|span| Span {
                        text: span.text,
                        style: Style {
                            bold: span.bold,
                            italic: span.italic,
                            strike: span.strike,
                            code: span.code,
                        },
                        link: span.link,
                        image: span.image,
                    })
                    .collect(),
            })
            .collect(),
    }
}

/// The core's rich body as the editor's lines.
fn rich_blocks(body: RichBody) -> Vec<RichBlock> {
    body.blocks
        .into_iter()
        .map(|block| {
            let (kind, level) = match block.kind {
                BlockKind::Paragraph => ("paragraph", 0),
                BlockKind::Heading(level) => ("heading", level),
                BlockKind::Bullet => ("bullet", 0),
                BlockKind::Numbered => ("numbered", 0),
                BlockKind::Quote => ("quote", 0),
                BlockKind::Code => ("code", 0),
            };
            RichBlock {
                kind: kind.into(),
                level,
                spans: block
                    .spans
                    .into_iter()
                    .map(|span| RichSpan {
                        text: span.text,
                        bold: span.style.bold,
                        italic: span.style.italic,
                        strike: span.style.strike,
                        code: span.style.code,
                        link: span.link,
                        image: span.image,
                    })
                    .collect(),
            }
        })
        .collect()
}

fn address(sender: &Sender) -> Address {
    Address {
        name: sender.name.clone(),
        email: sender.email.clone(),
    }
}

/// The outbox's row for a message ready to go, as the GTK app writes it.
pub(crate) fn queued(draft: &Draft, raw: Vec<u8>) -> Queued {
    let recipients = draft
        .to
        .iter()
        .chain(&draft.cc)
        .map(|a| a.display().to_string())
        .collect::<Vec<_>>()
        .join(", ");
    Queued {
        account_id: draft.account_id,
        draft_id: draft.draft_id.clone(),
        thread_id: draft.thread_id.clone(),
        subject: draft.subject.clone(),
        recipients,
        send_at: mailrs_sync::now_millis(),
        raw: Some(raw),
        composer: serde_json::to_string(draft).unwrap_or_default(),
        ..Queued::default()
    }
}
