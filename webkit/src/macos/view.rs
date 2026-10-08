//! The web view: a GTK widget that stands where the page goes, and a
//! WKWebView laid over it on the window.
//!
//! The GTK widget takes part in layout, focus and the widget tree like any
//! other, so popovers parent on it and the window can ask whether the focus
//! is in it. Each frame while it is mapped, it tells the WKWebView where it
//! is (see `place`). The WKWebView itself is made the first time something
//! needs it, so the custom schemes the app registers right after making a
//! view are known by then: WKWebView fixes its schemes when it is made.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;

use gtk::gdk;
use gtk::glib;
use gtk::glib::translate::ToGlibPtr;
use gtk::prelude::*;
use gtk::subclass::prelude::*;
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, Bool, NSObject, ProtocolObject};
use objc2::runtime::Sel;
use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send, sel};
use objc2_app_kit::{NSColor, NSView, NSWindow};
use objc2_foundation::{
    NSError, NSObjectProtocol, NSString, NSURL, NSURLAuthenticationChallenge, NSURLCredential, NSURLProtectionSpace, NSURLRequest,
    NSURLSessionAuthChallengeDisposition,
};
use objc2_web_kit::{
    WKFrameInfo, WKMediaCaptureType, WKNavigation, WKNavigationAction, WKNavigationActionPolicy,
    WKNavigationDelegate, WKNavigationResponse, WKNavigationResponsePolicy, WKPermissionDecision,
    WKSecurityOrigin, WKUIDelegate, WKUserContentController, WKWebView, WKWebViewConfiguration,
};

use super::asks::{
    AuthenticationRequest, FileChooserRequest, Notification, PermissionRequest, PermissionStateQuery, ScriptDialog,
    ScriptDialogType,
};
use super::content::UserContentManager;
use super::find::FindController;
use super::place::{self, Clip};
use super::policy::{NavigationAction, PolicyDecision, PolicyDecisionType};
use super::print::PrintOperation;
use super::session::NetworkSession;
use super::settings::Settings;
use super::value::Value;

