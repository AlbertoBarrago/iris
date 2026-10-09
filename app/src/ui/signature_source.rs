//! Where a formatted signature comes from: the clipboard, holding what was
//! copied from a message or from another mail client's signature editor,
//! or an `.htm` file such as Outlook saves its signatures in. Both hand
//! back raw HTML; `crate::signature` cleans it before it is kept.

use gtk::prelude::*;
use gtk::gio;
use mailrs_domain::translate::gettext;

/// The HTML on the clipboard. `Ok(None)` when it holds only text or
/// nothing, which is the case for a signature copied from a plain text
/// field. GDK's clipboard on macOS offers no HTML, so this reads AppKit's.
pub async fn from_clipboard() -> Result<Option<String>, String> {
    Ok(crate::macos_pasteboard::html())
}

/// Asks for an `.htm` or `.html` file and reads it, with the pictures it
/// names beside it brought in. `Ok(None)` when the person canceled.
pub async fn from_file(parent: Option<&gtk::Window>) -> Result<Option<String>, String> {
    let pages = gtk::FileFilter::new();
    pages.set_name(Some(&gettext("Web Pages")));
    for suffix in ["htm", "html"] {
        pages.add_suffix(suffix);
    }
    let filters = gio::ListStore::new::<gtk::FileFilter>();
    filters.append(&pages);
    let dialog = gtk::FileDialog::builder()
        .title(gettext("Import Signature"))
        .modal(true)
        .filters(&filters)
        .default_filter(&pages)
        .build();
    let file = match dialog.open_future(parent).await {
        Ok(file) => file,
        Err(e) if e.matches(gtk::DialogError::Dismissed) => return Ok(None),
        Err(e) => return Err(e.to_string()),
    };
    let path = file
        .path()
        .ok_or_else(|| gettext("The file is not on this computer."))?;
    let bytes = gio::spawn_blocking({
        let path = path.clone();
        move || std::fs::read(path)
    })
    .await
    .map_err(|_| gettext("Reading the file stopped."))?
    .map_err(|e| e.to_string())?;
    let dir = path.parent().map(std::path::Path::to_path_buf).unwrap_or_default();
    let html = crate::signature::decode(&bytes);
    let imported = gio::spawn_blocking(move || crate::signature::import(&html, &dir))
        .await
        .map_err(|_| gettext("Reading the file stopped."))?;
    Ok(Some(imported))
}
