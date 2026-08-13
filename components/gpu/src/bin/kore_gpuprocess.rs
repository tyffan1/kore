//! Dedicated GPU process for Kore.
//!
//! Connects to the browser process over the platform IPC transport,
//! receives [`IpcPayload::RenderGpuFrame`] messages carrying a display
//! list, renders them offscreen, and returns the RGBA pixels in
//! [`IpcPayload::GpuFrameRendered`].

use kore_gpu::{Renderer, RendererConfig};
use kore_ipc::{IpcMessage, IpcPayload, PlatformTransport};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let addr = std::env::args().nth(1).unwrap_or_default();

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    runtime.block_on(run(&addr))
}

async fn run(addr: &str) -> Result<(), Box<dyn std::error::Error>> {
    #[cfg(windows)]
    let transport = {
        let mut connected = None;
        for _ in 0..50 {
            if let Ok(t) = PlatformTransport::connect_named_pipe(addr) {
                connected = Some(t);
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        match connected {
            Some(t) => t,
            None => return Err("could not connect to browser process".into()),
        }
    };

    #[cfg(unix)]
    let transport = {
        let mut connected = None;
        for _ in 0..50 {
            if let Ok(t) = PlatformTransport::connect_unix_socket(addr).await {
                connected = Some(t);
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
        match connected {
            Some(t) => t,
            None => return Err("could not connect to browser process".into()),
        }
    };

    serve(transport).await
}

async fn serve(transport: PlatformTransport) -> Result<(), Box<dyn std::error::Error>> {
    let (mut sender, mut receiver) = transport.split();

    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::default());
    let config = RendererConfig::default();
    let mut size = (config.width, config.height);
    let mut renderer = pollster::block_on(Renderer::new_offscreen(&instance, config))?;

    loop {
        let message = match receiver.recv().await {
            Ok(m) => m,
            Err(_) => break,
        };
        let id = message.message_id;

        let payload = match message.payload {
            IpcPayload::RenderGpuFrame {
                frame_id,
                width,
                height,
                display_list,
            } => {
                if (width, height) != size {
                    renderer.resize(width, height);
                    size = (width, height);
                }
                match pollster::block_on(renderer.render_to_image(&display_list)) {
                    Ok(image) => IpcPayload::GpuFrameRendered {
                        frame_id,
                        width: image.width,
                        height: image.height,
                        pixels: image.pixels,
                    },
                    Err(e) => IpcPayload::GpuFrameFailed {
                        frame_id,
                        error: e.to_string(),
                    },
                }
            }
            _ => continue,
        };

        let reply = IpcMessage::new(id, 0, payload);
        if sender.send(&reply).await.is_err() {
            break;
        }
    }

    Ok(())
}