unsafe extern "C" {
    /// GTK's way to the NSWindow behind a surface on macOS.
    fn gdk_macos_surface_get_native_window(surface: *mut gdk::ffi::GdkSurface) -> *mut AnyObject;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LoadEvent {
    Started,
    Redirected,
    Committed,
    Finished,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WebProcessTerminationReason {
    Crashed,
    ExceededMemoryLimit,
    TerminatedByApi,
}

/// webkit6's context menu, for the app's handler to name. WKWebView builds
/// its own menu, the native one, and never asks: the handler is kept and
/// not called. The app's message menu does not depend on it, since the
/// page asks for that one itself.
pub struct ContextMenu;

impl ContextMenu {
    pub fn items(&self) -> Vec<ContextMenuItem> {
        Vec::new()
    }

    pub fn remove(&self, _item: &ContextMenuItem) {}
}

pub struct ContextMenuItem;

impl ContextMenuItem {
    pub fn stock_action(&self) -> ContextMenuAction {
        ContextMenuAction::NoAction
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ContextMenuAction {
    NoAction,
    Copy,
    CopyLinkToClipboard,
    CopyImageToClipboard,
    SelectAll,
}

pub struct HitTestResult;

type DecideHandler = Rc<dyn Fn(&WebView, &PolicyDecision, PolicyDecisionType) -> bool>;
type LoadHandler = Rc<dyn Fn(&WebView, LoadEvent)>;
type FailedHandler = Rc<dyn Fn(&WebView, LoadEvent, &str, &glib::Error) -> bool>;
type TerminatedHandler = Rc<dyn Fn(&WebView, WebProcessTerminationReason)>;
type DialogHandler = Rc<dyn Fn(&WebView, &ScriptDialog) -> bool>;
type PermissionHandler = Rc<dyn Fn(&WebView, &PermissionRequest) -> bool>;
type AuthenticateHandler = Rc<dyn Fn(&WebView, &AuthenticationRequest) -> bool>;

thread_local! {
    /// Every page and the widget it stands for, so WebKit's callbacks,
    /// which name the page, can reach the widget.
    static OWNERS: RefCell<HashMap<usize, glib::WeakRef<WebView>>> = RefCell::new(HashMap::new());
}

fn page_key(page: &WKWebView) -> usize {
    page as *const WKWebView as usize
}

/// The widget a page stands for.
pub(crate) fn owner_of(page: &WKWebView) -> Option<WebView> {
    OWNERS.with(|owners| owners.borrow().get(&page_key(page)).and_then(|weak| weak.upgrade()))
}

mod imp {
    use super::*;

    pub struct WebView {
        pub(super) page: RefCell<Option<Retained<Page>>>,
        pub(super) clip: RefCell<Option<Retained<Clip>>>,
        pub(super) delegate: RefCell<Option<Retained<Navigator>>>,
        /// The window the clip is on now, by address, to notice a move.
        pub(super) window: Cell<usize>,
        pub(super) content: RefCell<Option<UserContentManager>>,
        pub(super) settings: RefCell<Option<Settings>>,
        pub(super) session: RefCell<Option<NetworkSession>>,
        pub(super) zoom: Cell<f64>,
        pub(super) background: RefCell<Option<gdk::RGBA>>,
        pub(super) find: RefCell<Option<FindController>>,
        pub(super) decide: RefCell<Vec<DecideHandler>>,
        pub(super) load: RefCell<Vec<LoadHandler>>,
        pub(super) failed: RefCell<Vec<FailedHandler>>,
        pub(super) terminated: RefCell<Vec<TerminatedHandler>>,
        pub(super) dialogs: RefCell<Vec<DialogHandler>>,
        pub(super) permissions: RefCell<Vec<PermissionHandler>>,
        pub(super) authenticate: RefCell<Vec<AuthenticateHandler>>,
        pub(super) tick: RefCell<Option<gtk::TickCallbackId>>,
        pub(super) uri: RefCell<Option<String>>,
        /// The last placement `follow` logged, so the debug log gets one
        /// line per change rather than one per frame.
        pub(super) logged: RefCell<String>,
        /// Trackpad scrolling waiting to reach the page, in widget pixels,
        /// and whether a pass to hand it over is already queued.
        pub(super) scrolled: Cell<(f64, f64)>,
        pub(super) scroll_queued: Cell<bool>,
    }

    impl Default for WebView {
        fn default() -> WebView {
            WebView {
                page: RefCell::new(None),
                clip: RefCell::new(None),
                delegate: RefCell::new(None),
                window: Cell::new(0),
                content: RefCell::new(None),
                settings: RefCell::new(None),
                session: RefCell::new(None),
                zoom: Cell::new(1.0),
                background: RefCell::new(None),
                find: RefCell::new(None),
                decide: RefCell::new(Vec::new()),
                load: RefCell::new(Vec::new()),
                failed: RefCell::new(Vec::new()),
                terminated: RefCell::new(Vec::new()),
                dialogs: RefCell::new(Vec::new()),
                permissions: RefCell::new(Vec::new()),
                authenticate: RefCell::new(Vec::new()),
                tick: RefCell::new(None),
                uri: RefCell::new(None),
                logged: RefCell::new(String::new()),
                scrolled: Cell::new((0.0, 0.0)),
                scroll_queued: Cell::new(false),
            }
        }
    }

    #[glib::object_subclass]
    impl ObjectSubclass for WebView {
        const NAME: &'static str = "MailrsWebView";
        type Type = super::WebView;
        type ParentType = gtk::Widget;

        fn class_init(klass: &mut Self::Class) {
            klass.set_css_name("webview");
            klass.set_accessible_role(gtk::AccessibleRole::Document);
        }
    }

    impl ObjectImpl for WebView {
        fn constructed(&self) {
            self.parent_constructed();
            let widget = self.obj();
            widget.set_focusable(true);
            // GTK's macOS backend handles a trackpad's scrolling itself and
            // never hands it on to AppKit, so the page never saw it: a mouse
            // wheel scrolled the mail and a trackpad did nothing. GTK still
            // reports it here, in surface units, and it goes on to the page.
            let scroll = gtk::EventControllerScroll::new(gtk::EventControllerScrollFlags::BOTH_AXES);
            let weak = widget.downgrade();
            scroll.connect_scroll(move |controller, dx, dy| {
                if controller.unit() != gdk::ScrollUnit::Surface {
                    return glib::Propagation::Proceed;
                }
                let Some(view) = weak.upgrade() else { return glib::Propagation::Proceed };
                view.scroll_page_by(dx, dy);
                glib::Propagation::Stop
            });
            widget.add_controller(scroll);
            // GTK's focus and AppKit's first responder have to agree: a
            // key goes to whichever of the two AppKit thinks has it.
            let focus = gtk::EventControllerFocus::new();
            focus.connect_enter(|focus| {
                if let Some(view) = focus.widget().and_downcast::<super::WebView>() {
                    view.take_keys(true);
                }
            });
            focus.connect_leave(|focus| {
                if let Some(view) = focus.widget().and_downcast::<super::WebView>() {
                    view.take_keys(false);
                }
            });
            widget.add_controller(focus);
        }

        fn dispose(&self) {
            if let Some(tick) = self.tick.take() {
                tick.remove();
            }
            if let Some(clip) = self.clip.take() {
                clip.removeFromSuperview();
            }
            if let Some(page) = self.page.take() {
                OWNERS.with(|owners| owners.borrow_mut().remove(&page_key(&page)));
                unsafe { page.setNavigationDelegate(None) };
            }
        }
    }

    impl WidgetImpl for WebView {
        fn map(&self) {
            self.parent_map();
            let tick = self.obj().add_tick_callback(|widget, _| {
                widget.follow();
                glib::ControlFlow::Continue
            });
            self.tick.replace(Some(tick));
        }

        fn unmap(&self) {
            if let Some(tick) = self.tick.take() {
                tick.remove();
            }
            if let Some(clip) = self.clip.borrow().as_ref() {
                clip.setHidden(true);
            }
            self.parent_unmap();
        }

        fn snapshot(&self, snapshot: &gtk::Snapshot) {
            // The page draws itself above GTK. Under it, the view's own
            // colour, so nothing flashes before the first page.
            if let Some(color) = self.background.borrow().as_ref() {
                let widget = self.obj();
                let bounds = gtk::graphene::Rect::new(0.0, 0.0, widget.width() as f32, widget.height() as f32);
                snapshot.append_color(color, &bounds);
            }
        }
    }
}

glib::wrapper! {
    pub struct WebView(ObjectSubclass<imp::WebView>)
        @extends gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl Default for WebView {
    fn default() -> WebView {
        WebView::new()
    }
}

/// webkit6's builder, for the three things the app sets.
#[derive(Default)]
pub struct WebViewBuilder {
    session: Option<NetworkSession>,
    content: Option<UserContentManager>,
    settings: Option<Settings>,
}

impl WebViewBuilder {
    pub fn network_session(mut self, session: &NetworkSession) -> WebViewBuilder {
        self.session = Some(session.clone());
        self
    }

    pub fn user_content_manager(mut self, content: &UserContentManager) -> WebViewBuilder {
        self.content = Some(content.clone());
        self
    }

    pub fn settings(mut self, settings: &Settings) -> WebViewBuilder {
        self.settings = Some(settings.clone());
        self
    }

    pub fn build(self) -> WebView {
        let view: WebView = glib::Object::new();
        let imp = view.imp();
        imp.session.replace(self.session);
        imp.content.replace(self.content);
        imp.settings.replace(self.settings);
        view
    }
}

/// The methods webkit6 offers on a web view, in its prelude as there.
pub trait WebViewExt {
    fn load_html(&self, content: &str, base_uri: Option<&str>);

    fn load_uri(&self, uri: &str);

    /// The address of the page on screen.
    fn uri(&self) -> Option<glib::GString>;

    /// Runs `script` in the page and hands back what it answered.
    ///
    /// WebKitGTK answers a script that ends in `undefined` with an
    /// undefined value; WKWebView calls that an error. Here it is the
    /// undefined value again, so scripts run for their effect behave the
    /// same on both.
    fn evaluate_javascript<P: FnOnce(Result<Value, glib::Error>) + 'static>(
        &self,
        script: &str,
        _world_name: Option<&str>,
        _source_uri: Option<&str>,
        _cancellable: Option<&impl IsA<gtk::gio::Cancellable>>,
        callback: P,
    );

    fn zoom_level(&self) -> f64;

    fn set_zoom_level(&self, zoom_level: f64);

    fn set_background_color(&self, rgba: &gdk::RGBA);

    /// WebKitGTK frees the memory of the process that draws pages this
    /// way. WKWebView has no public call for it; its process goes when the
    /// view does, and the app's view lives as long as its window.
    fn terminate_web_process(&self);

    fn find_controller(&self) -> Option<FindController>;

    fn connect_decide_policy<F: Fn(&WebView, &PolicyDecision, PolicyDecisionType) -> bool + 'static>(
        &self,
        f: F,
    );

    fn connect_load_changed<F: Fn(&WebView, LoadEvent) + 'static>(&self, f: F);

    fn connect_web_process_terminated<F: Fn(&WebView, WebProcessTerminationReason) + 'static>(
        &self,
        f: F,
    );

    /// Kept for webkit6's sake and never called; see [`ContextMenu`].
    fn connect_context_menu<F: Fn(&WebView, &ContextMenu, &HitTestResult) -> bool + 'static>(
        &self,
        _f: F,
    );

    /// [`WebView::evaluate_javascript`], awaited.
    fn evaluate_javascript_future(
        &self,
        script: &str,
        world_name: Option<&str>,
        source_uri: Option<&str>,
    ) -> Pin<Box<dyn Future<Output = Result<Value, glib::Error>> + 'static>>;

    fn connect_load_failed<F: Fn(&WebView, LoadEvent, &str, &glib::Error) -> bool + 'static>(&self, f: F);

    fn connect_script_dialog<F: Fn(&WebView, &ScriptDialog) -> bool + 'static>(&self, f: F);

    fn connect_permission_request<F: Fn(&WebView, &PermissionRequest) -> bool + 'static>(&self, f: F);

    fn connect_authenticate<F: Fn(&WebView, &AuthenticationRequest) -> bool + 'static>(&self, f: F);

    /// Kept for webkit6's sake and never called: a WKWebView with no UI
    /// delegate for it opens no window.
    fn connect_create<F: Fn(&WebView, &NavigationAction) -> Option<gtk::Widget> + 'static>(&self, _f: F);

    /// Kept for webkit6's sake and never called; see [`PermissionStateQuery`].
    fn connect_query_permission_state<F: Fn(&WebView, &PermissionStateQuery) -> bool + 'static>(
        &self,
        _f: F,
    );

    /// Kept for webkit6's sake and never called: a WKWebView with no UI
    /// delegate for it opens no file panel.
    fn connect_run_file_chooser<F: Fn(&WebView, &FileChooserRequest) -> bool + 'static>(&self, _f: F);

    /// Kept for webkit6's sake and never called; see [`Notification`].
    fn connect_show_notification<F: Fn(&WebView, &Notification) -> bool + 'static>(&self, _f: F);

    /// Kept for webkit6's sake and never called: a WKWebView with no UI
    /// delegate for it ignores the page's `print()`.
    fn connect_print<F: Fn(&WebView, &PrintOperation) -> bool + 'static>(&self, _f: F);
}

