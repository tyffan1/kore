use std::cell::RefCell;
use std::sync::Arc;
use std::time::Instant;

use clipboard::ClipboardProvider;
use kore_browser::BrowserApp;
use kore_gpu::{ClipRect, Color, DisplayCommand, DisplayList, DrawRect, DrawText, Renderer, RendererConfig};
use kore_pipeline::Pipeline;
use kore_window::{AppEvent, EventLoop, InputEvent, Key, Modifiers, MouseButton, WindowBuilder, WindowHandle};

struct AppState {
    browser: BrowserApp,
    pipeline: Pipeline,
    rt: tokio::runtime::Runtime,
    display_list: DisplayList,
    content_display_list: DisplayList,
    page_links: Vec<(f32, f32, f32, f32, String)>,
    address_bar_focused: bool,
    url_buffer: String,
    cursor_pos: usize,
    selection_start: Option<usize>,
    ctrl_pressed: bool,
    shift_pressed: bool,
    mouse_x: f64,
    mouse_y: f64,
    loading: bool,
    page_title: Option<String>,
    cursor_visible: bool,
    last_cursor_blink: Instant,
    back_button_hover: bool,
    forward_button_hover: bool,
    reload_button_hover: bool,
    window_width: f32,
    window_height: f32,
    scroll_y: f32,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let _ = std::process::Command::new("cmd")
        .args(["/c", "chcp 65001"])
        .output();
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

    let rt = tokio::runtime::Runtime::new().unwrap();
    let state = RefCell::new(AppState {
        browser,
        pipeline: Pipeline::default(),
        rt,
        display_list: DisplayList::new(),
        content_display_list: DisplayList::new(),
        page_links: Vec::new(),
        address_bar_focused: false,
        url_buffer: String::new(),
        cursor_pos: 0,
        selection_start: None,
        ctrl_pressed: false,
        shift_pressed: false,
        mouse_x: 0.0,
        mouse_y: 0.0,
        loading: false,
        page_title: None,
        cursor_visible: true,
        last_cursor_blink: Instant::now(),
        back_button_hover: false,
        forward_button_hover: false,
        reload_button_hover: false,
        window_width: 1280.0,
        window_height: 720.0,
        scroll_y: 0.0,
    });

    let window = RefCell::new(None::<Arc<winit::window::Window>>);
    let renderer = RefCell::new(None::<Renderer>);

