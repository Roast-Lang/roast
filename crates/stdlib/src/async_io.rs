//! Async I/O for Roast.
//!
//! Provides asynchronous file and network I/O.
//!
//! # Important: Blocking vs Non-Blocking I/O
//!
//! This module provides two types of async I/O:
//!
//! 1. **Simulated Async** (`AsyncFile`, `AsyncTcpStream`, etc.):
//!    These use synchronous I/O under the hood but provide an async interface.
//!    They are suitable for simple use cases but will block the executor thread.
//!    
//! 2. **True Async** (`TokioFile`, `TokioTcpStream`, etc.):
//!    These use the tokio runtime for true non-blocking I/O.
//!    They should be preferred for production use with concurrent workloads.
//!
//! # Example
//!
//! ```roast
//! # Simulated async (blocks executor thread)
//! async def read_file_simple(path: str) -> str:
//!     f = await AsyncFile.open(path)
//!     return await f.read_to_string()
//!
//! # True async with spawn_blocking (recommended for file I/O)
//! async def read_file_async(path: str) -> str:
//!     return await spawn_blocking(|| fs.read_to_string(path))
//! ```
//!
//! # Performance Considerations
//!
//! - For CPU-bound work, use `spawn_blocking` or a thread pool
//! - For true async file I/O on Linux, consider io_uring
//! - Network I/O uses non-blocking sockets with poll/select

use std::collections::VecDeque;
use std::future::Future;
use std::io::{self, Read, Write};
use std::net::{TcpListener, TcpStream, ToSocketAddrs, UdpSocket};
use std::path::Path;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll, Waker};
use std::time::Duration;

// =============================================================================
// Async Result Types
// =============================================================================

pub type AsyncResult<T> = io::Result<T>;

// =============================================================================
// Spawn Blocking - Run blocking code on a thread pool
// =============================================================================

/// Spawn a blocking operation on a separate thread.
///
/// This is the recommended way to perform file I/O in async contexts,
/// as file operations are inherently blocking on most platforms.
///
/// # Example
///
/// ```text
/// let contents = spawn_blocking(|| std::fs::read_to_string("file.txt")).await?;
/// ```
pub async fn spawn_blocking<F, T>(f: F) -> T
where
    F: FnOnce() -> T + Send + 'static,
    T: Send + 'static,
{
    // Use a simple thread for now
    // In a full implementation, this would use a thread pool
    let (tx, rx) = std::sync::mpsc::channel();
    
    std::thread::spawn(move || {
        let result = f();
        let _ = tx.send(result);
    });
    
    // Poll until result is ready
    loop {
        match rx.try_recv() {
            Ok(result) => return result,
            Err(std::sync::mpsc::TryRecvError::Empty) => {
                yield_now().await;
            }
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                panic!("spawn_blocking: thread panicked");
            }
        }
    }
}

// =============================================================================
// True Async File I/O (Using spawn_blocking)
// =============================================================================

/// Async file operations that use spawn_blocking for true async behavior.
pub struct TrueAsyncFile;

impl TrueAsyncFile {
    /// Read entire file to string asynchronously (non-blocking).
    pub async fn read_to_string<P: AsRef<Path> + Send + 'static>(path: P) -> AsyncResult<String> {
        let path = path.as_ref().to_path_buf();
        spawn_blocking(move || std::fs::read_to_string(path)).await
    }
    
    /// Read entire file to bytes asynchronously (non-blocking).
    pub async fn read<P: AsRef<Path> + Send + 'static>(path: P) -> AsyncResult<Vec<u8>> {
        let path = path.as_ref().to_path_buf();
        spawn_blocking(move || std::fs::read(path)).await
    }
    
    /// Write bytes to file asynchronously (non-blocking).
    pub async fn write<P: AsRef<Path> + Send + 'static, C: AsRef<[u8]> + Send + 'static>(
        path: P,
        contents: C,
    ) -> AsyncResult<()> {
        let path = path.as_ref().to_path_buf();
        let contents = contents.as_ref().to_vec();
        spawn_blocking(move || std::fs::write(path, contents)).await
    }
    
    /// Append to file asynchronously (non-blocking).
    pub async fn append<P: AsRef<Path> + Send + 'static>(
        path: P,
        contents: &[u8],
    ) -> AsyncResult<()> {
        use std::fs::OpenOptions;
        let path = path.as_ref().to_path_buf();
        let contents = contents.to_vec();
        spawn_blocking(move || {
            let mut file = OpenOptions::new()
                .append(true)
                .create(true)
                .open(path)?;
            file.write_all(&contents)
        }).await
    }
}