impl WebViewExt for WebView {
    fn load_html(&self, content: &str, base_uri: Option<&str>) {
        let page = self.page();
        let base = base_uri.and_then(|uri| NSURL::URLWithString(&NSString::from_str(uri)));
        self.imp().uri.replace(Some(base_uri.unwrap_or("about:blank").to_string()));
        unsafe { page.loadHTMLString_baseURL(&NSString::from_str(content), base.as_deref()) };
    }

    fn load_uri(&self, uri: &str) {
        let page = self.page();
        let Some(url) = NSURL::URLWithString(&NSString::from_str(uri)) else {
            return;
        };
        self.imp().uri.replace(Some(uri.to_string()));
        unsafe { page.loadRequest(&NSURLRequest::requestWithURL(&url)) };
    }

    fn uri(&self) -> Option<glib::GString> {
        let page = self.imp().page.borrow().clone();
        page.and_then(|page| unsafe { page.URL() })
            .and_then(|url| url.absoluteString())
            .map(|uri| glib::GString::from(uri.to_string()))
            .or_else(|| self.imp().uri.borrow().as_deref().map(glib::GString::from))
    }

    fn evaluate_javascript<P: FnOnce(Result<Value, glib::Error>) + 'static>(
        &self,
        script: &str,
        _world_name: Option<&str>,
        _source_uri: Option<&str>,
        _cancellable: Option<&impl IsA<gtk::gio::Cancellable>>,
        callback: P,
    ) {
        let page = self.page();
        let callback = Cell::new(Some(callback));
        let done = block2::RcBlock::new(move |result: *mut AnyObject, err: *mut NSError| {
            let Some(callback) = callback.take() else { return };
            match unsafe { err.as_ref() } {
                None => callback(Ok(Value::from_object(unsafe { result.as_ref() }))),
                Some(err) if unsupported_result(err) => callback(Ok(Value::undefined())),
                Some(err) => callback(Err(glib::Error::new(
                    gtk::gio::IOErrorEnum::Failed,
                    &err.localizedDescription().to_string(),
                ))),
            }
        });
        unsafe { page.evaluateJavaScript_completionHandler(&NSString::from_str(script), Some(&done)) };
    }

    fn zoom_level(&self) -> f64 {
        self.imp().zoom.get()
    }

    fn set_zoom_level(&self, zoom_level: f64) {
        self.imp().zoom.set(zoom_level);
        if let Some(page) = self.imp().page.borrow().as_ref() {
            unsafe { page.setPageZoom(zoom_level) };
        }
    }

    fn set_background_color(&self, rgba: &gdk::RGBA) {
        self.imp().background.replace(Some(*rgba));
        if let Some(page) = self.imp().page.borrow().as_ref() {
            paint(page, rgba);
        }
        self.queue_draw();
    }

    fn terminate_web_process(&self) {}

    fn find_controller(&self) -> Option<FindController> {
        Some(
            self.imp()
                .find
                .borrow_mut()
                .get_or_insert_with(|| FindController::new(self))
                .clone(),
        )
    }

    fn connect_decide_policy<F: Fn(&WebView, &PolicyDecision, PolicyDecisionType) -> bool + 'static>(
        &self,
        f: F,
    ) {
        self.imp().decide.borrow_mut().push(Rc::new(f));
    }

    fn connect_load_changed<F: Fn(&WebView, LoadEvent) + 'static>(&self, f: F) {
        self.imp().load.borrow_mut().push(Rc::new(f));
    }

    fn connect_web_process_terminated<F: Fn(&WebView, WebProcessTerminationReason) + 'static>(
        &self,
        f: F,
    ) {
        self.imp().terminated.borrow_mut().push(Rc::new(f));
    }

    fn connect_context_menu<F: Fn(&WebView, &ContextMenu, &HitTestResult) -> bool + 'static>(
        &self,
        _f: F,
    ) {
    }

    fn evaluate_javascript_future(
        &self,
        script: &str,
        world_name: Option<&str>,
        source_uri: Option<&str>,
    ) -> Pin<Box<dyn Future<Output = Result<Value, glib::Error>> + 'static>> {
        let (tell, told) = futures::channel::oneshot::channel();
        self.evaluate_javascript(script, world_name, source_uri, gtk::gio::Cancellable::NONE, move |answer| {
            let _ = tell.send(answer);
        });
        Box::pin(async move {
            told.await.unwrap_or_else(|_| {
                Err(glib::Error::new(gtk::gio::IOErrorEnum::Cancelled, "the view went away"))
            })
        })
    }

    fn connect_load_failed<F: Fn(&WebView, LoadEvent, &str, &glib::Error) -> bool + 'static>(&self, f: F) {
        self.imp().failed.borrow_mut().push(Rc::new(f));
    }

    fn connect_script_dialog<F: Fn(&WebView, &ScriptDialog) -> bool + 'static>(&self, f: F) {
        self.imp().dialogs.borrow_mut().push(Rc::new(f));
    }

    fn connect_permission_request<F: Fn(&WebView, &PermissionRequest) -> bool + 'static>(&self, f: F) {
        self.imp().permissions.borrow_mut().push(Rc::new(f));
    }

    fn connect_authenticate<F: Fn(&WebView, &AuthenticationRequest) -> bool + 'static>(&self, f: F) {
        self.imp().authenticate.borrow_mut().push(Rc::new(f));
    }

    fn connect_create<F: Fn(&WebView, &NavigationAction) -> Option<gtk::Widget> + 'static>(&self, _f: F) {}

    fn connect_query_permission_state<F: Fn(&WebView, &PermissionStateQuery) -> bool + 'static>(
        &self,
        _f: F,
    ) {
    }

    fn connect_run_file_chooser<F: Fn(&WebView, &FileChooserRequest) -> bool + 'static>(&self, _f: F) {}

    fn connect_show_notification<F: Fn(&WebView, &Notification) -> bool + 'static>(&self, _f: F) {}

    fn connect_print<F: Fn(&WebView, &PrintOperation) -> bool + 'static>(&self, _f: F) {}
}

