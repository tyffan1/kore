use std::cell::RefCell;
use std::io::Read;
use std::sync::Arc;
use std::sync::mpsc;

use kore_gpu::{
    Color, DisplayCommand, DisplayList, DrawRect, Renderer, RendererConfig,
};
use kore_ipc::{FrameRenderCommand, IpcMessage, IpcPayload};
use kore_window::{AppEvent, EventLoop, WindowBuilder, WindowHandle};

struct RenderState {
    _window: Arc<winit::window::Window>,
    renderer: Renderer,
    display_list: DisplayList,
    tab_id: u64,
}

enum MainState {
    Uninitialized,
    Ready(RenderState),
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let tab_id: u64 = std::env::args()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);

    let config = WindowBuilder::new()
        .title(&format!("Kore Tab {tab_id}"))
        .size(1280, 720)
        .build();

    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::default());

    let (ipc_tx, ipc_rx) = mpsc::channel::<IpcMessage>();
    std::thread::spawn(move || ipc_reader_thread(ipc_tx));

    let state = RefCell::new(MainState::Uninitialized);

    let el = EventLoop::new()?;
    el.run(move |event, elwt| {
        match event {
            AppEvent::Redraw => {
                for msg in ipc_rx.try_iter() {
                    process_ipc_message(&msg, &mut *state.borrow_mut());
                }

                let mut borrow = state.borrow_mut();
                if matches!(*borrow, MainState::Uninitialized) {
                    match WindowHandle::new(elwt, &instance, &config) {
                        Ok(handle) => {
                            let (window, surface) = handle.into_parts();
                            match pollster::block_on(Renderer::new(
                                &instance,
                                surface,
                                RendererConfig::default(),
                            )) {
                                Ok(renderer) => {
                                    window.request_redraw();
                                    *borrow = MainState::Ready(RenderState {
                                        _window: window,
                                        renderer,
                                        display_list: create_demo_list(),
                                        tab_id,
                                    });
                                }
                                Err(e) => {
                                    eprintln!("Failed to create renderer: {e}");
                                }
                            }
                        }
                        Err(e) => {
                            eprintln!("Failed to create window: {e}");
                        }
                    }
                }

                if let MainState::Ready(rs) = &mut *borrow {
                    match rs.renderer.begin_frame() {
                        Ok(mut frame) => {
                            rs.renderer.submit(&mut frame, &rs.display_list);
                            if let Err(e) = rs.renderer.end_frame(frame) {
                                eprintln!("Render error: {e}");
                            }
                            rs._window.request_redraw();
                        }
                        Err(e) => {
                            eprintln!("Begin frame error: {e}");
                        }
                    }
                }
            }
            AppEvent::Resized { width, height } => {
                if let MainState::Ready(rs) = &mut *state.borrow_mut() {
                    rs.renderer.resize(width, height);
                }
            }
            AppEvent::CloseRequested => {}
            _ => {}
        }
    });
}

fn create_demo_list() -> DisplayList {
    let mut list = DisplayList::new();
    list.push_rect(DrawRect {
        x: 0.0, y: 0.0, width: 1280.0, height: 720.0,
        color: Color::from_rgba8(240, 240, 245, 255),
        opacity: 1.0, translate: (0.0, 0.0),
    });
    list.push_rect(DrawRect {
        x: 0.0, y: 0.0, width: 1280.0, height: 48.0,
        color: Color::from_rgba8(30, 30, 35, 255),
        opacity: 1.0, translate: (0.0, 0.0),
    });
    list.push_rect(DrawRect {
        x: 50.0, y: 80.0, width: 200.0, height: 150.0,
        color: Color::from_rgba8(66, 133, 244, 255),
        opacity: 1.0, translate: (0.0, 0.0),
    });
    list.push_rect(DrawRect {
        x: 280.0, y: 80.0, width: 200.0, height: 150.0,
        color: Color::from_rgba8(52, 168, 83, 255),
        opacity: 1.0, translate: (0.0, 0.0),
    });
    list.push_rect(DrawRect {
        x: 510.0, y: 80.0, width: 200.0, height: 150.0,
        color: Color::from_rgba8(251, 188, 4, 255),
        opacity: 1.0, translate: (0.0, 0.0),
    });
    list
}

fn process_ipc_message(msg: &IpcMessage, state: &mut MainState) {
    match &msg.payload {
        IpcPayload::RenderFrame(frame) => {
            if let MainState::Ready(rs) = state {
                rs.display_list.clear();
                for cmd in &frame.commands {
                    if let Some(dc) = frame_cmd_to_display_cmd(cmd) {
                        rs.display_list.push(dc);
                    }
                }
            }
        }
        IpcPayload::NavigateToUrl { tab_id: _, url: _ } => {}
        IpcPayload::TabClosed(tc) => {
            if let MainState::Ready(rs) = state {
                if rs.tab_id == tc.tab_id {
                    std::process::exit(0);
                }
            }
        }
        _ => {}
    }
}

fn frame_cmd_to_display_cmd(cmd: &FrameRenderCommand) -> Option<DisplayCommand> {
    match cmd {
        FrameRenderCommand::Clear { color: _ } => None,
        FrameRenderCommand::Rect {
            x,
            y,
            width,
            height,
            color,
        } => {
            let c = parse_hex_color(color);
            Some(DisplayCommand::Rect(DrawRect {
                x: *x as f32,
                y: *y as f32,
                width: *width as f32,
                height: *height as f32,
                color: c,
                opacity: 1.0,
                translate: (0.0, 0.0),
            }))
        }
        FrameRenderCommand::Text {
            x: _,
            y: _,
            text: _,
        } => None,
    }
}

fn parse_hex_color(hex: &str) -> Color {
    let hex = hex.trim_start_matches('#');
    if hex.len() == 6 {
        let r = u8::from_str_radix(&hex[0..2], 16).unwrap_or_default();
        let g = u8::from_str_radix(&hex[2..4], 16).unwrap_or_default();
        let b = u8::from_str_radix(&hex[4..6], 16).unwrap_or_default();
        Color::from_rgba8(r, g, b, 255)
    } else if hex.len() == 8 {
        let r = u8::from_str_radix(&hex[0..2], 16).unwrap_or_default();
        let g = u8::from_str_radix(&hex[2..4], 16).unwrap_or_default();
        let b = u8::from_str_radix(&hex[4..6], 16).unwrap_or_default();
        let a = u8::from_str_radix(&hex[6..8], 16).unwrap_or_default();
        Color::from_rgba8(r, g, b, a)
    } else {
        Color::WHITE
    }
}

fn ipc_reader_thread(tx: mpsc::Sender<IpcMessage>) {
    let mut stdin = std::io::stdin();
    loop {
        let mut len_buf = [0u8; 4];
        if stdin.read_exact(&mut len_buf).is_err() {
            break;
        }
        let len = u32::from_le_bytes(len_buf) as usize;
        let mut buf = vec![0u8; len];
        if stdin.read_exact(&mut buf).is_err() {
            break;
        }
        match IpcMessage::from_bytes(&buf) {
            Ok(msg) => {
                if tx.send(msg).is_err() {
                    break;
                }
            }
            Err(_) => break,
        }
    }
}
