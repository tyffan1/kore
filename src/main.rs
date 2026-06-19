use std::cell::RefCell;
use std::sync::mpsc;
use std::sync::Arc;
use std::time::Instant;

use clipboard::ClipboardProvider;
use kore_browser::BrowserApp;
use kore_devtools::DevTools;
use kore_gpu::{ClipRect, Color, DisplayCommand, DisplayList, DrawCircle, DrawRect, DrawText, Renderer, RendererConfig};
use kore_pipeline::{Pipeline, RenderOutput};
use kore_ui::WindowControlsStyle;
use kore_window::{AppEvent, EventLoop, InputEvent, Key, Modifiers, MouseButton, WindowBuilder, WindowHandle};

const SEARCH_ENGINES: &[(&str, &str)] = &[
    ("Bing", "https://www.bing.com/search?q="),
    ("DuckDuckGo", "https://html.duckduckgo.com/html/?q="),
];

struct AppState {
    browser: BrowserApp,
    pipeline: Arc<Pipeline>,
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
    window: Option<Arc<winit::window::Window>>,
    close_btn_hover: bool,
    max_btn_hover: bool,
    min_btn_hover: bool,
    search_engine_index: usize,
    render_tx: mpsc::SyncSender<RenderOutput>,
    render_rx: mpsc::Receiver<RenderOutput>,
    devtools: DevTools,
    focused: bool,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let _ = std::process::Command::new("cmd")
        .args(["/c", "chcp 65001"])
        .output();
    let session_path = std::env::temp_dir().join("kore_session.json");
    let _ = std::fs::remove_file(&session_path);
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

