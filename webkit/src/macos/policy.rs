use std::cell::Cell;

use gtk::glib;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PolicyDecisionType {
    NavigationAction,
    NewWindowAction,
    Response,
}

/// The question WKWebView asks before it goes somewhere: go, or stay.
///
/// webkit6 hands the app a GObject it downcasts to the kind of decision.
/// Here a decision carries its kind itself, and [`PolicyDecision::downcast_ref`]
/// gives it back the same way, so the app's code reads unchanged.
pub struct PolicyDecision {
    verdict: Cell<Option<bool>>,
    navigation: Option<NavigationPolicyDecision>,
    response: Option<ResponsePolicyDecision>,
}

/// A kind of decision [`PolicyDecision::downcast_ref`] can hand back.
pub trait DecisionKind {
    fn pick(decision: &PolicyDecision) -> Option<&Self>;
}

impl DecisionKind for NavigationPolicyDecision {
    fn pick(decision: &PolicyDecision) -> Option<&Self> {
        decision.navigation.as_ref()
    }
}

impl DecisionKind for ResponsePolicyDecision {
    fn pick(decision: &PolicyDecision) -> Option<&Self> {
        decision.response.as_ref()
    }
}

impl PolicyDecision {
    pub(crate) fn navigation(uri: String) -> PolicyDecision {
        PolicyDecision {
            verdict: Cell::new(None),
            navigation: Some(NavigationPolicyDecision {
                action: NavigationAction {
                    request: URIRequest { uri },
                },
            }),
            response: None,
        }
    }

    pub(crate) fn response(shows: bool) -> PolicyDecision {
        PolicyDecision {
            verdict: Cell::new(None),
            navigation: None,
            response: Some(ResponsePolicyDecision { shows }),
        }
    }

    pub fn downcast_ref<T: DecisionKind>(&self) -> Option<&T> {
        T::pick(self)
    }

    pub fn use_(&self) {
        self.verdict.set(Some(true));
    }

    pub fn ignore(&self) {
        self.verdict.set(Some(false));
    }

    /// Whether the view goes ahead. A handler that took the decision and
    /// said nothing leaves it ignored, as webkit6 would after a timeout.
    pub(crate) fn allowed(&self, handled: bool) -> bool {
        match self.verdict.get() {
            Some(verdict) => verdict,
            None => !handled,
        }
    }
}

/// A response the page is about to show.
pub struct ResponsePolicyDecision {
    shows: bool,
}

impl ResponsePolicyDecision {
    /// Whether WebKit can show what came back, rather than save it.
    pub fn is_mime_type_supported(&self) -> bool {
        self.shows
    }
}

pub struct NavigationPolicyDecision {
    action: NavigationAction,
}

impl NavigationPolicyDecision {
    pub fn navigation_action(&self) -> Option<NavigationAction> {
        Some(self.action.clone())
    }
}

#[derive(Clone)]
pub struct NavigationAction {
    request: URIRequest,
}

impl NavigationAction {
    pub fn request(&self) -> Option<URIRequest> {
        Some(self.request.clone())
    }
}

#[derive(Clone)]
pub struct URIRequest {
    uri: String,
}

impl URIRequest {
    pub fn uri(&self) -> Option<glib::GString> {
        Some(glib::GString::from(self.uri.as_str()))
    }
}