impl WebView {
    pub fn new() -> WebView {
        WebView::builder().build()
    }

    pub fn builder() -> WebViewBuilder {
        WebViewBuilder::default()
    }

    /// Runs a script for its effect alone.
    pub(crate) fn run_quietly(&self, script: &str) {
        self.evaluate_javascript(script, None, None, gtk::gio::Cancellable::NONE, |_| {});
    }

    /// The page, made now if it was not yet.
    pub(crate) fn page(&self) -> Retained<Page> {
        if let Some(page) = self.imp().page.borrow().as_ref() {
            return page.clone();
        }
        let page = self.make_page(super::main_thread());
        self.imp().page.replace(Some(page.clone()));
        page
    }

    /// The NSWindow the view is drawn in, once it is on screen.
    pub(crate) fn native_window(&self) -> Option<Retained<NSWindow>> {
        let surface = self.native()?.surface()?;
        let window = unsafe { gdk_macos_surface_get_native_window(surface.to_glib_none().0) };
        unsafe { Retained::retain(window as *mut NSWindow) }
    }

    fn make_page(&self, mtm: MainThreadMarker) -> Retained<Page> {
        let imp = self.imp();
        unsafe {
            let config = WKWebViewConfiguration::new(mtm);
            if let Some(session) = imp.session.borrow().as_ref() {
                config.setWebsiteDataStore(&session.store());
            }
            let controller = WKUserContentController::new(mtm);
            let content = imp.content.borrow_mut().get_or_insert_with(UserContentManager::new).clone();
            content.fill(&controller);
            if cfg!(debug_assertions) {
                debug_probe().fill(&controller);
            }
            config.setUserContentController(&controller);
            let scripts = imp.settings.borrow().as_ref().is_none_or(Settings::page_scripts);
            config.defaultWebpagePreferences().setAllowsContentJavaScript(scripts);
            let (schemes, handler) = super::scheme::registered();
            for scheme in schemes {
                config.setURLSchemeHandler_forURLScheme(
                    Some(ProtocolObject::from_ref(&*handler)),
                    &NSString::from_str(&scheme),
                );
            }

            let page = Page::new(mtm, &config, self);
            let delegate = Navigator::new(mtm, self);
            page.setNavigationDelegate(Some(ProtocolObject::from_ref(&*delegate)));
            page.setUIDelegate(Some(ProtocolObject::from_ref(&*delegate)));
            page.setPageZoom(imp.zoom.get());
            if let Some(color) = imp.background.borrow().as_ref() {
                paint(&page, color);
            }
            imp.delegate.replace(Some(delegate));
            OWNERS.with(|owners| {
                let weak = glib::WeakRef::new();
                weak.set(Some(self));
                owners.borrow_mut().insert(page_key(&page), weak);
            });

            let clip = Clip::new(mtm);
            clip.addSubview(&page);
            clip.setHidden(true);
            imp.clip.replace(Some(clip));
            page
        }
    }

