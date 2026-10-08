//! Updates on macOS, through Sparkle.
//!
//! The DMG's bundle carries Sparkle.framework (scripts/macos-dmg.sh) and
//! names its feed and public key in Info.plist. Sparkle checks the feed on
//! its own schedule, asks before installing, verifies each download against
//! the public key and replaces the app. Iris only starts it and adds
//! Check for Updates… to the Iris menu. A bundle without the framework,
//! such as the one macos-install.sh builds, starts nothing and shows no item.

use std::cell::RefCell;

use objc2::rc::{Allocated, Retained};
use objc2::runtime::{AnyClass, AnyObject, Bool};
use objc2::{class, msg_send, sel};
use objc2_foundation::NSString;

thread_local! {
    /// Sparkle's controller, kept for the life of the app: it owns the
    /// updater and its schedule, and the menu item does not retain it.
    static CONTROLLER: RefCell<Option<Retained<AnyObject>>> = const { RefCell::new(None) };
}

/// Loads Sparkle from the bundle and starts its updater. Answers whether
/// it is running. Call it on the main thread once the app has started.
pub fn start() -> bool {
    if CONTROLLER.with(|c| c.borrow().is_some()) {
        return true;
    }
    let Some(framework) = framework_path() else {
        return false;
    };
    unsafe {
        let path = NSString::from_str(&framework.to_string_lossy());
        let bundle: Option<Retained<AnyObject>> = msg_send![class!(NSBundle), bundleWithPath: &*path];
        let Some(bundle) = bundle else {
            tracing::warn!(path = %framework.display(), "Sparkle.framework is not a bundle");
            return false;
        };
        let loaded: Bool = msg_send![&*bundle, load];
        if !loaded.as_bool() {
            tracing::warn!("Sparkle.framework would not load");
            return false;
        }
        let Some(class) = AnyClass::get(c"SPUStandardUpdaterController") else {
            tracing::warn!("Sparkle loaded without its updater controller");
            return false;
        };
        let allocated: Allocated<AnyObject> = msg_send![class, alloc];
        let none: *mut AnyObject = std::ptr::null_mut();
        let controller: Option<Retained<AnyObject>> = msg_send![
            allocated,
            initWithStartingUpdater: Bool::YES,
            updaterDelegate: none,
            userDriverDelegate: none
        ];
        let Some(controller) = controller else {
            tracing::warn!("Sparkle's updater did not start");
            return false;
        };
        CONTROLLER.with(|c| c.replace(Some(controller)));
    }
    true
}

/// Puts Check for Updates… under About in the Iris menu, aimed at
/// Sparkle's controller, which enables it while no check is running.
pub fn add_menu_item(title: &str) {
    CONTROLLER.with(|c| {
        let Some(controller) = c.borrow().clone() else { return };
        unsafe {
            let app: Retained<AnyObject> = msg_send![class!(NSApplication), sharedApplication];
            let main_menu: Option<Retained<AnyObject>> = msg_send![&*app, mainMenu];
            let Some(main_menu) = main_menu else { return };
            let count: isize = msg_send![&*main_menu, numberOfItems];
            if count == 0 {
                return;
            }
            let first: Retained<AnyObject> = msg_send![&*main_menu, itemAtIndex: 0isize];
            let app_menu: Option<Retained<AnyObject>> = msg_send![&*first, submenu];
            let Some(app_menu) = app_menu else { return };
            let title = NSString::from_str(title);
            let key = NSString::from_str("");
            let allocated: Allocated<AnyObject> = msg_send![class!(NSMenuItem), alloc];
            let item: Retained<AnyObject> = msg_send![
                allocated,
                initWithTitle: &*title,
                action: sel!(checkForUpdates:),
                keyEquivalent: &*key
            ];
            let _: () = msg_send![&*item, setTarget: &*controller];
            // Under About, which GTK puts first.
            let _: () = msg_send![&*app_menu, insertItem: &*item, atIndex: 1isize];
        }
    });
}

/// Whether this bundle carries Sparkle, and so updates through it.
pub fn available() -> bool {
    framework_path().is_some()
}

/// Checks the feed now and shows Sparkle's own window with the answer,
/// as Check for Updates… in the Iris menu does.
pub fn check_now() {
    if !start() {
        return;
    }
    CONTROLLER.with(|c| {
        if let Some(controller) = c.borrow().as_ref() {
            let none: *mut AnyObject = std::ptr::null_mut();
            unsafe {
                let _: () = msg_send![&**controller, checkForUpdates: none];
            }
        }
    });
}

/// Turns Sparkle's daily check on or off, as the switch in Preferences says.
pub fn set_automatic(on: bool) {
    if !start() {
        return;
    }
    CONTROLLER.with(|c| {
        if let Some(controller) = c.borrow().as_ref() {
            unsafe {
                let updater: Option<Retained<AnyObject>> = msg_send![&**controller, updater];
                if let Some(updater) = updater {
                    let _: () = msg_send![&*updater, setAutomaticallyChecksForUpdates: Bool::new(on)];
                }
            }
        }
    });
}

/// Sparkle.framework inside the running bundle, if it carries one.
fn framework_path() -> Option<std::path::PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let framework = exe.parent()?.parent()?.join("Frameworks/Sparkle.framework");
    framework.is_dir().then_some(framework)
}
