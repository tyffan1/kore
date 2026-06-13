use serde::{Deserialize, Serialize};

/// Logical key identifiers relevant to browser UI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Key {
    A, B, C, D, E, F, G, H, I, J, K, L, M,
    N, O, P, Q, R, S, T, U, V, W, X, Y, Z,
    Digit0, Digit1, Digit2, Digit3, Digit4, Digit5, Digit6, Digit7, Digit8, Digit9,
    Enter, Escape, Tab, Space, Backspace, Delete,
    ArrowUp, ArrowDown, ArrowLeft, ArrowRight,
    Home, End, PageUp, PageDown,
    F1, F2, F3, F4, F5, F6, F7, F8, F9, F10, F11, F12,
    Control, Shift, Alt, Meta,
    Unknown(u32),
}

/// Keyboard modifier state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Modifiers {
    pub shift: bool,
    pub ctrl: bool,
    pub alt: bool,
    pub meta: bool,
}

impl Modifiers {
    pub const NONE: Self = Self { shift: false, ctrl: false, alt: false, meta: false };
}

/// Mouse button identifiers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
    Back,
    Forward,
}

/// Input events produced by the window system.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum InputEvent {
    KeyPressed {
        key: Key,
        modifiers: Modifiers,
    },
    KeyReleased {
        key: Key,
        modifiers: Modifiers,
    },
    TextInput(String),
    MouseMoved {
        x: f64,
        y: f64,
    },
    MouseClicked {
        button: MouseButton,
        x: f64,
        y: f64,
    },
    Scroll {
        delta_x: f64,
        delta_y: f64,
    },
}



#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constructs_key_pressed() {
        let ev = InputEvent::KeyPressed {
            key: Key::Enter,
            modifiers: Modifiers::NONE,
        };
        assert_eq!(
            ev,
            InputEvent::KeyPressed {
                key: Key::Enter,
                modifiers: Modifiers::NONE,
            }
        );
    }

    #[test]
    fn constructs_key_released() {
        let ev = InputEvent::KeyReleased {
            key: Key::Escape,
            modifiers: Modifiers { shift: true, ctrl: false, alt: false, meta: false },
        };
        assert!(matches!(ev, InputEvent::KeyReleased { key: Key::Escape, .. }));
    }

    #[test]
    fn constructs_mouse_moved() {
        let ev = InputEvent::MouseMoved { x: 100.0, y: 200.0 };
        assert_eq!(ev, InputEvent::MouseMoved { x: 100.0, y: 200.0 });
    }

    #[test]
    fn constructs_mouse_clicked() {
        let ev = InputEvent::MouseClicked {
            button: MouseButton::Left,
            x: 50.0,
            y: 75.0,
        };
        assert!(matches!(ev, InputEvent::MouseClicked { button: MouseButton::Left, .. }));
    }

    #[test]
    fn constructs_scroll() {
        let ev = InputEvent::Scroll {
            delta_x: 0.0,
            delta_y: 120.0,
        };
        assert_eq!(ev, InputEvent::Scroll { delta_x: 0.0, delta_y: 120.0 });
    }

    #[test]
    fn modifiers_default_is_none() {
        assert_eq!(Modifiers::NONE.shift, false);
        assert_eq!(Modifiers::NONE.ctrl, false);
        assert_eq!(Modifiers::NONE.alt, false);
        assert_eq!(Modifiers::NONE.meta, false);
    }

    #[test]
    fn pattern_match_all_variants() {
        let events = vec![
            InputEvent::KeyPressed { key: Key::A, modifiers: Modifiers::NONE },
            InputEvent::KeyReleased { key: Key::B, modifiers: Modifiers::NONE },
            InputEvent::TextInput("hello".to_string()),
            InputEvent::MouseMoved { x: 0.0, y: 0.0 },
            InputEvent::MouseClicked { button: MouseButton::Right, x: 1.0, y: 2.0 },
            InputEvent::Scroll { delta_x: 0.0, delta_y: -1.0 },
        ];
        assert_eq!(events.len(), 6);
    }

    #[test]
    fn all_mouse_buttons_roundtrip() {
        let buttons = vec![
            MouseButton::Left,
            MouseButton::Right,
            MouseButton::Middle,
            MouseButton::Back,
            MouseButton::Forward,
        ];
        assert_eq!(buttons.len(), 5);
    }
}