    /// Puts the page where the widget is now. Runs every frame while the
    /// widget is mapped; it changes nothing when nothing moved.
    fn follow(&self) {
        let imp = self.imp();
        if imp.page.borrow().is_none() {
            return;
        }
        let page = self.page();
        let Some(clip) = imp.clip.borrow().clone() else { return };
        let Some(window) = self.native_window() else {
            clip.setHidden(true);
            return;
        };
        let Some(content) = window.contentView() else { return };
        let address = Retained::as_ptr(&window) as usize;
        if imp.window.get() != address {
            clip.removeFromSuperview();
            content.addSubview(&clip);
            imp.window.set(address);
        }
        let Some(placement) = place::measure(self.upcast_ref()) else {
            self.log_placement(|| "hidden: no part shows".to_string());
            clip.setHidden(true);
            return;
        };
        let (dx, dy) = self.native().map(|native| native.surface_transform()).unwrap_or((0.0, 0.0));
        self.log_placement(|| {
            format!(
                "full {:?} visible {:?} surface ({dx}, {dy}) content {:?} flipped {} holes {:?}",
                placement.full,
                placement.visible,
                content.bounds().size,
                content.isFlipped(),
                placement.covers,
            )
        });
        let visible = &placement.visible;
        let (x, y) = (f64::from(visible.x()) + dx, f64::from(visible.y()) + dy);
        let (width, height) = (f64::from(visible.width()), f64::from(visible.height()));
        let y = if content.isFlipped() {
            y
        } else {
            content.bounds().size.height - y - height
        };
        let frame = place::rect(x, y, width, height);
        if clip.frame() != frame {
            clip.setFrame(frame);
        }
        // Inside the clip, which is flipped, the page keeps the widget's
        // whole size, shifted by however much of it is cut off.
        let full = &placement.full;
        let inner = place::rect(
            f64::from(full.x() - visible.x()),
            f64::from(full.y() - visible.y()),
            f64::from(full.width()),
            f64::from(full.height()),
        );
        if page.frame() != inner {
            page.setFrame(inner);
        }
        // The holes come in the window's coordinates; the clip wants its own.
        let (vx, vy) = (f64::from(visible.x()), f64::from(visible.y()));
        let holes = placement
            .holes
            .iter()
            .map(|hole| place::Hole {
                rect: place::rect(
                    hole.rect.origin.x - vx,
                    hole.rect.origin.y - vy,
                    hole.rect.size.width,
                    hole.rect.size.height,
                ),
                radius: hole.radius,
            })
            .collect();
        clip.set_holes(holes);
        if clip.isHidden() {
            clip.setHidden(false);
        }
    }

    /// Scrolls the page by a trackpad's `dx`, `dy`, gathered over a frame
    /// so the page hears once per frame rather than once per event. The
    /// page measures in CSS pixels, which the zoom level stands between.
    fn scroll_page_by(&self, dx: f64, dy: f64) {
        let imp = self.imp();
        let (x, y) = imp.scrolled.get();
        imp.scrolled.set((x + dx, y + dy));
        if imp.scroll_queued.replace(true) {
            return;
        }
        let weak = self.downgrade();
        glib::idle_add_local_once(move || {
            let Some(view) = weak.upgrade() else { return };
            let imp = view.imp();
            imp.scroll_queued.set(false);
            let (x, y) = imp.scrolled.replace((0.0, 0.0));
            let Some(page) = imp.page.borrow().clone() else { return };
            let zoom = imp.zoom.get().max(0.1);
            let script = format!("window.scrollBy({}, {})", x / zoom, y / zoom);
            unsafe { page.evaluateJavaScript_completionHandler(&NSString::from_str(&script), None) };
        });
    }

    /// Logs where the page went, at debug level, when it differs from the
    /// last line logged. A page drawn over a GTK dialog or left short of
    /// its widget is a wrong placement, and this is what shows which.
    fn log_placement(&self, line: impl FnOnce() -> String) {
        if !tracing::enabled!(tracing::Level::DEBUG) {
            return;
        }
        let line = line();
        if *self.imp().logged.borrow() == line {
            return;
        }
        let view = self.as_ptr() as usize;
        tracing::debug!(view = format!("{view:#x}"), "web view placed: {line}");
        self.imp().logged.replace(line);
    }

