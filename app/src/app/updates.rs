//! Updates come through Sparkle, which the DMG's bundle carries: it keeps
//! its own daily schedule and shows its own window. Iris hands it the
//! person's Check for Updates choice and a way to ask at once. A copy run
//! from a cargo build has no Sparkle and never updates.

use std::rc::Rc;

use gtk::gio;
use gtk::prelude::*;

use super::App;

impl App {
    /// Offers `app.check-for-updates` and gives Sparkle the person's choice,
    /// when this copy updates through it.
    pub(super) fn start_update_checks(self: &Rc<Self>) {
        if !self.can_update() {
            return;
        }
        let action = gio::SimpleAction::new("check-for-updates", None);
        action.connect_activate(|_, _| crate::sparkle::check_now());
        self.gio.add_action(&action);
        self.sync_update_schedule();
    }

    /// Gives Sparkle the person's Check for Updates choice.
    pub(crate) fn sync_update_schedule(&self) {
        if self.can_update() {
            crate::sparkle::set_automatic(self.settings().check_for_updates);
        }
    }

    /// Whether this copy updates itself: the demo and a cargo build do not.
    pub fn can_update(&self) -> bool {
        !self.core.demo && crate::sparkle::available()
    }
}
