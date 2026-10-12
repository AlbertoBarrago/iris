//! Translating the message on screen, for the Swift front end: the card
//! that offers it, the model's translation, and the page drawn with it,
//! all through `mailrs_appcore::translation` as the GTK thread run does.
//!
//! A translation is kept beside the message for as long as the app runs,
//! never in the store: it is text a model derived, and turning back to
//! what arrived and forward again costs no second request.

use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};

use mailrs_appcore::settings::Settings;
use mailrs_appcore::translation::{
    self, Body, CardWords, Language, Prose, Reading, done_words, failed_words, offered_words,
};
use mailrs_domain::{AccountId, MessageBody, MessageMeta, Role};
use mailrs_store::{bodies, messages};

use crate::CoreError;
use crate::mail::{LOCALE_DIR, Mail};

/// The translation card above the conversation.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct TranslationOffer {
    /// "This message is in Portuguese", or "Translated from Portuguese".
    pub title: String,
    /// Where the words would go, a note on a cut, or why nothing went.
    pub detail: String,
    /// "Translate", "Show Original" or "Show Translation".
    pub button: String,
    /// Whether `detail` says what went wrong.
    pub problem: bool,
    /// The Mac translates the message itself, through Apple's
    /// Translation framework, rather than a model.
    pub on_device: bool,
}

/// What the Mac's own translator is asked to translate.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct TranslationJob {
    /// The language the message reads as, such as `pt`, or empty when
    /// the words did not say, which leaves Apple's translator to tell.
    pub from: String,
    /// The interface's language, such as `it`.
    pub into: String,
    /// The message's prose, piece by piece, in order.
    pub pieces: Vec<String>,
}

/// One message in the reader's language.
struct Translated {
    from: Option<Language>,
    /// The body with its prose translated, which the page draws in place
    /// of what arrived while `shown`.
    body: MessageBody,
    cut: bool,
    shown: bool,
}

/// Every translation made since the app started, by account and message.
static MADE: LazyLock<Mutex<HashMap<(AccountId, String), Translated>>> = LazyLock::new(Default::default);

impl From<CardWords> for TranslationOffer {
    fn from(words: CardWords) -> Self {
        TranslationOffer {
            title: words.title,
            detail: words.detail,
            button: words.button,
            problem: words.problem,
            on_device: crate::prefs::load().translate_on_device,
        }
    }
}

#[uniffi::export]
impl Mail {
    /// The card for conversation `thread_id`, for an interface in
    /// `interface` (such as `it_IT`): an offer for a message in another
    /// language, or the translation already made. `None` hides it.
    pub fn translation_card(
        &self,
        account_id: i64,
        thread_id: String,
        interface: String,
    ) -> Result<Option<TranslationOffer>, CoreError> {
        let Some(found) = self.read_target(account_id, thread_id)? else {
            return Ok(None);
        };
        if let Some(made) = made().get(&(account_id, found.meta.id.clone())) {
            return Ok(Some(done_words(made.from, made.cut, made.shown).into()));
        }
        let Some(into) = interface_language(&interface) else {
            return Ok(None);
        };
        let Reading::Other(from) = translation::read_message(&found.prose().sample(), &found.same_writer, into) else {
            return Ok(None);
        };
        let settings = Settings::load(&Settings::default_path());
        let goes = match settings.translate_on_device {
            true => Ok(translation::on_device_words()),
            false => translation::destination(&settings.ai).map(|(_, goes)| goes),
        };
        Ok(Some(offered_words(from, goes.as_deref().map_err(String::as_str)).into()))
    }