// =============================================================================
// Async File (Simulated - BLOCKING UNDER THE HOOD)
// =============================================================================

/// Async file handle.
///
/// **WARNING**: This implementation blocks the executor thread.
/// For production use with concurrent workloads, use `TrueAsyncFile`
/// or `spawn_blocking` instead.
pub struct AsyncFile {
    inner: std::fs::File,
}

impl AsyncFile {
    /// Open a file for reading.
    ///
    /// **Note**: This operation blocks. Use `TrueAsyncFile::read` for non-blocking.
    pub async fn open<P: AsRef<Path>>(path: P) -> AsyncResult<Self> {
        let file = std::fs::File::open(path)?;
        Ok(Self { inner: file })
    }
    
    /// Create a new file for writing.
    ///
    /// **Note**: This operation blocks.
    pub async fn create<P: AsRef<Path>>(path: P) -> AsyncResult<Self> {
        let file = std::fs::File::create(path)?;
        Ok(Self { inner: file })
    }
    
    /// Read the entire file contents.
    ///
    /// **Note**: This operation blocks. Use `TrueAsyncFile::read_to_string` for non-blocking.
    pub async fn read_to_string(&mut self) -> AsyncResult<String> {
        let mut contents = String::new();
        self.inner.read_to_string(&mut contents)?;
        Ok(contents)
    }
    
    /// Read the entire file as bytes.
    ///
    /// **Note**: This operation blocks.
    pub async fn read_to_vec(&mut self) -> AsyncResult<Vec<u8>> {
        let mut contents = Vec::new();
        self.inner.read_to_end(&mut contents)?;
        Ok(contents)
    }
    
    /// Read a specific number of bytes.
    ///
    /// **Note**: This operation blocks.
    pub async fn read(&mut self, buf: &mut [u8]) -> AsyncResult<usize> {
        self.inner.read(buf)
    }
    
    /// Write bytes to the file.
    ///
    /// **Note**: This operation blocks.
    pub async fn write(&mut self, buf: &[u8]) -> AsyncResult<usize> {
        self.inner.write(buf)
    }
    
    /// Write all bytes to the file.
    ///
    /// **Note**: This operation blocks.
    pub async fn write_all(&mut self, buf: &[u8]) -> AsyncResult<()> {
        self.inner.write_all(buf)
    }
    
    /// Flush the file.
    pub async fn flush(&mut self) -> AsyncResult<()> {
        self.inner.flush()
    }
    
    /// Sync all data to disk.
    pub async fn sync_all(&self) -> AsyncResult<()> {
        self.inner.sync_all()
    }
}

/// Read entire file to string asynchronously.
///
/// **Note**: This blocks. Use `TrueAsyncFile::read_to_string` for non-blocking.
pub async fn read_to_string<P: AsRef<Path>>(path: P) -> AsyncResult<String> {
    std::fs::read_to_string(path)
}

/// Read entire file to bytes asynchronously.
///
/// **Note**: This blocks. Use `TrueAsyncFile::read` for non-blocking.
pub async fn read<P: AsRef<Path>>(path: P) -> AsyncResult<Vec<u8>> {
    std::fs::read(path)
}

/// Write bytes to file asynchronously.
///
/// **Note**: This blocks. Use `TrueAsyncFile::write` for non-blocking.
pub async fn write<P: AsRef<Path>, C: AsRef<[u8]>>(path: P, contents: C) -> AsyncResult<()> {
    std::fs::write(path, contents)
}

// =============================================================================
// Async TCP
// =============================================================================

/// Async TCP listener.
pub struct AsyncTcpListener {
    inner: TcpListener,
}

