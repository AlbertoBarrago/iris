//! The hidden WebKit view the unsubscribe run reads pages through. The
//! run itself, and the rules it decides by, live in
//! `mailrs_appcore::unsubscribe_page`.

mod webkit;

pub use mailrs_appcore::unsubscribe_page::*;
pub use self::webkit::WebkitBrowser;