    /// The card's button: translates the message on screen into
    /// `interface`'s language, or turns over the translation it has.
    /// Answers the card as it now reads; the page is drawn again after.
    pub fn translate_message(
        &self,
        account_id: i64,
        thread_id: String,
        interface: String,
    ) -> Result<TranslationOffer, CoreError> {
        let Some(found) = self.read_target(account_id, thread_id)? else {
            return Err(CoreError::Store(failed_words(None)));
        };
        let key = (account_id, found.meta.id.clone());
        if let Some(made) = made().get_mut(&key) {
            made.shown = !made.shown;
            return Ok(done_words(made.from, made.cut, made.shown).into());
        }
        let into = interface_language(&interface).unwrap_or(translation::ENGLISH);
        let ai = Settings::load(&Settings::default_path()).ai;
        let (config, _) = translation::destination(&ai).map_err(CoreError::Store)?;
        let prose = found.prose();
        let from = match translation::read_message(&prose.sample(), &found.same_writer, into) {
            Reading::Other(from) => from,
            _ => None,
        };
        let pieces: Vec<String> = prose.pieces().iter().map(|p| p.to_string()).collect();
        if pieces.is_empty() {
            return Err(CoreError::Store(failed_words(None)));
        }
        let fits = translation::fits(&pieces.iter().map(String::as_str).collect::<Vec<_>>());
        let cut = fits < pieces.len();
        let asked: Vec<String> = pieces[..fits].to_vec();
        let said = self
            .run(async move {
                let asked: Vec<&str> = asked.iter().map(String::as_str).collect();
                translation::ask(config, into, &asked).await
            })
            .map_err(|err| CoreError::Store(failed_words(Some(&err))))?;
        if said.iter().all(Option::is_none) {
            return Err(CoreError::Store(failed_words(None)));
        }
        let rebuilt = prose.rebuild(&said);
        let mut body = found.body;
        // The page cleans the translated HTML as it cleans any mail HTML.
        match body.html.as_deref().is_some_and(|h| !h.trim().is_empty()) {
            true => body.html = Some(rebuilt),
            false => body.text = Some(rebuilt),
        }
        let words = done_words(from, cut, true).into();
        made().insert(
            key,
            Translated {
                from,
                body,
                cut,
                shown: true,
            },
        );
        Ok(words)
    }

    /// What the Mac's translator is to translate in conversation
    /// `thread_id`: the message's prose, piece by piece, as much of it as
    /// one translation takes, with the language it reads as and the
    /// interface's.
    pub fn translation_job(&self, account_id: i64, thread_id: String, interface: String) -> Result<TranslationJob, CoreError> {
        let Some(found) = self.read_target(account_id, thread_id)? else {
            return Err(CoreError::Store(failed_words(None)));
        };
        let into = interface_language(&interface).unwrap_or(translation::ENGLISH);
        let prose = found.prose();
        let from = match translation::read_message(&prose.sample(), &found.same_writer, into) {
            Reading::Other(Some(language)) => language.code.to_string(),
            _ => String::new(),
        };
        let pieces: Vec<String> = prose.pieces().iter().map(|p| p.to_string()).collect();
        let fits = translation::fits(&pieces.iter().map(String::as_str).collect::<Vec<_>>());
        Ok(TranslationJob {
            from,
            into: into.code.to_string(),
            pieces: pieces[..fits].to_vec(),
        })
    }

    /// Puts what the Mac's translator answered for `translation_job`'s
    /// pieces, in order, into the page in place of the message's prose.
    /// A piece it could not translate stays as it arrived.
    pub fn apply_translation(
        &self,
        account_id: i64,
        thread_id: String,
        interface: String,
        translated: Vec<Option<String>>,
    ) -> Result<TranslationOffer, CoreError> {
        let Some(found) = self.read_target(account_id, thread_id)? else {
            return Err(CoreError::Store(failed_words(None)));
        };
        if translated.iter().all(Option::is_none) {
            return Err(CoreError::Store(failed_words(None)));
        }
        let into = interface_language(&interface).unwrap_or(translation::ENGLISH);
        let prose = found.prose();
        let from = match translation::read_message(&prose.sample(), &found.same_writer, into) {
            Reading::Other(from) => from,
            _ => None,
        };
        let cut = translated.len() < prose.pieces().len();
        let rebuilt = prose.rebuild(&translated);
        let mut body = found.body;
        match body.html.as_deref().is_some_and(|h| !h.trim().is_empty()) {
            true => body.html = Some(rebuilt),
            false => body.text = Some(rebuilt),
        }
        let words = done_words(from, cut, true).into();
        made().insert(
            (account_id, found.meta.id.clone()),
            Translated {
                from,
                body,
                cut,
                shown: true,
            },
        );
        Ok(words)
    }

    /// Turns over the translation conversation `thread_id` has, between
    /// it and the message as it arrived. `None` when it has none.
    pub fn turn_translation(&self, account_id: i64, thread_id: String) -> Result<Option<TranslationOffer>, CoreError> {
        let Some(found) = self.read_target(account_id, thread_id)? else {
            return Ok(None);
        };
        Ok(made().get_mut(&(account_id, found.meta.id.clone())).map(|made| {
            made.shown = !made.shown;
            done_words(made.from, made.cut, made.shown).into()
        }))
    }
}

