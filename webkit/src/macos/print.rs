use gtk::prelude::*;
use objc2::msg_send;
use objc2::runtime::AnyObject;
use objc2_app_kit::NSPrintInfo;

use super::view::WebView;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PrintOperationResponse {
    Print,
    Cancel,
}

/// Prints a view's page through the macOS print panel.
pub struct PrintOperation {
    view: WebView,
}

impl PrintOperation {
    pub fn new(web_view: &WebView) -> PrintOperation {
        PrintOperation { view: web_view.clone() }
    }

    /// Opens the print panel as a sheet on the view's window. The panel
    /// answers later, so this reports Print as soon as it is up; the app
    /// does not read the answer.
    pub fn run_dialog(&self, _parent: Option<&impl IsA<gtk::Window>>) -> PrintOperationResponse {
        let page = self.view.page();
        let Some(window) = self.view.native_window() else {
            return PrintOperationResponse::Cancel;
        };
        unsafe {
            let info = NSPrintInfo::sharedPrintInfo();
            let operation = page.printOperationWithPrintInfo(&info);
            // WKWebView prints blank pages from a frameless view; give the
            // operation's view the page's own size.
            if let Some(view) = operation.view() {
                view.setFrame(page.bounds());
            }
            let none: *mut AnyObject = std::ptr::null_mut();
            let _: () = msg_send![
                &*operation,
                runOperationModalForWindow: &*window,
                delegate: none,
                didRunSelector: std::ptr::null::<u8>(),
                contextInfo: std::ptr::null_mut::<std::ffi::c_void>()
            ];
        }
        PrintOperationResponse::Print
    }
}
