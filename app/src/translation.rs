//! Translating a message lives in `mailrs_appcore::translation`, shared
//! with Iris Next. The open thread keeps each translation it made here,
//! with the cleaned HTML only this app draws.

pub use mailrs_appcore::translation::*;
use mailrs_domain::MessageBody;

/// One message in the reader's language, kept for as long as the thread
/// stays open. It never reaches the store: it is text the model derived,
/// and tomorrow's model would write it differently.
pub struct Translation {
    /// The language it arrived in, when the words said which.
    pub from: Option<Language>,
    /// The body with its prose translated, beside the one that arrived
    /// rather than in place of it.
    pub body: MessageBody,
    /// The translated HTML after cleaning, which is what the page draws.
    /// `None` for a message with no HTML part.
    pub clean: Option<crate::open_thread::Cleaned>,
    /// Set when the message was too long and only its start was
    /// translated.
    pub cut: bool,
    /// Whether the page shows this rather than what arrived. The window
    /// turns it over when the reader asks for the original back.
    pub shown: bool,
}
