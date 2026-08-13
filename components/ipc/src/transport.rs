use crate::{split_transport, Receiver, Sender};
use thiserror::Error;
use tokio::io::{AsyncRead, AsyncWrite, ReadHalf, WriteHalf};

#[cfg(unix)]
use std::path::{Path, PathBuf};

#[cfg(unix)]
use tokio::net::{UnixListener, UnixStream};

#[cfg(windows)]
use tokio::net::windows::named_pipe::{
    ClientOptions, NamedPipeClient, NamedPipeServer, ServerOptions,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlatformAddress {
    #[cfg(windows)]
    NamedPipe(String),
    #[cfg(unix)]
    UnixSocket(PathBuf),
}

impl PlatformAddress {
    /// Generate a per-process, per-role unique address for a child process
    /// to connect back to.
    pub fn new_unique(role: &str) -> Self {
        #[cfg(windows)]
        {
            let name = format!(
                "\\\\.\\pipe\\kore-{role}-{}-{}",
                std::process::id(),
                unique_counter()
            );
            PlatformAddress::NamedPipe(name)
        }
        #[cfg(unix)]
        {
            let path = std::env::temp_dir().join(format!(
                "kore-{role}-{}-{}.sock",
                std::process::id(),
                unique_counter()
            ));
            PlatformAddress::UnixSocket(path)
        }
    }

    /// The string form passed to a child process on the command line.
    pub fn to_arg(&self) -> String {
        match self {
            #[cfg(windows)]
            PlatformAddress::NamedPipe(name) => name.clone(),
            #[cfg(unix)]
            PlatformAddress::UnixSocket(path) => path.to_string_lossy().to_string(),
        }
    }
}

#[cfg(unix)]
impl PlatformAddress {
    pub fn as_unix_path(&self) -> &Path {
        let PlatformAddress::UnixSocket(path) = self;
        path
    }
}

fn unique_counter() -> u64 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    COUNTER.fetch_add(1, Ordering::SeqCst)
}

#[derive(Debug, Error)]
pub enum TransportError {
    #[error("io error")]
    Io(#[from] std::io::Error),
}

// ── Streams ──────────────────────────────────────────────────────

#[cfg(windows)]
pub enum PlatformStream {
    Client(NamedPipeClient),
    Server(NamedPipeServer),
}

#[cfg(windows)]
impl AsyncRead for PlatformStream {
    fn poll_read(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        match self.get_mut() {
            PlatformStream::Client(s) => std::pin::Pin::new(s).poll_read(cx, buf),
            PlatformStream::Server(s) => std::pin::Pin::new(s).poll_read(cx, buf),
        }
    }
}

#[cfg(windows)]
impl AsyncWrite for PlatformStream {
    fn poll_write(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> std::task::Poll<std::io::Result<usize>> {
        match self.get_mut() {
            PlatformStream::Client(s) => std::pin::Pin::new(s).poll_write(cx, buf),
            PlatformStream::Server(s) => std::pin::Pin::new(s).poll_write(cx, buf),
        }
    }

    fn poll_flush(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        match self.get_mut() {
            PlatformStream::Client(s) => std::pin::Pin::new(s).poll_flush(cx),
            PlatformStream::Server(s) => std::pin::Pin::new(s).poll_flush(cx),
        }
    }

    fn poll_shutdown(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        match self.get_mut() {
            PlatformStream::Client(s) => std::pin::Pin::new(s).poll_shutdown(cx),
            PlatformStream::Server(s) => std::pin::Pin::new(s).poll_shutdown(cx),
        }
    }
}

#[cfg(unix)]
pub type PlatformStream = UnixStream;

pub struct PlatformTransport {
    stream: PlatformStream,
}

impl PlatformTransport {
    #[cfg(windows)]
    pub fn from_named_pipe(stream: NamedPipeClient) -> Self {
        Self {
            stream: PlatformStream::Client(stream),
        }
    }

    #[cfg(windows)]
    pub fn from_named_pipe_server(stream: NamedPipeServer) -> Self {
        Self {
            stream: PlatformStream::Server(stream),
        }
    }

    #[cfg(windows)]
    pub fn connect_named_pipe(pipe_name: &str) -> Result<Self, TransportError> {
        let stream = ClientOptions::new().open(pipe_name)?;
        Ok(Self {
            stream: PlatformStream::Client(stream),
        })
    }

    #[cfg(unix)]
    pub fn from_unix_socket(stream: UnixStream) -> Self {
        Self { stream }
    }

    #[cfg(unix)]
    pub async fn connect_unix_socket(path: impl AsRef<Path>) -> Result<Self, TransportError> {
        let stream = UnixStream::connect(path).await?;
        Ok(Self { stream })
    }

    pub fn split(
        self,
    ) -> (
        Sender<WriteHalf<PlatformStream>>,
        Receiver<ReadHalf<PlatformStream>>,
    )
    where
        PlatformStream: AsyncRead + AsyncWrite + Unpin,
    {
        split_transport(self.stream)
    }
}

// ── Server side (browser process) ────────────────────────────────

pub struct PlatformListener {
    #[cfg(windows)]
    server: NamedPipeServer,
    #[cfg(unix)]
    listener: UnixListener,
    #[cfg(unix)]
    path: PathBuf,
}

impl PlatformListener {
    #[cfg(windows)]
    pub fn bind(address: &PlatformAddress) -> Result<Self, TransportError> {
        let PlatformAddress::NamedPipe(name) = address;
        let server = ServerOptions::new().first_pipe_instance(true).create(name)?;
        Ok(Self { server })
    }

    #[cfg(unix)]
    pub fn bind(address: &PlatformAddress) -> Result<Self, TransportError> {
        let path = address.as_unix_path();
        let _ = std::fs::remove_file(path);
        let listener = UnixListener::bind(path)?;
        Ok(Self {
            listener,
            path: path.to_path_buf(),
        })
    }

    /// Wait for the child process to connect and hand back the transport.
    #[cfg(windows)]
    pub async fn accept(self) -> Result<PlatformTransport, TransportError> {
        self.server.connect().await?;
        Ok(PlatformTransport::from_named_pipe_server(self.server))
    }

    #[cfg(unix)]
    pub async fn accept(self) -> Result<PlatformTransport, TransportError> {
        let (stream, _addr) = self.listener.accept().await?;
        let _ = std::fs::remove_file(&self.path);
        Ok(PlatformTransport::from_unix_socket(stream))
    }
}