    el.run(move |event, elwt| {
        match event {
            AppEvent::Redraw => {
                if renderer.borrow().is_none() {
                    match WindowHandle::new(elwt, &instance, &config) {
                        Ok(handle) => {
                            let (w, surface) = handle.into_parts();
                            let rcfg = RendererConfig {
                                width: config.width,
                                height: config.height,
                                ..RendererConfig::default()
                            };
                            match pollster::block_on(Renderer::new(
                                &instance,
                                surface,
                                rcfg,
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

                if let Some(ref w) = *window.borrow() {
                    let s = state.borrow();
                    let title = s
                        .page_title
                        .as_deref()
                        .map(|t| format!("{t} - Kore"))
                        .unwrap_or_else(|| "Kore".to_string());
                    w.set_title(&title);
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
                state.borrow_mut().window_width = width as f32;
                state.borrow_mut().window_height = height as f32;
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
        InputEvent::KeyPressed { key, modifiers } => {
            match key {
                Key::Control => state.ctrl_pressed = true,
                Key::Shift => state.shift_pressed = true,
                _ => {}
            }

            // Handle Ctrl shortcuts first, regardless of address bar focus
            let is_ctrl = modifiers.ctrl || state.ctrl_pressed;
            if is_ctrl {
                handle_global_shortcuts(state, key, modifiers);
                // Return early for shortcuts that shouldn't also reach address bar
                match key {
                    Key::T | Key::W | Key::R => return,
                    _ => {}
                }
            }

            if state.address_bar_focused {
                handle_address_bar_key(state, key, modifiers);
            } else {
                handle_scroll_key(state, key);
            }
        }

        InputEvent::TextInput(ch) => {
            if state.address_bar_focused && !ch.is_empty() {
                if !state.ctrl_pressed && ch.chars().all(|c| !c.is_control()) {
                    handle_text_input(state, &ch);
                }
            }
        }

        InputEvent::KeyReleased { key, .. } => {
            match key {
                Key::Control => state.ctrl_pressed = false,
                Key::Shift => state.shift_pressed = false,
                _ => {}
            }
        }

        InputEvent::MouseMoved { x, y } => {
            state.mouse_x = x;
            state.mouse_y = y;
        }

        InputEvent::MouseClicked { button: MouseButton::Left, .. } => {
            handle_mouse_click(state, state.mouse_x, state.mouse_y);
        }

        InputEvent::Scroll { delta_y, .. } => {
            let new_scroll = state.scroll_y - delta_y as f32;
            state.scroll_y = new_scroll.max(0.0);
        }

        _ => {}
    }
}

fn handle_mouse_click(state: &mut AppState, x: f64, y: f64) {
    const ADDRESS_BAR_X: f64 = 10.0;
    const ADDRESS_BAR_Y: f64 = 44.0;
    const ADDRESS_BAR_WIDTH: f64 = 1280.0 - 20.0;
    const ADDRESS_BAR_HEIGHT: f64 = 32.0;

    const BACK_BTN_X: f64 = 8.0;
    const BACK_BTN_Y: f64 = 4.0;
    const BTN_SIZE: f64 = 28.0;

    const FORWARD_BTN_X: f64 = 44.0;
    const FORWARD_BTN_Y: f64 = 4.0;

    const RELOAD_BTN_X: f64 = 80.0;
    const RELOAD_BTN_Y: f64 = 4.0;

    let in_address_bar = x >= ADDRESS_BAR_X
        && x <= ADDRESS_BAR_X + ADDRESS_BAR_WIDTH
        && y >= ADDRESS_BAR_Y
        && y <= ADDRESS_BAR_Y + ADDRESS_BAR_HEIGHT;

    let in_back_btn = x >= BACK_BTN_X
        && x <= BACK_BTN_X + BTN_SIZE
        && y >= BACK_BTN_Y
        && y <= BACK_BTN_Y + BTN_SIZE;

    let in_forward_btn = x >= FORWARD_BTN_X
        && x <= FORWARD_BTN_X + BTN_SIZE
        && y >= FORWARD_BTN_Y
        && y <= FORWARD_BTN_Y + BTN_SIZE;

    let in_reload_btn = x >= RELOAD_BTN_X
        && x <= RELOAD_BTN_X + BTN_SIZE
        && y >= RELOAD_BTN_Y
        && y <= RELOAD_BTN_Y + BTN_SIZE;

    if in_address_bar {
        state.address_bar_focused = true;
        state.cursor_pos = state.url_buffer.chars().count();
        state.cursor_visible = true;
        state.last_cursor_blink = Instant::now();
    } else {
        state.address_bar_focused = false;
        state.selection_start = None;
    }

    if in_back_btn {
        if let Some(active) = state.browser.tab_manager.active_tab_mut() {
            if let Some(url) = active.go_back() {
                navigate(state, url);
            }
        }
    } else if in_forward_btn {
        if let Some(active) = state.browser.tab_manager.active_tab_mut() {
            if let Some(url) = active.go_forward() {
                navigate(state, url);
            }
        }
    } else if in_reload_btn {
        if let Some(active) = state.browser.tab_manager.active_tab() {
            let url = active.url.clone();
            navigate(state, url);
        }
        return;
    }

    // Check page links
    let sy = state.scroll_y;
    for (lx, ly, lw, lh, href) in &state.page_links {
        if x >= *lx as f64
            && x <= (*lx + *lw) as f64
            && y >= (*ly + 84.0 - sy) as f64
            && y <= (*ly + 84.0 + *lh - sy) as f64
        {
            if let Ok(url) = parse_url(href) {
                navigate(state, url);
            }
            break;
        }
    }
}

fn handle_text_input(state: &mut AppState, ch: &str) {
    if ch.is_empty() {
        return;
    }
    delete_selection(state);
    let mut buf: Vec<char> = state.url_buffer.chars().collect();
    for c in ch.chars() {
        buf.insert(state.cursor_pos, c);
        state.cursor_pos += 1;
    }
    state.url_buffer = buf.into_iter().collect();
    state.selection_start = None;
}

fn delete_selection(state: &mut AppState) {
    if let Some(start) = state.selection_start {
        let end = state.cursor_pos;
        if start != end {
            let (min, max) = if start < end { (start, end) } else { (end, start) };
            let mut buf: Vec<char> = state.url_buffer.chars().collect();
            buf.drain(min..max);
            state.url_buffer = buf.into_iter().collect();
            state.cursor_pos = min;
        }
    }
    state.selection_start = None;
}

fn handle_scroll_key(state: &mut AppState, key: Key) {
    match key {
        Key::ArrowDown => {
            state.scroll_y += 40.0;
        }
        Key::ArrowUp => {
            state.scroll_y = (state.scroll_y - 40.0).max(0.0);
        }
        Key::PageDown | Key::Space => {
            state.scroll_y += 400.0;
        }
        Key::PageUp => {
            state.scroll_y = (state.scroll_y - 400.0).max(0.0);
        }
        Key::Home => {
            state.scroll_y = 0.0;
        }
        _ => {}
    }
}

fn handle_address_bar_key(state: &mut AppState, key: Key, modifiers: Modifiers) {
    let is_ctrl = modifiers.ctrl || state.ctrl_pressed;
    let is_shift = modifiers.shift || state.shift_pressed;

    match key {
        Key::Enter => {
            let url_str = state.url_buffer.trim();
            if !url_str.is_empty() {
                let url = parse_url(url_str);
                if let Ok(url) = url {
                    if let Some(tab) = state.browser.tab_manager.active_tab_mut() {
                        tab.navigate(url.clone());
                    }
                    navigate(state, url);
                }
            }
            state.address_bar_focused = false;
        }
        Key::Escape => {
            state.address_bar_focused = false;
            state.selection_start = None;
        }
        Key::Backspace => {
            if state.selection_start.is_some() {
                delete_selection(state);
            } else if state.cursor_pos > 0 {
                let mut buf: Vec<char> = state.url_buffer.chars().collect();
                state.cursor_pos -= 1;
                buf.remove(state.cursor_pos);
                state.url_buffer = buf.into_iter().collect();
            }
        }
        Key::Delete => {
            if state.selection_start.is_some() {
                delete_selection(state);
            } else if state.cursor_pos < state.url_buffer.chars().count() {
                let mut buf: Vec<char> = state.url_buffer.chars().collect();
                buf.remove(state.cursor_pos);
                state.url_buffer = buf.into_iter().collect();
            }
        }
        Key::ArrowLeft => {
            if is_ctrl {
                state.cursor_pos = find_word_start(&state.url_buffer, state.cursor_pos);
            } else if state.cursor_pos > 0 {
                state.cursor_pos -= 1;
            }
            if !is_shift {
                state.selection_start = None;
            } else if state.selection_start.is_none() {
                state.selection_start = Some(state.cursor_pos);
            }
        }
        Key::ArrowRight => {
            if is_ctrl {
                state.cursor_pos = find_word_end(&state.url_buffer, state.cursor_pos);
            } else if state.cursor_pos < state.url_buffer.chars().count() {
                state.cursor_pos += 1;
            }
            if !is_shift {
                state.selection_start = None;
            } else if state.selection_start.is_none() {
                state.selection_start = Some(state.cursor_pos);
            }
        }
        Key::Home => {
            state.cursor_pos = 0;
            if !is_shift {
                state.selection_start = None;
            } else if state.selection_start.is_none() {
                state.selection_start = Some(state.cursor_pos);
            }
        }
        Key::End => {
            state.cursor_pos = state.url_buffer.chars().count();
            if !is_shift {
                state.selection_start = None;
            } else if state.selection_start.is_none() {
                state.selection_start = Some(state.cursor_pos);
            }
        }
        Key::A if is_ctrl => {
            state.cursor_pos = state.url_buffer.chars().count();
            state.selection_start = Some(0);
        }
        _ => {}
    }
    state.cursor_visible = true;
    state.last_cursor_blink = Instant::now();
}

fn handle_global_shortcuts(state: &mut AppState, key: Key, modifiers: Modifiers) {
    if !modifiers.ctrl && !state.ctrl_pressed {
        return;
    }

    match key {
        Key::V => {
            delete_selection(state);
            if let Ok(mut ctx) = clipboard::ClipboardContext::new() {
                if let Ok(text) = ctx.get_contents() {
                    let mut buf: Vec<char> = state.url_buffer.chars().collect();
                    for c in text.chars() {
                        buf.insert(state.cursor_pos, c);
                        state.cursor_pos += 1;
                    }
                    state.url_buffer = buf.into_iter().collect();
                }
            }
            state.selection_start = None;
        }
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
                state.cursor_pos = state.url_buffer.chars().count();
                state.selection_start = Some(0);
            }
        }
        Key::R => {
            if let Some(active) = state.browser.tab_manager.active_tab() {
                let url = active.url.clone();
                navigate(state, url);
            }
        }
        Key::ArrowLeft => {
            if let Some(active) = state.browser.tab_manager.active_tab_mut() {
                if let Some(url) = active.go_back() {
                    navigate(state, url);
                }
            }
        }
        Key::ArrowRight => {
            if let Some(active) = state.browser.tab_manager.active_tab_mut() {
                if let Some(url) = active.go_forward() {
                    navigate(state, url);
                }
            }
        }
        _ => {}
    }
}

fn parse_url(input: &str) -> Result<url::Url, url::ParseError> {
    if input.starts_with("http://") || input.starts_with("https://") || input.starts_with("about:") {
        url::Url::parse(input)
    } else {
        url::Url::parse(&format!("https://{input}"))
    }
}

fn find_word_start(s: &str, pos: usize) -> usize {
    let mut p = pos.saturating_sub(1);
    while p > 0 && s.chars().nth(p).map_or(false, |c| c.is_alphanumeric()) {
        p -= 1;
    }
    if p < pos && !s.chars().nth(p).map_or(false, |c| c.is_alphanumeric()) {
        p += 1;
    }
    p
}

fn find_word_end(s: &str, pos: usize) -> usize {
    let mut p = pos;
    while p < s.len() && s.chars().nth(p).map_or(false, |c| c.is_alphanumeric()) {
        p += 1;
    }
    p
}

fn navigate(state: &mut AppState, url: url::Url) {
    state.scroll_y = 0.0;

    if url.as_str() == "about:blank" || url.as_str() == "about:newtab" {
        state.content_display_list.clear();
        state.page_links.clear();
        state.page_title = None;
        return;
    }

    state.loading = true;

    let render_output = match state.rt.block_on(state.pipeline.render(&url)) {
        Ok(output) => output,
        Err(e) => {
            eprintln!("Render pipeline error: {e}");
            state.loading = false;
            return;
        }
    };

    state.content_display_list = render_output.display_list;
    state.page_links = render_output.links;
    state.page_title = render_output.title;
    state.loading = false;
}

fn build_display_list(state: &mut AppState) {
    let width = state.window_width;
    let height = state.window_height;

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

    let url_bg = Color::from_rgba8(245, 245, 245, 255);
    let url_text = if state.address_bar_focused {
        state.url_buffer.clone()
    } else if let Some(active) = state.browser.tab_manager.active_tab() {
        active.url.as_str().to_string()
    } else {
        String::new()
    };
    let is_secure = state.browser.tab_manager.active_tab()
        .map(|t| t.url.scheme() == "https")
        .unwrap_or(false);
    let cursor_pos = state.cursor_pos;
    let selection_start = state.selection_start;
    let cursor_visible = state.cursor_visible;
    let address_bar_focused = state.address_bar_focused;

    let list = &mut state.display_list;
    list.push_rect(DrawRect {
        x: 10.0,
        y: 46.0,
        width: width - 20.0,
        height: 28.0,
        color: url_bg,
    });

    draw_address_bar(
        list,
        url_text,
        is_secure,
        address_bar_focused,
        cursor_pos,
        selection_start,
        cursor_visible,
    );

    // Loading indicator: a thin colored bar at the bottom of the address bar
    if state.loading {
        list.push_rect(DrawRect {
            x: 10.0,
            y: 76.0,
            width: width - 20.0,
            height: 3.0,
            color: Color::from_rgba8(66, 133, 244, 255),
        });
    }

    let content_area_y = 84.0;
    let content_area_h = height - 92.0;

    list.push_rect(DrawRect {
        x: 8.0,
        y: content_area_y,
        width: width - 16.0,
        height: content_area_h,
        color: Color::from_rgba8(255, 255, 255, 255),
    });

    let content_height = state.content_display_list.commands().iter().fold(0.0f32, |max_y, cmd| {
        match cmd {
            DisplayCommand::Rect(rect) => max_y.max(rect.y + rect.height),
            DisplayCommand::Text(text) => max_y.max(text.y + 20.0),
            _ => max_y,
        }
    }) + 20.0;

    list.push_clip(ClipRect {
        x: 8.0,
        y: content_area_y,
        width: width - 16.0,
        height: content_area_h,
    });

    let sy = state.scroll_y;
    let visible_top = content_area_y;
    let visible_bottom = content_area_y + content_area_h;

    for cmd in state.content_display_list.commands() {
        match cmd {
            DisplayCommand::Rect(rect) => {
                let render_y = content_area_y + rect.y - sy;
                let render_bottom = render_y + rect.height;
                if render_bottom < visible_top || render_y > visible_bottom {
                    continue;
                }
                list.push_rect(DrawRect {
                    x: 8.0 + rect.x,
                    y: render_y,
                    width: rect.width,
                    height: rect.height,
                    color: rect.color,
                });
            }
            DisplayCommand::Text(text) => {
                let render_x = 8.0 + text.x;
                if render_x > width - 20.0 {
                    continue;
                }
                let render_y = content_area_y + text.y - sy;
                if render_y + 20.0 < visible_top || render_y > visible_bottom {
                    continue;
                }
                list.push_text(DrawText {
                    x: render_x,
                    y: render_y,
                    ..text.clone()
                });
            }
            _ => {}
        }
    }

    list.pop_clip();

    // Scrollbar
    if content_height > content_area_h {
        let scrollbar_width = 8.0;
        let sb_x = width - 16.0;
        let scrollable = content_height - content_area_h;
        let scroll_frac = (state.scroll_y / scrollable).min(1.0);
        let visible_ratio = (content_area_h / content_height).min(1.0);
        let thumb_height = (visible_ratio * content_area_h).max(20.0);
        let thumb_y = content_area_y + scroll_frac * (content_area_h - thumb_height);

        list.push_rect(DrawRect {
            x: sb_x,
            y: content_area_y,
            width: scrollbar_width,
            height: content_area_h,
            color: Color::from_rgba8(200, 200, 210, 100),
        });
        list.push_rect(DrawRect {
            x: sb_x,
            y: thumb_y,
            width: scrollbar_width,
            height: thumb_height,
            color: Color::from_rgba8(120, 120, 130, 150),
        });
    }
}

fn draw_address_bar(
    list: &mut DisplayList,
    url_text: String,
    is_secure: bool,
    focused: bool,
    cursor_pos: usize,
    selection_start: Option<usize>,
    cursor_visible: bool,
) {
    let font_size = 14.0;
    let text_x = 40.0;
    let text_y = 53.0;

    let (security_char, security_color) = if is_secure {
        ("S", Color::from_rgba8(0, 150, 0, 255))
    } else {
        ("!", Color::from_rgba8(200, 100, 0, 255))
    };
    list.push_text(DrawText {
        x: 14.0,
        y: text_y,
        text: security_char.to_string(),
        font_size,
        color: security_color,
        font_family: Some("sans-serif".to_string()),
        bold: true,
        italic: false,
    });

    let mut char_x = text_x;
    for ch in url_text.chars() {
        list.push_text(DrawText {
            x: char_x,
            y: text_y,
            text: ch.to_string(),
            font_size,
            color: Color::BLACK,
            font_family: Some("sans-serif".to_string()),
            bold: false,
            italic: false,
        });
        char_x += font_size * 0.6;
    }

    if focused {
        let cursor_x = text_x + (cursor_pos as f32 * font_size * 0.6);
        if let Some(sel_start) = selection_start {
            let start = sel_start.min(cursor_pos);
            let end = sel_start.max(cursor_pos);
            let sel_x = text_x + (start as f32 * font_size * 0.6);
            let sel_width = (end - start) as f32 * font_size * 0.6;
            list.push_rect(DrawRect {
                x: sel_x,
                y: text_y - 2.0,
                width: sel_width.max(1.0),
                height: font_size + 4.0,
                color: Color::from_rgba8(66, 133, 244, 100),
            });
        }

        if cursor_visible {
            list.push_rect(DrawRect {
                x: cursor_x,
                y: text_y - 2.0,
                width: 2.0,
                height: font_size + 4.0,
                color: Color::BLACK,
            });
        }
    }
}