    let (tx, rx) = mpsc::sync_channel::<RenderOutput>(4);
    let state = RefCell::new(AppState {
        browser,
        pipeline: Arc::new(Pipeline::default()),
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
        window: None,
        close_btn_hover: false,
        max_btn_hover: false,
        min_btn_hover: false,
        search_engine_index: 0,
        render_tx: tx,
        render_rx: rx,
        devtools: DevTools::new(),
        focused: true,
    });

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
                                    state.borrow_mut().window = Some(w.clone());
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
                    if let Ok(output) = s.render_rx.try_recv() {
                        s.content_display_list = output.display_list;
                        s.page_title = output.title;
                        s.page_links = output.links;
                        s.loading = false;
                        if let Some(ref w) = s.window {
                            w.request_redraw();
                        }
                    }
                    build_display_list(&mut s);
                }

                {
                    let s = state.borrow();
                    let title = s
                        .page_title
                        .as_deref()
                        .map(|t| format!("{t} - Kore"))
                        .unwrap_or_else(|| "Kore".to_string());
                    if let Some(ref w) = s.window {
                        w.set_title(&title);
                    }
                }

                if let Some(r) = renderer.borrow_mut().as_mut() {
                    let display_list = &state.borrow().display_list;
                    match r.begin_frame() {
                        Ok(mut frame) => {
                            r.submit(&mut frame, display_list);
                            if let Err(e) = r.end_frame(frame) {
                                eprintln!("Render error: {e}");
                            }
                            if let Some(ref w) = state.borrow().window {
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

            AppEvent::FocusChanged(focused) => {
                state.borrow_mut().focused = focused;
                if let Some(ref w) = state.borrow().window {
                    w.request_redraw();
                }
            }
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
            update_hover_states(state);
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

fn update_hover_states(state: &mut AppState) {
    let x = state.mouse_x;
    let y = state.mouse_y;
    let w = state.window_width as f64;

    state.back_button_hover = x >= 8.0 && x <= 36.0 && y >= 36.0 && y <= 72.0;
    state.forward_button_hover = x >= 42.0 && x <= 70.0 && y >= 36.0 && y <= 72.0;
    state.reload_button_hover = x >= 76.0 && x <= 104.0 && y >= 36.0 && y <= 72.0;

    if y < 36.0 {
        match WindowControlsStyle::current() {
            WindowControlsStyle::MacOS => {
                let in_group = x >= 10.0 && x <= 82.0 && y >= 10.0 && y <= 26.0;
                if in_group {
                    state.close_btn_hover = (x - 18.0).powi(2) + (y - 18.0).powi(2) <= 64.0;
                    state.min_btn_hover = (x - 42.0).powi(2) + (y - 18.0).powi(2) <= 64.0;
                    state.max_btn_hover = (x - 66.0).powi(2) + (y - 18.0).powi(2) <= 64.0;
                } else {
                    state.close_btn_hover = false;
                    state.min_btn_hover = false;
                    state.max_btn_hover = false;
                }
            }
            WindowControlsStyle::Windows | WindowControlsStyle::Linux => {
                state.min_btn_hover = x >= w - 138.0 && x <= w - 92.0;
                state.max_btn_hover = x >= w - 92.0 && x <= w - 46.0;
                state.close_btn_hover = x >= w - 46.0 && x <= w;
            }
        }
    } else {
        state.close_btn_hover = false;
        state.min_btn_hover = false;
        state.max_btn_hover = false;
    }
}

fn handle_mouse_click(state: &mut AppState, x: f64, y: f64) {
    let w = state.window_width as f64;

    // ── Row 1: Titlebar + Tab bar (y < 36) ──
    if y < 36.0 {
        match WindowControlsStyle::current() {
            WindowControlsStyle::MacOS => {
                if (x - 18.0).powi(2) + (y - 18.0).powi(2) <= 64.0 {
                    let _ = state.browser.shutdown();
                    std::process::exit(0);
                }
                if (x - 66.0).powi(2) + (y - 18.0).powi(2) <= 64.0 {
                    if let Some(ref win) = state.window {
                        let is_max = win.is_maximized();
                        win.set_maximized(!is_max);
                    }
                    return;
                }
                if (x - 42.0).powi(2) + (y - 18.0).powi(2) <= 64.0 {
                    if let Some(ref win) = state.window {
                        win.set_minimized(true);
                    }
                    return;
                }
            }
            WindowControlsStyle::Windows | WindowControlsStyle::Linux => {
                if x >= w - 46.0 && x <= w {
                    let _ = state.browser.shutdown();
                    std::process::exit(0);
                }
                if x >= w - 92.0 && x <= w - 46.0 {
                    if let Some(ref win) = state.window {
                        let is_max = win.is_maximized();
                        win.set_maximized(!is_max);
                    }
                    return;
                }
                if x >= w - 138.0 && x <= w - 92.0 {
                    if let Some(ref win) = state.window {
                        win.set_minimized(true);
                    }
                    return;
                }
            }
        }

        let style = WindowControlsStyle::current();
        let tab_start_x: f32 = match style {
            WindowControlsStyle::MacOS => 80.0,
            _ => 32.0,
        };
        let right_margin: f64 = match style {
            WindowControlsStyle::MacOS => 36.0,
            _ => 174.0, // 46×3 + 36 (new tab button)
        };
        let tabs = state.browser.list_tabs().to_vec();

        // Tab clicks
        for (i, tab) in tabs.iter().enumerate() {
            let tx = (tab_start_x + (i as f32) * 180.0) as f64;
            // Close button on tab
            if x >= tx + 148.0 && x <= tx + 168.0 && y >= 0.0 && y <= 36.0 {
                let id = tab.id;
                let _ = state.browser.close_tab(id);
                if state.browser.tab_count() == 0 {
                    if let Ok(url) = url::Url::parse("about:blank") {
                        let _ = state.browser.open_tab(url);
                    }
                }
                reset_content_state(state);
                if let Some(active) = state.browser.tab_manager.active_tab() {
                    navigate(state, active.url.clone());
                }
                return;
            }
            // Tab body
            if x >= tx && x <= tx + 170.0 && y >= 0.0 && y <= 36.0 {
                let _ = state.browser.switch_tab(tab.id);
                reset_content_state(state);
                if let Some(active) = state.browser.tab_manager.active_tab() {
                    navigate(state, active.url.clone());
                }
                return;
            }
        }

        // New tab button
        let new_tab_x = ((tab_start_x + (tabs.len() as f32) * 180.0) as f64).min(w - right_margin);
        if x >= new_tab_x && x <= new_tab_x + 36.0 && y >= 0.0 && y <= 36.0 {
            if let Ok(url) = url::Url::parse("about:blank") {
                if state.browser.open_tab(url).is_ok() {
                    if let Some(tab) = state.browser.tab_manager.active_tab() {
                        let _ = state.browser.switch_tab(tab.id);
                    }
                    reset_content_state(state);
                }
            }
            return;
        }

        // Click on titlebar empty area → window drag
        if let Some(ref win) = state.window {
            let _ = win.drag_window();
        }
        return;
    }

    // ── Row 2: Navigation bar (y >= 36, y < 72) ──
    const ADDRESS_BAR_X: f64 = 110.0;
    const ADDRESS_BAR_Y: f64 = 36.0;
    const ADDRESS_BAR_WIDTH: f64 = 1280.0 - 120.0;
    const ADDRESS_BAR_HEIGHT: f64 = 36.0;

    const BACK_BTN_X: f64 = 8.0;
    const BACK_BTN_Y: f64 = 36.0;
    const BACK_BTN_W: f64 = 28.0;
    const BACK_BTN_H: f64 = 36.0;

    const FORWARD_BTN_X: f64 = 42.0;
    const FORWARD_BTN_Y: f64 = 36.0;
    const FORWARD_BTN_W: f64 = 28.0;
    const FORWARD_BTN_H: f64 = 36.0;

    const RELOAD_BTN_X: f64 = 76.0;
    const RELOAD_BTN_Y: f64 = 36.0;
    const RELOAD_BTN_W: f64 = 28.0;
    const RELOAD_BTN_H: f64 = 36.0;

    let in_address_bar = x >= ADDRESS_BAR_X
        && x <= ADDRESS_BAR_X + ADDRESS_BAR_WIDTH
        && y >= ADDRESS_BAR_Y
        && y <= ADDRESS_BAR_Y + ADDRESS_BAR_HEIGHT;

    let in_back_btn = x >= BACK_BTN_X
        && x <= BACK_BTN_X + BACK_BTN_W
        && y >= BACK_BTN_Y
        && y <= BACK_BTN_Y + BACK_BTN_H;

    let in_forward_btn = x >= FORWARD_BTN_X
        && x <= FORWARD_BTN_X + FORWARD_BTN_W
        && y >= FORWARD_BTN_Y
        && y <= FORWARD_BTN_Y + FORWARD_BTN_H;

    let in_reload_btn = x >= RELOAD_BTN_X
        && x <= RELOAD_BTN_X + RELOAD_BTN_W
        && y >= RELOAD_BTN_Y
        && y <= RELOAD_BTN_Y + RELOAD_BTN_H;

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
            state.loading = true;
            state.content_display_list.clear();
            let url = active.url.clone();
            navigate(state, url);
        }
        return;
    }

    // Check page links (content area)
    let sy = state.scroll_y;
    for (lx, ly, lw, lh, href) in &state.page_links {
        if x >= *lx as f64
            && x <= (*lx + *lw) as f64
            && y >= (72.0 + *ly - sy) as f64
            && y <= (72.0 + *ly + *lh - sy) as f64
        {
            let url = if href.starts_with("http://") || href.starts_with("https://") {
                url::Url::parse(href).ok()
            } else {
                parse_url(href, state.search_engine_index).ok()
            };
            if let Some(url) = url {
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
                let url = parse_url(url_str, state.search_engine_index);
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
            if state.browser.open_tab(url).is_ok() {
                if let Some(tab) = state.browser.tab_manager.active_tab() {
                    let _ = state.browser.switch_tab(tab.id);
                }
                reset_content_state(state);
            }
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
            reset_content_state(state);
            if let Some(tab) = state.browser.tab_manager.active_tab() {
                navigate(state, tab.url.clone());
            }
        }
        Key::I if modifiers.shift => {
            eprintln!("DevTools toggled");
            state.devtools.toggle();
            return;
        }
        Key::L => {
            state.address_bar_focused = true;
            if let Some(active) = state.browser.tab_manager.active_tab() {
                state.url_buffer = active.url.as_str().to_string();
                state.cursor_pos = state.url_buffer.chars().count();
                state.selection_start = Some(0);
            }
        }
        Key::Comma => {
            state.search_engine_index = (state.search_engine_index + 1) % SEARCH_ENGINES.len();
            state.address_bar_focused = true;
            state.url_buffer = format!("Search engine: {}", SEARCH_ENGINES[state.search_engine_index].0);
            state.cursor_pos = state.url_buffer.chars().count();
            state.selection_start = Some(0);
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

fn parse_url(input: &str, search_engine_index: usize) -> Result<url::Url, url::ParseError> {
    let trimmed = input.trim();

    if trimmed.starts_with('/') {
        return url::Url::parse(&format!("https://localhost{trimmed}"));
    }

    if trimmed.starts_with("http://") || trimmed.starts_with("https://")
       || trimmed.starts_with("about:") {
        return url::Url::parse(trimmed);
    }

    if !trimmed.contains(' ') && trimmed.contains('.') {
        return url::Url::parse(&format!("https://{trimmed}"));
    }

    let search_url = SEARCH_ENGINES[search_engine_index % SEARCH_ENGINES.len()].1;
    let query = urlencoding::encode(trimmed);
    url::Url::parse(&format!("{search_url}{query}"))
}

fn reset_content_state(state: &mut AppState) {
    state.content_display_list.clear();
    state.page_title = None;
    state.scroll_y = 0.0;
    state.url_buffer = String::new();
    state.page_links.clear();
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

fn navigate(state: &mut AppState, mut url: url::Url) {
    state.scroll_y = 0.0;

    // Resolve relative URLs against current page origin
    let url_str = url.as_str().to_string();
    if url_str.starts_with('/') {
        if let Some(active) = state.browser.tab_manager.active_tab() {
            if let Ok(base) = url::Url::parse(&format!(
                "{}://{}",
                active.url.scheme(),
                active.url.host_str().unwrap_or("localhost")
            )) {
                if let Ok(resolved) = base.join(&url_str) {
                    url = resolved;
                }
            }
        }
    }

    if url.as_str() == "about:blank" || url.as_str() == "about:newtab" {
        state.content_display_list.clear();
        state.page_links.clear();
        state.page_title = None;
        return;
    }

    state.loading = true;
    state.content_display_list.clear();

    let tx = state.render_tx.clone();
    let pipeline = Arc::clone(&state.pipeline);
    let url = url.clone();
    std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("Failed to build runtime");
        match rt.block_on(pipeline.render(&url)) {
            Ok(output) => {
                let _ = tx.send(output);
            }
            Err(e) => {
                eprintln!("Pipeline error: {e}");
            }
        }
    });
}

fn draw_titlebar(list: &mut DisplayList, tabs: &[kore_browser::Tab], page_title: Option<&str>, w: f32, min_hov: bool, max_hov: bool, close_hov: bool, focused: bool) {
    let style = WindowControlsStyle::current();
    let tab_start: f32 = match style {
        WindowControlsStyle::MacOS => 80.0,
        _ => 32.0,
    };
    let right_margin: f32 = match style {
        WindowControlsStyle::MacOS => 36.0,
        _ => 174.0, // 46×3 + 36 (new tab button)
    };
    list.push_rect(DrawRect { x: 0.0, y: 0.0, width: w, height: 36.0, color: Color::from_rgba8(26, 26, 36, 255) });

    match style {
        WindowControlsStyle::MacOS => {
            let gray = Color::from_rgba8(0x9E, 0x9E, 0x9E, 255);
            let close_color = if focused { Color::from_rgba8(0xFF, 0x5F, 0x56, 255) } else { gray };
            let min_color = if focused { Color::from_rgba8(0xFF, 0xBD, 0x2E, 255) } else { gray };
            let max_color = if focused { Color::from_rgba8(0x27, 0xC9, 0x3F, 255) } else { gray };

            list.push_circle(DrawCircle { cx: 18.0, cy: 18.0, radius: 8.0, color: close_color });
            list.push_circle(DrawCircle { cx: 42.0, cy: 18.0, radius: 8.0, color: min_color });
            list.push_circle(DrawCircle { cx: 66.0, cy: 18.0, radius: 8.0, color: max_color });

            let hovered = (close_hov || min_hov || max_hov) && focused;
            if hovered {
                let close_icon = Color::from_rgba8(100, 20, 20, 204);
                list.push_rect(DrawRect { x: 15.0, y: 11.0, width: 1.5, height: 8.0, color: close_icon });
                list.push_rect(DrawRect { x: 15.0, y: 11.0, width: 8.0, height: 1.5, color: close_icon });

                let min_icon = Color::from_rgba8(80, 50, 0, 204);
                list.push_rect(DrawRect { x: 38.0, y: 15.25, width: 8.0, height: 1.5, color: min_icon });

                let max_icon = Color::from_rgba8(0, 60, 20, 204);
                list.push_rect(DrawRect { x: 62.0, y: 11.0, width: 6.0, height: 1.5, color: max_icon });
                list.push_rect(DrawRect { x: 62.0, y: 11.0, width: 1.5, height: 6.0, color: max_icon });
                list.push_rect(DrawRect { x: 66.5, y: 15.5, width: 6.0, height: 1.5, color: max_icon });
                list.push_rect(DrawRect { x: 66.5, y: 15.5, width: 1.5, height: 6.0, color: max_icon });
            }
        }
        WindowControlsStyle::Windows | WindowControlsStyle::Linux => {
            let min_bg = if min_hov { Color::from_rgba8(0xE5, 0xE5, 0xE5, 255) } else { Color::TRANSPARENT };
            list.push_rect(DrawRect { x: w - 138.0, y: 0.0, width: 46.0, height: 36.0, color: min_bg });
            list.push_text(DrawText { x: w - 121.0, y: 11.0, text: "\u{2212}".to_string(), font_size: 14.0, color: Color::WHITE, font_family: Some("sans-serif".to_string()), bold: false, italic: false });

            let max_bg = if max_hov { Color::from_rgba8(0xE5, 0xE5, 0xE5, 255) } else { Color::TRANSPARENT };
            list.push_rect(DrawRect { x: w - 92.0, y: 0.0, width: 46.0, height: 36.0, color: max_bg });
            list.push_text(DrawText { x: w - 75.0, y: 11.0, text: "\u{25A1}".to_string(), font_size: 14.0, color: Color::WHITE, font_family: Some("sans-serif".to_string()), bold: false, italic: false });

            let close_bg = if close_hov { Color::from_rgba8(0xC4, 0x2B, 0x1C, 255) } else { Color::TRANSPARENT };
            list.push_rect(DrawRect { x: w - 46.0, y: 0.0, width: 46.0, height: 36.0, color: close_bg });
            list.push_text(DrawText { x: w - 29.0, y: 11.0, text: "\u{00D7}".to_string(), font_size: 14.0, color: Color::WHITE, font_family: Some("sans-serif".to_string()), bold: false, italic: false });
        }
    }

    for (i, tab) in tabs.iter().enumerate() {
        let tx = tab_start + (i as f32) * 180.0;
        let tab_color = if tab.is_active { Color::from_rgba8(45, 45, 61, 255) } else { Color::from_rgba8(26, 26, 36, 255) };
        if tab.is_active {
            list.push_rect(DrawRect { x: tx + 1.0, y: 1.0, width: 168.0, height: 35.0, color: tab_color });
        } else {
            list.push_rect(DrawRect { x: tx, y: 0.0, width: 170.0, height: 36.0, color: tab_color });
        }
        let title: String = if tab.is_active && page_title.is_some() {
            page_title.unwrap().chars().take(20).collect()
        } else if tab.url.as_str() == "about:blank" { "New Tab".to_string() } else { tab.url.as_str().chars().take(20).collect() };
        list.push_text(DrawText { x: tx + 8.0, y: 11.0, text: title, font_size: 13.0, color: Color::WHITE, font_family: Some("sans-serif".to_string()), bold: false, italic: false });
        list.push_text(DrawText { x: tx + 150.0, y: 11.0, text: "×".to_string(), font_size: 13.0, color: Color::from_rgba8(180, 180, 190, 255), font_family: Some("sans-serif".to_string()), bold: false, italic: false });
    }

    let new_tab_x = (tab_start + (tabs.len() as f32) * 180.0).min(w - right_margin);
    list.push_rect(DrawRect { x: new_tab_x, y: 0.0, width: 36.0, height: 36.0, color: Color::from_rgba8(26, 26, 36, 255) });
    list.push_text(DrawText { x: new_tab_x + 11.0, y: 11.0, text: "+".to_string(), font_size: 14.0, color: Color::WHITE, font_family: Some("sans-serif".to_string()), bold: false, italic: false });
}

fn draw_navbar(list: &mut DisplayList, w: f32, back_hov: bool, fwd_hov: bool, rld_hov: bool, loading: bool, url_text: String, is_secure: bool, cursor_pos: usize, selection_start: Option<usize>, cursor_visible: bool, address_bar_focused: bool) {
    list.push_rect(DrawRect { x: 0.0, y: 36.0, width: w, height: 36.0, color: Color::from_rgba8(40, 40, 58, 255) });
    list.push_rect(DrawRect { x: 0.0, y: 36.0, width: w, height: 1.0, color: Color::from_rgba8(0, 0, 0, 51) });

    let hover_bright = Color::from_rgba8(88, 88, 100, 255);
    let btn_base = Color::from_rgba8(58, 58, 63, 255);

    let back_bg = if back_hov { hover_bright } else { btn_base };
    list.push_rect(DrawRect { x: 8.0, y: 40.0, width: 28.0, height: 28.0, color: back_bg });
    list.push_text(DrawText { x: 14.0, y: 48.0, text: "<".to_string(), font_size: 14.0, color: Color::WHITE, font_family: Some("sans-serif".to_string()), bold: false, italic: false });

    let fwd_bg = if fwd_hov { hover_bright } else { btn_base };
    list.push_rect(DrawRect { x: 42.0, y: 40.0, width: 28.0, height: 28.0, color: fwd_bg });
    list.push_text(DrawText { x: 48.0, y: 48.0, text: ">".to_string(), font_size: 14.0, color: Color::WHITE, font_family: Some("sans-serif".to_string()), bold: false, italic: false });

    let rld_bg = if rld_hov { hover_bright } else { btn_base };
    list.push_rect(DrawRect { x: 76.0, y: 40.0, width: 28.0, height: 28.0, color: rld_bg });
    list.push_text(DrawText { x: 80.0, y: 48.0, text: "R".to_string(), font_size: 14.0, color: Color::WHITE, font_family: Some("sans-serif".to_string()), bold: false, italic: false });

    list.push_rect(DrawRect { x: 110.0, y: 40.0, width: w - 120.0, height: 28.0, color: Color::from_rgba8(210, 210, 215, 255) });
    list.push_rect(DrawRect { x: 112.0, y: 42.0, width: w - 124.0, height: 24.0, color: Color::from_rgba8(255, 255, 255, 255) });
    draw_address_bar(list, url_text, is_secure, address_bar_focused, cursor_pos, selection_start, cursor_visible);

    if loading {
        list.push_rect(DrawRect { x: 110.0, y: 69.0, width: w - 120.0, height: 3.0, color: Color::from_rgba8(66, 133, 244, 255) });
    }
}

fn build_display_list(state: &mut AppState) {
    let width = state.window_width;
    let height = state.window_height;

    let tabs: Vec<kore_browser::Tab> = state.browser.list_tabs().to_vec();
    let page_title = state.page_title.as_deref();
    let back_hov = state.back_button_hover;
    let fwd_hov = state.forward_button_hover;
    let rld_hov = state.reload_button_hover;
    let close_hov = state.close_btn_hover;
    let max_hov = state.max_btn_hover;
    let min_hov = state.min_btn_hover;
    let loading = state.loading;
    let focused = state.focused;

    let url_text = if state.address_bar_focused {
        state.url_buffer.clone()
    } else if let Some(active) = state.browser.tab_manager.active_tab() {
        let url_str = active.url.as_str();
        if url_str == "about:blank" { String::new() } else { url_str.to_string() }
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
    list.clear();

    list.push_rect(DrawRect { x: 0.0, y: 0.0, width, height, color: Color::from_rgba8(240, 240, 245, 255) });

    // Row 1 - Titlebar + Tabs (y=0..36)
    draw_titlebar(list, &tabs, page_title, width, min_hov, max_hov, close_hov, focused);

    // Row 2 - Navigation + Address bar (y=36..72)
    draw_navbar(list, width, back_hov, fwd_hov, rld_hov, loading, url_text.clone(), is_secure, cursor_pos, selection_start, cursor_visible, address_bar_focused);

    // Content area (y=72..height)
    let content_area_y = 72.0;
    let content_area_h = height - 80.0;

    list.push_rect(DrawRect {
        x: 16.0, y: content_area_y, width: width - 32.0, height: content_area_h,
        color: Color::from_rgba8(255, 255, 255, 255),
    });

    let content_height = state.content_display_list.commands().iter().fold(0.0f32, |max_y, cmd| {
        match cmd {
            DisplayCommand::Rect(rect) => max_y.max(rect.y + rect.height),
            DisplayCommand::Text(text) => max_y.max(text.y + text.font_size * 1.5),
            _ => max_y,
        }
    }) + 20.0;

    list.push_clip(ClipRect { x: 16.0, y: content_area_y, width: width - 32.0, height: content_area_h });

    let sy = state.scroll_y;
    for cmd in state.content_display_list.commands() {
        match cmd {
            DisplayCommand::Rect(rect) => {
                let render_y = content_area_y + rect.y - sy;
                if render_y + rect.height < content_area_y || render_y > height { continue; }
                list.push_rect(DrawRect { x: 16.0 + rect.x, y: render_y, width: rect.width, height: rect.height, color: rect.color });
            }
            DisplayCommand::Text(text) => {
                let render_x = 16.0 + text.x;
                if render_x > width - 20.0 { continue; }
                let render_y = content_area_y + text.y - sy;
                let text_height = text.font_size * 1.5;
                if render_y + text_height < content_area_y || render_y > height { continue; }
                list.push_text(DrawText { x: render_x, y: render_y, ..text.clone() });
            }
            _ => {}
        }
    }
    list.pop_clip();

    // Mask header (y=0..72) to cover any content overflow, then redraw UI
    list.push_rect(DrawRect { x: 0.0, y: 0.0, width, height: 72.0, color: Color::from_rgba8(240, 240, 245, 255) });
    draw_titlebar(list, &tabs, page_title, width, min_hov, max_hov, close_hov, focused);
    draw_navbar(list, width, back_hov, fwd_hov, rld_hov, loading, url_text, is_secure, cursor_pos, selection_start, cursor_visible, address_bar_focused);

    // Scrollbar
    if content_height > content_area_h {
        let scrollbar_width = 8.0;
        let sb_x = width - 24.0;
        let scrollable = content_height - content_area_h;
        let scroll_frac = (state.scroll_y / scrollable).min(1.0);
        let visible_ratio = (content_area_h / content_height).min(1.0);
        let thumb_height = (visible_ratio * content_area_h).max(20.0);
        let thumb_y = content_area_y + scroll_frac * (content_area_h - thumb_height);
        list.push_rect(DrawRect { x: sb_x, y: content_area_y, width: scrollbar_width, height: content_area_h, color: Color::from_rgba8(200, 200, 210, 100) });
        list.push_rect(DrawRect { x: sb_x, y: thumb_y, width: scrollbar_width, height: thumb_height, color: Color::from_rgba8(120, 120, 130, 150) });
    }

    if address_bar_focused {
        let label = format!("Search: {}", SEARCH_ENGINES[state.search_engine_index].0);
        list.push_text(DrawText {
            x: 10.0, y: height - 20.0, text: label, font_size: 12.0,
            color: Color::from_rgba8(150, 150, 150, 255),
            font_family: Some("sans-serif".to_string()), bold: false, italic: false,
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
    let text_x = 128.0;
    let text_y = 48.0;

    // Security indicator: colored dot with white border ring
    let dot_color = if is_secure {
        Color::from_rgba8(0, 180, 0, 255)
    } else {
        Color::from_rgba8(200, 80, 60, 255)
    };
    list.push_rect(DrawRect { x: 115.0, y: 49.0, width: 12.0, height: 12.0, color: Color::from_rgba8(255, 255, 255, 220) });
    list.push_rect(DrawRect { x: 117.0, y: 51.0, width: 8.0, height: 8.0, color: dot_color });

    let is_empty = url_text.is_empty();
    let display_text = if is_empty && !focused {
        "Search or enter address".to_string()
    } else {
        url_text
    };
    let text_color = if is_empty && !focused {
        Color::from_rgba8(150, 150, 150, 255)
    } else {
        Color::BLACK
    };

    list.push_text(DrawText {
        x: text_x,
        y: text_y,
        text: display_text,
        font_size,
        color: text_color,
        font_family: Some("sans-serif".to_string()),
        bold: false,
        italic: false,
    });

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
