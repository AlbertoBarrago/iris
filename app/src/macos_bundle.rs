//! What a self-contained Iris.app needs before GTK starts. The DMG's
//! bundle carries GTK's libraries and data inside it (scripts/macos-dmg.sh)
//! rather than reading them from Homebrew, and a few of GTK's parts find
//! their files only through the environment or a path compiled into them:
//! gdk-pixbuf its image loaders, fontconfig its config, and GTK, GLib and
//! libadwaita their translations. A bundle built by macos-bundle.sh alone,
//! which loads Homebrew's GTK, has none of these files and is left alone.

use std::path::{Path, PathBuf};

/// Where the bundle keeps its resources, `Iris.app/Contents/Resources`,
/// when Iris runs from a bundle.
fn resources() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let contents = exe.parent()?.parent()?;
    let resources = contents.join("Resources");
    resources.is_dir().then_some(resources)
}

/// A folder of our own under `~/Library/Caches`, for the files below that
/// name the bundle's place, which only the running copy knows.
fn cache_dir() -> Option<PathBuf> {
    let home = PathBuf::from(std::env::var_os("HOME")?);
    let dir = home.join("Library/Caches/io.github.AlbertoBarrago.Iris");
    std::fs::create_dir_all(&dir).ok()?;
    Some(dir)
}

/// Points gdk-pixbuf and fontconfig at the bundle's own files. Runs first
/// thing in `main`, before any other thread reads the environment.
pub fn set_environment() {
    let Some(resources) = resources() else { return };
    let loaders = resources.join("lib/gdk-pixbuf-2.0/2.10.0");
    let template = loaders.join("loaders.cache.in");
    // The loaders' cache names each loader by its full path, so the copy in
    // the bundle names a placeholder, swapped here for wherever the app
    // was dragged to.
    if let Ok(text) = std::fs::read_to_string(&template)
        && let Some(cache) = cache_dir().map(|dir| dir.join("loaders.cache"))
    {
        let filled = text.replace("@BUNDLE_LOADERS@", &loaders.join("loaders").to_string_lossy());
        match std::fs::write(&cache, filled) {
            // SAFETY: called first thing in `main`, before any other
            // thread exists to read the environment.
            Ok(()) => unsafe { std::env::set_var("GDK_PIXBUF_MODULE_FILE", &cache) },
            // Logging is not up yet this early, so the line goes to stderr.
            Err(err) => eprintln!("could not write the image loaders' cache: {err}"),
        }
    }
    let fonts = resources.join("etc/fonts/fonts.conf");
    if fonts.is_file() {
        // SAFETY: as above.
        unsafe { std::env::set_var("FONTCONFIG_FILE", &fonts) };
    }
}

/// Puts the folders that hold the programs Iris runs on `PATH`: gpg and
/// gpgsm for signing and encryption, pdftotext for the assistant. An app
/// opened from the Dock or Finder starts with macOS's bare
/// `/usr/bin:/bin:/usr/sbin:/sbin`, so Homebrew's tools, and GPG Suite's
/// in `/usr/local/MacGPG2/bin`, were missing unless Iris started from a
/// terminal. The folders `path_helper` reads for a login shell come first,
/// then Homebrew's; what the PATH already holds stays first. Runs first
/// thing in `main`, before any other thread reads the environment.
pub fn add_tool_paths() {
    let current = std::env::var_os("PATH").unwrap_or_default();
    let mut candidates = Vec::new();
    for listing in std::iter::once(PathBuf::from("/etc/paths")).chain(paths_d()) {
        if let Ok(text) = std::fs::read_to_string(&listing) {
            candidates.extend(text.lines().map(str::trim).filter(|l| !l.is_empty()).map(PathBuf::from));
        }
    }
    candidates.extend(["/opt/homebrew/bin", "/opt/homebrew/sbin", "/usr/local/bin"].map(PathBuf::from));
    let joined = with_tool_dirs(&current, candidates, |dir| dir.is_dir());
    // SAFETY: called first thing in `main`, before any other thread exists
    // to read the environment.
    unsafe { std::env::set_var("PATH", joined) };
}

/// The files in `/etc/paths.d`, in the order `path_helper` reads them.
fn paths_d() -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = std::fs::read_dir("/etc/paths.d")
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| entry.path())
        .collect();
    files.sort();
    files
}

/// `current` with each of `candidates` that `exists` added at the end,
/// once.
fn with_tool_dirs(
    current: &std::ffi::OsStr,
    candidates: Vec<PathBuf>,
    exists: impl Fn(&std::path::Path) -> bool,
) -> std::ffi::OsString {
    let mut dirs: Vec<PathBuf> = std::env::split_paths(current).collect();
    for dir in candidates {
        if !dirs.contains(&dir) && exists(&dir) {
            dirs.push(dir);
        }
    }
    std::env::join_paths(dirs).unwrap_or_else(|_| current.to_os_string())
}

/// Binds GTK's, GLib's and libadwaita's own words to the bundle's
/// catalogues. Each library binds its domain to the folder it was built
/// for as it starts, so this runs once GTK and libadwaita are up.
pub fn bind_toolkit_translations() {
    let Some(resources) = resources() else { return };
    let locale = resources.join("share/locale");
    if !has_catalogues(&locale) {
        return;
    }
    for domain in ["gtk40", "glib20", "libadwaita"] {
        if let Err(err) = gettextrs::bindtextdomain(domain, &locale) {
            tracing::warn!(domain, error = %err, "could not bind the toolkit's translations");
        }
        let _ = gettextrs::bind_textdomain_codeset(domain, "UTF-8");
    }
}

fn has_catalogues(locale: &Path) -> bool {
    std::fs::read_dir(locale).is_ok_and(|mut entries| entries.next().is_some())
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::with_tool_dirs;

    #[test]
    fn tool_folders_join_the_path_after_what_it_holds() {
        let joined = with_tool_dirs(
            "/usr/bin:/bin".as_ref(),
            vec![
                PathBuf::from("/bin"),
                PathBuf::from("/opt/homebrew/bin"),
                PathBuf::from("/nowhere"),
            ],
            |dir| dir != std::path::Path::new("/nowhere"),
        );
        assert_eq!(joined, "/usr/bin:/bin:/opt/homebrew/bin");
    }
}