/// The body the page draws for `message_id`: its translation while that
/// is shown, else `None` for the body as it arrived.
pub(crate) fn shown_body(account_id: AccountId, message_id: &str) -> Option<MessageBody> {
    made()
        .get(&(account_id, message_id.to_string()))
        .filter(|made| made.shown)
        .map(|made| made.body.clone())
}

fn made() -> std::sync::MutexGuard<'static, HashMap<(AccountId, String), Translated>> {
    MADE.lock().unwrap_or_else(|p| p.into_inner())
}

/// The message a translation applies to: the newest in the thread with
/// a body, not a draft of the person's own.
struct Target {
    meta: MessageMeta,
    body: MessageBody,
    /// The writer's other messages in the thread, to fall back on when
    /// this one is too short to tell its language.
    same_writer: String,
}

impl Target {
    /// The words the page shows for the message. The HTML is cleaned
    /// first, so the words come out of the markup the reader sees.
    fn prose(&self) -> Prose {
        match self.body.html.as_deref().filter(|h| !h.trim().is_empty()) {
            Some(html) => {
                let pictures = format!(
                    "mailrs-cid:{}/{}/0/",
                    self.meta.account_id,
                    mailrs_render::escape_cid(&self.meta.id)
                );
                let clean = mailrs_render::sanitize::sanitize_html(html, Some(&pictures));
                Prose::read(Body::Html(&clean))
            }
            None => Prose::read(Body::Text(self.body.text.as_deref().unwrap_or(""))),
        }
    }
}

impl Mail {
    fn read_target(&self, account_id: AccountId, thread_id: String) -> Result<Option<Target>, CoreError> {
        let db = self.db.clone();
        Ok(self.run(async move {
            db.read(move |c| {
                let metas = messages::thread_messages(c, account_id, &thread_id)?;
                let mut found = None;
                for meta in metas.iter().rev().filter(|m| !m.in_role(Role::Drafts)) {
                    if let Some(body) = bodies::peek_body(c, account_id, &meta.id)? {
                        found = Some((meta.clone(), body));
                        break;
                    }
                }
                let Some((meta, body)) = found else { return Ok(None) };
                let writer = meta.from.as_ref().map(|a| a.email.to_lowercase());
                let mut same_writer = String::new();
                for other in metas.iter().rev() {
                    if other.id == meta.id || other.from.as_ref().map(|a| a.email.to_lowercase()) != writer {
                        continue;
                    }
                    let Some(body) = bodies::peek_body(c, account_id, &other.id)? else { continue };
                    let prose = match (&body.text, &body.html) {
                        (Some(text), _) if !text.trim().is_empty() => Prose::read(Body::Text(text)),
                        (_, Some(html)) => Prose::read(Body::Html(html)),
                        _ => continue,
                    };
                    if !same_writer.is_empty() {
                        same_writer.push(' ');
                    }
                    same_writer.push_str(&prose.sample());
                }
                Ok(Some(Target { meta, body, same_writer }))
            })
            .await
        })?)
    }
}

/// The interface's language, when this app can count its words: `asked`
/// (such as `it_IT`) while a catalog for it is installed, else English.
fn interface_language(asked: &str) -> Option<Language> {
    let mut installed = vec!["en".to_string()];
    if let Some(dir) = LOCALE_DIR.get()
        && let Ok(entries) = std::fs::read_dir(dir)
    {
        installed.extend(
            entries
                .flatten()
                .filter(|e| e.path().join("LC_MESSAGES").join("iris.mo").is_file())
                .map(|e| e.file_name().to_string_lossy().into_owned()),
        );
    }
    translation::interface_language("", asked, &installed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_interface_without_a_catalog_reads_as_english() {
        assert_eq!(interface_language("de_DE").map(|l| l.code), Some("en"));
        assert_eq!(interface_language("en_US").map(|l| l.code), Some("en"));
    }

    #[test]
    fn a_translation_is_drawn_only_while_shown() {
        let key = (99, "m-test".to_string());
        made().insert(
            key.clone(),
            Translated {
                from: None,
                body: MessageBody {
                    text: Some("Good morning".into()),
                    ..MessageBody::default()
                },
                cut: false,
                shown: true,
            },
        );
        assert_eq!(shown_body(99, "m-test").and_then(|b| b.text).as_deref(), Some("Good morning"));
        if let Some(made) = made().get_mut(&key) {
            made.shown = false;
        }
        assert_eq!(shown_body(99, "m-test"), None);
        made().remove(&key);
    }
}
