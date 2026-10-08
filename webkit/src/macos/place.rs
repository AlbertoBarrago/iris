//! Where on the window the page goes, and what GTK draws over it.
//!
//! GTK draws a whole window into one surface, and a WKWebView is a view of
//! its own laid on top of that surface. So the page has to be told where
//! its widget is, cut to what of the widget is visible, and kept out of the
//! way of the GTK widgets meant to be drawn above it: the event card laid
//! over the message, a toast, a dialog. Those become holes in the page,
//! through which GTK's drawing and GTK's clicks show.

use std::cell::{Cell, RefCell};

use gtk::graphene;
use gtk::prelude::*;
use objc2::rc::Retained;
use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send};
use objc2_app_kit::NSView;
use objc2_core_foundation::{CGPoint, CGRect, CGSize};
use objc2_core_graphics::CGPath;
use objc2_foundation::NSPoint;
use objc2_quartz_core::CAShapeLayer;

/// Containers whose children are drawn on top of one another, later ones
/// above earlier ones. A child drawn after the one that holds the page
/// covers the page where they meet.
const STACKING: &[&str] = &[
    "GtkOverlay",
    "AdwToastOverlay",
    "AdwOverlaySplitView",
    "AdwBottomSheet",
    "AdwDialogHost",
    "AdwFloatingSheet",
];

/// A widget drawn over the page: its bounds and how round its corners are.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Hole {
    pub(crate) rect: CGRect,
    pub(crate) radius: f64,
}

/// How round a covering widget's corners are. GTK gives no easy way to read
/// a widget's border radius, so this goes by kind: a toast is a pill, and
/// the rest (the event card, a dialog) use libadwaita's card radius.
fn corner_radius(widget: &gtk::Widget, bounds: &graphene::Rect) -> f64 {
    match widget.css_name().as_str() {
        "toast" => f64::from(bounds.height()) / 2.0,
        _ => 12.0,
    }
}

/// The sheet a libadwaita dialog draws, when `widget` is a dialog: the
/// `background` child of its floating or bottom sheet. The rest of the
/// dialog is the backdrop, which dims the window behind the sheet.
fn dialog_sheet(widget: &gtk::Widget) -> Option<gtk::Widget> {
    if widget.type_().name() != "AdwDialog" {
        return None;
    }
    fn find(widget: &gtk::Widget, depth: usize) -> Option<gtk::Widget> {
        let mut child = widget.first_child();
        while let Some(current) = child {
            let in_sheet = matches!(widget.type_().name(), "AdwFloatingSheet" | "AdwBottomSheet");
            if in_sheet && current.has_css_class("background") {
                return Some(current);
            }
            if depth < 4
                && let Some(found) = find(&current, depth + 1)
            {
                return Some(found);
            }
            child = current.next_sibling();
        }
        None
    }
    find(widget, 0)
}

/// The part of the window the page takes, in the window's GTK coordinates.
pub(crate) struct Placement {
    /// The whole widget, where the page lays itself out.
    pub(crate) full: graphene::Rect,
    /// The part of it that shows: the widget cut by every ancestor.
    pub(crate) visible: graphene::Rect,
    /// The widgets GTK draws over the page, whole, each with the radius of
    /// its corners. Parts of them may lie outside `visible`.
    pub(crate) holes: Vec<Hole>,
    /// Each hole's widget and bounds, for the debug log.
    pub(crate) covers: Vec<String>,
}

/// Measures `widget` against its native (the window it is drawn in).
/// Answers nothing when no part of it shows.
pub(crate) fn measure(widget: &gtk::Widget) -> Option<Placement> {
    if !widget.is_drawable() {
        return None;
    }
    let native = widget.native()?;
    let native = native.upcast_ref::<gtk::Widget>();
    let full = widget.compute_bounds(native)?;
    let mut visible = full;
    let mut holes = Vec::new();
    let mut covers = Vec::new();
    let mut child = widget.clone();
    while let Some(parent) = child.parent() {
        if let Some(bounds) = parent.compute_bounds(native) {
            visible = visible.intersection(&bounds)?;
        }
        if STACKING.contains(&parent.type_().name()) {
            let mut later = child.next_sibling();
            while let Some(above) = later {
                // The whole widget, not just the part over the page: a
                // toast that also spans the list beside the page must keep
                // its rounded ends where they are, not grow new ones at the
                // page's edge. The clip cuts the rest away.
                // A dialog spans the whole window with its dimmed backdrop,
                // and cutting all of that away left no mail on screen while
                // any dialog was open. Only its sheet covers the page; the
                // mail stays visible around it, undimmed.
                let above = dialog_sheet(&above).unwrap_or(above);
                if above.is_drawable()
                    && let Some(bounds) = above.compute_bounds(native)
                    && bounds.intersection(&full).is_some()
                {
                    holes.push(Hole {
                        rect: rect(
                            f64::from(bounds.x()),
                            f64::from(bounds.y()),
                            f64::from(bounds.width()),
                            f64::from(bounds.height()),
                        ),
                        radius: corner_radius(&above, &bounds),
                    });
                    covers.push(format!(
                        "{} in {} at ({}, {}) {}x{}",
                        above.type_().name(),
                        parent.type_().name(),
                        bounds.x(),
                        bounds.y(),
                        bounds.width(),
                        bounds.height()
                    ));
                }
                later = above.next_sibling();
            }
        }
        if &parent == native {
            break;
        }
        child = parent;
    }
    if visible.width() < 1.0 || visible.height() < 1.0 {
        return None;
    }
    Some(Placement { full, visible, holes, covers })
}

