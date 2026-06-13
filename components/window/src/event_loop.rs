use winit::event_loop::ActiveEventLoop;

use crate::event::{InputEvent, Key, Modifiers, MouseButton};
use crate::error::WindowError;

/// Higher-level events that the application can process.
#[derive(Debug, Clone)]
pub enum AppEvent {
    /// An input event from keyboard, mouse, or scroll.
    Input(InputEvent),

    /// The window content needs to be redrawn.
    Redraw,

    /// The window was resized.
    Resized { width: u32, height: u32 },

    /// The user requested to close the window.
    CloseRequested,

    /// Focus state changed.
    FocusChanged(bool),
}

/// Wraps a winit event loop and maps events to the kore types.
pub struct EventLoop {
    inner: winit::event_loop::EventLoop<()>,
}

impl EventLoop {
    pub fn new() -> Result<Self, WindowError> {
        let inner = winit::event_loop::EventLoop::new()
            .map_err(|e| WindowError::EventLoop(format!("{e}")))?;
        Ok(Self { inner })
    }

    /// Run the event loop, calling `handler` for each event.
    ///
    /// This function never returns (it calls `process::exit` internally).
    #[allow(deprecated)]
    pub fn run<F>(self, mut handler: F) -> !
    where
        F: FnMut(AppEvent, &ActiveEventLoop) + 'static,
    {
        let mut modifiers = Modifiers::NONE;
        let outcome = self.inner.run(move |event, elwt| {
            for app_event in Self::map_event(&event, &mut modifiers) {
                handler(app_event, elwt);
            }
        });
        std::process::exit(match outcome {
            Ok(()) => 0,
            Err(_) => 1,
        })
    }

    /// Map a winit `Event` to zero or more `AppEvent`s.
    fn map_event(
        event: &winit::event::Event<()>,
        modifiers: &mut Modifiers,
    ) -> Vec<AppEvent> {
        match event {
            winit::event::Event::WindowEvent { event: winit_event, .. } => {
                Self::map_window_event(winit_event, modifiers)
            }
            winit::event::Event::AboutToWait => vec![AppEvent::Redraw],
            _ => vec![],
        }
    }

    /// Map a winit `WindowEvent` to zero or more `AppEvent`s.
    fn map_window_event(
        event: &winit::event::WindowEvent,
        modifiers: &mut Modifiers,
    ) -> Vec<AppEvent> {
        match event {
            winit::event::WindowEvent::Resized(size) => vec![AppEvent::Resized {
                width: size.width,
                height: size.height,
            }],
            winit::event::WindowEvent::CloseRequested => vec![AppEvent::CloseRequested],
            winit::event::WindowEvent::Focused(focused) => {
                vec![AppEvent::FocusChanged(*focused)]
            }
            winit::event::WindowEvent::ModifiersChanged(m) => {
                *modifiers = mods_from_winit(m.state());
                vec![]
            }
            winit::event::WindowEvent::KeyboardInput { event: ke, .. } => {
                let key = key_from_winit(&ke.physical_key);
                let mut events = Vec::new();

                // ALWAYS emit KeyPressed / KeyReleased for the physical key
                match ke.state {
                    winit::event::ElementState::Pressed => {
                        events.push(AppEvent::Input(InputEvent::KeyPressed {
                            key,
                            modifiers: *modifiers,
                        }));
                    }
                    winit::event::ElementState::Released => {
                        events.push(AppEvent::Input(InputEvent::KeyReleased {
                            key,
                            modifiers: *modifiers,
                        }));
                    }
                }

                // ALSO emit TextInput if text is available, this is a
                // printable key, and Ctrl/Meta is NOT held.
                if ke.state == winit::event::ElementState::Pressed {
                    if let Some(text) = &ke.text {
                        if !text.is_empty()
                            && !is_named_key(key)
                            && !modifiers.ctrl
                            && !modifiers.meta
                        {
                            events
                                .push(AppEvent::Input(InputEvent::TextInput(text.to_string())));
                        }
                    }
                }

                events
            }
            winit::event::WindowEvent::CursorMoved { position, .. } => {
                vec![AppEvent::Input(InputEvent::MouseMoved {
                    x: position.x,
                    y: position.y,
                })]
            }
            winit::event::WindowEvent::MouseInput { button, state, .. } => {
                if *state == winit::event::ElementState::Pressed {
                    vec![AppEvent::Input(InputEvent::MouseClicked {
                        button: mouse_button_from_winit(*button),
                        x: 0.0,
                        y: 0.0,
                    })]
                } else {
                    vec![]
                }
            }
            winit::event::WindowEvent::MouseWheel { delta, .. } => {
                let (dx, dy) = match delta {
                    winit::event::MouseScrollDelta::LineDelta(x, y) => {
                        (*x as f64 * 40.0, *y as f64 * 40.0)
                    }
                    winit::event::MouseScrollDelta::PixelDelta(pos) => (pos.x, pos.y),
                };
                vec![AppEvent::Input(InputEvent::Scroll {
                    delta_x: dx,
                    delta_y: dy,
                })]
            }
            _ => vec![],
        }
    }
}

