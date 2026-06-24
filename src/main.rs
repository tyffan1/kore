use std::cell::RefCell;
use std::sync::mpsc;
use std::sync::Arc;
use std::time::Instant;

use clipboard::ClipboardProvider;
use kore_browser::BrowserApp;
use kore_devtools::DevTools;
use kore_gpu::{ClipRect, Color, DisplayCommand, DisplayList, DrawCircle, DrawRect, DrawText, Renderer, RendererConfig};
use kore_pipeline::{Pipeline, RenderOutput};
use kore_ui::{ModernTheme, WindowControlsStyle};
use kore_window::{AppEvent, EventLoop, InputEvent, Key, Modifiers, MouseButton, WindowBuilder, WindowHandle};

const SEARCH_ENGINES: &[(&str, &str)] = &[
    ("Bing", "https://www.bing.com/search?q="),
    ("DuckDuckGo", "https://html.duckduckgo.com/html/?q="),
];

const UI_FONT: &str = "SF Pro Text";

fn c(r: u8, g: u8, b: u8) -> Color {
    Color::from_rgba8(r, g, b, 255)
}

fn text_draw(x: f32, y: f32, text: String, font_size: f32, color: Color) -> DrawText {
    DrawText { x, y, text, font_size, color, font_family: Some(UI_FONT.to_string()), bold: false, italic: false, opacity: 1.0, translate: (0.0, 0.0) }
}

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
    hovered_tab_index: Option<usize>,
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
        hovered_tab_index: None,
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
                if let Some(ref w) = state.borrow().window {
                    w.request_redraw();
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

/// ── Helpers ──

fn addr_bar_rect(w: f32) -> (f32, f32, f32, f32) {
    let h = 28.0;
    let y = 44.0;
    let max_w = 600.0;
    let min_margin = 120.0;
    let avail = w - min_margin * 2.0;
    let bw = avail.min(max_w).max(200.0);
    let bx = ((w - bw) / 2.0).max(min_margin);
    (bx, y, bw, h)
}

fn tab_col_count(style: WindowControlsStyle) -> f32 {
    match style {
        WindowControlsStyle::MacOS => 80.0,
        _ => 32.0,
    }
}

fn right_margin(style: WindowControlsStyle) -> f32 {
    match style {
        WindowControlsStyle::MacOS => 36.0,
        _ => 174.0,
    }
}

fn tab_x(i: usize, style: WindowControlsStyle) -> f32 {
    tab_col_count(style) + (i as f32) * 180.0
}

fn tab_hit(tx: f32, x: f64) -> bool {
    x >= tx as f64 && x <= (tx + 170.0) as f64
}

fn close_hit(tx: f32, x: f64) -> bool {
    x >= (tx + 146.0) as f64 && x <= (tx + 166.0) as f64
}

/// ── Hover state ──

fn update_hover_states(state: &mut AppState) {
    let x = state.mouse_x;
    let y = state.mouse_y;
    let w = state.window_width as f64;
    let style = WindowControlsStyle::current();

    // Toolbar buttons (y 36..80)
    state.back_button_hover = x >= 8.0 && x <= 36.0 && y >= 36.0 && y <= 80.0;
    state.forward_button_hover = x >= 44.0 && x <= 72.0 && y >= 36.0 && y <= 80.0;
    state.reload_button_hover = x >= 80.0 && x <= 108.0 && y >= 36.0 && y <= 80.0;

    // Window controls (tab bar area y<36)
    if y < 36.0 {
        match style {
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

    // Tab hover
    if y < 36.0 {
        let tabs = state.browser.list_tabs().to_vec();
        let was = state.hovered_tab_index;
        state.hovered_tab_index = None;
        for (i, _) in tabs.iter().enumerate() {
            let tx = tab_x(i, style);
            let right_m = right_margin(style);
            let right_limit = w - right_m as f64;
            if tx as f64 > right_limit {
                break;
            }
            if tab_hit(tx, x) {
                state.hovered_tab_index = Some(i);
                break;
            }
        }
        // If mouse is now over a different area, reset
        if state.hovered_tab_index != was && was.is_some() {
            if let Some(ref win) = state.window {
                win.request_redraw();
            }
        }
    } else {
        state.hovered_tab_index = None;
    }
}

/// ── Mouse click ──

fn handle_mouse_click(state: &mut AppState, x: f64, y: f64) {
    let w = state.window_width as f64;
    let style = WindowControlsStyle::current();

    // ── Row 1: Titlebar + Tab bar (y < 36) ──
    if y < 36.0 {
        match style {
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

        let tabs = state.browser.list_tabs().to_vec();

        // Tab clicks
        for (i, tab) in tabs.iter().enumerate() {
            let tx = tab_x(i, style);
            let right_m = right_margin(style);
            let right_limit = w - right_m as f64;
            if tx as f64 > right_limit {
                break;
            }
            // Close button on tab
            if close_hit(tx, x) && y >= 0.0 && y <= 36.0 {
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
            if tab_hit(tx, x) && y >= 0.0 && y <= 36.0 {
                let _ = state.browser.switch_tab(tab.id);
                reset_content_state(state);
                if let Some(active) = state.browser.tab_manager.active_tab() {
                    navigate(state, active.url.clone());
                }
                return;
            }
        }

        // New tab button
        let new_tab_x = ((tab_col_count(style) + (tabs.len() as f32) * 180.0) as f64).min(w - right_margin(style) as f64);
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

    // ── Row 2: Navigation bar (y >= 36, y < 80) ──
    let (addr_x, addr_y, addr_w, addr_h) = addr_bar_rect(state.window_width);

    let in_addr = x >= addr_x as f64 && x <= (addr_x + addr_w) as f64
        && y >= addr_y as f64 && y <= (addr_y + addr_h) as f64;

    let in_back = x >= 8.0 && x <= 36.0 && y >= 36.0 && y <= 80.0;
    let in_forward = x >= 44.0 && x <= 72.0 && y >= 36.0 && y <= 80.0;
    let in_reload = x >= 80.0 && x <= 108.0 && y >= 36.0 && y <= 80.0;

    if in_addr {
        state.address_bar_focused = true;
        state.cursor_pos = state.url_buffer.chars().count();
        state.cursor_visible = true;
        state.last_cursor_blink = Instant::now();
    } else {
        state.address_bar_focused = false;
        state.selection_start = None;
    }

    if in_back {
        if let Some(active) = state.browser.tab_manager.active_tab_mut() {
            if let Some(url) = active.go_back() {
                navigate(state, url);
            }
        }
    } else if in_forward {
        if let Some(active) = state.browser.tab_manager.active_tab_mut() {
            if let Some(url) = active.go_forward() {
                navigate(state, url);
            }
        }
    } else if in_reload {
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
            && y >= (80.0 + *ly - sy) as f64
            && y <= (80.0 + *ly + *lh - sy) as f64
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
    state.cursor_pos = state.cursor_pos.min(state.url_buffer.chars().count());
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
    let buf_len = state.url_buffer.chars().count();
    if state.cursor_pos > buf_len {
        state.cursor_pos = buf_len;
    }
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
    state.cursor_pos = 0;
    state.selection_start = None;
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

/// ── Drawing helpers ──

fn draw_pill(list: &mut DisplayList, x: f32, y: f32, w: f32, h: f32, _radius: f32, color: Color) {
    let r = h * 0.3;
    list.push_rect(DrawRect { x, y: y + r, width: w, height: h - 2.0 * r, color, opacity: 1.0, translate: (0.0, 0.0) });
    list.push_rect(DrawRect { x: x + r, y, width: w - 2.0 * r, height: r, color, opacity: 1.0, translate: (0.0, 0.0) });
    list.push_rect(DrawRect { x: x + r, y: y + h - r, width: w - 2.0 * r, height: r, color, opacity: 1.0, translate: (0.0, 0.0) });
}

/// Draw a left-pointing chevron using a proper typographic symbol.
fn draw_chevron_left(list: &mut DisplayList, cx: f32, cy: f32, _size: f32, color: Color) {
    list.push_text(text_draw(cx - 5.0, cy - 6.0, "\u{2039}".to_string(), 16.0, color));
}

/// Draw a right-pointing chevron.
fn draw_chevron_right(list: &mut DisplayList, cx: f32, cy: f32, _size: f32, color: Color) {
    list.push_text(text_draw(cx - 4.0, cy - 6.0, "\u{203A}".to_string(), 16.0, color));
}

/// Draw a circular refresh arrow using a typographic symbol.
fn draw_refresh(list: &mut DisplayList, cx: f32, cy: f32, _r: f32, color: Color) {
    list.push_text(text_draw(cx - 5.0, cy - 7.0, "\u{21BB}".to_string(), 16.0, color));
}

/// Draw the lock icon for HTTPS.
fn draw_lock_icon(list: &mut DisplayList, x: f32, y: f32, size: f32, color: Color) {
    let bw = size * 0.5;
    let bh = size * 0.4;
    let arc_w = size * 0.35;
    let arc_h = size * 0.35;
    // Shackle (arc)
    list.push_rect(DrawRect { x: x + (size - arc_w) / 2.0, y, width: arc_w, height: arc_h * 0.6, color, opacity: 1.0, translate: (0.0, 0.0) });
    // Body
    list.push_rect(DrawRect { x: x + (size - bw) / 2.0, y: y + arc_h * 0.5, width: bw, height: bh, color, opacity: 1.0, translate: (0.0, 0.0) });
    // Keyhole
    list.push_rect(DrawRect { x: x + size / 2.0 - 1.0, y: y + arc_h * 0.5 + bh * 0.3, width: 2.0, height: 3.0, color: c(ModernTheme::AddressBarBg.0, ModernTheme::AddressBarBg.1, ModernTheme::AddressBarBg.2), opacity: 1.0, translate: (0.0, 0.0) });
}

/// ── Drawing functions ──

const TAB_BAR_H: f32 = 36.0;
const TOOLBAR_H: f32 = 44.0;
const HEADER_H: f32 = TAB_BAR_H + TOOLBAR_H; // 80
const TAB_VIS_W: f32 = 170.0;
const TAB_PILL_R: f32 = 8.0;
const BTN_SIZE: f32 = 28.0;
const BTN_Y: f32 = TAB_BAR_H + (TOOLBAR_H - BTN_SIZE) / 2.0; // 44
const BACK_X: f32 = 8.0;
const FORWARD_X: f32 = 44.0;
const RELOAD_X: f32 = 80.0;

fn draw_titlebar(list: &mut DisplayList, tabs: &[kore_browser::Tab], page_title: Option<&str>, w: f32, min_hov: bool, max_hov: bool, close_hov: bool, focused: bool, hovered_tab: Option<usize>) {
    let style = WindowControlsStyle::current();
    let rm = right_margin(style);

    // Tab bar background
    list.push_rect(DrawRect { x: 0.0, y: 0.0, width: w, height: TAB_BAR_H, color: c(ModernTheme::TabBarBg.0, ModernTheme::TabBarBg.1, ModernTheme::TabBarBg.2), opacity: 1.0, translate: (0.0, 0.0) });

    // ── Window controls ──
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
                list.push_rect(DrawRect { x: 15.0, y: 11.0, width: 1.5, height: 8.0, color: close_icon, opacity: 1.0, translate: (0.0, 0.0) });
                list.push_rect(DrawRect { x: 15.0, y: 11.0, width: 8.0, height: 1.5, color: close_icon, opacity: 1.0, translate: (0.0, 0.0) });

                let min_icon = Color::from_rgba8(80, 50, 0, 204);
                list.push_rect(DrawRect { x: 38.0, y: 15.25, width: 8.0, height: 1.5, color: min_icon, opacity: 1.0, translate: (0.0, 0.0) });

                let max_icon = Color::from_rgba8(0, 60, 20, 204);
                list.push_rect(DrawRect { x: 62.0, y: 11.0, width: 6.0, height: 1.5, color: max_icon, opacity: 1.0, translate: (0.0, 0.0) });
                list.push_rect(DrawRect { x: 62.0, y: 11.0, width: 1.5, height: 6.0, color: max_icon, opacity: 1.0, translate: (0.0, 0.0) });
                list.push_rect(DrawRect { x: 66.5, y: 15.5, width: 6.0, height: 1.5, color: max_icon, opacity: 1.0, translate: (0.0, 0.0) });
                list.push_rect(DrawRect { x: 66.5, y: 15.5, width: 1.5, height: 6.0, color: max_icon, opacity: 1.0, translate: (0.0, 0.0) });
            }
        }
        WindowControlsStyle::Windows | WindowControlsStyle::Linux => {
            let min_bg = if min_hov { c(ModernTheme::WinBtnHover.0, ModernTheme::WinBtnHover.1, ModernTheme::WinBtnHover.2) } else { Color::TRANSPARENT };
            list.push_rect(DrawRect { x: w - 138.0, y: 0.0, width: 46.0, height: TAB_BAR_H, color: min_bg, opacity: 1.0, translate: (0.0, 0.0) });
            list.push_text(text_draw(w - 121.0, 11.0, "\u{2212}".to_string(), 14.0, c(ModernTheme::TextPrimary.0, ModernTheme::TextPrimary.1, ModernTheme::TextPrimary.2)));

            let max_bg = if max_hov { c(ModernTheme::WinBtnHover.0, ModernTheme::WinBtnHover.1, ModernTheme::WinBtnHover.2) } else { Color::TRANSPARENT };
            list.push_rect(DrawRect { x: w - 92.0, y: 0.0, width: 46.0, height: TAB_BAR_H, color: max_bg, opacity: 1.0, translate: (0.0, 0.0) });
            list.push_text(text_draw(w - 75.0, 11.0, "\u{25A1}".to_string(), 14.0, c(ModernTheme::TextPrimary.0, ModernTheme::TextPrimary.1, ModernTheme::TextPrimary.2)));

            let close_bg = if close_hov { c(ModernTheme::CloseRed.0, ModernTheme::CloseRed.1, ModernTheme::CloseRed.2) } else { Color::TRANSPARENT };
            list.push_rect(DrawRect { x: w - 46.0, y: 0.0, width: 46.0, height: TAB_BAR_H, color: close_bg, opacity: 1.0, translate: (0.0, 0.0) });
            list.push_text(text_draw(w - 29.0, 11.0, "\u{00D7}".to_string(), 14.0, c(ModernTheme::TextPrimary.0, ModernTheme::TextPrimary.1, ModernTheme::TextPrimary.2)));
        }
    }

    // ── Tabs ──
    for (i, tab) in tabs.iter().enumerate() {
        let tx = tab_x(i, style);
        if tx > w - rm {
            break;
        }
        let tab_w = TAB_VIS_W;

        if tab.is_active {
            // Active tab: pill shape
            draw_pill(list, tx, 1.0, tab_w, TAB_BAR_H - 2.0, TAB_PILL_R, c(ModernTheme::TabActiveBg.0, ModernTheme::TabActiveBg.1, ModernTheme::TabActiveBg.2));
        } else if hovered_tab == Some(i) {
            // Hovered inactive tab: subtle background
            draw_pill(list, tx, 1.0, tab_w, TAB_BAR_H - 2.0, TAB_PILL_R, c(ModernTheme::TabHoverBg.0, ModernTheme::TabHoverBg.1, ModernTheme::TabHoverBg.2));
        }

        // Tab title text
        let title: String = if tab.is_active && page_title.is_some() {
            page_title.unwrap().chars().take(20).collect()
        } else if tab.url.as_str() == "about:blank" {
            "New Tab".to_string()
        } else {
            tab.url.as_str().chars().take(20).collect()
        };
        let text_color = if tab.is_active {
            c(ModernTheme::TextPrimary.0, ModernTheme::TextPrimary.1, ModernTheme::TextPrimary.2)
        } else {
            c(ModernTheme::TextSecondary.0, ModernTheme::TextSecondary.1, ModernTheme::TextSecondary.2)
        };
        list.push_text(text_draw(tx + 12.0, 11.0, title, 13.0, text_color));

        // Close button: only visible on hover
        if hovered_tab == Some(i) {
            let close_cx = tx + tab_w - 14.0;
            let close_cy = TAB_BAR_H / 2.0;
            // Circle background
            list.push_circle(DrawCircle { cx: close_cx, cy: close_cy, radius: 8.0, color: c(ModernTheme::TabHoverBg.0, ModernTheme::TabHoverBg.1, ModernTheme::TabHoverBg.2) });
            // ×
            list.push_text(text_draw(close_cx - 3.5, close_cy - 6.0, "\u{00D7}".to_string(), 12.0, c(ModernTheme::TextSecondary.0, ModernTheme::TextSecondary.1, ModernTheme::TextSecondary.2)));
        }
    }

    // ── New tab button ──
    let new_tab_x = tab_x(tabs.len(), style).min(w - rm);
    let is_nth_over = hovered_tab == Some(tabs.len());
    if is_nth_over {
        draw_pill(list, new_tab_x, 1.0, 32.0, TAB_BAR_H - 2.0, TAB_PILL_R, c(ModernTheme::TabHoverBg.0, ModernTheme::TabHoverBg.1, ModernTheme::TabHoverBg.2));
    }
    list.push_text(text_draw(new_tab_x + 10.0, 11.0, "+".to_string(), 14.0, c(ModernTheme::TextSecondary.0, ModernTheme::TextSecondary.1, ModernTheme::TextSecondary.2)));
}

fn draw_navbar(list: &mut DisplayList, w: f32, back_hov: bool, fwd_hov: bool, rld_hov: bool, loading: bool, url_text: String, is_secure: bool, cursor_pos: usize, selection_start: Option<usize>, cursor_visible: bool, address_bar_focused: bool) {
    // Toolbar background
    list.push_rect(DrawRect { x: 0.0, y: TAB_BAR_H, width: w, height: TOOLBAR_H, color: c(ModernTheme::ToolbarBg.0, ModernTheme::ToolbarBg.1, ModernTheme::ToolbarBg.2), opacity: 1.0, translate: (0.0, 0.0) });

    // No top border — color depth separates tab bar from toolbar
    // Subtle bottom border
    list.push_rect(DrawRect { x: 0.0, y: TAB_BAR_H + TOOLBAR_H - 1.0, width: w, height: 1.0, color: c(ModernTheme::BorderSubtle.0, ModernTheme::BorderSubtle.1, ModernTheme::BorderSubtle.2), opacity: 1.0, translate: (0.0, 0.0) });

    // ── Back button ──
    let back_bg = if back_hov { c(ModernTheme::TabHoverBg.0, ModernTheme::TabHoverBg.1, ModernTheme::TabHoverBg.2) } else { Color::TRANSPARENT };
    draw_pill(list, BACK_X, BTN_Y, BTN_SIZE, BTN_SIZE, 8.0, back_bg);
    let back_color = c(ModernTheme::TextPrimary.0, ModernTheme::TextPrimary.1, ModernTheme::TextPrimary.2);
    draw_chevron_left(list, BACK_X + BTN_SIZE / 2.0, BTN_Y + BTN_SIZE / 2.0, 12.0, back_color);

    // ── Forward button ──
    let fwd_bg = if fwd_hov { c(ModernTheme::TabHoverBg.0, ModernTheme::TabHoverBg.1, ModernTheme::TabHoverBg.2) } else { Color::TRANSPARENT };
    draw_pill(list, FORWARD_X, BTN_Y, BTN_SIZE, BTN_SIZE, 8.0, fwd_bg);
    let fwd_color = c(ModernTheme::TextPrimary.0, ModernTheme::TextPrimary.1, ModernTheme::TextPrimary.2);
    draw_chevron_right(list, FORWARD_X + BTN_SIZE / 2.0, BTN_Y + BTN_SIZE / 2.0, 12.0, fwd_color);

    // ── Reload button ──
    let rld_bg = if rld_hov { c(ModernTheme::TabHoverBg.0, ModernTheme::TabHoverBg.1, ModernTheme::TabHoverBg.2) } else { Color::TRANSPARENT };
    draw_pill(list, RELOAD_X, BTN_Y, BTN_SIZE, BTN_SIZE, 8.0, rld_bg);
    let rld_color = c(ModernTheme::TextPrimary.0, ModernTheme::TextPrimary.1, ModernTheme::TextPrimary.2);
    draw_refresh(list, RELOAD_X + BTN_SIZE / 2.0, BTN_Y + BTN_SIZE / 2.0, 9.0, rld_color);

    // ── Address bar ──
    let (addr_x, addr_y, addr_w, addr_h) = addr_bar_rect(w);
    let addr_r = addr_h / 2.0; // full pill

    // Address bar border (drawn first, then bg covers the center)
    let border_color = if address_bar_focused {
        c(ModernTheme::Accent.0, ModernTheme::Accent.1, ModernTheme::Accent.2)
    } else {
        c(ModernTheme::BorderSubtle.0, ModernTheme::BorderSubtle.1, ModernTheme::BorderSubtle.2)
    };
    draw_pill(list, addr_x - 1.0, addr_y - 1.0, addr_w + 2.0, addr_h + 2.0, addr_r + 1.0, border_color);

    // Address bar background (pill) — drawn over the border, leaving only a 1px edge
    draw_pill(list, addr_x, addr_y, addr_w, addr_h, addr_r, c(ModernTheme::AddressBarBg.0, ModernTheme::AddressBarBg.1, ModernTheme::AddressBarBg.2));

    // Security lock icon (https)
    if is_secure {
        draw_lock_icon(list, addr_x + 10.0, addr_y + (addr_h - 12.0) / 2.0, 12.0, c(ModernTheme::Accent.0, ModernTheme::Accent.1, ModernTheme::Accent.2));
    }

    // Address bar text (t.y = TOP, not baseline; verified in renderer.rs:427)
    // For default 1280px window: addr_x=340, addr_y=44, addr_w=600, addr_h=28
    //   text_x = addr_x + 14 = 354 (non-secure), + 28 = 368 (secure)
    //   text_y = 44 + 14 - 7 + 3 = 54  (centered with descender room)
    let text_x = addr_x + if is_secure { 28.0 } else { 14.0 };
    let text_y = addr_y + (addr_h / 2.0) - (14.0 / 2.0) + 3.0;
    let is_empty = url_text.is_empty();
    let display_text = if is_empty && !address_bar_focused {
        "Search or enter address".to_string()
    } else {
        url_text
    };
    let text_color = if is_empty && !address_bar_focused {
        Color::from_rgba8(255, 255, 255, 115)
    } else {
        c(ModernTheme::TextPrimary.0, ModernTheme::TextPrimary.1, ModernTheme::TextPrimary.2)
    };

    // Clip text to pill bounds
    list.push_clip(ClipRect {
        x: addr_x + 1.0,
        y: addr_y,
        width: addr_w - 2.0,
        height: addr_h,
    });
    list.push_text(text_draw(text_x, text_y, display_text, 14.0, text_color));

    // Selection + cursor
    if address_bar_focused {
        let cursor_x = text_x + (cursor_pos as f32 * 14.0 * 0.6);
        if let Some(sel_start) = selection_start {
            let start = sel_start.min(cursor_pos);
            let end = sel_start.max(cursor_pos);
            let sel_x = text_x + (start as f32 * 14.0 * 0.6);
            let sel_width = (end - start) as f32 * 14.0 * 0.6;
            list.push_rect(DrawRect {
                x: sel_x,
                y: text_y - 2.0,
                width: sel_width.max(1.0),
                height: 14.0 + 4.0,
                color: Color::from_rgba8(ModernTheme::Accent.0, ModernTheme::Accent.1, ModernTheme::Accent.2, 100),
                opacity: 1.0,
                translate: (0.0, 0.0),
            });
        }

        if cursor_visible {
            list.push_rect(DrawRect {
                x: cursor_x,
                y: text_y - 2.0,
                width: 2.0,
                height: 14.0 + 4.0,
                color: c(ModernTheme::TextPrimary.0, ModernTheme::TextPrimary.1, ModernTheme::TextPrimary.2),
                opacity: 1.0,
                translate: (0.0, 0.0),
            });
        }
    }
    list.pop_clip();

    // Loading progress bar
    if loading {
        list.push_rect(DrawRect { x: addr_x, y: addr_y + addr_h - 2.0, width: addr_w, height: 2.0, color: Color::from_rgba8(ModernTheme::Accent.0, ModernTheme::Accent.1, ModernTheme::Accent.2, 180), opacity: 1.0, translate: (0.0, 0.0) });
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
    let hovered_tab = state.hovered_tab_index;

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

    // Page background behind content
    list.push_rect(DrawRect { x: 0.0, y: 0.0, width, height, color: c(ModernTheme::TabBarBg.0, ModernTheme::TabBarBg.1, ModernTheme::TabBarBg.2), opacity: 1.0, translate: (0.0, 0.0) });

    // Row 1 - Titlebar + Tabs (y=0..36)
    draw_titlebar(list, &tabs, page_title, width, min_hov, max_hov, close_hov, focused, hovered_tab);

    // Row 2 - Navigation + Address bar (y=36..80)
    draw_navbar(list, width, back_hov, fwd_hov, rld_hov, loading, url_text.clone(), is_secure, cursor_pos, selection_start, cursor_visible, address_bar_focused);

    // Content area (y=80..height)
    let content_area_y = HEADER_H;
    let content_area_h = height - HEADER_H - 8.0; // 8px bottom guard

    list.push_rect(DrawRect {
        x: 16.0, y: content_area_y, width: width - 32.0, height: content_area_h,
        color: Color::from_rgba8(255, 255, 255, 255),
        opacity: 1.0,
        translate: (0.0, 0.0),
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
                list.push_rect(DrawRect { x: 16.0 + rect.x, y: render_y, width: rect.width, height: rect.height, color: rect.color, opacity: 1.0, translate: (0.0, 0.0) });
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

    // Mask header (y=0..80) to cover any content overflow, then redraw UI
    list.push_rect(DrawRect { x: 0.0, y: 0.0, width, height: HEADER_H, color: c(ModernTheme::TabBarBg.0, ModernTheme::TabBarBg.1, ModernTheme::TabBarBg.2), opacity: 1.0, translate: (0.0, 0.0) });
    draw_titlebar(list, &tabs, page_title, width, min_hov, max_hov, close_hov, focused, hovered_tab);
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
        list.push_rect(DrawRect { x: sb_x, y: content_area_y, width: scrollbar_width, height: content_area_h, color: Color::from_rgba8(ModernTheme::BorderSubtle.0, ModernTheme::BorderSubtle.1, ModernTheme::BorderSubtle.2, 100), opacity: 1.0, translate: (0.0, 0.0) });
        list.push_rect(DrawRect { x: sb_x, y: thumb_y, width: scrollbar_width, height: thumb_height, color: Color::from_rgba8(ModernTheme::TextSecondary.0, ModernTheme::TextSecondary.1, ModernTheme::TextSecondary.2, 150), opacity: 1.0, translate: (0.0, 0.0) });
    }

}