    /// Gives the keys to the page, or takes them back for GTK, when GTK's
    /// focus enters or leaves the view.
    fn take_keys(&self, page_has_focus: bool) {
        let Some(page) = self.imp().page.borrow().clone() else { return };
        let Some(window) = self.native_window() else { return };
        let first = window.firstResponder();
        let page_is_first = first.as_deref().is_some_and(|first| {
            let first: &NSObject = first;
            let whole: &NSObject = &page;
            std::ptr::eq(first, whole) || is_inside(first, &page)
        });
        if page_has_focus && !page_is_first {
            window.makeFirstResponder(Some(&page));
        } else if !page_has_focus && page_is_first {
            let content = window.contentView();
            window.makeFirstResponder(content.as_deref().map(|view| -> &objc2_app_kit::NSResponder { view }));
        }
    }

    fn decide(&self, decision: &PolicyDecision, kind: PolicyDecisionType) -> bool {
        let handlers: Vec<DecideHandler> = self.imp().decide.borrow().clone();
        let mut handled = false;
        for handler in handlers {
            if handler(self, decision, kind) {
                handled = true;
                break;
            }
        }
        decision.allowed(handled)
    }

    fn loaded(&self, event: LoadEvent) {
        let handlers: Vec<LoadHandler> = self.imp().load.borrow().clone();
        for handler in handlers {
            handler(self, event);
        }
    }

    fn lost_process(&self) {
        let handlers: Vec<TerminatedHandler> = self.imp().terminated.borrow().clone();
        for handler in handlers {
            handler(self, WebProcessTerminationReason::Crashed);
        }
    }

    fn load_failed(&self, event: LoadEvent, err: &NSError) {
        let error = glib::Error::new(gtk::gio::IOErrorEnum::Failed, &err.localizedDescription().to_string());
        let uri = self.uri().map(|uri| uri.to_string()).unwrap_or_default();
        let handlers: Vec<FailedHandler> = self.imp().failed.borrow().clone();
        for handler in handlers {
            if handler(self, event, &uri, &error) {
                break;
            }
        }
    }

    fn script_dialog(&self, dialog: &ScriptDialog) {
        let handlers: Vec<DialogHandler> = self.imp().dialogs.borrow().clone();
        for handler in handlers {
            if handler(self, dialog) {
                break;
            }
        }
    }

    /// Whether the page may have the camera or microphone. Nobody asked
    /// is no.
    fn permitted(&self) -> bool {
        let request = PermissionRequest::new();
        let handlers: Vec<PermissionHandler> = self.imp().permissions.borrow().clone();
        for handler in handlers {
            if handler(self, &request) {
                break;
            }
        }
        request.allowed()
    }

    /// Whether anyone handles password requests on this view.
    fn asks_for_passwords(&self) -> bool {
        !self.imp().authenticate.borrow().is_empty()
    }

    /// Whether a handler turned a password request down. With no handler
    /// the system decides, which with no saved password is no.
    fn refuses_password(&self) -> bool {
        let request = AuthenticationRequest::new();
        let handlers: Vec<AuthenticateHandler> = self.imp().authenticate.borrow().clone();
        for handler in handlers {
            if handler(self, &request) {
                break;
            }
        }
        request.cancelled()
    }
}

/// What a debug build's pages report: content the page's policy blocked and
/// pictures that did not load, with their addresses. WKWebView keeps its
/// console to itself unless the Web Inspector is attached, so the page says
/// these through a message handler instead, and they land in the log.
const PROBE: &str = r#"(function () {
  var say = function (what) {
    try { window.webkit.messageHandlers.mailrsProbe.postMessage(what); } catch (e) {}
  };
  document.addEventListener('securitypolicyviolation', function (e) {
    say('blocked by ' + e.violatedDirective + ': ' + e.blockedURI);
  }, true);
  window.addEventListener('load', function () {
    var roots = [document];
    document.querySelectorAll('*').forEach(function (el) { if (el.shadowRoot) roots.push(el.shadowRoot); });
    roots.forEach(function (root) {
      root.querySelectorAll('img').forEach(function (img) {
        if (img.complete && img.naturalWidth === 0 && img.currentSrc) say('picture did not load: ' + img.currentSrc);
      });
    });
  });
})()"#;

/// The probe's scripts and handler, shared by every page of a debug build.
fn debug_probe() -> UserContentManager {
    thread_local! {
        static PROBE_CONTENT: UserContentManager = {
            let content = UserContentManager::new();
            content.add_script(&super::content::UserScript::new(
                PROBE,
                super::content::UserContentInjectedFrames::TopFrame,
                super::content::UserScriptInjectionTime::Start,
                &[],
                &[],
            ));
            content.register_script_message_handler("mailrsProbe", None);
            content.connect_script_message_received(Some("mailrsProbe"), |_, said| {
                tracing::warn!(page = %said.to_str(), "web view");
            });
            content
        };
    }
    PROBE_CONTENT.with(Clone::clone)
}

/// Whether `challenge` is the server's certificate being checked, which is
/// the system's to decide, rather than a site asking for a password.
///
/// WebKit hands the delegate a WKNSURLAuthenticationChallenge, a proxy that
/// forwards each message to the real challenge. objc2's debug check looks
/// `protectionSpace` up on the proxy's own class, finds nothing and aborts,
/// so that one message goes out through `objc_msgSend` itself, which the
/// forwarding answers. The protection space that comes back is a real one.
fn is_server_trust(challenge: &NSURLAuthenticationChallenge) -> bool {
    type Getter = unsafe extern "C-unwind" fn(*const NSURLAuthenticationChallenge, Sel) -> *mut NSURLProtectionSpace;
    let space = unsafe {
        let send: Getter = std::mem::transmute(objc2::ffi::objc_msgSend as unsafe extern "C-unwind" fn());
        Retained::retain(send(challenge, sel!(protectionSpace)))
    };
    space.is_some_and(|space| space.authenticationMethod().to_string() == "NSURLAuthenticationMethodServerTrust")
}

