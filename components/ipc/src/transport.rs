use crate::{split_transport, Receiver, Sender};
use thiserror::Error;
use tokio::io::{AsyncRead, AsyncWrite, ReadHalf, WriteHalf};

#[cfg(unix)]
use std::path::{Path, PathBuf};

#[cfg(unix)]
use tokio::net::UnixStream;

#[cfg(windows)]
use tokio::net::windows::named_pipe::{ClientOptions, NamedPipeClient};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlatformAddress {
    #[cfg(windows)]
    NamedPipe(String),
    #[cfg(unix)]
    UnixSocket(PathBuf),
}

#[derive(Debug, Error)]
pub enum TransportError {
    #[error("io error")]
    Io(#[from] std::io::Error),
}

#[cfg(windows)]
type PlatformStream = NamedPipeClient;

#[cfg(unix)]
type PlatformStream = UnixStream;

pub struct PlatformTransport {
    stream: PlatformStream,
}

impl PlatformTransport {
    #[cfg(windows)]
    pub fn from_named_pipe(stream: NamedPipeClient) -> Self {
        Self { stream }
    }

    #[cfg(windows)]
    pub fn connect_named_pipe(pipe_name: &str) -> Result<Self, TransportError> {
        let stream = ClientOptions::new().open(pipe_name)?;
        Ok(Self { stream })
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
