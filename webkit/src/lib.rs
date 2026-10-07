//! `webkit6` on Linux, and a stand-in with the same names over WKWebView on
//! macOS, where WebKitGTK does not run.
//!
//! The app is written against webkit6 and names this crate as `webkit`.
//! On Linux it is webkit6 itself, so nothing there changes. On macOS the
//! [`macos`] module offers the part of webkit6's API the app calls, with
//! the same type and method names, backed by a WKWebView laid over the GTK
//! widget that stands for it.

#[cfg(not(target_os = "macos"))]
pub use webkit6::*;

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
pub use macos::*;
