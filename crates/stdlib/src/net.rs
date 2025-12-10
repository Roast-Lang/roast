//! Networking primitives.
//!
//! TCP, UDP, and Unix socket support.

use std::io::{self, Read, Write};
use std::net::{TcpListener as StdTcpListener, TcpStream as StdTcpStream};
use std::net::{UdpSocket as StdUdpSocket, SocketAddr};
use std::time::Duration;

/// TCP listener for accepting connections.
pub struct TcpListener {
    inner: StdTcpListener,
}

impl TcpListener {
    /// Bind to an address.
    pub fn bind(addr: &str) -> io::Result<Self> {
        let inner = StdTcpListener::bind(addr)?;
        Ok(Self { inner })
    }
    
    /// Accept a connection.
    pub fn accept(&self) -> io::Result<(TcpStream, String)> {
        let (stream, addr) = self.inner.accept()?;
        Ok((TcpStream { inner: stream }, addr.to_string()))
    }
    
    /// Set non-blocking mode.
    pub fn set_nonblocking(&self, nonblocking: bool) -> io::Result<()> {
        self.inner.set_nonblocking(nonblocking)
    }
    
    /// Get local address.
    pub fn local_addr(&self) -> io::Result<String> {
        self.inner.local_addr().map(|a| a.to_string())
    }
    
    /// Iterate over incoming connections.
    pub fn incoming(&self) -> impl Iterator<Item = io::Result<TcpStream>> + '_ {
        self.inner.incoming().map(|r| r.map(|s| TcpStream { inner: s }))
    }
}

/// TCP stream for reading and writing.
pub struct TcpStream {
    inner: StdTcpStream,
}

impl TcpStream {
    /// Connect to an address.
    pub fn connect(addr: &str) -> io::Result<Self> {
        let inner = StdTcpStream::connect(addr)?;
        Ok(Self { inner })
    }
    
    /// Connect with timeout.
    pub fn connect_timeout(addr: &str, timeout_secs: u64) -> io::Result<Self> {
        let addr: SocketAddr = addr.parse()
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?;
        let inner = StdTcpStream::connect_timeout(&addr, Duration::from_secs(timeout_secs))?;
        Ok(Self { inner })
    }
    
    /// Read data.
    pub fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.inner.read(buf)
    }
    
    /// Read exact amount.
    pub fn read_exact(&mut self, buf: &mut [u8]) -> io::Result<()> {
        self.inner.read_exact(buf)
    }
    
    /// Read all available data.
    pub fn read_all(&mut self) -> io::Result<Vec<u8>> {
        let mut buf = Vec::new();
        self.inner.read_to_end(&mut buf)?;
        Ok(buf)
    }
    
    /// Read as string.
    pub fn read_string(&mut self) -> io::Result<String> {
        let mut buf = String::new();
        self.inner.read_to_string(&mut buf)?;
        Ok(buf)
    }
    
    /// Write data.
    pub fn write(&mut self, data: &[u8]) -> io::Result<usize> {
        self.inner.write(data)
    }
    
    /// Write all data.
    pub fn write_all(&mut self, data: &[u8]) -> io::Result<()> {
        self.inner.write_all(data)
    }
    
    /// Flush writes.
    pub fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
    
    /// Set read timeout.
    pub fn set_read_timeout(&self, timeout_secs: Option<u64>) -> io::Result<()> {
        self.inner.set_read_timeout(timeout_secs.map(Duration::from_secs))
    }
    
    /// Set write timeout.
    pub fn set_write_timeout(&self, timeout_secs: Option<u64>) -> io::Result<()> {
        self.inner.set_write_timeout(timeout_secs.map(Duration::from_secs))
    }
    
    /// Set non-blocking mode.
    pub fn set_nonblocking(&self, nonblocking: bool) -> io::Result<()> {
        self.inner.set_nonblocking(nonblocking)
    }
    
    /// Set TCP nodelay.
    pub fn set_nodelay(&self, nodelay: bool) -> io::Result<()> {
        self.inner.set_nodelay(nodelay)
    }
    
    /// Get peer address.
    pub fn peer_addr(&self) -> io::Result<String> {
        self.inner.peer_addr().map(|a| a.to_string())
    }
    
    /// Get local address.
    pub fn local_addr(&self) -> io::Result<String> {
        self.inner.local_addr().map(|a| a.to_string())
    }
    
    /// Shutdown connection.
    pub fn shutdown(&self, how: Shutdown) -> io::Result<()> {
        self.inner.shutdown(match how {
            Shutdown::Read => std::net::Shutdown::Read,
            Shutdown::Write => std::net::Shutdown::Write,
            Shutdown::Both => std::net::Shutdown::Both,
        })
    }
    
    /// Clone the stream.
    pub fn try_clone(&self) -> io::Result<TcpStream> {
        self.inner.try_clone().map(|s| TcpStream { inner: s })
    }
}

