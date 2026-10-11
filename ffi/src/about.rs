//! The words of Iris Next's About window: who made it, what it is built
//! on, a short poem for the messenger it is named after, and where to
//! find it on GitHub.

use mailrs_domain::translate::{fill, gettext};

/// Where Iris lives.
const REPOSITORY: &str = "https://github.com/AlbertoBarrago/iris";

/// One link at the bottom of the window.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct AboutLink {
    pub title: String,
    /// A line under the title, such as the licence's name; empty for none.
    pub subtitle: String,
    pub url: String,
}

#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct AboutWords {
    pub title: String,
    pub name: String,
    pub author: String,
    pub author_url: String,
    /// "Based on {app}", with the app's name and page apart, so the
    /// window can make the name a link.
    pub based_on: String,
    pub upstream: String,
    pub upstream_url: String,
    pub comments: String,
    pub poem: Vec<String>,
    pub star_line: String,
    pub star_button: String,
    pub star_url: String,
    pub links: Vec<AboutLink>,
}

/// What the About window says.
#[uniffi::export]
pub fn about_words() -> AboutWords {
    AboutWords {
        title: gettext("About Iris"),
        name: gettext("Iris"),
        author: "Alberto Barrago (albz)".into(),
        author_url: "https://github.com/AlbertoBarrago".into(),
        based_on: gettext("Based on {app}"),
        upstream: "Penguin Mail".into(),
        upstream_url: "https://github.com/c9dev/penguin-mail".into(),
        comments: gettext("Mail and calendar for macOS. Your mail stays on your computer."),
        poem: vec![
            gettext("Iris carried word between the sky and the sea,"),
            gettext("and folded every message into light."),
            gettext("Here she keeps your letters close and quiet,"),
            gettext("with no cloud between you and the ones you write to."),
        ],
        star_line: gettext("If Iris makes your day a little lighter, leave it a star on GitHub."),
        star_button: gettext("Star on GitHub"),
        star_url: REPOSITORY.into(),
        links: vec![
            AboutLink {
                title: gettext("What's New"),
                subtitle: String::new(),
                url: format!("{REPOSITORY}/blob/main/CHANGELOG.md"),
            },
            AboutLink {
                title: gettext("Website"),
                subtitle: String::new(),
                url: REPOSITORY.into(),
            },
            AboutLink {
                title: gettext("Report a Problem"),
                subtitle: String::new(),
                url: format!("{REPOSITORY}/issues/new/choose"),
            },
            AboutLink {
                title: gettext("License"),
                subtitle: "GPL-3.0-or-later".into(),
                url: format!("{REPOSITORY}/blob/main/LICENSE"),
            },
        ],
    }
}

/// "Version 0.1.0", for `version` as the bundle reads it.
#[uniffi::export]
pub fn version_words(version: String) -> String {
    fill(&gettext("Version {version}"), &[("version", &version)])
}
