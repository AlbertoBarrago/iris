//! Pictures and fonts on the web that a message's page shows.
//!
//! The page sends each one through `mailrs-remote:` (see
//! `mailrs_render::REMOTE_SCHEME`) and the app asks here, so the core
//! downloads them rather than WebKit. WebKit honours
//! `Cross-Origin-Resource-Policy`, and a sender that answers `same-origin`,
//! as claude.ai does, never shows its pictures in a page of ours. Asked
//! from here, no browser policy applies, no cookie or referrer goes along,
//! and every download passes the same rules.
//!
//! Whether a message may load them at all is the page's policy to say,
//! from [`crate::images`]: a page that may not never asks.

use std::sync::OnceLock;

use reqwest::Url;

use crate::assistant::sources::web;

/// The most one picture or font may weigh. A newsletter's large banner
/// stays well under it.
const MAX_BYTES: usize = 10 * 1024 * 1024;

/// A downloaded picture or font, with the type its server gave it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fetched {
    pub mime_type: String,
    pub data: Vec<u8>,
}

/// The client every download shares, so connections to a sender's server
/// are reused across a conversation's pictures.
fn client() -> &'static reqwest::Client {
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    CLIENT.get_or_init(|| web::client(false))
}

/// Downloads the picture or font at `address`: http or https only, never
/// on this computer or the local network, where mail has no business
/// sending requests, and only what answers as a picture or a font.
pub async fn fetch(address: &str) -> Result<Fetched, String> {
    fetch_with(client(), address, false).await
}

/// [`fetch`] through `client`, which lets the tests reach a local server
/// when `local` says so.
async fn fetch_with(client: &reqwest::Client, address: &str, local: bool) -> Result<Fetched, String> {
    let url = Url::parse(address).map_err(|e| format!("{address} is not a web address: {e}"))?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err(format!("{address} is not on the web"));
    }
    if !local && web::is_local(&url) {
        return Err(format!("{address} is on this computer or the local network"));
    }
    let mut response = client
        .get(url)
        .send()
        .await
        .map_err(|e| format!("Could not download {address}: {e}"))?;
    let status = response.status();
    if !status.is_success() {
        return Err(format!("{address} answered HTTP {}", status.as_u16()));
    }
    let mime_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default()
        .split(';')
        .next()
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();
    if !shown(&mime_type) {
        return Err(format!("{address} is {mime_type}, not a picture or a font"));
    }
    let mut data = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|e| format!("The download of {address} stopped: {e}"))?
    {
        if data.len() + chunk.len() > MAX_BYTES {
            return Err(format!("{address} weighs more than {MAX_BYTES} bytes"));
        }
        data.extend_from_slice(&chunk);
    }
    Ok(Fetched { mime_type, data })
}

/// Whether a page may be handed something of type `mime_type`. Servers
/// give fonts several names, and some give a file no type of its own;
/// WebKit reads such a file by its content.
fn shown(mime_type: &str) -> bool {
    mime_type.starts_with("image/")
        || mime_type.starts_with("font/")
        || mime_type.starts_with("application/font-")
        || mime_type.starts_with("application/x-font-")
        || matches!(mime_type, "application/vnd.ms-fontobject" | "application/octet-stream")
}

#[cfg(test)]
mod tests {
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;

    async fn serving(kind: &str, body: Vec<u8>) -> MockServer {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/logo.png"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", kind)
                    // What claude.ai answers, which no page of ours may show.
                    .insert_header("cross-origin-resource-policy", "same-origin")
                    .set_body_bytes(body),
            )
            .mount(&server)
            .await;
        server
    }

    #[tokio::test]
    async fn a_picture_comes_back_with_its_type() {
        let server = serving("image/png; charset=binary", b"\x89PNG".to_vec()).await;
        let got = fetch_with(&web::client(true), &format!("{}/logo.png", server.uri()), true)
            .await
            .unwrap();
        assert_eq!(
            got,
            Fetched {
                mime_type: "image/png".into(),
                data: b"\x89PNG".to_vec()
            }
        );
    }

    #[tokio::test]
    async fn a_page_is_not_a_picture() {
        let server = serving("text/html", b"<p>hi</p>".to_vec()).await;
        let problem = fetch_with(&web::client(true), &format!("{}/logo.png", server.uri()), true)
            .await
            .unwrap_err();
        assert!(problem.contains("text/html"), "{problem}");
    }

    #[tokio::test]
    async fn a_picture_too_heavy_is_refused() {
        let server = serving("image/png", vec![0; MAX_BYTES + 1]).await;
        let problem = fetch_with(&web::client(true), &format!("{}/logo.png", server.uri()), true)
            .await
            .unwrap_err();
        assert!(problem.contains("weighs more"), "{problem}");
    }

    #[tokio::test]
    async fn mail_cannot_reach_this_computer_or_other_schemes() {
        for address in [
            "http://127.0.0.1/logo.png",
            "http://192.168.1.1/reset",
            "http://printer.local/x.png",
            "file:///etc/hosts",
            "not an address",
        ] {
            assert!(fetch(address).await.is_err(), "{address}");
        }
    }

    #[test]
    fn pictures_and_fonts_are_shown_and_nothing_else() {
        for kind in ["image/png", "image/svg+xml", "font/woff2", "application/font-woff", "application/octet-stream"] {
            assert!(shown(kind), "{kind}");
        }
        for kind in ["text/html", "application/json", "text/css", ""] {
            assert!(!shown(kind), "{kind}");
        }
    }
}
