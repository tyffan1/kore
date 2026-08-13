//! Network process manager.
//!
//! Spawns the dedicated `kore-network` child process, connects to it over
//! the platform IPC transport, and forwards page-load fetches to it. The
//! HTTP/HTTPS stack (TLS, redirects, cookies) therefore runs outside the
//! browser process, in a sandboxed child that may only use the network.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;


use kore_ipc::{
    IpcMessage, IpcPayload, PlatformAddress, PlatformListener, PlatformStream, ProcessId, Receiver,
    Sender,
};
use kore_net::{FetchRequest, FetchResponse};
use kore_sandbox::{Policy, PolicyBuilder, SandboxedProcess};
use tokio::io::{ReadHalf, WriteHalf};
use tokio::sync::{oneshot, Mutex};

use crate::error::BrowserError;

/// The concrete stream type produced by [`PlatformTransport::split`].
type NetStream = PlatformStream;

/// A single in-flight fetch waiting for its correlated response.
type PendingFetches = Arc<Mutex<HashMap<u64, oneshot::Sender<Result<FetchResponse, String>>>>>;

pub struct NetworkProcess {
    process: SandboxedProcess,
    process_id: ProcessId,
    sender: Arc<Mutex<Sender<WriteHalf<NetStream>>>>,
    pending: PendingFetches,
    next_message_id: AtomicU64,
}

impl NetworkProcess {
    /// Spawn the network child and wait for it to connect back.
    pub async fn spawn() -> Result<Self, BrowserError> {
        let address = PlatformAddress::new_unique("net");
        let listener =
            PlatformListener::bind(&address).map_err(|e| BrowserError::NetworkSpawn(e.to_string()))?;

        let policy = Self::network_policy();
        let args = vec![address.to_arg()];
        let process = SandboxedProcess::spawn("kore-network", &args, &policy)
            .map_err(|e| BrowserError::NetworkSpawn(e.to_string()))?;
        let process_id = process.id();

        let transport = tokio::time::timeout(
            std::time::Duration::from_secs(10),
            listener.accept(),
        )
        .await
        .map_err(|_| BrowserError::NetworkSpawn("timed out waiting for the network process".into()))?
        .map_err(|e| BrowserError::NetworkSpawn(e.to_string()))?;

        let (sender, receiver) = transport.split();
        let pending = spawn_dispatcher(receiver);

        Ok(Self {
            process,
            process_id,
            sender: Arc::new(Mutex::new(sender)),
            pending,
            next_message_id: AtomicU64::new(1),
        })
    }

    /// The network process may only talk to the network — no filesystem,
    /// bounded memory and CPU.
    fn network_policy() -> Policy {
        PolicyBuilder::new()
            .allow_filesystem(false)
            .allow_networking(true)
            .max_memory(256 * 1024 * 1024)
            .max_cpu_time(30000)
            .build()
    }

    pub fn process_id(&self) -> ProcessId {
        self.process_id
    }

    /// Fetch a URL through the network process. Safe to call concurrently:
    /// responses are correlated by message id and routed by a dispatcher
    /// task back to the caller that issued the request.
    pub async fn fetch(&self, request: FetchRequest) -> Result<FetchResponse, BrowserError> {
        let message_id = self.next_message_id.fetch_add(1, Ordering::SeqCst);
        let (tx, rx) = oneshot::channel();
        self.pending.lock().await.insert(message_id, tx);

        let message = IpcMessage::new(
            message_id,
            self.process_id,
            IpcPayload::Fetch { request },
        );

{
            let mut sender = self.sender.lock().await;
            if let Err(e) = sender.send(&message).await {
                self.pending.lock().await.remove(&message_id);
                return Err(BrowserError::NetworkIpc(e.to_string()));
            }
        }

        match tokio::time::timeout(std::time::Duration::from_secs(30), rx).await {
            Ok(Ok(response)) => response.map_err(BrowserError::NetworkIpc),
            Ok(Err(_)) => {
                self.pending.lock().await.remove(&message_id);
                Err(BrowserError::NetworkIpc(
                    "network process closed the connection".into(),
                ))
            }
            Err(_) => {
                self.pending.lock().await.remove(&message_id);
                Err(BrowserError::NetworkIpc(
                    "network process timed out".into(),
                ))
            }
        }
    }

    pub fn shutdown(mut self) -> Result<(), BrowserError> {
        self.process
            .kill()
            .map_err(|e| BrowserError::NetworkIpc(e.to_string()))
    }
}

/// Background task owning the receive half of the connection. Routes
/// `FetchResult` responses to the waiting callers by message id.
fn spawn_dispatcher(receiver: Receiver<ReadHalf<NetStream>>) -> PendingFetches {
    let pending: PendingFetches = Arc::new(Mutex::new(HashMap::new()));
    let pending_clone = Arc::clone(&pending);
    tokio::spawn(async move {
        let mut receiver = receiver;
        loop {
            let message = match receiver.recv().await {
                Ok(m) => m,
                Err(_) => break,
            };
            if let IpcPayload::FetchResult { response } = message.payload {
                if let Some(tx) = pending_clone.lock().await.remove(&message.message_id) {
                    let _ = tx.send(response);
                }
            }
        }
    });
    pending
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn network_policy_isolates_filesystem() {
        let policy = NetworkProcess::network_policy();
        assert!(policy.allow_networking);
        assert!(!policy.allow_filesystem);
        assert_eq!(policy.max_memory, Some(256 * 1024 * 1024));
        assert_eq!(policy.max_cpu_time, Some(30000));
    }
}