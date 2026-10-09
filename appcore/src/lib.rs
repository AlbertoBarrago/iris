//! What Iris does between its windows and the sync engine, free of any
//! toolkit: writing a message, its signature and its rich text. The GTK
//! app and the SwiftUI one both build on it, so a draft is made, saved
//! and sent the same way in either.

/// The app's id, which notifications, the Keychain and Launch Services
/// know Iris by.
pub const APP_ID: &str = "io.github.AlbertoBarrago.Iris";

pub mod attachcheck;
pub mod compose;
pub mod images;
pub mod contacts;
pub mod notify;
pub mod richtext;
pub mod settings;
pub mod signature;
pub mod standard;
pub mod stray_markdown;
