//! The sync engine that runs now, which the mail modules look accounts up
//! in. Changing the sync settings replaces it, so the modules hold this
//! rather than one engine.

use std::sync::{Arc, Mutex};

use mailrs_domain::AccountId;
use mailrs_sync::{AccountSync, Accounts, SyncEngine};

#[derive(Default)]
pub struct RunningEngine(Mutex<Option<Arc<SyncEngine>>>);

impl RunningEngine {
    pub fn current(&self) -> Option<Arc<SyncEngine>> {
        self.lock().clone()
    }

    /// Puts `engine` in place of the one running, and hands that one back.
    pub fn replace(&self, engine: Option<Arc<SyncEngine>>) -> Option<Arc<SyncEngine>> {
        std::mem::replace(&mut *self.lock(), engine)
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Option<Arc<SyncEngine>>> {
        self.0.lock().expect("engine lock poisoned")
    }
}

impl Accounts for RunningEngine {
    fn account(&self, account_id: AccountId) -> Option<Arc<AccountSync>> {
        self.current()?.account(account_id).ok()
    }
}
