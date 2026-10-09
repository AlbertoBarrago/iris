//! The HTML on the macOS pasteboard. GDK's macOS clipboard offers text and
//! pictures, so the formatted copy a mail client puts there under
//! `public.html` is read from AppKit directly.

use objc2_app_kit::{NSPasteboard, NSPasteboardTypeHTML};

/// The HTML the general pasteboard holds, if it holds any.
pub fn html() -> Option<String> {
    let board = NSPasteboard::generalPasteboard();
    // SAFETY: AppKit's own constant for `public.html`, read once.
    let kind = unsafe { NSPasteboardTypeHTML };
    board
        .stringForType(kind)
        .map(|html| html.to_string())
        .filter(|html| !html.trim().is_empty())
}
