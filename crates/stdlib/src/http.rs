//! HTTP client and server.
//!
//! Simple HTTP implementation for common use cases.

use std::collections::HashMap;
use std::io::{self, Read, Write, BufRead, BufReader};

/// HTTP method.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Method {
    GET,
    POST,
    PUT,
    DELETE,
    PATCH,
    HEAD,
    OPTIONS,
}

impl Method {
    pub fn as_str(&self) -> &'static str {
        match self {
            Method::GET => "GET",
            Method::POST => "POST",
            Method::PUT => "PUT",
            Method::DELETE => "DELETE",
            Method::PATCH => "PATCH",
            Method::HEAD => "HEAD",
            Method::OPTIONS => "OPTIONS",
        }
    }
}

/// HTTP request.
#[derive(Debug)]
pub struct Request {
    pub method: Method,
    pub path: String,
    pub headers: HashMap<String, String>,
    pub body: Vec<u8>,
}

impl Request {
    /// Create a new GET request.
    pub fn get(path: &str) -> Self {
        Self {
            method: Method::GET,
            path: path.to_string(),
            headers: HashMap::new(),
            body: Vec::new(),
        }
    }
    
    /// Create a new POST request.
    pub fn post(path: &str) -> Self {
        Self {
            method: Method::POST,
            path: path.to_string(),
            headers: HashMap::new(),
            body: Vec::new(),
        }
    }
    
    /// Add a header.
    pub fn header(mut self, key: &str, value: &str) -> Self {
        self.headers.insert(key.to_string(), value.to_string());
        self
    }
    
    /// Set body.
    pub fn body(mut self, body: Vec<u8>) -> Self {
        self.body = body;
        self
    }
    
    /// Set JSON body.
    pub fn json(mut self, json: &str) -> Self {
        self.headers.insert("Content-Type".to_string(), "application/json".to_string());
        self.body = json.as_bytes().to_vec();
        self
    }
    
    /// Set form body.
    pub fn form(mut self, data: &[(&str, &str)]) -> Self {
        self.headers.insert("Content-Type".to_string(), "application/x-www-form-urlencoded".to_string());
        let body: Vec<String> = data.iter()
            .map(|(k, v)| format!("{}={}", url_encode(k), url_encode(v)))
            .collect();
        self.body = body.join("&").into_bytes();
        self
    }
}

/// HTTP response.
#[derive(Debug)]
pub struct Response {
    pub status: u16,
    pub status_text: String,
    pub headers: HashMap<String, String>,
    pub body: Vec<u8>,
}

impl Response {
    /// Create a new response.
    pub fn new(status: u16) -> Self {
        let status_text = match status {
            200 => "OK",
            201 => "Created",
            204 => "No Content",
            301 => "Moved Permanently",
            302 => "Found",
            304 => "Not Modified",
            400 => "Bad Request",
            401 => "Unauthorized",
            403 => "Forbidden",
            404 => "Not Found",
            500 => "Internal Server Error",
            _ => "Unknown",
        };
        
        Self {
            status,
            status_text: status_text.to_string(),
            headers: HashMap::new(),
            body: Vec::new(),
        }
    }
    
    /// Check if status is success (2xx).
    pub fn is_success(&self) -> bool {
        (200..300).contains(&self.status)
    }
    
    /// Check if status is redirect (3xx).
    pub fn is_redirect(&self) -> bool {
        (300..400).contains(&self.status)
    }
    
    /// Check if status is client error (4xx).
    pub fn is_client_error(&self) -> bool {
        (400..500).contains(&self.status)
    }
    
    /// Check if status is server error (5xx).
    pub fn is_server_error(&self) -> bool {
        (500..600).contains(&self.status)
    }
    
    /// Get body as string.
    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.body).to_string()
    }
    
    /// Add a header.
    pub fn header(mut self, key: &str, value: &str) -> Self {
        self.headers.insert(key.to_string(), value.to_string());
        self
    }
    
    /// Set body.
    pub fn body(mut self, body: Vec<u8>) -> Self {
        self.body = body;
        self
    }
    
    /// Set text body.
    pub fn text_body(mut self, text: &str) -> Self {
        self.headers.insert("Content-Type".to_string(), "text/plain".to_string());
        self.body = text.as_bytes().to_vec();
        self
    }
    
    /// Set HTML body.
    pub fn html(mut self, html: &str) -> Self {
        self.headers.insert("Content-Type".to_string(), "text/html".to_string());
        self.body = html.as_bytes().to_vec();
        self
    }
    
    /// Set JSON body.
    pub fn json(mut self, json: &str) -> Self {
        self.headers.insert("Content-Type".to_string(), "application/json".to_string());
        self.body = json.as_bytes().to_vec();
        self
    }
}