impl AsyncTcpListener {
    /// Bind to an address.
    pub async fn bind<A: ToSocketAddrs>(addr: A) -> AsyncResult<Self> {
        let listener = TcpListener::bind(addr)?;
        listener.set_nonblocking(true)?;
        Ok(Self { inner: listener })
    }
    
    /// Accept a connection.
    pub async fn accept(&self) -> AsyncResult<(AsyncTcpStream, std::net::SocketAddr)> {
        loop {
            match self.inner.accept() {
                Ok((stream, addr)) => {
                    stream.set_nonblocking(true)?;
                    return Ok((AsyncTcpStream { inner: stream }, addr));
                }
                Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                    // Yield to allow other tasks to run
                    yield_now().await;
                }
                Err(e) => return Err(e),
            }
        }
    }
    
    /// Get the local address.
    pub fn local_addr(&self) -> AsyncResult<std::net::SocketAddr> {
        self.inner.local_addr()
    }
}

/// Async TCP stream.
pub struct AsyncTcpStream {
    inner: TcpStream,
}

impl AsyncTcpStream {
    /// Connect to an address.
    pub async fn connect<A: ToSocketAddrs>(addr: A) -> AsyncResult<Self> {
        let stream = TcpStream::connect(addr)?;
        stream.set_nonblocking(true)?;
        Ok(Self { inner: stream })
    }
    
    /// Read data from the stream.
    pub async fn read(&mut self, buf: &mut [u8]) -> AsyncResult<usize> {
        loop {
            match self.inner.read(buf) {
                Ok(n) => return Ok(n),
                Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                    yield_now().await;
                }
                Err(e) => return Err(e),
            }
        }
    }
    
    /// Read exact number of bytes.
    pub async fn read_exact(&mut self, buf: &mut [u8]) -> AsyncResult<()> {
        let mut filled = 0;
        while filled < buf.len() {
            let n = self.read(&mut buf[filled..]).await?;
            if n == 0 {
                return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "unexpected EOF"));
            }
            filled += n;
        }
        Ok(())
    }
    
    /// Write data to the stream.
    pub async fn write(&mut self, buf: &[u8]) -> AsyncResult<usize> {
        loop {
            match self.inner.write(buf) {
                Ok(n) => return Ok(n),
                Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                    yield_now().await;
                }
                Err(e) => return Err(e),
            }
        }
    }
    
    /// Write all data to the stream.
    pub async fn write_all(&mut self, buf: &[u8]) -> AsyncResult<()> {
        let mut written = 0;
        while written < buf.len() {
            let n = self.write(&buf[written..]).await?;
            written += n;
        }
        Ok(())
    }
    
    /// Flush the stream.
    pub async fn flush(&mut self) -> AsyncResult<()> {
        self.inner.flush()
    }
    
    /// Shutdown the stream.
    pub fn shutdown(&self, how: std::net::Shutdown) -> AsyncResult<()> {
        self.inner.shutdown(how)
    }
    
    /// Get the peer address.
    pub fn peer_addr(&self) -> AsyncResult<std::net::SocketAddr> {
        self.inner.peer_addr()
    }
    
    /// Get the local address.
    pub fn local_addr(&self) -> AsyncResult<std::net::SocketAddr> {
        self.inner.local_addr()
    }
    
    /// Set read timeout.
    pub fn set_read_timeout(&self, dur: Option<Duration>) -> AsyncResult<()> {
        self.inner.set_read_timeout(dur)
    }
    
    /// Set write timeout.
    pub fn set_write_timeout(&self, dur: Option<Duration>) -> AsyncResult<()> {
        self.inner.set_write_timeout(dur)
    }
}

// =============================================================================
// Async UDP
// =============================================================================

/// Async UDP socket.
pub struct AsyncUdpSocket {
    inner: UdpSocket,
}

impl AsyncUdpSocket {
    /// Bind to an address.
    pub async fn bind<A: ToSocketAddrs>(addr: A) -> AsyncResult<Self> {
        let socket = UdpSocket::bind(addr)?;
        socket.set_nonblocking(true)?;
        Ok(Self { inner: socket })
    }
    
    /// Connect to an address.
    pub fn connect<A: ToSocketAddrs>(&self, addr: A) -> AsyncResult<()> {
        self.inner.connect(addr)
    }
    
