//! What a page may ask of the view: a dialog, a permission, a password, a
//! file, a window. The app's hidden unsubscribe view answers each of these;
//! the types carry the question to its handlers and the answer back.
//!
//! What WKWebView would do with no answer at all is as strict as what the
//! app asks for: a WKWebView opens no window, no file panel and no print
//! panel unless its delegate does so, and webkit6 notifications have no
//! counterpart. Those handlers are kept and never called.

use std::cell::Cell;

use gtk::glib;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScriptDialogType {
    Alert,
    Confirm,
    Prompt,
    BeforeUnloadConfirm,
}

/// An `alert()`, `confirm()` or `prompt()` the page raised.
pub struct ScriptDialog {
    kind: ScriptDialogType,
    message: String,
    confirmed: Cell<bool>,
}

impl ScriptDialog {
    pub(crate) fn new(kind: ScriptDialogType, message: String) -> ScriptDialog {
        ScriptDialog {
            kind,
            message,
            confirmed: Cell::new(false),
        }
    }

    pub fn dialog_type(&self) -> ScriptDialogType {
        self.kind
    }

    pub fn message(&self) -> Option<glib::GString> {
        Some(glib::GString::from(self.message.as_str()))
    }

    /// The answer to a `confirm()`. Unanswered, it is Cancel, as WebKit
    /// answers when nobody does.
    pub fn confirm_set_confirmed(&self, confirmed: bool) {
        self.confirmed.set(confirmed);
    }

    pub(crate) fn confirmed(&self) -> bool {
        self.confirmed.get()
    }
}

/// A request for the camera or the microphone.
pub struct PermissionRequest {
    allowed: Cell<bool>,
}

impl PermissionRequest {
    pub(crate) fn new() -> PermissionRequest {
        PermissionRequest {
            allowed: Cell::new(false),
        }
    }

    pub fn allow(&self) {
        self.allowed.set(true);
    }

    pub fn deny(&self) {
        self.allowed.set(false);
    }

    pub(crate) fn allowed(&self) -> bool {
        self.allowed.get()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PermissionState {
    Granted,
    Denied,
    Prompt,
}

/// A page asking what it would be allowed. WKWebView never asks this of
/// the app.
pub struct PermissionStateQuery;

impl PermissionStateQuery {
    pub fn finish(&self, _state: PermissionState) {}
}

/// A site asking for a user name and password.
pub struct AuthenticationRequest {
    cancelled: Cell<bool>,
}

impl AuthenticationRequest {
    pub(crate) fn new() -> AuthenticationRequest {
        AuthenticationRequest {
            cancelled: Cell::new(false),
        }
    }

    pub fn cancel(&self) {
        self.cancelled.set(true);
    }

    pub(crate) fn cancelled(&self) -> bool {
        self.cancelled.get()
    }
}

/// A file input asking for a file. Never asked; see the module notes.
pub struct FileChooserRequest;

impl FileChooserRequest {
    pub fn cancel(&self) {}
}

/// A web notification. Never shown; see the module notes.
pub struct Notification;

/// A download a page set off. WKWebView only downloads what a navigation
/// policy turns into a download, and the views here never do, so no
/// download ever starts.
pub struct Download;

impl Download {
    pub fn cancel(&self) {}
}