pub(crate) struct ClipIvars {
    /// The holes, in the clip view's own (flipped) coordinates.
    holes: RefCell<Vec<Hole>>,
    /// The size the mask was drawn for.
    size: Cell<(f64, f64)>,
}

define_class!(
    /// Holds the page, cut to the part of its widget that shows. Flipped,
    /// so its coordinates run top-down like GTK's.
    #[unsafe(super(NSView))]
    #[thread_kind = MainThreadOnly]
    #[name = "MailrsPageClip"]
    #[ivars = ClipIvars]
    pub(crate) struct Clip;

    impl Clip {
        #[unsafe(method(isFlipped))]
        fn is_flipped(&self) -> bool {
            true
        }

        /// A click in a hole belongs to the GTK widget drawn there.
        #[unsafe(method(hitTest:))]
        fn hit_test(&self, point: NSPoint) -> *mut NSView {
            let local = match unsafe { self.superview() } {
                Some(parent) => self.convertPoint_fromView(point, Some(&parent)),
                None => point,
            };
            let in_hole = self.ivars().holes.borrow().iter().any(|hole| contains(&hole.rect, local));
            if in_hole {
                return std::ptr::null_mut();
            }
            unsafe { msg_send![super(self), hitTest: point] }
        }
    }
);

fn contains(rect: &CGRect, point: CGPoint) -> bool {
    point.x >= rect.origin.x
        && point.y >= rect.origin.y
        && point.x < rect.origin.x + rect.size.width
        && point.y < rect.origin.y + rect.size.height
}

impl Clip {
    pub(crate) fn new(mtm: MainThreadMarker) -> Retained<Clip> {
        let this = Clip::alloc(mtm).set_ivars(ClipIvars {
            holes: RefCell::new(Vec::new()),
            size: Cell::new((0.0, 0.0)),
        });
        let this: Retained<Clip> = unsafe { msg_send![super(this), initWithFrame: CGRect::ZERO] };
        this.setWantsLayer(true);
        this.setClipsToBounds(true);
        // GTK draws the window into its content view's own layer, as tiles,
        // and the page's layer sits among them. A resize such as going full
        // screen makes new tiles, which landed above the page and covered
        // part of it, leaving the mail cut off in an L. Raised above every
        // tile, the page stays on top whatever order they come in; the
        // holes still let GTK's covering widgets through.
        if let Some(layer) = this.layer() {
            layer.setZPosition(1000.0);
        }
        this
    }

    /// Cuts the page to `holes`, given in the clip's coordinates. The
    /// mask is the clip's bounds less every hole; `hitTest` above sends
    /// the holes' clicks on to GTK.
    ///
    /// Holes overlap: the event card sits under any dialog opened over the
    /// message. The mask used to be the bounds and the holes in one
    /// even-odd path, which fills a point covered by two holes again, so
    /// the page came back over the dialog where the card was. Subtracting
    /// the holes, drawn all the same way round so they join, cannot do
    /// that.
    pub(crate) fn set_holes(&self, holes: Vec<Hole>) {
        // The mask is drawn to the clip's size, so a resize redraws it.
        let bounds = self.bounds();
        let size = (bounds.size.width, bounds.size.height);
        if *self.ivars().holes.borrow() == holes && self.ivars().size.get() == size {
            return;
        }
        self.ivars().size.set(size);
        let Some(layer) = self.layer() else {
            return;
        };
        if holes.is_empty() {
            unsafe { layer.setMask(None) };
        } else {
            unsafe {
                let page = objc2_core_graphics::CGMutablePath::new();
                objc2_core_graphics::CGMutablePath::add_rect(Some(&page), std::ptr::null(), bounds);
                let path = objc2_core_graphics::CGMutablePath::new();
                for hole in &holes {
                    // A corner radius past half the side makes Core Graphics
                    // refuse the rectangle.
                    let radius = hole.radius.min(hole.rect.size.width / 2.0).min(hole.rect.size.height / 2.0);
                    objc2_core_graphics::CGMutablePath::add_rounded_rect(
                        Some(&path),
                        std::ptr::null(),
                        hole.rect,
                        radius,
                        radius,
                    );
                }
                let shown = CGPath::new_copy_by_subtracting_path(
                    Some(page.as_ref() as &CGPath),
                    Some(path.as_ref() as &CGPath),
                    false,
                );
                let mask = CAShapeLayer::new();
                mask.setFrame(bounds);
                mask.setPath(shown.as_deref());
                layer.setMask(Some(&mask));
            }
        }
        *self.ivars().holes.borrow_mut() = holes;
    }
}

/// A GTK rectangle as a Core Graphics one.
pub(crate) fn rect(x: f64, y: f64, width: f64, height: f64) -> CGRect {
    CGRect::new(CGPoint::new(x, y), CGSize::new(width, height))
}
