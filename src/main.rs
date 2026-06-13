use std::cell::RefCell;

use kore_browser::BrowserApp;
use kore_gpu::{Color, DisplayList, DrawRect, Renderer, RendererConfig};
use kore_ui::Theme;
use kore_window::{AppEvent, EventLoop, WindowBuilder, WindowHandle};

struct UiData {
    tabs: Vec<(u64, bool, String)>,
}

struct AppState {
    browser: BrowserApp,
    _theme: Theme,
    display_list: DisplayList,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let session_path = std::env::temp_dir().join("kore_session.json");
    let mut browser = BrowserApp::new(session_path);
    browser.init()?;

    if browser.tab_count() == 0 {
        let default_url = url::Url::parse("https://example.com/")?;
        browser.open_tab(default_url)?;
    }

    let config = WindowBuilder::new()
        .title("Kore")
        .size(1280, 720)
        .build();

    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::default());

    let el = EventLoop::new()?;

    let app_state = RefCell::new(AppState {
        browser,
        _theme: Theme::System,
        display_list: DisplayList::new(),
    });

    let window = RefCell::new(None::<std::sync::Arc<winit::window::Window>>);
    let renderer = RefCell::new(None::<Renderer>);

    el.run(move |event, elwt| {
        match event {
            AppEvent::Redraw => {
                if renderer.borrow().is_none() {
                    match WindowHandle::new(elwt, &instance, &config) {
                        Ok(handle) => {
                            let (w, surface) = handle.into_parts();
                            match pollster::block_on(Renderer::new(
                                &instance,
                                surface,
                                RendererConfig::default(),
                            )) {
                                Ok(r) => {
                                    w.request_redraw();
                                    *window.borrow_mut() = Some(w);
                                    *renderer.borrow_mut() = Some(r);
                                }
                                Err(e) => {
                                    eprintln!("Renderer init error: {e}");
                                }
                            }
                        }
                        Err(e) => {
                            eprintln!("Window init error: {e}");
                        }
                    }
                }

                let ui = {
                    let state = app_state.borrow();
                    let tabs = state
                        .browser
                        .list_tabs()
                        .iter()
                        .map(|t| (t.id, t.is_active, t.title.clone()))
                        .collect();
                    UiData { tabs }
                };

                {
                    let mut list = app_state.borrow_mut();
                    build_ui_display_list(&mut list.display_list, &ui);
                }

                if let Some(r) = renderer.borrow_mut().as_mut() {
                    let list = &app_state.borrow().display_list;
                    match r.begin_frame() {
                        Ok(mut frame) => {
                            r.submit(&mut frame, list);
                            if let Err(e) = r.end_frame(frame) {
                                eprintln!("Render error: {e}");
                            }
                            if let Some(ref w) = *window.borrow() {
                                w.request_redraw();
                            }
                        }
                        Err(e) => {
                            eprintln!("Begin frame error: {e}");
                        }
                    }
                }
            }
            AppEvent::Input(kore_window::InputEvent::KeyPressed {
                key,
                modifiers: _,
            }) => {
                match key {
                    kore_window::Key::F5 | kore_window::Key::F11 => {}
                    _ => {}
                }
            }
            AppEvent::Resized { width, height } => {
                if let Some(r) = renderer.borrow_mut().as_mut() {
                    r.resize(width, height);
                }
            }
            AppEvent::CloseRequested => {
                let state = app_state.borrow();
                if let Err(e) = state.browser.shutdown() {
                    eprintln!("Error saving session: {e}");
                }
            }
            _ => {}
        }
    });
}

fn build_ui_display_list(list: &mut DisplayList, ui: &UiData) {
    let width = 1280.0;
    let height = 720.0;

    list.clear();

    list.push_rect(DrawRect {
        x: 0.0,
        y: 0.0,
        width,
        height,
        color: Color::from_rgba8(240, 240, 245, 255),
    });

    // Tab bar background
    list.push_rect(DrawRect {
        x: 0.0,
        y: 0.0,
        width,
        height: 36.0,
        color: Color::from_rgba8(30, 30, 35, 255),
    });

    for (i, (_id, is_active, _title)) in ui.tabs.iter().enumerate() {
        let tx = 8.0 + (i as f32) * 180.0;
        let tab_color = if *is_active {
            Color::from_rgba8(50, 50, 55, 255)
        } else {
            Color::from_rgba8(40, 40, 45, 255)
        };
        list.push_rect(DrawRect {
            x: tx,
            y: 4.0,
            width: 170.0,
            height: 28.0,
            color: tab_color,
        });
    }

    // URL bar border
    list.push_rect(DrawRect {
        x: 8.0,
        y: 42.0,
        width: width - 16.0,
        height: 36.0,
        color: Color::from_rgba8(200, 200, 210, 255),
    });

    // URL bar inner
    list.push_rect(DrawRect {
        x: 10.0,
        y: 44.0,
        width: width - 20.0,
        height: 32.0,
        color: Color::from_rgba8(255, 255, 255, 255),
    });

    // Content area
    list.push_rect(DrawRect {
        x: 8.0,
        y: 84.0,
        width: width - 16.0,
        height: height - 92.0,
        color: Color::from_rgba8(255, 255, 255, 255),
    });

    for (i, (_id, is_active, _title)) in ui.tabs.iter().enumerate() {
        if *is_active {
            list.push_rect(DrawRect {
                x: 40.0,
                y: 120.0,
                width: 300.0,
                height: 200.0,
                color: Color::from_rgba8(
                    66 + (i as u8) * 40 % 200,
                    133 + (i as u8) * 30 % 200,
                    244 - (i as u8) * 20 % 200,
                    255,
                ),
            });
        }
    }
}
