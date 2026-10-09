//! The About window: which Iris this is, who made it, where to read
//! about it, and a button that asks Sparkle for a newer version.
//! libadwaita's own About dialog takes no widgets of ours, so this one draws
//! its main page the same way and adds the button.

use std::rc::Rc;

use adw::prelude::*;
use mailrs_domain::translate::{fill, gettext};

pub(crate) const REPOSITORY: &str = "https://github.com/AlbertoBarrago/iris";

pub struct About {
    pub dialog: adw::Dialog,
}

impl About {
    pub fn new(can_update: bool, store: &std::path::Path, kept_here: &[String]) -> Rc<About> {
        let content = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(6)
            .margin_top(12)
            .margin_bottom(24)
            .margin_start(24)
            .margin_end(24)
            .build();

        let icon = gtk::Image::builder()
            .icon_name(crate::APP_ID)
            .pixel_size(128)
            .margin_bottom(12)
            .build();
        icon.add_css_class("icon-dropshadow");
        icon.set_accessible_role(gtk::AccessibleRole::Presentation);
        content.append(&icon);

        let name = gtk::Label::new(Some(&gettext("Iris")));
        name.add_css_class("title-1");
        name.set_wrap(true);
        content.append(&name);

        // A link, so the name opens the author's page as the rows below
        // open theirs.
        let developer = gtk::Label::builder()
            .label(r#"<a href="https://github.com/AlbertoBarrago">Alberto Barrago (albz)</a>"#)
            .use_markup(true)
            .build();
        content.append(&developer);

        // Iris is a modified Penguin Mail, which the GPL asks a copy to say.
        let upstream = gtk::Label::builder()
            .label(fill(
                &gettext("Based on {app}"),
                &[("app", r#"<a href="https://github.com/c9dev/penguin-mail">Penguin Mail</a>"#)],
            ))
            .use_markup(true)
            .css_classes(["dim-label", "caption"])
            .build();
        content.append(&upstream);

        let version = gtk::Label::new(Some(&fill(
            &gettext("Version {version}"),
            &[("version", env!("CARGO_PKG_VERSION"))],
        )));
        version.add_css_class("dim-label");
        version.set_margin_top(6);
        content.append(&version);

        let comments = gtk::Label::builder()
            .label(gettext(
                "Mail and calendar for macOS. Your mail stays on your computer.",
            ))
            .wrap(true)
            .justify(gtk::Justification::Center)
            .max_width_chars(40)
            .margin_top(12)
            .build();
        content.append(&comments);

        // A POP3 account's mail lives only in the store, so About names the
        // file to back up and says whose mail it holds. The store runs in
        // WAL mode: while the app runs, a copy of this file alone can miss
        // the newest mail, so the line says to quit first.
        if !kept_here.is_empty() {
            // A path breaks only after a "/": broken anywhere, "mailrs."
            // and "db" landed on two lines and read as two names.
            let path = gtk::Label::builder()
                .label(path_markup(&store.display().to_string()))
                .use_markup(true)
                .selectable(true)
                .wrap(true)
                .wrap_mode(gtk::pango::WrapMode::Word)
                .justify(gtk::Justification::Center)
                .margin_top(12)
                .build();
            path.add_css_class("caption");
            path.add_css_class("monospace");
            content.append(&path);
            for line in kept_here {
                let label = gtk::Label::builder()
                    .label(line)
                    .wrap(true)
                    .justify(gtk::Justification::Center)
                    .max_width_chars(40)
                    .build();
                label.add_css_class("caption");
                label.add_css_class("dim-label");
                content.append(&label);
            }
        }

        // Sparkle answers in a window of its own, so the button only asks.
        // Hidden when this copy never updates: the demo and a cargo build.
        let button = gtk::Button::builder()
            .label(gettext("Check for Updates"))
            .action_name("app.check-for-updates")
            .halign(gtk::Align::Center)
            .margin_top(18)
            .visible(can_update)
            .build();
        button.add_css_class("pill");
        content.append(&button);

        let links = gtk::ListBox::builder()
            .selection_mode(gtk::SelectionMode::None)
            .margin_top(24)
            .build();
        links.add_css_class("boxed-list");
        let link = |title: String, subtitle: Option<&str>, uri: String| {
            let row = adw::ActionRow::builder()
                .title(title)
                .activatable(true)
                .build();
            if let Some(subtitle) = subtitle {
                row.set_subtitle(subtitle);
            }
            row.add_suffix(&gtk::Image::from_icon_name("adw-external-link-symbolic"));
            row.connect_activated(move |row| {
                let window = row.root().and_downcast::<gtk::Window>();
                gtk::UriLauncher::new(&uri).launch(
                    window.as_ref(),
                    None::<&gtk::gio::Cancellable>,
                    |_| {},
                );
            });
            links.append(&row);
        };
        link(
            gettext("What's New"),
            None,
            format!("{REPOSITORY}/blob/main/CHANGELOG.md"),
        );
        link(gettext("Website"), None, REPOSITORY.into());
        link(
            gettext("Report a Problem"),
            None,
            format!("{REPOSITORY}/issues/new/choose"),
        );
        link(
            gettext("License"),
            Some("GPL-3.0-or-later"),
            format!("{REPOSITORY}/blob/main/LICENSE"),
        );
        content.append(&links);

        let header = adw::HeaderBar::builder().show_title(false).build();
        let view = adw::ToolbarView::new();
        view.add_top_bar(&header);
        view.set_content(Some(
            &gtk::ScrolledWindow::builder()
                .hscrollbar_policy(gtk::PolicyType::Never)
                .propagate_natural_height(true)
                .child(&content)
                .build(),
        ));
        let dialog = adw::Dialog::builder()
            .title(gettext("About Iris"))
            .content_width(380)
            .child(&view)
            .build();

        Rc::new(About { dialog })
    }
}

/// `path` as Pango markup that lets a line break only after a "/". Each
/// name between separators goes in a span that forbids breaks inside
/// it, so "mailrs.db" or "iris" never splits, and the label's
/// text, the one a selection copies, stays the path itself. The
/// separators stay outside the spans: GTK merges touching spans that
/// forbid breaks into one, which forbade every break in the path.
fn path_markup(path: &str) -> String {
    path.split('/')
        .map(|name| match name {
            "" => String::new(),
            name => format!(
                "<span allow_breaks=\"false\">{}</span>",
                gtk::glib::markup_escape_text(name)
            ),
        })
        .collect::<Vec<_>>()
        .join("/")
}

#[cfg(test)]
mod tests {
    use super::path_markup;

    #[test]
    fn a_path_may_break_only_after_a_separator() {
        assert_eq!(
            path_markup("/tmp/iris-demo/mailrs.db"),
            "/<span allow_breaks=\"false\">tmp</span>\
             /<span allow_breaks=\"false\">iris-demo</span>\
             /<span allow_breaks=\"false\">mailrs.db</span>"
        );
    }

    #[test]
    fn a_path_is_escaped_for_markup() {
        assert_eq!(
            path_markup("/home/r&d/<x>"),
            "/<span allow_breaks=\"false\">home</span>\
             /<span allow_breaks=\"false\">r&amp;d</span>\
             /<span allow_breaks=\"false\">&lt;x&gt;</span>"
        );
    }

    #[test]
    fn a_path_keeps_every_separator_once() {
        assert_eq!(path_markup("a//b/"), "<span allow_breaks=\"false\">a</span>//<span allow_breaks=\"false\">b</span>/");
    }
}
