//! I/O source abstractions

use async_trait::async_trait;
use std::io::{self, SeekFrom};
use std::path::Path;
use tokio::fs::File;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncSeek, AsyncSeekExt};

pub type Result<T> = std::result::Result<T, io::Error>;

/// Async source trait for reading media data
///
/// Implementors provide async read and seek operations.
/// Sources can be local files, network streams, or in-memory buffers.
#[async_trait]
pub trait Source: AsyncRead + AsyncSeek + Send + Unpin {
    /// Get total size of the source in bytes (if known)
    async fn size(&self) -> Result<Option<u64>>;

    /// Check if this source supports seeking
    fn seekable(&self) -> bool;

    /// Read exactly `buf.len()` bytes or return error
    async fn read_exact_at(&mut self, offset: u64, buf: &mut [u8]) -> Result<()> {
        self.seek(SeekFrom::Start(offset)).await?;
        AsyncReadExt::read_exact(self, buf).await?;
        Ok(())
    }
}

/// File-based source for local media files
///
/// Uses async I/O via tokio::fs::File. For performance-critical scenarios,
/// memory-mapped files can be used (future enhancement).
///
/// # Examples
/// ```no_run
/// use av_io::FileSource;
///
/// # async fn example() -> std::io::Result<()> {
/// let source = FileSource::open("video.mp4").await?;
/// # Ok(())
/// # }
/// ```
pub struct FileSource {
    file: File,
    size: u64,
}

impl FileSource {
    /// Open a file as a source
    pub async fn open(path: impl AsRef<Path>) -> Result<Self> {
        let file = File::open(path).await?;
        let metadata = file.metadata().await?;
        let size = metadata.len();

        Ok(Self { file, size })
    }

    /// Get file size in bytes
    pub fn size(&self) -> u64 {
        self.size
    }
}

#[async_trait]
impl Source for FileSource {
    async fn size(&self) -> Result<Option<u64>> {
        Ok(Some(self.size))
    }

    fn seekable(&self) -> bool {
        true
    }
}

impl AsyncRead for FileSource {
    fn poll_read(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<io::Result<()>> {
        std::pin::Pin::new(&mut self.file).poll_read(cx, buf)
    }
}

impl AsyncSeek for FileSource {
    fn start_seek(mut self: std::pin::Pin<&mut Self>, position: SeekFrom) -> io::Result<()> {
        std::pin::Pin::new(&mut self.file).start_seek(position)
    }

    fn poll_complete(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<io::Result<u64>> {
        std::pin::Pin::new(&mut self.file).poll_complete(cx)
    }
}

/// In-memory source backed by a byte vector
///
/// Useful for testing and for sources that are already in memory.
///
/// # Examples
/// ```
/// use av_io::MemorySource;
///
/// let data = vec![0u8; 1024];
/// let source = MemorySource::new(data);
/// assert_eq!(source.len(), 1024);
/// ```
pub struct MemorySource {
    data: Vec<u8>,
    position: usize,
}

impl MemorySource {
    /// Create a new memory source from a byte vector
    pub fn new(data: Vec<u8>) -> Self {
        Self { data, position: 0 }
    }

    /// Get the length of the underlying data
    pub fn len(&self) -> usize {
        self.data.len()
    }

    /// Check if the source is empty
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }
}

#[async_trait]
impl Source for MemorySource {
    async fn size(&self) -> Result<Option<u64>> {
        Ok(Some(self.data.len() as u64))
    }

    fn seekable(&self) -> bool {
        true
    }
}

impl AsyncRead for MemorySource {
    fn poll_read(
        mut self: std::pin::Pin<&mut Self>,
        _cx: &mut std::task::Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<io::Result<()>> {
        let remaining = self.data.len() - self.position;
        let to_read = remaining.min(buf.remaining());

        if to_read > 0 {
            buf.put_slice(&self.data[self.position..self.position + to_read]);
            self.position += to_read;
        }

        std::task::Poll::Ready(Ok(()))
    }
}

impl AsyncSeek for MemorySource {
    fn start_seek(mut self: std::pin::Pin<&mut Self>, position: SeekFrom) -> io::Result<()> {
        let new_pos = match position {
            SeekFrom::Start(offset) => offset as i64,
            SeekFrom::End(offset) => self.data.len() as i64 + offset,
            SeekFrom::Current(offset) => self.position as i64 + offset,
        };

        if new_pos < 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "seek to negative position",
            ));
        }

        self.position = new_pos as usize;
        Ok(())
    }

    fn poll_complete(
        self: std::pin::Pin<&mut Self>,
        _cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<io::Result<u64>> {
        std::task::Poll::Ready(Ok(self.position as u64))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_memory_source_read() {
        let data = vec![1, 2, 3, 4, 5];
        let mut source = MemorySource::new(data);

        let mut buf = [0u8; 3];
        AsyncReadExt::read_exact(&mut source, &mut buf)
            .await
            .unwrap();

        assert_eq!(buf, [1, 2, 3]);
    }

    #[tokio::test]
    async fn test_memory_source_seek() {
        let data = vec![1, 2, 3, 4, 5];
        let mut source = MemorySource::new(data);

        // Seek to position 2
        AsyncSeekExt::seek(&mut source, SeekFrom::Start(2))
            .await
            .unwrap();

        let mut buf = [0u8; 2];
        AsyncReadExt::read_exact(&mut source, &mut buf)
            .await
            .unwrap();

        assert_eq!(buf, [3, 4]);
    }

    #[tokio::test]
    async fn test_memory_source_size() {
        let data = vec![1, 2, 3, 4, 5];
        let source = MemorySource::new(data);

        let size = source.size().await.unwrap();
        assert_eq!(size, Some(5));
    }

    #[test]
    fn test_memory_source_len() {
        let data = vec![1, 2, 3, 4, 5];
        let source = MemorySource::new(data);

        assert_eq!(source.len(), 5);
        assert!(!source.is_empty());
    }

    #[test]
    fn test_memory_source_empty() {
        let source = MemorySource::new(vec![]);
        assert!(source.is_empty());
    }
}