/// Whether `responder` is a view somewhere inside `page`. WKWebView hands
/// the keys to views of its own inside it.
fn is_inside(responder: &NSObject, page: &Page) -> bool {
    let Some(view) = responder.downcast_ref::<NSView>() else {
        return false;
    };
    view.isDescendantOf(page)
}

/// WKWebView's errors for a script whose answer it cannot hand over, such
/// as `undefined`: WKErrorJavaScriptResultTypeIsUnsupported.
fn unsupported_result(err: &NSError) -> bool {
    err.domain().to_string() == "WKErrorDomain" && err.code() == 5
}

/// Gives the page `rgba` behind its content, and drops WKWebView's own
/// white, so a dark page never flashes white while it loads.
fn paint(page: &WKWebView, rgba: &gdk::RGBA) {
    unsafe {
        let color = NSColor::colorWithSRGBRed_green_blue_alpha(
            f64::from(rgba.red()),
            f64::from(rgba.green()),
            f64::from(rgba.blue()),
            f64::from(rgba.alpha()),
        );
        page.setUnderPageBackgroundColor(Some(&color));
        let no = objc2_foundation::NSNumber::new_bool(false);
        let _: () = msg_send![page, setValue: &*no, forKey: &*NSString::from_str("drawsBackground")];
    }
}

pub(crate) struct PageIvars {
    owner: glib::WeakRef<WebView>,
}

define_class!(
    /// WKWebView, telling its widget when AppKit gives it the keys.
    #[unsafe(super(WKWebView, NSView, objc2_app_kit::NSResponder, NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "MailrsPage"]
    #[ivars = PageIvars]
    pub(crate) struct Page;

    impl Page {
        #[unsafe(method(becomeFirstResponder))]
        fn become_first_responder(&self) -> bool {
            let took: bool = unsafe { msg_send![super(self), becomeFirstResponder] };
            if took && let Some(owner) = self.ivars().owner.upgrade() {
                // After AppKit is done, so GTK's focus change does not
                // reach back into it.
                glib::idle_add_local_once(move || {
                    if !owner.has_focus() {
                        owner.grab_focus();
                    }
                });
            }
            took
        }
    }
);

impl Page {
    fn new(mtm: MainThreadMarker, config: &WKWebViewConfiguration, owner: &WebView) -> Retained<Page> {
        let weak = glib::WeakRef::new();
        weak.set(Some(owner));
        let this = Page::alloc(mtm).set_ivars(PageIvars { owner: weak });
        let frame = place::rect(0.0, 0.0, 1.0, 1.0);
        unsafe { msg_send![super(this), initWithFrame: frame, configuration: config] }
    }
}

pub(crate) struct NavigatorIvars {
    owner: glib::WeakRef<WebView>,
}