fn key_from_winit(key: &winit::keyboard::PhysicalKey) -> Key {
    use winit::keyboard::KeyCode;
    match key {
        winit::keyboard::PhysicalKey::Code(code) => match code {
            KeyCode::KeyA => Key::A,
            KeyCode::KeyB => Key::B,
            KeyCode::KeyC => Key::C,
            KeyCode::KeyD => Key::D,
            KeyCode::KeyE => Key::E,
            KeyCode::KeyF => Key::F,
            KeyCode::KeyG => Key::G,
            KeyCode::KeyH => Key::H,
            KeyCode::KeyI => Key::I,
            KeyCode::KeyJ => Key::J,
            KeyCode::KeyK => Key::K,
            KeyCode::KeyL => Key::L,
            KeyCode::KeyM => Key::M,
            KeyCode::KeyN => Key::N,
            KeyCode::KeyO => Key::O,
            KeyCode::KeyP => Key::P,
            KeyCode::KeyQ => Key::Q,
            KeyCode::KeyR => Key::R,
            KeyCode::KeyS => Key::S,
            KeyCode::KeyT => Key::T,
            KeyCode::KeyU => Key::U,
            KeyCode::KeyV => Key::V,
            KeyCode::KeyW => Key::W,
            KeyCode::KeyX => Key::X,
            KeyCode::KeyY => Key::Y,
            KeyCode::KeyZ => Key::Z,
            KeyCode::Digit0 => Key::Digit0,
            KeyCode::Digit1 => Key::Digit1,
            KeyCode::Digit2 => Key::Digit2,
            KeyCode::Digit3 => Key::Digit3,
            KeyCode::Digit4 => Key::Digit4,
            KeyCode::Digit5 => Key::Digit5,
            KeyCode::Digit6 => Key::Digit6,
            KeyCode::Digit7 => Key::Digit7,
            KeyCode::Digit8 => Key::Digit8,
            KeyCode::Digit9 => Key::Digit9,
            KeyCode::Enter => Key::Enter,
            KeyCode::Escape => Key::Escape,
            KeyCode::Tab => Key::Tab,
            KeyCode::Space => Key::Space,
            KeyCode::Backspace => Key::Backspace,
            KeyCode::Delete => Key::Delete,
            KeyCode::ArrowUp => Key::ArrowUp,
            KeyCode::ArrowDown => Key::ArrowDown,
            KeyCode::ArrowLeft => Key::ArrowLeft,
            KeyCode::ArrowRight => Key::ArrowRight,
            KeyCode::Home => Key::Home,
            KeyCode::End => Key::End,
            KeyCode::PageUp => Key::PageUp,
            KeyCode::PageDown => Key::PageDown,
            KeyCode::F1 => Key::F1,
            KeyCode::F2 => Key::F2,
            KeyCode::F3 => Key::F3,
            KeyCode::F4 => Key::F4,
            KeyCode::F5 => Key::F5,
            KeyCode::F6 => Key::F6,
            KeyCode::F7 => Key::F7,
            KeyCode::F8 => Key::F8,
            KeyCode::F9 => Key::F9,
            KeyCode::F10 => Key::F10,
            KeyCode::F11 => Key::F11,
            KeyCode::F12 => Key::F12,
            KeyCode::ControlLeft | KeyCode::ControlRight => Key::Control,
            KeyCode::ShiftLeft | KeyCode::ShiftRight => Key::Shift,
            KeyCode::AltLeft | KeyCode::AltRight => Key::Alt,
            KeyCode::SuperLeft | KeyCode::SuperRight => Key::Meta,
            _ => Key::Unknown(*code as u32),
        },
        winit::keyboard::PhysicalKey::Unidentified(_) => Key::Unknown(0),
    }
}

