//! The media type a file goes out with, by Apple's uniform type
//! identifiers: `UTType` knows each extension's preferred media type.

use std::path::Path;

use objc2::msg_send;
use objc2::rc::Retained;
use objc2::runtime::{AnyClass, AnyObject};
use objc2_foundation::NSString;

// Linked by name so `UTType` is there in any binary, a test one included,
// not only in an app that brings it in through AppKit.
#[link(name = "UniformTypeIdentifiers", kind = "framework")]
unsafe extern "C" {}

/// The media type for `filename`, `application/octet-stream` when its
/// extension names none.
pub fn mime_type(filename: &str) -> String {
    by_extension(filename).unwrap_or_else(|| "application/octet-stream".into())
}

/// The preferred media type `UTType` gives the extension of `filename`.
pub fn by_extension(filename: &str) -> Option<String> {
    let extension = Path::new(filename).extension()?.to_str()?;
    // UniformTypeIdentifiers comes in with AppKit, so the class is there
    // whenever the app runs; looked up by name, it needs no crate of its own.
    let class = AnyClass::get(c"UTType")?;
    let extension = NSString::from_str(extension);
    // SAFETY: `typeWithFilenameExtension:` takes an NSString and returns
    // a UTType or nil; `preferredMIMEType` returns an NSString or nil.
    unsafe {
        let kind: Option<Retained<AnyObject>> =
            msg_send![class, typeWithFilenameExtension: &*extension];
        let mime: Option<Retained<NSString>> = msg_send![&*kind?, preferredMIMEType];
        mime.map(|m| m.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::mime_type;

    #[test]
    fn common_files_get_their_exact_media_type() {
        assert_eq!(mime_type("plan.txt"), "text/plain");
        assert_eq!(mime_type("photo.JPG"), "image/jpeg");
        assert_eq!(mime_type("blob.zzqx"), "application/octet-stream");
    }
}
