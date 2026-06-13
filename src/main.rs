use std::cell::RefCell;

use kore_browser::BrowserApp;
use kore_css::parse_stylesheet;
use kore_gpu::{Color, DisplayCommand, DisplayList, DrawRect, Renderer, RendererConfig};
use kore_html::parse_document;
use kore_layout::{layout_document, LayoutConfig};
use kore_net::{FetchRequest, HttpClient};
use kore_ui::Theme;
use kore_window::{AppEvent, EventLoop, InputEvent, Key, WindowBuilder, WindowHandle};

const DEFAULT_CSS: &str = r#"
html, body, div, p, h1, h2, h3, h4, h5, h6, ul, ol, li,
header, footer, main, nav, section, article, aside,
figure, figcaption, blockquote, dl, dt, dd, form, table {
    display: block;
}
head, script, style, link, meta, title {
    display: none;
}
"#;

struct AppState {
    browser: BrowserApp,
    http_client: HttpClient,
    theme: Theme,
    display_list: DisplayList,
    content_display_list: DisplayList,
    address_bar_focused: bool,
    url_buffer: String,
    ctrl_pressed: bool,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let session_path = std::env::temp_dir().join("kore_session.json");
    let mut browser = BrowserApp::new(session_path);
    browser.init()?;

    if browser.tab_count() == 0 {
        let default_url = url::Url::parse("about:blank")?;
        browser.open_tab(default_url)?;
    }

    let config = WindowBuilder::new()
        .title("Kore")
        .size(1280, 720)
        .build();

    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::default());
    let el = EventLoop::new()?;

    let state = RefCell::new(AppState {
        browser,
        http_client: HttpClient::default(),
        theme: Theme::System,
        display_list: DisplayList::new(),
        content_display_list: DisplayList::new(),
        address_bar_focused: false,
        url_buffer: String::new(),
        ctrl_pressed: false,
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
                    return;
                }

                {
                    let mut s = state.borrow_mut();
                    build_display_list(&mut s);
                }

                if let Some(r) = renderer.borrow_mut().as_mut() {
                    let display_list = &state.borrow().display_list;
                    match r.begin_frame() {
                        Ok(mut frame) => {
                            r.submit(&mut frame, display_list);
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

            AppEvent::Input(input) => {
                let mut s = state.borrow_mut();
                handle_input(&mut s, input);
            }

            AppEvent::Resized { width, height } => {
                if let Some(r) = renderer.borrow_mut().as_mut() {
                    r.resize(width, height);
                }
            }

            AppEvent::CloseRequested => {
                let s = state.borrow();
                if let Err(e) = s.browser.shutdown() {
                    eprintln!("Error saving session: {e}");
                }
                elwt.exit();
            }

            _ => {}
        }
    });
}

fn handle_input(state: &mut AppState, event: InputEvent) {
    match event {
        InputEvent::TextInput(ch) => {
            if state.address_bar_focused && !ch.is_empty() {
                state.url_buffer.push_str(&ch);
            }
        }

        InputEvent::KeyPressed { key, modifiers: _ } => {
            if key == Key::Control {
                state.ctrl_pressed = true;
            }

            if state.address_bar_focused {
                match key {
                    Key::Enter => {
                        let url_str = state.url_buffer.clone();
                        if !url_str.is_empty() {
                            let url = if url_str.starts_with("http://") || url_str.starts_with("https://") {
                                url::Url::parse(&url_str)
                            } else {
                                url::Url::parse(&format!("https://{url_str}"))
                            };
                            if let Ok(url) = url {
                                if let Some(tab) = state.browser.tab_manager.active_tab_mut() {
                                    tab.url = url.clone();
                                }
                                navigate(state, url);
                            }
                        }
                        state.address_bar_focused = false;
                    }
                    Key::Escape => {
                        state.address_bar_focused = false;
                    }
                    Key::Backspace => {
                        state.url_buffer.pop();
                    }
                    _ => {}
                }
                return;
            }

            if state.ctrl_pressed {
                match key {
                    Key::T => {
                        let Ok(url) = url::Url::parse("about:blank") else { return };
                        let _ = state.browser.open_tab(url);
                    }
                    Key::W => {
                        if let Some(active) = state.browser.tab_manager.active_tab() {
                            let id = active.id;
                            let _ = state.browser.close_tab(id);
                        }
                        if state.browser.tab_count() == 0 {
                            let Ok(url) = url::Url::parse("about:blank") else { return };
                            let _ = state.browser.open_tab(url);
                        }
                    }
                    Key::L => {
                        state.address_bar_focused = true;
                        if let Some(active) = state.browser.tab_manager.active_tab() {
                            state.url_buffer = active.url.as_str().to_string();
                        }
                    }
                    _ => {}
                }
            }
        }

        InputEvent::KeyReleased { key, .. } => {
            if key == Key::Control {
                state.ctrl_pressed = false;
            }
        }

        _ => {}
    }
}

