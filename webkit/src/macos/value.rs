use gtk::glib;
use objc2::msg_send;
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2_foundation::NSString;

/// What a script answered or a page posted, as JavaScriptCore's `Value`
/// stands for it in webkit6. The app only ever reads it as text.
#[derive(Clone, Debug, Default)]
pub struct Value(Option<String>);

impl Value {
    /// Reads an object WKWebView handed over: a string, a number, null,
    /// or nothing at all for `undefined`.
    pub(crate) fn from_object(object: Option<&AnyObject>) -> Value {
        Value(object.map(|object| {
            let text: Retained<NSString> = unsafe { msg_send![object, description] };
            text.to_string()
        }))
    }

    pub(crate) fn undefined() -> Value {
        Value(None)
    }

    /// The value as JavaScript's `String()` would write it.
    pub fn to_str(&self) -> glib::GString {
        glib::GString::from(self.0.as_deref().unwrap_or("undefined"))
    }
}