/// Keys that should only produce `KeyPressed` events, not `TextInput`.
fn is_named_key(key: Key) -> bool {
    matches!(
        key,
        Key::Enter
            | Key::Escape
            | Key::Tab
            | Key::Backspace
            | Key::Delete
            | Key::ArrowUp
            | Key::ArrowDown
            | Key::ArrowLeft
            | Key::ArrowRight
            | Key::Home
            | Key::End
            | Key::PageUp
            | Key::PageDown
            | Key::F1
            | Key::F2
            | Key::F3
            | Key::F4
            | Key::F5
            | Key::F6
            | Key::F7
            | Key::F8
            | Key::F9
            | Key::F10
            | Key::F11
            | Key::F12
            | Key::Control
            | Key::Shift
            | Key::Alt
            | Key::Meta
    )
}

fn mods_from_winit(mods: winit::keyboard::ModifiersState) -> Modifiers {
    Modifiers {
        shift: mods.shift_key(),
        ctrl: mods.control_key(),
        alt: mods.alt_key(),
        meta: mods.super_key(),
    }
}

fn mouse_button_from_winit(button: winit::event::MouseButton) -> MouseButton {
    use winit::event::MouseButton as WB;
    match button {
        WB::Left => MouseButton::Left,
        WB::Right => MouseButton::Right,
        WB::Middle => MouseButton::Middle,
        WB::Back => MouseButton::Back,
        WB::Forward => MouseButton::Forward,
        _ => MouseButton::Left,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::*;

    #[test]
    fn close_requested_maps_correctly() {
        let we = winit::event::WindowEvent::CloseRequested;
        let mapped = EventLoop::map_window_event(&we, &mut Modifiers::NONE);
        assert!(matches!(mapped[..], [AppEvent::CloseRequested]));
    }

    #[test]
    fn resize_maps_correctly() {
        let we = winit::event::WindowEvent::Resized(winit::dpi::PhysicalSize::new(800, 600));
        let mapped = EventLoop::map_window_event(&we, &mut Modifiers::NONE);
        assert!(matches!(
            mapped[..],
            [AppEvent::Resized {
                width: 800,
                height: 600
            }]
        ));
    }

    #[test]
    fn focus_changed_maps() {
        let we = winit::event::WindowEvent::Focused(true);
        let mapped = EventLoop::map_window_event(&we, &mut Modifiers::NONE);
        assert!(matches!(mapped[..], [AppEvent::FocusChanged(true)]));
    }

    #[test]
    fn key_event_maps_key_a() {
        let key = key_from_winit(&winit::keyboard::PhysicalKey::Code(
            winit::keyboard::KeyCode::KeyA,
        ));
        assert_eq!(key, Key::A);
    }

    #[test]
    fn key_event_maps_enter() {
        let key = key_from_winit(&winit::keyboard::PhysicalKey::Code(
            winit::keyboard::KeyCode::Enter,
        ));
        assert_eq!(key, Key::Enter);
    }

    #[test]
    fn key_event_maps_unknown_as_unknown() {
        let key = key_from_winit(&winit::keyboard::PhysicalKey::Code(
            winit::keyboard::KeyCode::AudioVolumeUp,
        ));
        assert!(matches!(key, Key::Unknown(_)));
    }

    #[test]
    fn named_key_backspace_is_named() {
        assert!(is_named_key(Key::Backspace));
    }

    #[test]
    fn named_key_delete_is_named() {
        assert!(is_named_key(Key::Delete));
    }

    #[test]
    fn letter_key_not_named() {
        assert!(!is_named_key(Key::V));
    }

    #[test]
    fn digit_key_not_named() {
        assert!(!is_named_key(Key::Digit0));
    }

    #[test]
    fn space_not_named() {
        assert!(!is_named_key(Key::Space));
    }

    #[test]
    fn modifiers_changed_updates_state() {
        let mut mods = Modifiers::NONE;
        let mut state = winit::keyboard::ModifiersState::empty();
        state.set(winit::keyboard::ModifiersState::CONTROL, true);
        let winit_mods = winit::event::Modifiers::from(state);
        let we = winit::event::WindowEvent::ModifiersChanged(winit_mods);
        let mapped = EventLoop::map_window_event(&we, &mut mods);
        assert!(mapped.is_empty());
        assert!(mods.ctrl);
        assert!(!mods.shift);
    }
}
