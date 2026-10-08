//! A strip along one edge of a pane that the pointer drags to make the
//! pane wider or narrower. libadwaita's split views give a pane a width
//! between a least and a most and offer nothing to drag, so the window
//! lays this over the pane's edge and sets both bounds to the width the
//! drag reaches.

use std::cell::Cell;
use std::rc::Rc;

use adw::prelude::*;
use gtk::{gdk, glib};

use super::room::Room;
use crate::app::App;
use crate::settings::{Change, Pane};

/// Width of the strip, in pixels. Wide enough to find with the pointer,
/// narrow enough to leave the pane's own controls alone.
const GRIP: i32 = 6;

/// A pane wrapped in an overlay that carries the strip on one edge.
pub struct PaneEdge {
    /// What goes into the split view in place of the pane.
    pub overlay: gtk::Overlay,
    strip: gtk::Separator,
    /// +1 when dragging right widens the pane (strip on its end edge),
    /// -1 when dragging left does (strip on its start edge).
    sign: i32,
}

impl PaneEdge {
    /// Wraps `pane`, with the strip on its `side` edge: `End` for a pane
    /// on the window's left, `Start` for one on its right. `name` is what
    /// a screen reader calls the strip.
    pub fn around(pane: &impl IsA<gtk::Widget>, side: gtk::PackType, name: &str) -> PaneEdge {
        let (halign, sign) = match side {
            gtk::PackType::Start => (gtk::Align::Start, -1),
            _ => (gtk::Align::End, 1),
        };
        let strip = gtk::Separator::builder()
            .orientation(gtk::Orientation::Vertical)
            .halign(halign)
            .width_request(GRIP)
            .css_classes(["pane-edge"])
            .cursor(&gdk::Cursor::from_name("col-resize", None).expect("col-resize is a CSS cursor name"))
            .build();
        crate::ui::name(&strip, name);
        let overlay = gtk::Overlay::builder().child(pane).build();
        overlay.add_overlay(&strip);
        PaneEdge {
            overlay,
            strip,
            sign,
        }
    }

    /// Wires the drag. `width` reads the pane's width now; `resize` takes
    /// each width the drag reaches; `done` takes the width the drag ended
    /// on, or `None` after a double click, which puts the default back.
    pub fn connect(
        &self,
        width: impl Fn() -> i32 + 'static,
        resize: impl Fn(Option<i32>) + 'static,
        done: impl Fn(Option<i32>) + 'static,
    ) {
        let (resize, done) = (Rc::new(resize), Rc::new(done));
        let started = Rc::new(Cell::new(0));
        let reached = Rc::new(Cell::new(None::<i32>));
        let drag = gtk::GestureDrag::new();
        let (from, sign) = (Rc::clone(&started), self.sign);
        let at = Rc::clone(&reached);
        drag.connect_drag_begin(move |_, _, _| {
            from.set(width());
            at.set(None);
        });
        let (from, at, live) = (Rc::clone(&started), Rc::clone(&reached), Rc::clone(&resize));
        drag.connect_drag_update(move |_, dx, _| {
            let wanted = from.get() + sign * dx.round() as i32;
            at.set(Some(wanted));
            live(Some(wanted));
        });
        let (at, finished) = (Rc::clone(&reached), Rc::clone(&done));
        drag.connect_drag_end(move |_, _, _| {
            if let Some(wanted) = at.take() {
                finished(Some(wanted));
            }
        });
        self.strip.add_controller(drag);
        let reset = gtk::GestureClick::new();
        reset.connect_pressed(move |_, presses, _, _| {
            if presses == 2 {
                resize(None);
                done(None);
            }
        });
        self.strip.add_controller(reset);
    }
}

/// Sets a split view's sidebar to exactly `width`, or back to `least` and
/// `most` with `None`. The bound that would cross the other goes first, so
/// the least never sits above the most, which libadwaita refuses.
pub fn fix_width(set_least: impl Fn(f64), set_most: impl Fn(f64), most_now: f64, width: Option<i32>, defaults: (f64, f64)) {
    let (least, most) = match width {
        Some(width) => (f64::from(width), f64::from(width)),
        None => defaults,
    };
    if least > most_now {
        set_most(most);
        set_least(least);
    } else {
        set_least(least);
        set_most(most);
    }
}

/// The three panes the person can resize, each with its strip and the
/// split view that sets its width. The assistant's width goes through the
/// room (room.rs), which reads it from the shared cell.
pub struct Edges<'a> {
    pub sidebar: (&'a PaneEdge, &'a adw::OverlaySplitView),
    pub list: (&'a PaneEdge, &'a adw::NavigationSplitView),
    pub assistant: (&'a PaneEdge, &'a adw::OverlaySplitView, Rc<Cell<Option<i32>>>),
}