/// URL encode a string.
pub fn url_encode(s: &str) -> String {
    let mut result = String::new();
    for c in s.chars() {
        if c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.' || c == '~' {
            result.push(c);
        } else if c == ' ' {
            result.push('+');
        } else {
            for b in c.to_string().as_bytes() {
                result.push_str(&format!("%{:02X}", b));
            }
        }
    }
    result
}

/// URL decode a string.
pub fn url_decode(s: &str) -> String {
    let mut result = Vec::new();
    let mut chars = s.chars().peekable();
    
    while let Some(c) = chars.next() {
        if c == '+' {
            result.push(b' ');
        } else if c == '%' {
            let hex: String = chars.by_ref().take(2).collect();
            if let Ok(byte) = u8::from_str_radix(&hex, 16) {
                result.push(byte);
            }
        } else {
            for b in c.to_string().as_bytes() {
                result.push(*b);
            }
        }
    }
    
    String::from_utf8_lossy(&result).to_string()
}

/// Parse query string.
pub fn parse_query(query: &str) -> HashMap<String, String> {
    let mut result = HashMap::new();
    
    for pair in query.split('&') {
        if let Some((key, value)) = pair.split_once('=') {
            result.insert(url_decode(key), url_decode(value));
        }
    }
    
    result
}

/// Build query string.
pub fn build_query(params: &[(&str, &str)]) -> String {
    params.iter()
        .map(|(k, v)| format!("{}={}", url_encode(k), url_encode(v)))
        .collect::<Vec<_>>()
        .join("&")
}

/// Simple HTTP client.
pub struct Client {
    pub timeout_secs: u64,
    pub headers: HashMap<String, String>,
}

impl Client {
    /// Create a new client.
    pub fn new() -> Self {
        let mut headers = HashMap::new();
        headers.insert("User-Agent".to_string(), "Roast/1.0".to_string());
        
        Self {
            timeout_secs: 30,
            headers,
        }
    }
    
    /// Set default header.
    pub fn default_header(&mut self, key: &str, value: &str) -> &mut Self {
        self.headers.insert(key.to_string(), value.to_string());
        self
    }
    
    /// Set timeout.
    pub fn timeout(&mut self, secs: u64) -> &mut Self {
        self.timeout_secs = secs;
        self
    }
    
    /// Send a request.
    pub fn send(&self, request: Request) -> io::Result<Response> {
        // This is a simplified implementation
        // Real implementation would use net module
        
        Err(io::Error::new(io::ErrorKind::Other, "Not implemented"))
    }
    
    /// GET request.
    pub fn get(&self, url: &str) -> io::Result<Response> {
        self.send(Request::get(url))
    }
    
    /// POST request.
    pub fn post(&self, url: &str, body: &str) -> io::Result<Response> {
        self.send(Request::post(url).body(body.as_bytes().to_vec()))
    }
}

impl Default for Client {
    fn default() -> Self {
        Self::new()
    }
}

/// Cookie.
#[derive(Clone, Debug)]
pub struct Cookie {
    pub name: String,
    pub value: String,
    pub domain: Option<String>,
    pub path: Option<String>,
    pub expires: Option<u64>,
    pub secure: bool,
    pub http_only: bool,
}

impl Cookie {
    /// Create a new cookie.
    pub fn new(name: &str, value: &str) -> Self {
        Self {
            name: name.to_string(),
            value: value.to_string(),
            domain: None,
            path: None,
            expires: None,
            secure: false,
            http_only: false,
        }
    }
    
    /// Set domain.
    pub fn domain(mut self, domain: &str) -> Self {
        self.domain = Some(domain.to_string());
        self
    }
    
    /// Set path.
    pub fn path(mut self, path: &str) -> Self {
        self.path = Some(path.to_string());
        self
    }
    
    /// Set expiry (unix timestamp).
    pub fn expires(mut self, timestamp: u64) -> Self {
        self.expires = Some(timestamp);
        self
    }
    
    /// Set secure flag.
    pub fn secure(mut self, secure: bool) -> Self {
        self.secure = secure;
        self
    }
    
    /// Set http-only flag.
    pub fn http_only(mut self, http_only: bool) -> Self {
        self.http_only = http_only;
        self
    }
    
    /// Format as Set-Cookie header value.
    pub fn to_header(&self) -> String {
        let mut parts = vec![format!("{}={}", self.name, self.value)];
        
        if let Some(ref domain) = self.domain {
            parts.push(format!("Domain={}", domain));
        }
        if let Some(ref path) = self.path {
            parts.push(format!("Path={}", path));
        }
        if self.secure {
            parts.push("Secure".to_string());
        }
        if self.http_only {
            parts.push("HttpOnly".to_string());
        }
        
        parts.join("; ")
    }
}

