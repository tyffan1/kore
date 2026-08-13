//! Dedicated network process for Kore.
//!
//! Connects to the browser process over the platform IPC transport and
//! serves [`IpcPayload::Fetch`] requests with the shared HTTP/HTTPS stack
//! (`HttpClient`). This keeps TLS, redirects, cookies and response
//! buffering out of the browser process.

use kore_ipc::{IpcMessage, IpcPayload, PlatformTransport};
use kore_net::{HttpClient, HttpClientConfig};

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

    let client = HttpClient::new(HttpClientConfig::default());

    loop {
        let message = match receiver.recv().await {
            Ok(m) => m,
            Err(_) => break,
        };
        let id = message.message_id;

        let payload = match message.payload {
            IpcPayload::Fetch { request } => {
                let response = client.fetch(request).await.map_err(|e| e.to_string());
                IpcPayload::FetchResult { response }
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