/// Wires each strip to its pane, puts back the widths the preferences
/// kept, and keeps each new width in them when a drag ends.
pub fn wire(app: &Rc<App>, edges: Edges<'_>) {
    let saved = app.settings_with(|settings| settings.layout.panes.clone());

    let (edge, split) = edges.sidebar;
    let defaults = (split.min_sidebar_width(), split.max_sidebar_width());
    let weak = split.downgrade();
    let apply: Rc<dyn Fn(Option<i32>)> = Rc::new(move |width| {
        let Some(split) = weak.upgrade() else { return };
        fix_width(
            |v| split.set_min_sidebar_width(v),
            |v| split.set_max_sidebar_width(v),
            split.max_sidebar_width(),
            width.map(|w| clamp(Pane::Sidebar, w)),
            defaults,
        );
        rearrange(split.upcast_ref());
    });
    hook(app, edge, Pane::Sidebar, apply, saved.get(&Pane::Sidebar).copied());

    let (edge, columns) = edges.list;
    let defaults = (columns.min_sidebar_width(), columns.max_sidebar_width());
    let weak = columns.downgrade();
    let apply: Rc<dyn Fn(Option<i32>)> = Rc::new(move |width| {
        let Some(columns) = weak.upgrade() else { return };
        fix_width(
            |v| columns.set_min_sidebar_width(v),
            |v| columns.set_max_sidebar_width(v),
            columns.max_sidebar_width(),
            width.map(|w| clamp(Pane::List, w)),
            defaults,
        );
        rearrange(columns.upcast_ref());
    });
    hook(app, edge, Pane::List, apply, saved.get(&Pane::List).copied());

    let (edge, split, chosen) = edges.assistant;
    let weak = split.downgrade();
    let apply: Rc<dyn Fn(Option<i32>)> = Rc::new(move |width| {
        chosen.set(width.map(|w| clamp(Pane::Assistant, w)));
        if let Some(split) = weak.upgrade() {
            rearrange(split.upcast_ref());
        }
    });
    hook(app, edge, Pane::Assistant, apply, saved.get(&Pane::Assistant).copied());
}

/// Applies `saved`, then connects the strip: each step of a drag applies
/// its width, and the end of one keeps it.
fn hook(app: &Rc<App>, edge: &PaneEdge, pane: Pane, apply: Rc<dyn Fn(Option<i32>)>, saved: Option<i32>) {
    if saved.is_some() {
        apply(saved);
    }
    let overlay = edge.overlay.downgrade();
    let app = Rc::downgrade(app);
    edge.connect(
        move || overlay.upgrade().map_or(0, |o| o.width()),
        move |width| apply(width),
        move |width| {
            if let Some(app) = app.upgrade() {
                app.change_settings(Change::PaneWidth { pane, width });
            }
        },
    );
}

fn clamp(pane: Pane, width: i32) -> i32 {
    let (least, most) = pane.bounds();
    width.clamp(least, most)
}

/// Has the room decide again, since a pane's new width changes what fits.
fn rearrange(within: &gtk::Widget) {
    if let Some(room) = within.ancestor(Room::static_type()).and_downcast::<Room>() {
        room.rearrange();
    }
}

/// Keeps the window's size and whether it is maximized in the
/// preferences, half a second after the last change, so a resize by
/// dragging writes the file once. GTK moves `default-width` and
/// `default-height` with the window as the person resizes it. A size in
/// full screen is the screen's, not one the person chose, so it is not
/// kept.
pub fn keep_size(app: &Rc<App>, window: &adw::Window) {
    let generation = Rc::new(Cell::new(0u64));
    let save = {
        let (app, window) = (Rc::downgrade(app), window.downgrade());
        move || {
            let Some(window) = window.upgrade() else { return };
            let mine = generation.get() + 1;
            generation.set(mine);
            let (app, window, generation) = (app.clone(), window.downgrade(), Rc::clone(&generation));
            glib::timeout_add_local_once(std::time::Duration::from_millis(500), move || {
                let (Some(app), Some(window)) = (app.upgrade(), window.upgrade()) else { return };
                if generation.get() != mine || window.is_fullscreen() {
                    return;
                }
                let (width, height) = window.default_size();
                app.change_settings(Change::WindowSize {
                    width,
                    height,
                    maximized: window.is_maximized(),
                });
            });
        }
    };
    let save = Rc::new(save);
    for property in ["default-width", "default-height", "maximized"] {
        let save = Rc::clone(&save);
        window.connect_notify_local(Some(property), move |_, _| save());
    }
}