define_class!(
    /// Asks the widget's handlers where the page may go, and tells them
    /// how a load is going.
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "MailrsNavigator"]
    #[ivars = NavigatorIvars]
    pub(crate) struct Navigator;

    unsafe impl NSObjectProtocol for Navigator {}

    unsafe impl WKNavigationDelegate for Navigator {
        #[unsafe(method(webView:decidePolicyForNavigationAction:decisionHandler:))]
        fn decide_policy(
            &self,
            _page: &WKWebView,
            action: &WKNavigationAction,
            decision_handler: &block2::DynBlock<dyn Fn(WKNavigationActionPolicy)>,
        ) {
            let allowed = match self.ivars().owner.upgrade() {
                None => false,
                Some(owner) => {
                    let uri = unsafe { action.request().URL() }
                        .and_then(|url| url.absoluteString())
                        .map(|uri| uri.to_string())
                        .unwrap_or_default();
                    // A link that asks for a new window has no frame to
                    // load into, which is how webkit6 tells the two apart.
                    let kind = match unsafe { action.targetFrame() } {
                        Some(_) => PolicyDecisionType::NavigationAction,
                        None => PolicyDecisionType::NewWindowAction,
                    };
                    owner.decide(&PolicyDecision::navigation(uri), kind)
                }
            };
            let policy = match allowed {
                true => WKNavigationActionPolicy::Allow,
                false => WKNavigationActionPolicy::Cancel,
            };
            decision_handler.call((policy,));
        }

        #[unsafe(method(webView:didStartProvisionalNavigation:))]
        fn did_start(&self, _page: &WKWebView, _navigation: Option<&WKNavigation>) {
            if let Some(owner) = self.ivars().owner.upgrade() {
                owner.loaded(LoadEvent::Started);
            }
        }

        #[unsafe(method(webView:didCommitNavigation:))]
        fn did_commit(&self, _page: &WKWebView, _navigation: Option<&WKNavigation>) {
            if let Some(owner) = self.ivars().owner.upgrade() {
                owner.loaded(LoadEvent::Committed);
            }
        }

        #[unsafe(method(webView:didFinishNavigation:))]
        fn did_finish(&self, _page: &WKWebView, _navigation: Option<&WKNavigation>) {
            if let Some(owner) = self.ivars().owner.upgrade() {
                owner.loaded(LoadEvent::Finished);
            }
        }

        #[unsafe(method(webViewWebContentProcessDidTerminate:))]
        fn did_terminate(&self, _page: &WKWebView) {
            if let Some(owner) = self.ivars().owner.upgrade() {
                owner.lost_process();
            }
        }

        #[unsafe(method(webView:didFailProvisionalNavigation:withError:))]
        fn did_fail_early(&self, _page: &WKWebView, _navigation: Option<&WKNavigation>, error: &NSError) {
            if let Some(owner) = self.ivars().owner.upgrade() {
                owner.load_failed(LoadEvent::Started, error);
            }
        }

        #[unsafe(method(webView:didFailNavigation:withError:))]
        fn did_fail(&self, _page: &WKWebView, _navigation: Option<&WKNavigation>, error: &NSError) {
            if let Some(owner) = self.ivars().owner.upgrade() {
                owner.load_failed(LoadEvent::Committed, error);
            }
        }

        #[unsafe(method(webView:decidePolicyForNavigationResponse:decisionHandler:))]
        fn decide_response(
            &self,
            _page: &WKWebView,
            response: &WKNavigationResponse,
            decision_handler: &block2::DynBlock<dyn Fn(WKNavigationResponsePolicy)>,
        ) {
            let allowed = match self.ivars().owner.upgrade() {
                None => false,
                Some(owner) => {
                    let shows = unsafe { response.canShowMIMEType() };
                    owner.decide(&PolicyDecision::response(shows), PolicyDecisionType::Response)
                }
            };
            let policy = match allowed {
                true => WKNavigationResponsePolicy::Allow,
                false => WKNavigationResponsePolicy::Cancel,
            };
            decision_handler.call((policy,));
        }

        /// A password request goes to the widget's handlers. The server's
        /// certificate is the system's to check, as in WebKitGTK, where
        /// that check never reaches the app either.
        #[unsafe(method(webView:didReceiveAuthenticationChallenge:completionHandler:))]
        fn authenticate(
            &self,
            _page: &WKWebView,
            challenge: &NSURLAuthenticationChallenge,
            completion_handler: &block2::DynBlock<
                dyn Fn(NSURLSessionAuthChallengeDisposition, *mut NSURLCredential),
            >,
        ) {
            // Only a view with password handlers looks at the challenge at
            // all; the mail views leave every challenge to the system.
            let refuse = match self.ivars().owner.upgrade() {
                Some(owner) if owner.asks_for_passwords() => {
                    !is_server_trust(challenge) && owner.refuses_password()
                }
                Some(_) => false,
                None => true,
            };
            let disposition = match refuse {
                true => NSURLSessionAuthChallengeDisposition::CancelAuthenticationChallenge,
                false => NSURLSessionAuthChallengeDisposition::PerformDefaultHandling,
            };
            completion_handler.call((disposition, std::ptr::null_mut()));
        }
    }

    unsafe impl WKUIDelegate for Navigator {
        #[unsafe(method(webView:runJavaScriptAlertPanelWithMessage:initiatedByFrame:completionHandler:))]
        fn alert(
            &self,
            _page: &WKWebView,
            message: &NSString,
            _frame: &WKFrameInfo,
            completion_handler: &block2::DynBlock<dyn Fn()>,
        ) {
            if let Some(owner) = self.ivars().owner.upgrade() {
                owner.script_dialog(&ScriptDialog::new(ScriptDialogType::Alert, message.to_string()));
            }
            completion_handler.call(());
        }

        #[unsafe(method(webView:runJavaScriptConfirmPanelWithMessage:initiatedByFrame:completionHandler:))]
        fn confirm(
            &self,
            _page: &WKWebView,
            message: &NSString,
            _frame: &WKFrameInfo,
            completion_handler: &block2::DynBlock<dyn Fn(Bool)>,
        ) {
            let dialog = ScriptDialog::new(ScriptDialogType::Confirm, message.to_string());
            if let Some(owner) = self.ivars().owner.upgrade() {
                owner.script_dialog(&dialog);
            }
            completion_handler.call((Bool::new(dialog.confirmed()),));
        }

        /// A prompt is shown to the handlers and answered with nothing,
        /// as a dialog nobody fills in would be.
        #[unsafe(method(webView:runJavaScriptTextInputPanelWithPrompt:defaultText:initiatedByFrame:completionHandler:))]
        fn prompt(
            &self,
            _page: &WKWebView,
            prompt: &NSString,
            _default_text: Option<&NSString>,
            _frame: &WKFrameInfo,
            completion_handler: &block2::DynBlock<dyn Fn(*mut NSString)>,
        ) {
            if let Some(owner) = self.ivars().owner.upgrade() {
                owner.script_dialog(&ScriptDialog::new(ScriptDialogType::Prompt, prompt.to_string()));
            }
            completion_handler.call((std::ptr::null_mut(),));
        }

        #[unsafe(method(webView:requestMediaCapturePermissionForOrigin:initiatedByFrame:type:decisionHandler:))]
        fn media_capture(
            &self,
            _page: &WKWebView,
            _origin: &WKSecurityOrigin,
            _frame: &WKFrameInfo,
            _kind: WKMediaCaptureType,
            decision_handler: &block2::DynBlock<dyn Fn(WKPermissionDecision)>,
        ) {
            let allowed = self.ivars().owner.upgrade().is_some_and(|owner| owner.permitted());
            let decision = match allowed {
                true => WKPermissionDecision::Grant,
                false => WKPermissionDecision::Deny,
            };
            decision_handler.call((decision,));
        }
    }
);

impl Navigator {
    fn new(mtm: MainThreadMarker, owner: &WebView) -> Retained<Navigator> {
        let weak = glib::WeakRef::new();
        weak.set(Some(owner));
        let this = Navigator::alloc(mtm).set_ivars(NavigatorIvars { owner: weak });
        unsafe { msg_send![super(this), init] }
    }
}
