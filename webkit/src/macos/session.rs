use std::cell::RefCell;
use std::rc::Rc;

use objc2::rc::Retained;
use objc2_web_kit::WKWebsiteDataStore;

/// A set of cookies and caches shared by the views built with it.
/// Ephemeral, as every session the app makes is: it lives in memory and
/// nothing reaches the disk.
#[derive(Clone)]
pub struct NetworkSession(Rc<RefCell<Option<Retained<WKWebsiteDataStore>>>>);

impl NetworkSession {
    pub fn new_ephemeral() -> NetworkSession {
        NetworkSession(Rc::new(RefCell::new(None)))
    }

    /// The data store behind the session, made the first time a view
    /// needs it.
    /// Kept for webkit6's sake and never called; see [`super::Download`].
    pub fn connect_download_started<F: Fn(&NetworkSession, &super::Download) + 'static>(&self, _f: F) {}

    pub(crate) fn store(&self) -> Retained<WKWebsiteDataStore> {
        self.0
            .borrow_mut()
            .get_or_insert_with(|| unsafe { WKWebsiteDataStore::nonPersistentDataStore(super::main_thread()) })
            .clone()
    }
}
