//! The webkit6 API the app uses, over WKWebView.
//!
//! Each type keeps webkit6's name and the signatures the app calls, so the
//! app's code reads the same on both systems. Behaviour that WKWebView
//! cannot offer is left out on purpose and noted where the method lives.

mod asks;
mod content;
mod filter;
mod find;
mod place;
mod policy;
mod print;
mod scheme;
mod session;
mod settings;
mod value;
mod view;

pub use asks::{
    AuthenticationRequest, Download, FileChooserRequest, Notification, PermissionRequest, PermissionState,
    PermissionStateQuery, ScriptDialog, ScriptDialogType,
};
pub use content::{UserContentInjectedFrames, UserContentManager, UserScript, UserScriptInjectionTime};
pub use filter::{UserContentFilter, UserContentFilterStore};
pub use find::{FindController, FindOptions};
pub use policy::{
    NavigationAction, NavigationPolicyDecision, PolicyDecision, PolicyDecisionType, ResponsePolicyDecision,
    URIRequest,
};
pub use print::{PrintOperation, PrintOperationResponse};
pub use scheme::{URISchemeRequest, WebContext};
pub use session::NetworkSession;
pub use settings::Settings;
pub use value::Value;
pub use view::{
    ContextMenu, ContextMenuAction, ContextMenuItem, HitTestResult, LoadEvent, WebProcessTerminationReason,
    WebView, WebViewBuilder, WebViewExt,
};

/// webkit6's extension traits, as the app brings them in.
pub mod prelude {
    pub use super::view::WebViewExt;
}

/// The main thread's marker. Every type here lives on GTK's thread, which
/// on macOS is the process's main thread.
fn main_thread() -> objc2::MainThreadMarker {
    objc2::MainThreadMarker::new().expect("WebKit is used from GTK's thread, the main thread")
}
