//! The person's folders, resolved as GLib resolves them on macOS: the XDG
//! variables when set, else the XDG defaults under the home folder. The
//! GTK app read these from GLib, so the paths stay the same without it.

use std::path::PathBuf;

pub fn home_dir() -> PathBuf {
    std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default()
}

pub fn user_config_dir() -> PathBuf {
    xdg("XDG_CONFIG_HOME", ".config")
}

pub fn user_data_dir() -> PathBuf {
    xdg("XDG_DATA_HOME", ".local/share")
}

pub fn user_cache_dir() -> PathBuf {
    xdg("XDG_CACHE_HOME", ".cache")
}

/// `variable` when it names an absolute folder, else `fallback` under home.
fn xdg(variable: &str, fallback: &str) -> PathBuf {
    std::env::var_os(variable)
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .unwrap_or_else(|| home_dir().join(fallback))
}
