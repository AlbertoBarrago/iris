use std::cell::RefCell;
use std::rc::{Rc, Weak};

use objc2::rc::Retained;
use objc2::runtime::{NSObject, ProtocolObject};
use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send};
use objc2_foundation::{NSObjectProtocol, NSString};
use objc2_web_kit::{
    WKScriptMessage, WKScriptMessageHandler, WKUserContentController, WKUserScript,
    WKUserScriptInjectionTime,
};

use super::filter::UserContentFilter;
use super::value::Value;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UserContentInjectedFrames {
    AllFrames,
    TopFrame,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UserScriptInjectionTime {
    Start,
    End,
}

/// A script every page of a view runs as it loads.
#[derive(Clone)]
pub struct UserScript {
    source: String,
    frames: UserContentInjectedFrames,
    time: UserScriptInjectionTime,
}

impl UserScript {
    /// webkit6's allow and block lists limit a script to some addresses.
    /// The app passes none, so they are not carried over.
    pub fn new(
        source: &str,
        injected_frames: UserContentInjectedFrames,
        injection_time: UserScriptInjectionTime,
        _allow_list: &[&str],
        _block_list: &[&str],
    ) -> UserScript {
        UserScript {
            source: source.to_string(),
            frames: injected_frames,
            time: injection_time,
        }
    }

    fn native(&self, mtm: MainThreadMarker) -> Retained<WKUserScript> {
        let time = match self.time {
            UserScriptInjectionTime::Start => WKUserScriptInjectionTime::AtDocumentStart,
            UserScriptInjectionTime::End => WKUserScriptInjectionTime::AtDocumentEnd,
        };
        unsafe {
            WKUserScript::initWithSource_injectionTime_forMainFrameOnly(
                WKUserScript::alloc(mtm),
                &NSString::from_str(&self.source),
                time,
                self.frames == UserContentInjectedFrames::TopFrame,
            )
        }
    }
}

type Listener = Rc<dyn Fn(&UserContentManager, &Value)>;

/// Scripts, message handlers and content filters shared by the views
/// built with it.
///
/// WKWebView takes these through a `WKUserContentController` that belongs
/// to one view, fixed when the view is made. This manager keeps the list
/// itself, fills each view's controller from it, then passes on whatever
/// is added later to every controller it filled.
#[derive(Clone)]
pub struct UserContentManager(Rc<Inner>);

struct Inner {
    scripts: RefCell<Vec<UserScript>>,
    handlers: RefCell<Vec<String>>,
    listeners: RefCell<Vec<(Option<String>, Listener)>>,
    filters: RefCell<Vec<UserContentFilter>>,
    controllers: RefCell<Vec<Retained<WKUserContentController>>>,
    bridge: RefCell<Option<Retained<Bridge>>>,
}

impl Default for UserContentManager {
    fn default() -> UserContentManager {
        UserContentManager::new()
    }
}

impl UserContentManager {
    pub fn new() -> UserContentManager {
        UserContentManager(Rc::new(Inner {
            scripts: RefCell::new(Vec::new()),
            handlers: RefCell::new(Vec::new()),
            listeners: RefCell::new(Vec::new()),
            filters: RefCell::new(Vec::new()),
            controllers: RefCell::new(Vec::new()),
            bridge: RefCell::new(None),
        }))
    }

    pub fn add_script(&self, script: &UserScript) {
        let mtm = super::main_thread();
        for controller in self.0.controllers.borrow().iter() {
            unsafe { controller.addUserScript(&script.native(mtm)) };
        }
        self.0.scripts.borrow_mut().push(script.clone());
    }

    /// Lets the page post to `window.webkit.messageHandlers.<name>`, the
    /// same object WKWebView offers. Answers false when the name is taken,
    /// as webkit6 does.
    pub fn register_script_message_handler(&self, name: &str, _world_name: Option<&str>) -> bool {
        if self.0.handlers.borrow().iter().any(|known| known == name) {
            return false;
        }
        let bridge = self.bridge();
        for controller in self.0.controllers.borrow().iter() {
            unsafe {
                controller.addScriptMessageHandler_name(
                    ProtocolObject::from_ref(&*bridge),
                    &NSString::from_str(name),
                )
            };
        }
        self.0.handlers.borrow_mut().push(name.to_string());
        true
    }

    /// Calls `f` for each message posted to the handler `detail` names,
    /// or to any handler when it names none.
    pub fn connect_script_message_received<F: Fn(&UserContentManager, &Value) + 'static>(
        &self,
        detail: Option<&str>,
        f: F,
    ) {
        self.0
            .listeners
            .borrow_mut()
            .push((detail.map(str::to_string), Rc::new(f)));
    }

    pub fn add_filter(&self, filter: &UserContentFilter) {
        for controller in self.0.controllers.borrow().iter() {
            unsafe { controller.addContentRuleList(filter.native()) };
        }
        self.0.filters.borrow_mut().push(filter.clone());
    }

    pub fn remove_all_filters(&self) {
        for controller in self.0.controllers.borrow().iter() {
            unsafe { controller.removeAllContentRuleLists() };
        }
        self.0.filters.borrow_mut().clear();
    }

    /// Fills a new view's controller with everything added so far, and
    /// keeps it to pass on what comes later.
    pub(crate) fn fill(&self, controller: &Retained<WKUserContentController>) {
        let mtm = super::main_thread();
        for script in self.0.scripts.borrow().iter() {
            unsafe { controller.addUserScript(&script.native(mtm)) };
        }
        let bridge = self.bridge();
        for name in self.0.handlers.borrow().iter() {
            unsafe {
                controller.addScriptMessageHandler_name(
                    ProtocolObject::from_ref(&*bridge),
                    &NSString::from_str(name),
                )
            };
        }
        for filter in self.0.filters.borrow().iter() {
            unsafe { controller.addContentRuleList(filter.native()) };
        }
        self.0.controllers.borrow_mut().push(controller.clone());
    }

    fn bridge(&self) -> Retained<Bridge> {
        self.0
            .bridge
            .borrow_mut()
            .get_or_insert_with(|| Bridge::new(super::main_thread(), Rc::downgrade(&self.0)))
            .clone()
    }

    fn deliver(inner: &Rc<Inner>, name: &str, value: &Value) {
        let manager = UserContentManager(Rc::clone(inner));
        // Copied out first: a listener may add scripts or handlers.
        let listeners: Vec<Listener> = inner
            .listeners
            .borrow()
            .iter()
            .filter(|(detail, _)| detail.as_deref().is_none_or(|detail| detail == name))
            .map(|(_, listener)| Rc::clone(listener))
            .collect();
        for listener in listeners {
            listener(&manager, value);
        }
    }
}

pub(crate) struct BridgeIvars {
    manager: Weak<Inner>,
}

define_class!(
    /// Takes the page's messages for every handler of one manager.
    /// WKUserContentController holds it strongly, so it holds the manager
    /// weakly.
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "MailrsScriptMessageBridge"]
    #[ivars = BridgeIvars]
    pub(crate) struct Bridge;

    unsafe impl NSObjectProtocol for Bridge {}

    unsafe impl WKScriptMessageHandler for Bridge {
        #[unsafe(method(userContentController:didReceiveScriptMessage:))]
        fn did_receive(&self, _controller: &WKUserContentController, message: &WKScriptMessage) {
            let Some(manager) = self.ivars().manager.upgrade() else {
                return;
            };
            let name = unsafe { message.name() }.to_string();
            let body = unsafe { message.body() };
            UserContentManager::deliver(&manager, &name, &Value::from_object(Some(&body)));
        }
    }
);

impl Bridge {
    fn new(mtm: MainThreadMarker, manager: Weak<Inner>) -> Retained<Bridge> {
        let this = Bridge::alloc(mtm).set_ivars(BridgeIvars { manager });
        unsafe { msg_send![super(this), init] }
    }
}
