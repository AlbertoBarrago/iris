//! A stand-in for WebKitGTK's `webkit6`, which does not run on macOS, with
//! the same names over WKWebView.
//!
//! The app was written against webkit6 and names this crate as `webkit`.
//! The [`macos`] module offers the part of webkit6's API the app calls,
//! with the same type and method names, backed by a WKWebView laid over
//! the GTK widget that stands for it.

mod macos;
pub use macos::*;
