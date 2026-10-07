use std::cell::Cell;
use std::future::Future;
use std::pin::Pin;

use futures::channel::oneshot;
use gtk::{gio, glib};
use objc2::rc::Retained;
use objc2_foundation::{NSError, NSString, NSURL};
use objc2_web_kit::{WKContentRuleList, WKContentRuleListStore};

/// A compiled content blocker. WebKitGTK and WKWebView read the same JSON
/// rules, Safari's content blocker format, so the app's rules go in as
/// they are.
#[derive(Clone)]
pub struct UserContentFilter(Retained<WKContentRuleList>);

impl std::fmt::Debug for UserContentFilter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("UserContentFilter")
    }
}

impl UserContentFilter {
    pub(crate) fn native(&self) -> &WKContentRuleList {
        &self.0
    }
}

/// Compiles rules and keeps them in a folder, as webkit6's store does.
pub struct UserContentFilterStore {
    path: String,
}

impl UserContentFilterStore {
    pub fn new(storage_path: &str) -> UserContentFilterStore {
        UserContentFilterStore {
            path: storage_path.to_string(),
        }
    }

    pub fn save_future(
        &self,
        identifier: &str,
        source: &glib::Bytes,
    ) -> Pin<Box<dyn Future<Output = Result<UserContentFilter, glib::Error>> + 'static>> {
        let (tell, told) = oneshot::channel();
        let rules = String::from_utf8_lossy(source).into_owned();
        let mtm = super::main_thread();
        let url = NSURL::fileURLWithPath(&NSString::from_str(&self.path));
        match unsafe { WKContentRuleListStore::storeWithURL(Some(&url), mtm) } {
            None => {
                let _ = tell.send(Err(failed("no content rule store at that folder")));
            }
            Some(store) => {
                let tell = Cell::new(Some(tell));
                let done = block2::RcBlock::new(move |list: *mut WKContentRuleList, err: *mut NSError| {
                    let Some(tell) = tell.take() else { return };
                    let answer = match (unsafe { Retained::retain(list) }, unsafe { err.as_ref() }) {
                        (Some(list), _) => Ok(UserContentFilter(list)),
                        (None, Some(err)) => Err(failed(&err.localizedDescription().to_string())),
                        (None, None) => Err(failed("the rules compiled to nothing")),
                    };
                    let _ = tell.send(answer);
                });
                unsafe {
                    store.compileContentRuleListForIdentifier_encodedContentRuleList_completionHandler(
                        Some(&NSString::from_str(identifier)),
                        Some(&NSString::from_str(&rules)),
                        Some(&done),
                    )
                };
            }
        }
        Box::pin(async move {
            told
                .await
                .unwrap_or_else(|_| Err(failed("the rule store went away before it answered")))
        })
    }
}

fn failed(why: &str) -> glib::Error {
    glib::Error::new(gio::IOErrorEnum::Failed, why)
}
