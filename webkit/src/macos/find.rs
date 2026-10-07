use std::cell::{Cell, RefCell};
use std::ops::{BitOr, BitOrAssign};
use std::ptr::NonNull;
use std::rc::Rc;

use gtk::glib;
use objc2_foundation::NSString;
use objc2_web_kit::{WKFindConfiguration, WKFindResult};

use super::view::{WebView, WebViewExt};

/// webkit6's find flags, with its bit values. WKWebView reads two of them:
/// case and wrapping.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FindOptions(u32);

impl FindOptions {
    pub const NONE: FindOptions = FindOptions(0);
    pub const CASE_INSENSITIVE: FindOptions = FindOptions(1 << 0);
    pub const AT_WORD_STARTS: FindOptions = FindOptions(1 << 1);
    pub const TREAT_MEDIAL_CAPITAL_AS_WORD_START: FindOptions = FindOptions(1 << 2);
    pub const BACKWARDS: FindOptions = FindOptions(1 << 3);
    pub const WRAP_AROUND: FindOptions = FindOptions(1 << 4);

    pub fn bits(&self) -> u32 {
        self.0
    }
}

impl BitOr for FindOptions {
    type Output = FindOptions;
    fn bitor(self, other: FindOptions) -> FindOptions {
        FindOptions(self.0 | other.0)
    }
}

impl BitOrAssign for FindOptions {
    fn bitor_assign(&mut self, other: FindOptions) {
        self.0 |= other.0;
    }
}

type Found = Rc<dyn Fn(&FindController, u32)>;
type Failed = Rc<dyn Fn(&FindController)>;

/// Finds text in a view's page.
///
/// WKWebView finds and highlights one match at a time and does not count
/// them, so the count comes from a script that reads the page's text,
/// shadow roots included, the way the reader sees it.
#[derive(Clone)]
pub struct FindController(Rc<Inner>);

struct Inner {
    view: glib::WeakRef<WebView>,
    text: RefCell<String>,
    options: Cell<u32>,
    found: RefCell<Vec<Found>>,
    counted: RefCell<Vec<Found>>,
    failed: RefCell<Vec<Failed>>,
}

/// Counts the matches of a needle across the document and its open shadow
/// roots, skipping style and script text, up to a limit.
const COUNT_SCRIPT: &str = r#"(function (needle, fold, limit) {
  if (fold) needle = needle.toLowerCase();
  var count = 0;
  var walk = function (root) {
    var walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT);
    var node;
    while ((node = walker.nextNode())) {
      var parent = node.parentNode && node.parentNode.nodeName;
      if (parent === 'STYLE' || parent === 'SCRIPT') continue;
      var text = fold ? node.data.toLowerCase() : node.data;
      var at = text.indexOf(needle);
      while (at !== -1) {
        if (++count >= limit) return;
        at = text.indexOf(needle, at + needle.length);
      }
    }
    var all = root.querySelectorAll('*');
    for (var i = 0; i < all.length && count < limit; i++) {
      if (all[i].shadowRoot) walk(all[i].shadowRoot);
    }
  };
  walk(document);
  return String(count);
})"#;

impl FindController {
    pub(crate) fn new(view: &WebView) -> FindController {
        let weak = glib::WeakRef::new();
        weak.set(Some(view));
        FindController(Rc::new(Inner {
            view: weak,
            text: RefCell::new(String::new()),
            options: Cell::new(0),
            found: RefCell::new(Vec::new()),
            counted: RefCell::new(Vec::new()),
            failed: RefCell::new(Vec::new()),
        }))
    }

    pub fn search(&self, search_text: &str, find_options: u32, _max_match_count: u32) {
        *self.0.text.borrow_mut() = search_text.to_string();
        self.0.options.set(find_options);
        self.find(find_options & FindOptions::BACKWARDS.0 != 0);
    }

    pub fn search_next(&self) {
        self.find(false);
    }

    pub fn search_previous(&self) {
        self.find(true);
    }

    /// Drops the highlight the last match left.
    pub fn search_finish(&self) {
        self.0.text.borrow_mut().clear();
        if let Some(view) = self.0.view.upgrade() {
            view.run_quietly("window.getSelection && window.getSelection().removeAllRanges()");
        }
    }

    pub fn count_matches(&self, search_text: &str, find_options: u32, max_match_count: u32) {
        let Some(view) = self.0.view.upgrade() else { return };
        let fold = find_options & FindOptions::CASE_INSENSITIVE.0 != 0;
        let Ok(needle) = serde_json_string(search_text) else { return };
        let script = format!("{COUNT_SCRIPT}({needle}, {fold}, {})", max_match_count.max(1));
        let this = self.clone();
        view.evaluate_javascript(&script, None, None, gtk::gio::Cancellable::NONE, move |answer| {
            let count = answer
                .ok()
                .and_then(|value| value.to_str().parse::<u32>().ok())
                .unwrap_or(0);
            let counted: Vec<Found> = this.0.counted.borrow().clone();
            for listener in counted {
                listener(&this, count);
            }
        });
    }

    pub fn connect_found_text<F: Fn(&FindController, u32) + 'static>(&self, f: F) {
        self.0.found.borrow_mut().push(Rc::new(f));
    }

    pub fn connect_counted_matches<F: Fn(&FindController, u32) + 'static>(&self, f: F) {
        self.0.counted.borrow_mut().push(Rc::new(f));
    }

    pub fn connect_failed_to_find_text<F: Fn(&FindController) + 'static>(&self, f: F) {
        self.0.failed.borrow_mut().push(Rc::new(f));
    }

    fn find(&self, backwards: bool) {
        let text = self.0.text.borrow().clone();
        let Some(view) = self.0.view.upgrade() else { return };
        if text.is_empty() {
            return;
        }
        let options = self.0.options.get();
        let page = view.page();
        let mtm = super::main_thread();
        let this = self.clone();
        unsafe {
            let config = WKFindConfiguration::new(mtm);
            config.setBackwards(backwards);
            config.setCaseSensitive(options & FindOptions::CASE_INSENSITIVE.0 == 0);
            config.setWraps(options & FindOptions::WRAP_AROUND.0 != 0);
            let done = block2::RcBlock::new(move |result: NonNull<WKFindResult>| {
                let found = result.as_ref().matchFound();
                if found {
                    let listeners: Vec<Found> = this.0.found.borrow().clone();
                    for listener in listeners {
                        listener(&this, 1);
                    }
                } else {
                    let listeners: Vec<Failed> = this.0.failed.borrow().clone();
                    for listener in listeners {
                        listener(&this);
                    }
                }
            });
            page.findString_withConfiguration_completionHandler(&NSString::from_str(&text), Some(&config), &done);
        }
    }
}

/// `text` as a JavaScript string literal.
fn serde_json_string(text: &str) -> Result<String, std::fmt::Error> {
    use std::fmt::Write;
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c if (c as u32) < 0x20 || c == '\u{2028}' || c == '\u{2029}' => {
                write!(out, "\\u{:04x}", c as u32)?;
            }
            c => out.push(c),
        }
    }
    out.push('"');
    Ok(out)
}
