use crate::IpcMessage;
use std::io;
use thiserror::Error;
use tokio::io::{split, AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, ReadHalf, WriteHalf};

pub const MAX_FRAME_BYTES: usize = 16 * 1024 * 1024;

#[derive(Debug, Error)]
pub enum IpcError {
    #[error("io error")]
    Io(#[from] io::Error),
    #[error("serialization error")]
    Codec(#[from] bincode::Error),
    #[error("ipc frame length {actual} exceeds maximum {max}")]
    FrameTooLarge { actual: usize, max: usize },
    #[error("peer closed the IPC stream")]
    PeerClosed,
}

pub struct Sender<W> {
    writer: W,
}

impl<W> Sender<W>
where
    W: AsyncWrite + Unpin,
{
    pub fn new(writer: W) -> Self {
        Self { writer }
    }

    pub async fn send(&mut self, message: &IpcMessage) -> Result<(), IpcError> {
        let bytes = message.to_bytes()?;
        if bytes.len() > MAX_FRAME_BYTES {
            return Err(IpcError::FrameTooLarge {
                actual: bytes.len(),
                max: MAX_FRAME_BYTES,
            });
        }

        self.writer.write_u32(bytes.len() as u32).await?;
        self.writer.write_all(&bytes).await?;
        self.writer.flush().await?;
        Ok(())
    }
}

pub struct Receiver<R> {
    reader: R,
}

impl<R> Receiver<R>
where
    R: AsyncRead + Unpin,
{
    pub fn new(reader: R) -> Self {
        Self { reader }
    }

    pub async fn recv(&mut self) -> Result<IpcMessage, IpcError> {
        let len = match self.reader.read_u32().await {
            Ok(len) => len as usize,
            Err(err) if err.kind() == io::ErrorKind::UnexpectedEof => {
                return Err(IpcError::PeerClosed);
            }
            Err(err) => return Err(IpcError::Io(err)),
        };

        if len > MAX_FRAME_BYTES {
            return Err(IpcError::FrameTooLarge {
                actual: len,
                max: MAX_FRAME_BYTES,
            });
        }

        let mut bytes = vec![0; len];
        self.reader.read_exact(&mut bytes).await?;
        Ok(IpcMessage::from_bytes(&bytes)?)
    }
}

pub fn split_transport<T>(transport: T) -> (Sender<WriteHalf<T>>, Receiver<ReadHalf<T>>)
where
    T: AsyncRead + AsyncWrite + Unpin,
{
    let (reader, writer) = split(transport);
    (Sender::new(writer), Receiver::new(reader))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::IpcPayload;
    use url::Url;

    #[tokio::test]
    async fn sends_and_receives_length_prefixed_messages() -> Result<(), Box<dyn std::error::Error>>
    {
        let (left, right) = tokio::io::duplex(1024);
        let (mut sender, _) = split_transport(left);
        let (_, mut receiver) = split_transport(right);
        let message = IpcMessage::new(
            100,
            5,
            IpcPayload::NavigateToUrl {
                tab_id: 9,
                url: Url::parse("https://example.com/")?,
            },
        );

        sender.send(&message).await?;
        let received = receiver.recv().await?;

        assert_eq!(received, message);
        Ok(())
    }
}