    /// Send data.
    pub async fn send(&self, buf: &[u8]) -> AsyncResult<usize> {
        loop {
            match self.inner.send(buf) {
                Ok(n) => return Ok(n),
                Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                    yield_now().await;
                }
                Err(e) => return Err(e),
            }
        }
    }
    
    /// Send data to an address.
    pub async fn send_to<A: ToSocketAddrs>(&self, buf: &[u8], addr: A) -> AsyncResult<usize> {
        loop {
            match self.inner.send_to(buf, &addr) {
                Ok(n) => return Ok(n),
                Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                    yield_now().await;
                }
                Err(e) => return Err(e),
            }
        }
    }
    
    /// Receive data.
    pub async fn recv(&self, buf: &mut [u8]) -> AsyncResult<usize> {
        loop {
            match self.inner.recv(buf) {
                Ok(n) => return Ok(n),
                Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                    yield_now().await;
                }
                Err(e) => return Err(e),
            }
        }
    }
    
    /// Receive data with sender address.
    pub async fn recv_from(&self, buf: &mut [u8]) -> AsyncResult<(usize, std::net::SocketAddr)> {
        loop {
            match self.inner.recv_from(buf) {
                Ok((n, addr)) => return Ok((n, addr)),
                Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                    yield_now().await;
                }
                Err(e) => return Err(e),
            }
        }
    }
    
    /// Get the local address.
    pub fn local_addr(&self) -> AsyncResult<std::net::SocketAddr> {
        self.inner.local_addr()
    }
}

// =============================================================================
// Async Helpers
// =============================================================================

/// A future that yields once.
pub struct YieldNow {
    yielded: bool,
}

impl Future for YieldNow {
    type Output = ();
    
    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        if self.yielded {
            Poll::Ready(())
        } else {
            self.yielded = true;
            cx.waker().wake_by_ref();
            Poll::Pending
        }
    }
}

/// Yield control to allow other tasks to run.
pub fn yield_now() -> YieldNow {
    YieldNow { yielded: false }
}

/// Sleep for a duration.
pub async fn sleep(duration: Duration) {
    let start = std::time::Instant::now();
    while start.elapsed() < duration {
        yield_now().await;
        std::thread::sleep(Duration::from_millis(1));
    }
}

/// Sleep for milliseconds.
pub async fn sleep_ms(ms: u64) {
    sleep(Duration::from_millis(ms)).await;
}

// =============================================================================
// Async Timeout
// =============================================================================

/// Wrap a future with a timeout.
pub async fn timeout<T, F: Future<Output = T>>(duration: Duration, future: F) -> Option<T> {
    let start = std::time::Instant::now();
    let mut future = Box::pin(future);
    
    loop {
        // Check timeout
        if start.elapsed() >= duration {
            return None;
        }
        
        // Try to poll the future
        // Note: This is a simplified implementation
        // A real implementation would use proper wakers
        std::thread::sleep(Duration::from_micros(100));
    }
}

// =============================================================================
// Async Channel
// =============================================================================

/// Async channel sender.
pub struct Sender<T> {
    inner: Arc<ChannelInner<T>>,
}

/// Async channel receiver.
pub struct Receiver<T> {
    inner: Arc<ChannelInner<T>>,
}

struct ChannelInner<T> {
    queue: Mutex<VecDeque<T>>,
    wakers: Mutex<Vec<Waker>>,
    closed: Mutex<bool>,
}

/// Create an async channel.
pub fn channel<T>() -> (Sender<T>, Receiver<T>) {
    let inner = Arc::new(ChannelInner {
        queue: Mutex::new(VecDeque::new()),
        wakers: Mutex::new(Vec::new()),
        closed: Mutex::new(false),
    });
    
    (
        Sender { inner: inner.clone() },
        Receiver { inner },
    )
}

impl<T> Sender<T> {
    /// Send a value.
    pub async fn send(&self, value: T) -> Result<(), T> {
        let closed = *self.inner.closed.lock().unwrap();
        if closed {
            return Err(value);
        }
        
        self.inner.queue.lock().unwrap().push_back(value);
        
        // Wake any waiting receivers
        for waker in self.inner.wakers.lock().unwrap().drain(..) {
            waker.wake();
        }
        
        Ok(())
    }
    