fn navigate(state: &mut AppState, url: url::Url) {
    if url.as_str() == "about:blank" || url.as_str() == "about:newtab" {
        state.content_display_list.clear();
        return;
    }

    let request = match FetchRequest::get(url.as_str()) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("FetchRequest error: {e}");
            return;
        }
    };

    let response = match pollster::block_on(state.http_client.fetch(request)) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("HTTP fetch error: {e}");
            return;
        }
    };

    let html_str = match String::from_utf8(response.body.to_vec()) {
        Ok(s) => s,
        Err(_) => {
            eprintln!("Response body is not valid UTF-8");
            return;
        }
    };

    let document = match parse_document(&html_str) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("HTML parse error: {e:?}");
            return;
        }
    };

    let stylesheet = match parse_stylesheet(DEFAULT_CSS) {
        Ok(s) => s,
        Err(_) => {
            eprintln!("Default CSS parse error");
            return;
        }
    };

    let (content_w, content_h) = (1264.0, 628.0);
    let layout_config = LayoutConfig {
        viewport_width: content_w,
        viewport_height: content_h,
    };

    let layout_tree = match layout_document(&document, &stylesheet, layout_config) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("Layout error: {e:?}");
            return;
        }
    };

    let mut dl = DisplayList::new();
    for node in &layout_tree.nodes {
        if node.rect.width > 0.0 && node.rect.height > 0.0 {
            dl.push_rect(DrawRect {
                x: node.rect.x,
                y: node.rect.y,
                width: node.rect.width,
                height: node.rect.height,
                color: Color::from_rgba8(200, 200, 220, 255),
            });
        }
    }
    state.content_display_list = dl;
}

fn build_display_list(state: &mut AppState) {
    let width = 1280.0;
    let height = 720.0;

    let list = &mut state.display_list;
    list.clear();

    list.push_rect(DrawRect {
        x: 0.0,
        y: 0.0,
        width,
        height,
        color: Color::from_rgba8(240, 240, 245, 255),
    });

    list.push_rect(DrawRect {
        x: 0.0,
        y: 0.0,
        width,
        height: 36.0,
        color: Color::from_rgba8(30, 30, 35, 255),
    });

    let tabs = state.browser.list_tabs().to_vec();
    for (i, tab) in tabs.iter().enumerate() {
        let tx = 8.0 + (i as f32) * 180.0;
        let tab_color = if tab.is_active {
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

    list.push_rect(DrawRect {
        x: 8.0,
        y: 42.0,
        width: width - 16.0,
        height: 36.0,
        color: Color::from_rgba8(200, 200, 210, 255),
    });

    let url_bg = if state.address_bar_focused {
        Color::from_rgba8(255, 255, 230, 255)
    } else {
        Color::WHITE
    };
    list.push_rect(DrawRect {
        x: 10.0,
        y: 44.0,
        width: width - 20.0,
        height: 32.0,
        color: url_bg,
    });

    list.push_rect(DrawRect {
        x: 8.0,
        y: 84.0,
        width: width - 16.0,
        height: height - 92.0,
        color: Color::from_rgba8(255, 255, 255, 255),
    });

    for cmd in state.content_display_list.commands() {
        if let DisplayCommand::Rect(rect) = cmd {
            list.push_rect(DrawRect {
                x: 8.0 + rect.x,
                y: 84.0 + rect.y,
                width: rect.width,
                height: rect.height,
                color: rect.color,
            });
        }
    }
}
