use freya::prelude::*;

use crate::state::NavDirection;

const MAC: bool = cfg!(target_os = "macos");

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shortcut {
    NewTab,
    CloseTab,
    AddProject,
    NextTab,
    PrevTab,
    SplitVertical,
    SplitHorizontal,
    ClosePanel,
    CloseOtherPanels,
    ToggleSidebar,
    Navigate(NavDirection),
    IncreaseFontSize,
    DecreaseFontSize,
    Copy,
    Paste,
}

/// Cmd on macOS, Ctrl+Shift elsewhere, since plain Ctrl belongs to the shell there.
fn command_shift(modifiers: Modifiers) -> bool {
    modifiers.contains(Modifiers::ctrl_or_meta()) && (MAC || modifiers.contains(Modifiers::SHIFT))
}

/// Cmd combos on macOS never reach the shell, even when unbound.
pub fn reserved_for_app(modifiers: Modifiers) -> bool {
    MAC && modifiers.contains(Modifiers::META)
}

/// Maps a key event to the shortcut it triggers, if any.
pub fn resolve(event: &KeyboardEventData) -> Option<Shortcut> {
    let modifiers = event.modifiers;
    let ctrl = modifiers.contains(Modifiers::CONTROL);
    let shift = modifiers.contains(Modifiers::SHIFT);
    let alt = modifiers.contains(Modifiers::ALT);

    if let Key::Named(named) = &event.key {
        return match named {
            NamedKey::Tab if ctrl && shift => Some(Shortcut::PrevTab),
            NamedKey::Tab if ctrl => Some(Shortcut::NextTab),
            NamedKey::ArrowLeft if alt => Some(Shortcut::Navigate(NavDirection::Left)),
            NamedKey::ArrowRight if alt => Some(Shortcut::Navigate(NavDirection::Right)),
            NamedKey::ArrowUp if alt => Some(Shortcut::Navigate(NavDirection::Up)),
            NamedKey::ArrowDown if alt => Some(Shortcut::Navigate(NavDirection::Down)),
            _ => None,
        };
    }

    let Key::Character(character) = &event.key else {
        return None;
    };

    if alt {
        // Option remaps characters on macOS, so match the physical key there.
        let matched = if MAC {
            match event.code {
                Code::KeyP => Some(Shortcut::SplitVertical),
                Code::Equal | Code::NumpadAdd => Some(Shortcut::SplitHorizontal),
                Code::Minus | Code::NumpadSubtract => Some(Shortcut::ClosePanel),
                Code::Digit1 => Some(Shortcut::CloseOtherPanels),
                Code::KeyB => Some(Shortcut::ToggleSidebar),
                _ => None,
            }
        } else {
            match character.as_str() {
                "p" | "P" => Some(Shortcut::SplitVertical),
                "+" | "=" => Some(Shortcut::SplitHorizontal),
                "-" => Some(Shortcut::ClosePanel),
                "1" => Some(Shortcut::CloseOtherPanels),
                "b" | "B" => Some(Shortcut::ToggleSidebar),
                _ => None,
            }
        };
        if matched.is_some() {
            return matched;
        }
    }

    if command_shift(modifiers) {
        let matched = match character.as_str() {
            "t" | "T" => Some(Shortcut::NewTab),
            "w" | "W" => Some(Shortcut::CloseTab),
            "o" | "O" => Some(Shortcut::AddProject),
            "c" | "C" => Some(Shortcut::Copy),
            "v" | "V" => Some(Shortcut::Paste),
            _ => None,
        };
        if matched.is_some() {
            return matched;
        }
    }

    if modifiers.contains(Modifiers::ctrl_or_meta()) {
        return match character.as_str() {
            "+" | "=" => Some(Shortcut::IncreaseFontSize),
            "-" => Some(Shortcut::DecreaseFontSize),
            _ => None,
        };
    }

    None
}