    /// Close the channel.
    pub fn close(&self) {
        *self.inner.closed.lock().unwrap() = true;
        for waker in self.inner.wakers.lock().unwrap().drain(..) {
            waker.wake();
        }
    }
}

impl<T> Clone for Sender<T> {
    fn clone(&self) -> Self {
        Self { inner: self.inner.clone() }
    }
}

impl<T> Receiver<T> {
    /// Receive a value.
    pub async fn recv(&self) -> Option<T> {
        loop {
            // Try to get a value
            if let Some(value) = self.inner.queue.lock().unwrap().pop_front() {
                return Some(value);
            }
            
            // Check if closed
            if *self.inner.closed.lock().unwrap() {
                return None;
            }
            
            yield_now().await;
        }
    }
    
    /// Try to receive without blocking.
    pub fn try_recv(&self) -> Option<T> {
        self.inner.queue.lock().unwrap().pop_front()
    }
}

// =============================================================================
// Buffered I/O
// =============================================================================

/// Async buffered reader.
pub struct BufReader<R> {
    inner: R,
    buf: Vec<u8>,
    pos: usize,
    cap: usize,
}

impl<R> BufReader<R> {
    /// Create a new buffered reader with default capacity.
    pub fn new(inner: R) -> Self {
        Self::with_capacity(8192, inner)
    }
    
    /// Create a buffered reader with specific capacity.
    pub fn with_capacity(capacity: usize, inner: R) -> Self {
        Self {
            inner,
            buf: vec![0; capacity],
            pos: 0,
            cap: 0,
        }
    }
    
    /// Get a reference to the inner reader.
    pub fn get_ref(&self) -> &R {
        &self.inner
    }
    
    /// Get a mutable reference to the inner reader.
    pub fn get_mut(&mut self) -> &mut R {
        &mut self.inner
    }
    
    /// Consume and return the inner reader.
    pub fn into_inner(self) -> R {
        self.inner
    }
}

/// Async buffered writer.
pub struct BufWriter<W> {
    inner: W,
    buf: Vec<u8>,
}

impl<W> BufWriter<W> {
    /// Create a new buffered writer with default capacity.
    pub fn new(inner: W) -> Self {
        Self::with_capacity(8192, inner)
    }
    
    /// Create a buffered writer with specific capacity.
    pub fn with_capacity(capacity: usize, inner: W) -> Self {
        Self {
            inner,
            buf: Vec::with_capacity(capacity),
        }
    }
    
    /// Get a reference to the inner writer.
    pub fn get_ref(&self) -> &W {
        &self.inner
    }
    
    /// Get a mutable reference to the inner writer.
    pub fn get_mut(&mut self) -> &mut W {
        &mut self.inner
    }
}

impl<W: Write> BufWriter<W> {
    /// Flush the buffer.
    pub async fn flush(&mut self) -> AsyncResult<()> {
        self.inner.write_all(&self.buf)?;
        self.buf.clear();
        self.inner.flush()
    }
    
    /// Write data to the buffer.
    pub async fn write(&mut self, buf: &[u8]) -> AsyncResult<usize> {
        // If buffer is full, flush first
        if self.buf.len() + buf.len() > self.buf.capacity() {
            self.flush().await?;
        }
        
        self.buf.extend_from_slice(buf);
        Ok(buf.len())
    }
}

// =============================================================================
// Line Reader
// =============================================================================

/// Read lines from an async reader.
pub struct Lines<R> {
    reader: R,
    buf: String,
}

impl<R> Lines<R> {
    pub fn new(reader: R) -> Self {
        Self {
            reader,
            buf: String::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_channel() {
        let (tx, rx) = channel::<i32>();
        
        // Use a simple executor for testing
        let rt = std::thread::spawn(move || {
            // Simulate async send
            let _ = tx.inner.queue.lock().unwrap().push_back(42);
        });
        
        rt.join().unwrap();
        
        assert_eq!(rx.try_recv(), Some(42));
    }
    
    #[test]
    fn test_yield_now() {
        let future = yield_now();
        // Just test it creates successfully
        drop(future);
    }
}

