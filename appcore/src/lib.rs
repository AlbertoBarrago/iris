//! What Iris does between its windows and the sync engine, free of any
//! toolkit: writing a message, its signature and its rich text. The GTK
//! app and the SwiftUI one both build on it, so a draft is made, saved
//! and sent the same way in either.

/// The app's id, which notifications, the Keychain and Launch Services
/// know Iris by.
pub const APP_ID: &str = "io.github.AlbertoBarrago.Iris";

pub mod assistant;
pub mod attachcheck;
pub mod compose;
pub mod contacts;
pub mod dirs;
pub mod engine;
pub mod file_type;
pub mod format;
pub mod images;
pub mod label_colors;
pub mod notify;
pub mod offered;
pub mod permission;
pub mod pgp;
pub mod protection;
pub mod richtext;
pub mod rules;
pub mod search;
pub mod settings;
pub mod signature;
pub mod smime;
pub mod standard;
pub mod stray_markdown;
pub mod templates;
pub mod unsubscribe;
pub mod unsubscribe_lines;
pub mod unsubscribe_page;
pub mod wanted;
