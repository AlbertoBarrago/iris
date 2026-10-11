//! Dragging an event: the numbers live in appcore (`calendar_drag`),
//! shared with Iris Next; the grid's own keys, read off GDK, stay here.

pub use mailrs_appcore::calendar_drag::*;

/// What a key does to the event the time grid has focused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GridKey {
    /// Shift+Up or Shift+Down: move it by quarter hours.
    Move(i64),
    /// Ctrl+Shift+Up or Ctrl+Shift+Down: move its start.
    Start(i64),
    /// Shift+Alt+Up or Shift+Alt+Down: move its end.
    End(i64),
    /// Shift+Left or Shift+Right: move it by days.
    Days(i64),
}

/// The change `key` with `modifiers` makes to the focused event, when it
/// is one of the grid's own keys, which all hold Shift.
pub fn grid_key(key: gtk::gdk::Key, modifiers: gtk::gdk::ModifierType) -> Option<GridKey> {
    use gtk::gdk::{Key, ModifierType};
    if !modifiers.contains(ModifierType::SHIFT_MASK) {
        return None;
    }
    let alt = modifiers.contains(ModifierType::ALT_MASK);
    // `<Control>` in the key table, Command on macOS.
    let control = modifiers.contains(crate::ui::platform_keys::PRIMARY);
    let steps = match key {
        Key::Up => -1,
        Key::Down => 1,
        Key::Left if !alt && !control => return Some(GridKey::Days(-1)),
        Key::Right if !alt && !control => return Some(GridKey::Days(1)),
        _ => return None,
    };
    match (alt, control) {
        (false, false) => Some(GridKey::Move(steps)),
        (true, false) => Some(GridKey::End(steps)),
        (false, true) => Some(GridKey::Start(steps)),
        (true, true) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_grid_keys_move_resize_and_carry_the_focused_event() {
        use gtk::gdk::{Key, ModifierType as M};
        let shift = M::SHIFT_MASK;
        assert_eq!(grid_key(Key::Up, shift), Some(GridKey::Move(-1)));
        assert_eq!(grid_key(Key::Down, shift | M::ALT_MASK), Some(GridKey::End(1)));
        assert_eq!(grid_key(Key::Up, shift | crate::ui::platform_keys::PRIMARY), Some(GridKey::Start(-1)));
        assert_eq!(grid_key(Key::Right, shift), Some(GridKey::Days(1)));
    }

    #[test]
    fn the_grid_keys_want_shift() {
        use gtk::gdk::{Key, ModifierType as M};
        assert_eq!(grid_key(Key::Up, M::empty()), None);
        assert_eq!(grid_key(Key::Up, crate::ui::platform_keys::PRIMARY), None);
        assert_eq!(grid_key(Key::Up, M::SHIFT_MASK | crate::ui::platform_keys::PRIMARY | M::ALT_MASK), None);
    }
}
