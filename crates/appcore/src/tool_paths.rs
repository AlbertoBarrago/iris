//! The folders that hold the programs Iris runs, put on `PATH` for an
//! app opened from the Dock. Both front ends call this first thing.

use std::path::PathBuf;

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
