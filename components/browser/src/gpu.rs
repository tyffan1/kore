//! GPU process manager.
//!
//! Spawns the dedicated `kore-gpuprocess` child, connects to it over the
//! platform IPC transport, and sends compositing frames to it. The child
//! owns the wgpu device/queue and rasterizes display lists offscreen; the
//! browser process blits the returned pixels onto its window surface.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use kore_gpu::{DisplayList, GpuImage};
use kore_ipc::{
    IpcMessage, IpcPayload, PlatformAddress, PlatformListener, PlatformStream, ProcessId, Receiver,
    Sender,
};
use kore_sandbox::{Policy, PolicyBuilder, SandboxedProcess};
use tokio::io::{ReadHalf, WriteHalf};
use tokio::sync::Mutex;

use crate::error::BrowserError;

/// The concrete stream type produced by [`PlatformTransport::split`].
type GpuStream = PlatformStream;

pub struct GpuProcess {
    process: SandboxedProcess,
    process_id: ProcessId,
    sender: Arc<Mutex<Sender<WriteHalf<GpuStream>>>>,
    receiver: Arc<Mutex<Receiver<ReadHalf<GpuStream>>>>,
    next_message_id: AtomicU64,
}

impl GpuProcess {
    /// Spawn the GPU child and wait for it to connect back.
    pub async fn spawn() -> Result<Self, BrowserError> {
        let address = PlatformAddress::new_unique("gpu");
        let listener =
            PlatformListener::bind(&address).map_err(|e| BrowserError::GpuSpawn(e.to_string()))?;

        let policy = Self::gpu_policy();
        let args = vec![address.to_arg()];
        let process = SandboxedProcess::spawn("kore-gpuprocess", &args, &policy)
            .map_err(|e| BrowserError::GpuSpawn(e.to_string()))?;
        let process_id = process.id();

        let transport = tokio::time::timeout(
            std::time::Duration::from_secs(10),
            listener.accept(),
        )
        .await
        .map_err(|_| BrowserError::GpuSpawn("timed out waiting for the GPU process".into()))?
        .map_err(|e| BrowserError::GpuSpawn(e.to_string()))?;

        let (sender, receiver) = transport.split();

        Ok(Self {
            process,
            process_id,
            sender: Arc::new(Mutex::new(sender)),
            receiver: Arc::new(Mutex::new(receiver)),
            next_message_id: AtomicU64::new(1),
        })
    }

    /// The GPU process needs no network; filesystem is allowed so it can
    /// read system fonts during rasterization.
    fn gpu_policy() -> Policy {
        PolicyBuilder::new()
            .allow_filesystem(true)
            .allow_networking(false)
            .max_memory(512 * 1024 * 1024)
            .max_cpu_time(30000)
            .build()
    }

    pub fn process_id(&self) -> ProcessId {
        self.process_id
    }

    /// Render a display list in the GPU process and return the RGBA pixels.
    ///
    /// Note: the browser process issues compositing frames from a single
    /// thread, so request/response correlation is assumed sequential.
    pub async fn render_frame(
        &self,
        display_list: DisplayList,
        width: u32,
        height: u32,
    ) -> Result<GpuImage, BrowserError> {
        let message_id = self.next_message_id.fetch_add(1, Ordering::SeqCst);
        let message = IpcMessage::new(
            message_id,
            self.process_id,
            IpcPayload::RenderGpuFrame {
                frame_id: message_id,
                width,
                height,
                display_list,
            },
        );

        {
            let mut sender = self.sender.lock().await;
            sender
                .send(&message)
                .await
                .map_err(|e| BrowserError::GpuIpc(e.to_string()))?;
        }

        loop {
            let message = tokio::time::timeout(
                std::time::Duration::from_secs(15),
                self.receiver.lock().await.recv(),
            )
            .await
            .map_err(|_| BrowserError::GpuIpc("GPU process timed out".into()))?
            .map_err(|e| BrowserError::GpuIpc(e.to_string()))?;

            if message.message_id != message_id {
                continue;
            }
            return match message.payload {
                IpcPayload::GpuFrameRendered {
                    width,
                    height,
                    pixels,
                    ..
                } => Ok(GpuImage {
                    width,
                    height,
                    pixels,
                }),
                IpcPayload::GpuFrameFailed { error, .. } => {
                    Err(BrowserError::GpuIpc(format!("render failed: {error}")))
                }
                _ => Err(BrowserError::GpuIpc(
                    "unexpected response from GPU process".into(),
                )),
            };
        }
    }

    pub fn shutdown(mut self) -> Result<(), BrowserError> {
        self.process
            .kill()
            .map_err(|e| BrowserError::GpuIpc(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gpu_policy_blocks_network() {
        let policy = GpuProcess::gpu_policy();
        assert!(policy.allow_filesystem);
        assert!(!policy.allow_networking);
        assert_eq!(policy.max_memory, Some(512 * 1024 * 1024));
        assert_eq!(policy.max_cpu_time, Some(30000));
    }
}