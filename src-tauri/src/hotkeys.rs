use tauri_plugin_global_shortcut::{Code, Modifiers, Shortcut, ShortcutState};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HotkeyAction { Menu, NewRoom }

pub fn bindings() -> [(Shortcut, HotkeyAction, &'static str); 2] {
    let modifiers = Some(Modifiers::CONTROL | Modifiers::ALT);
    [
        (Shortcut::new(modifiers, Code::KeyM), HotkeyAction::Menu, "Ctrl+Alt+M"),
        (Shortcut::new(modifiers, Code::KeyN), HotkeyAction::NewRoom, "Ctrl+Alt+N"),
    ]
}

pub fn action(shortcut: &Shortcut, state: ShortcutState) -> Option<HotkeyAction> {
    if state != ShortcutState::Pressed { return None; }
    bindings().iter().find(|(key, _, _)| key == shortcut).map(|(_, action, _)| *action)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn shortcuts_only_dispatch_their_own_pressed_combination() {
        for (shortcut, expected, _) in bindings() {
            assert_eq!(action(&shortcut, ShortcutState::Pressed), Some(expected));
            assert_eq!(action(&shortcut, ShortcutState::Released), None);
        }
        for modifiers in [None, Some(Modifiers::CONTROL), Some(Modifiers::ALT), Some(Modifiers::CONTROL | Modifiers::ALT | Modifiers::SHIFT)] {
            assert_eq!(action(&Shortcut::new(modifiers, Code::KeyN), ShortcutState::Pressed), None);
        }
        assert_ne!(bindings()[0].0.id(), bindings()[1].0.id());
    }
}
