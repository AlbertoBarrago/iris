//! Gmail's label palette, shared by the windows and the assistant.

use mailrs_domain::translate::gettext;

/// Label colours from Gmail's palette: background and text.
/// [`label_color_name`] names them in the reader's language.
pub const LABEL_COLORS: [(&str, &str); 9] = [
    ("#fb4c2f", "#ffffff"),
    ("#ffad47", "#ffffff"),
    ("#fad165", "#000000"),
    ("#16a766", "#ffffff"),
    ("#2da2bb", "#ffffff"),
    ("#4a86e8", "#ffffff"),
    ("#a479e2", "#ffffff"),
    ("#f691b3", "#ffffff"),
    ("#999999", "#ffffff"),
];

/// What the label colour menu calls the colour at `index`.
pub fn label_color_name(index: usize) -> String {
    match index {
        0 => gettext("Red"),
        1 => gettext("Orange"),
        2 => gettext("Yellow"),
        3 => gettext("Green"),
        4 => gettext("Teal"),
        5 => gettext("Blue"),
        6 => gettext("Purple"),
        7 => gettext("Pink"),
        _ => gettext("Gray"),
    }
}