/// Shutdown mode.
#[derive(Clone, Copy, Debug)]
pub enum Shutdown {
    Read,
    Write,
    Both,
}

/// UDP socket.
pub struct UdpSocket {
    inner: StdUdpSocket,
}

impl UdpSocket {
    /// Bind to an address.
    pub fn bind(addr: &str) -> io::Result<Self> {
        let inner = StdUdpSocket::bind(addr)?;
        Ok(Self { inner })
    }
    
    /// Connect to a remote address.
    pub fn connect(&self, addr: &str) -> io::Result<()> {
        self.inner.connect(addr)
    }
    
    /// Send data to connected address.
    pub fn send(&self, data: &[u8]) -> io::Result<usize> {
        self.inner.send(data)
    }
    
    /// Send data to specific address.
    pub fn send_to(&self, data: &[u8], addr: &str) -> io::Result<usize> {
        self.inner.send_to(data, addr)
    }
    
    /// Receive data.
    pub fn recv(&self, buf: &mut [u8]) -> io::Result<usize> {
        self.inner.recv(buf)
    }
    
    /// Receive data with source address.
    pub fn recv_from(&self, buf: &mut [u8]) -> io::Result<(usize, String)> {
        let (size, addr) = self.inner.recv_from(buf)?;
        Ok((size, addr.to_string()))
    }
    
    /// Set broadcast enabled.
    pub fn set_broadcast(&self, on: bool) -> io::Result<()> {
        self.inner.set_broadcast(on)
    }
    
    /// Set read timeout.
    pub fn set_read_timeout(&self, timeout_secs: Option<u64>) -> io::Result<()> {
        self.inner.set_read_timeout(timeout_secs.map(Duration::from_secs))
    }
    
    /// Set non-blocking mode.
    pub fn set_nonblocking(&self, nonblocking: bool) -> io::Result<()> {
        self.inner.set_nonblocking(nonblocking)
    }
    
    /// Get local address.
    pub fn local_addr(&self) -> io::Result<String> {
        self.inner.local_addr().map(|a| a.to_string())
    }
}

/// DNS resolution.
pub mod dns {
    use std::net::ToSocketAddrs;
    
    /// Resolve hostname to IP addresses.
    pub fn resolve(host: &str) -> std::io::Result<Vec<String>> {
        let host_with_port = if host.contains(':') {
            host.to_string()
        } else {
            format!("{}:0", host)
        };
        
        let addrs: Vec<String> = host_with_port
            .to_socket_addrs()?
            .map(|a| a.ip().to_string())
            .collect();
        
        Ok(addrs)
    }
    
    /// Get hostname.
    pub fn hostname() -> Option<String> {
        hostname::get().ok().and_then(|h| h.into_string().ok())
    }
}

/// Simple HTTP helpers.
pub mod http {
    use super::*;
    
    /// Simple HTTP GET request.
    pub fn get(url: &str) -> io::Result<HttpResponse> {
        // Parse URL
        let url = url.trim_start_matches("http://");
        let (host_port, path) = url.split_once('/').unwrap_or((url, ""));
        let (host, port) = host_port.split_once(':').unwrap_or((host_port, "80"));
        
        let addr = format!("{}:{}", host, port);
        let mut stream = TcpStream::connect(&addr)?;
        
        // Send request
        let request = format!(
            "GET /{} HTTP/1.1\r\nHost: {}\r\nConnection: close\r\n\r\n",
            path, host
        );
        stream.write_all(request.as_bytes())?;
        
        // Read response
        let response = stream.read_string()?;
        
        // Parse response
        let mut lines = response.lines();
        let status_line = lines.next().unwrap_or("");
        let status_code = status_line.split_whitespace()
            .nth(1)
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);
        
        // Find body (after empty line)
        let body_start = response.find("\r\n\r\n")
            .map(|i| i + 4)
            .unwrap_or(0);
        let body = response[body_start..].to_string();
        
        Ok(HttpResponse { status_code, body })
    }
    
    /// HTTP response.
    #[derive(Debug)]
    pub struct HttpResponse {
        pub status_code: u16,
        pub body: String,
    }
}

