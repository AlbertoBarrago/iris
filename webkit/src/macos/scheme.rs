use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::{Rc, Weak};

use gtk::prelude::*;
use gtk::{gio, glib};
use objc2::rc::Retained;
use objc2::runtime::{NSObject, ProtocolObject};
use objc2::{AllocAnyThread, MainThreadMarker, MainThreadOnly, Message, define_class, msg_send};
use objc2_foundation::{NSData, NSError, NSObjectProtocol, NSString, NSURLResponse};
use objc2_web_kit::{WKURLSchemeHandler, WKURLSchemeTask, WKWebView};

use super::view::WebView;

type Handler = Rc<dyn Fn(&URISchemeRequest)>;

thread_local! {
    /// The schemes the app answers, by name. WKWebView fixes a view's
    /// schemes when the view is made, so each view takes every scheme
    /// registered by then; the app registers its one scheme before it
    /// loads anything.
    static SCHEMES: RefCell<Vec<(String, Handler)>> = const { RefCell::new(Vec::new()) };
    /// The requests still open, by their task, so a request WebKit gave up
    /// on is never answered: answering a stopped task throws.
    static OPEN: RefCell<HashMap<usize, Weak<Request>>> = RefCell::new(HashMap::new());
    static HANDLER: RefCell<Option<Retained<SchemeHandler>>> = const { RefCell::new(None) };
}

/// The process's web context, which in webkit6 owns the custom schemes.
pub struct WebContext;

impl WebContext {
    #[allow(clippy::should_implement_trait)]
    pub fn default() -> Option<WebContext> {
        Some(WebContext)
    }

    pub fn register_uri_scheme<F: Fn(&URISchemeRequest) + 'static>(&self, scheme: &str, f: F) {
        SCHEMES.with(|schemes| schemes.borrow_mut().push((scheme.to_string(), Rc::new(f))));
    }
}

/// The schemes a view made now should answer, and the object that does.
pub(crate) fn registered() -> (Vec<String>, Retained<SchemeHandler>) {
    let names = SCHEMES.with(|schemes| schemes.borrow().iter().map(|(name, _)| name.clone()).collect());
    let handler = HANDLER.with(|handler| {
        handler
            .borrow_mut()
            .get_or_insert_with(|| SchemeHandler::new(super::main_thread()))
            .clone()
    });
    (names, handler)
}

/// One request for an address of a custom scheme. The app may answer it
/// at once or hold it until what it asks for arrives.
#[derive(Clone)]
pub struct URISchemeRequest(Rc<Request>);

struct Request {
    task: Retained<ProtocolObject<dyn WKURLSchemeTask>>,
    uri: String,
    view: glib::WeakRef<WebView>,
    done: Cell<bool>,
}

impl URISchemeRequest {
    pub fn uri(&self) -> Option<glib::GString> {
        Some(glib::GString::from(self.0.uri.as_str()))
    }

    pub fn web_view(&self) -> Option<WebView> {
        self.0.view.upgrade()
    }

    /// Answers with everything `stream` holds, as `content_type`.
    pub fn finish(&self, stream: &impl IsA<gio::InputStream>, _length: i64, content_type: Option<&str>) {
        if !self.open() {
            return;
        }
        let mut bytes = Vec::new();
        let mut chunk = vec![0u8; 64 * 1024];
        loop {
            match stream.read(&mut chunk, gio::Cancellable::NONE) {
                Ok(0) => break,
                Ok(read) => bytes.extend_from_slice(&chunk[..read]),
                Err(err) => {
                    let mut err = err;
                    return self.finish_error(&mut err);
                }
            }
        }
        let task = &self.0.task;
        unsafe {
            let request = task.request();
            let Some(url) = request.URL() else {
                return self.fail("the request has no address");
            };
            let mime = content_type.map(NSString::from_str);
            let response = NSURLResponse::initWithURL_MIMEType_expectedContentLength_textEncodingName(
                NSURLResponse::alloc(),
                &url,
                mime.as_deref(),
                bytes.len() as isize,
                None,
            );
            task.didReceiveResponse(&response);
            task.didReceiveData(&NSData::with_bytes(&bytes));
            task.didFinish();
        }
        self.close();
    }

    pub fn finish_error(&self, error: &mut glib::Error) {
        self.fail(error.message());
    }

    fn fail(&self, why: &str) {
        if !self.open() {
            return;
        }
        unsafe {
            let error = NSError::errorWithDomain_code_userInfo(&NSString::from_str("MailrsScheme"), -1100, None);
            tracing::debug!(reason = why, uri = %self.0.uri, "refused a custom scheme request");
            self.0.task.didFailWithError(&error);
        }
        self.close();
    }

    /// Whether the request can still be answered: not answered before,
    /// and not given up on by WebKit.
    fn open(&self) -> bool {
        !self.0.done.get()
    }

    fn close(&self) {
        self.0.done.set(true);
        OPEN.with(|open| open.borrow_mut().remove(&key(&self.0.task)));
    }
}

fn key(task: &ProtocolObject<dyn WKURLSchemeTask>) -> usize {
    task as *const _ as *const () as usize
}

define_class!(
    /// Answers every custom scheme for every view, through the handlers
    /// [`WebContext::register_uri_scheme`] took.
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "MailrsSchemeHandler"]
    pub(crate) struct SchemeHandler;

    unsafe impl NSObjectProtocol for SchemeHandler {}

    unsafe impl WKURLSchemeHandler for SchemeHandler {
        #[unsafe(method(webView:startURLSchemeTask:))]
        fn start(&self, web_view: &WKWebView, task: &ProtocolObject<dyn WKURLSchemeTask>) {
            let uri = unsafe { task.request().URL() }
                .and_then(|url| url.absoluteString())
                .map(|uri| uri.to_string())
                .unwrap_or_default();
            let scheme = uri.split(':').next().unwrap_or_default().to_string();
            let handler = SCHEMES.with(|schemes| {
                schemes
                    .borrow()
                    .iter()
                    .find(|(name, _)| *name == scheme)
                    .map(|(_, handler)| Rc::clone(handler))
            });
            let request = URISchemeRequest(Rc::new(Request {
                task: task.retain(),
                uri,
                view: glib::WeakRef::new(),
                done: Cell::new(false),
            }));
            if let Some(view) = super::view::owner_of(web_view) {
                request.0.view.set(Some(&view));
            }
            OPEN.with(|open| open.borrow_mut().insert(key(task), Rc::downgrade(&request.0)));
            match handler {
                Some(handler) => handler(&request),
                None => request.fail("no handler for this scheme"),
            }
        }

        #[unsafe(method(webView:stopURLSchemeTask:))]
        fn stop(&self, _web_view: &WKWebView, task: &ProtocolObject<dyn WKURLSchemeTask>) {
            let request = OPEN.with(|open| open.borrow_mut().remove(&key(task)));
            if let Some(request) = request.and_then(|request| request.upgrade()) {
                request.done.set(true);
            }
        }
    }
);

impl SchemeHandler {
    fn new(mtm: MainThreadMarker) -> Retained<SchemeHandler> {
        unsafe { msg_send![SchemeHandler::alloc(mtm), init] }
    }
}
