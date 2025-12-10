//! Simple web framework for Roast.
//!
//! Provides Flask-like routing and HTTP handling.

use std::collections::HashMap;
use std::fmt;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::Arc;

// =============================================================================
// HTTP Types
// =============================================================================

/// HTTP method.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Method {
    Get,
    Post,
    Put,
    Delete,
    Patch,
    Head,
    Options,
}

impl Method {
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_uppercase().as_str() {
            "GET" => Some(Method::Get),
            "POST" => Some(Method::Post),
            "PUT" => Some(Method::Put),
            "DELETE" => Some(Method::Delete),
            "PATCH" => Some(Method::Patch),
            "HEAD" => Some(Method::Head),
            "OPTIONS" => Some(Method::Options),
            _ => None,
        }
    }
}

impl fmt::Display for Method {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Method::Get => write!(f, "GET"),
            Method::Post => write!(f, "POST"),
            Method::Put => write!(f, "PUT"),
            Method::Delete => write!(f, "DELETE"),
            Method::Patch => write!(f, "PATCH"),
            Method::Head => write!(f, "HEAD"),
            Method::Options => write!(f, "OPTIONS"),
        }
    }
}

/// HTTP request.
#[derive(Clone, Debug)]
pub struct Request {
    pub method: Method,
    pub path: String,
    pub query_string: String,
    pub headers: HashMap<String, String>,
    pub body: Vec<u8>,
    pub params: HashMap<String, String>,
}

impl Request {
    /// Get a header value.
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers.get(&name.to_lowercase()).map(|s| s.as_str())
    }
    
    /// Get a query parameter.
    pub fn query(&self, name: &str) -> Option<&str> {
        self.params.get(name).map(|s| s.as_str())
    }
    
    /// Get URL path parameter.
    pub fn param(&self, name: &str) -> Option<&str> {
        self.params.get(name).map(|s| s.as_str())
    }
    
    /// Get body as string.
    pub fn body_str(&self) -> String {
        String::from_utf8_lossy(&self.body).to_string()
    }
    
    /// Get JSON body as string (parse manually).
    pub fn json_str(&self) -> Option<String> {
        String::from_utf8(self.body.clone()).ok()
    }
    
    /// Parse query string into params.
    fn parse_query_string(qs: &str) -> HashMap<String, String> {
        let mut params = HashMap::new();
        for pair in qs.split('&') {
            if let Some((key, value)) = pair.split_once('=') {
                let key = urlencoding::decode(key).unwrap_or_default().to_string();
                let value = urlencoding::decode(value).unwrap_or_default().to_string();
                params.insert(key, value);
            }
        }
        params
    }
}

/// HTTP response.
#[derive(Clone, Debug)]
pub struct Response {
    pub status: u16,
    pub status_text: String,
    pub headers: HashMap<String, String>,
    pub body: Vec<u8>,
}

impl Response {
    /// Create a new response.
    pub fn new(status: u16) -> Self {
        Self {
            status,
            status_text: status_text(status).to_string(),
            headers: HashMap::new(),
            body: Vec::new(),
        }
    }
    
    /// Create 200 OK response.
    pub fn ok() -> Self {
        Self::new(200)
    }
    
    /// Create 404 Not Found response.
    pub fn not_found() -> Self {
        Self::new(404).body_str("Not Found")
    }
    
    /// Create 500 Internal Server Error response.
    pub fn internal_error() -> Self {
        Self::new(500).body_str("Internal Server Error")
    }
    
    /// Create redirect response.
    pub fn redirect(location: &str) -> Self {
        Self::new(302).header("Location", location)
    }
    
    /// Set header.
    pub fn header(mut self, name: &str, value: &str) -> Self {
        self.headers.insert(name.to_string(), value.to_string());
        self
    }
    
    /// Set content type.
    pub fn content_type(self, ct: &str) -> Self {
        self.header("Content-Type", ct)
    }
    
    /// Set body from string.
    pub fn body_str(mut self, s: &str) -> Self {
        self.body = s.as_bytes().to_vec();
        self
    }
    
    /// Set body from bytes.
    pub fn body_bytes(mut self, b: Vec<u8>) -> Self {
        self.body = b;
        self
    }
    
    /// Set JSON body from string.
    pub fn json_str(self, json: &str) -> Self {
        self.content_type("application/json")
            .body_str(json)
    }
    
    /// Set HTML body.
    pub fn html(self, s: &str) -> Self {
        self.content_type("text/html; charset=utf-8").body_str(s)
    }
    
    /// Set plain text body.
    pub fn text(self, s: &str) -> Self {
        self.content_type("text/plain; charset=utf-8").body_str(s)
    }
    
    /// Convert to HTTP response bytes.
    fn to_bytes(&self) -> Vec<u8> {
        let mut response = format!(
            "HTTP/1.1 {} {}\r\n",
            self.status, self.status_text
        );
        
        // Add Content-Length if not present
        if !self.headers.contains_key("Content-Length") {
            response.push_str(&format!("Content-Length: {}\r\n", self.body.len()));
        }
        
        for (name, value) in &self.headers {
            response.push_str(&format!("{}: {}\r\n", name, value));
        }
        
        response.push_str("\r\n");
        
        let mut bytes = response.into_bytes();
        bytes.extend_from_slice(&self.body);
        bytes
    }
}

fn status_text(status: u16) -> &'static str {
    match status {
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
        405 => "Method Not Allowed",
        500 => "Internal Server Error",
        502 => "Bad Gateway",
        503 => "Service Unavailable",
        _ => "Unknown",
    }
}

// =============================================================================
// URL Encoding/Decoding
// =============================================================================

mod urlencoding {
    pub fn decode(s: &str) -> Result<String, ()> {
        let mut result = String::new();
        let mut chars = s.chars().peekable();
        
        while let Some(c) = chars.next() {
            if c == '%' {
                let hex: String = chars.by_ref().take(2).collect();
                if hex.len() == 2 {
                    if let Ok(byte) = u8::from_str_radix(&hex, 16) {
                        result.push(byte as char);
                        continue;
                    }
                }
                result.push('%');
                result.push_str(&hex);
            } else if c == '+' {
                result.push(' ');
            } else {
                result.push(c);
            }
        }
        
        Ok(result)
    }
    
    pub fn encode(s: &str) -> String {
        let mut result = String::new();
        
        for c in s.chars() {
            match c {
                'a'..='z' | 'A'..='Z' | '0'..='9' | '-' | '_' | '.' | '~' => {
                    result.push(c);
                }
                ' ' => result.push('+'),
                _ => {
                    for byte in c.to_string().bytes() {
                        result.push_str(&format!("%{:02X}", byte));
                    }
                }
            }
        }
        
        result
    }
}

// =============================================================================
// Router
// =============================================================================

/// Route handler type.
pub type Handler = Box<dyn Fn(Request) -> Response + Send + Sync>;

/// A route.
struct Route {
    method: Method,
    pattern: String,
    handler: Handler,
}

impl Route {
    fn matches(&self, method: Method, path: &str) -> Option<HashMap<String, String>> {
        if self.method != method {
            return None;
        }
        
        let mut params = HashMap::new();
        let pattern_parts: Vec<&str> = self.pattern.split('/').collect();
        let path_parts: Vec<&str> = path.split('/').collect();
        
        if pattern_parts.len() != path_parts.len() {
            return None;
        }
        
        for (pattern, path) in pattern_parts.iter().zip(path_parts.iter()) {
            if pattern.starts_with('<') && pattern.ends_with('>') {
                // URL parameter
                let name = &pattern[1..pattern.len() - 1];
                params.insert(name.to_string(), path.to_string());
            } else if pattern != path {
                return None;
            }
        }
        
        Some(params)
    }
}

/// Router for matching routes.
pub struct Router {
    routes: Vec<Route>,
}

impl Router {
    /// Create a new router.
    pub fn new() -> Self {
        Self { routes: Vec::new() }
    }
    
    /// Add a route.
    pub fn route(mut self, method: Method, pattern: &str, handler: Handler) -> Self {
        self.routes.push(Route {
            method,
            pattern: pattern.to_string(),
            handler,
        });
        self
    }
    
    /// Add a GET route.
    pub fn get(self, pattern: &str, handler: Handler) -> Self {
        self.route(Method::Get, pattern, handler)
    }
    
    /// Add a POST route.
    pub fn post(self, pattern: &str, handler: Handler) -> Self {
        self.route(Method::Post, pattern, handler)
    }
    
    /// Add a PUT route.
    pub fn put(self, pattern: &str, handler: Handler) -> Self {
        self.route(Method::Put, pattern, handler)
    }
    
    /// Add a DELETE route.
    pub fn delete(self, pattern: &str, handler: Handler) -> Self {
        self.route(Method::Delete, pattern, handler)
    }
    
    /// Match a request to a route.
    fn match_route(&self, method: Method, path: &str) -> Option<(&Route, HashMap<String, String>)> {
        for route in &self.routes {
            if let Some(params) = route.matches(method, path) {
                return Some((route, params));
            }
        }
        None
    }
}

impl Default for Router {
    fn default() -> Self {
        Self::new()
    }
}

// =============================================================================
// App
// =============================================================================

/// Web application.
pub struct App {
    router: Router,
    middleware: Vec<Box<dyn Fn(Request, Box<dyn Fn(Request) -> Response>) -> Response + Send + Sync>>,
    not_found_handler: Option<Handler>,
    error_handler: Option<Box<dyn Fn(String) -> Response + Send + Sync>>,
}

impl App {
    /// Create a new application.
    pub fn new() -> Self {
        Self {
            router: Router::new(),
            middleware: Vec::new(),
            not_found_handler: None,
            error_handler: None,
        }
    }
    
    /// Add a route.
    pub fn route(mut self, method: Method, pattern: &str, handler: Handler) -> Self {
        self.router = self.router.route(method, pattern, handler);
        self
    }
    
    /// Add a GET route.
    pub fn get<F>(self, pattern: &str, handler: F) -> Self
    where
        F: Fn(Request) -> Response + Send + Sync + 'static,
    {
        self.route(Method::Get, pattern, Box::new(handler))
    }
    
    /// Add a POST route.
    pub fn post<F>(self, pattern: &str, handler: F) -> Self
    where
        F: Fn(Request) -> Response + Send + Sync + 'static,
    {
        self.route(Method::Post, pattern, Box::new(handler))
    }
    
    /// Add a PUT route.
    pub fn put<F>(self, pattern: &str, handler: F) -> Self
    where
        F: Fn(Request) -> Response + Send + Sync + 'static,
    {
        self.route(Method::Put, pattern, Box::new(handler))
    }
    
    /// Add a DELETE route.
    pub fn delete<F>(self, pattern: &str, handler: F) -> Self
    where
        F: Fn(Request) -> Response + Send + Sync + 'static,
    {
        self.route(Method::Delete, pattern, Box::new(handler))
    }
    
    /// Set 404 handler.
    pub fn not_found<F>(mut self, handler: F) -> Self
    where
        F: Fn(Request) -> Response + Send + Sync + 'static,
    {
        self.not_found_handler = Some(Box::new(handler));
        self
    }
    
    /// Set error handler.
    pub fn on_error<F>(mut self, handler: F) -> Self
    where
        F: Fn(String) -> Response + Send + Sync + 'static,
    {
        self.error_handler = Some(Box::new(handler));
        self
    }
    
    /// Handle a request.
    fn handle(&self, mut request: Request) -> Response {
        if let Some((route, params)) = self.router.match_route(request.method, &request.path) {
            request.params.extend(params);
            (route.handler)(request)
        } else if let Some(ref handler) = self.not_found_handler {
            handler(request)
        } else {
            Response::not_found()
        }
    }
    
    /// Run the application.
    pub fn run(&self, addr: &str) -> std::io::Result<()> {
        let listener = TcpListener::bind(addr)?;
        println!("Server running on http://{}", addr);
        
        for stream in listener.incoming() {
            match stream {
                Ok(stream) => {
                    if let Err(e) = self.handle_connection(stream) {
                        eprintln!("Error handling connection: {}", e);
                    }
                }
                Err(e) => eprintln!("Connection error: {}", e),
            }
        }
        
        Ok(())
    }
    
    fn handle_connection(&self, mut stream: TcpStream) -> std::io::Result<()> {
        let request = parse_request(&stream)?;
        let response = self.handle(request);
        stream.write_all(&response.to_bytes())?;
        stream.flush()?;
        Ok(())
    }
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}

/// Parse an HTTP request from a stream.
fn parse_request(stream: &TcpStream) -> std::io::Result<Request> {
    let mut reader = BufReader::new(stream);
    
    // Read request line
    let mut request_line = String::new();
    reader.read_line(&mut request_line)?;
    
    let parts: Vec<&str> = request_line.trim().split_whitespace().collect();
    if parts.len() < 2 {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "Invalid request line",
        ));
    }
    
    let method = Method::from_str(parts[0]).unwrap_or(Method::Get);
    let full_path = parts[1];
    
    let (path, query_string) = if let Some(idx) = full_path.find('?') {
        (full_path[..idx].to_string(), full_path[idx + 1..].to_string())
    } else {
        (full_path.to_string(), String::new())
    };
    
    // Read headers
    let mut headers = HashMap::new();
    loop {
        let mut line = String::new();
        reader.read_line(&mut line)?;
        let line = line.trim();
        
        if line.is_empty() {
            break;
        }
        
        if let Some(idx) = line.find(':') {
            let name = line[..idx].trim().to_lowercase();
            let value = line[idx + 1..].trim().to_string();
            headers.insert(name, value);
        }
    }
    
    // Read body
    let mut body = Vec::new();
    if let Some(len) = headers.get("content-length") {
        if let Ok(len) = len.parse::<usize>() {
            body.resize(len, 0);
            reader.read_exact(&mut body)?;
        }
    }
    
    // Parse query parameters
    let params = Request::parse_query_string(&query_string);
    
    Ok(Request {
        method,
        path,
        query_string,
        headers,
        body,
        params,
    })
}

// =============================================================================
// Template Engine (Simple)
// =============================================================================

/// Simple template engine.
pub struct Template {
    content: String,
}

impl Template {
    /// Create a template from a string.
    pub fn new(content: &str) -> Self {
        Self {
            content: content.to_string(),
        }
    }
    
    /// Render the template with variables.
    pub fn render(&self, vars: &HashMap<String, String>) -> String {
        let mut result = self.content.clone();
        
        for (key, value) in vars {
            result = result.replace(&format!("{{{{{}}}}}", key), value);
        }
        
        result
    }
    
    /// Render with HTML escaping.
    pub fn render_safe(&self, vars: &HashMap<String, String>) -> String {
        let mut result = self.content.clone();
        
        for (key, value) in vars {
            let escaped = html_escape(value);
            result = result.replace(&format!("{{{{{}}}}}", key), &escaped);
        }
        
        result
    }
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

// =============================================================================
// JSON Helpers
// =============================================================================

/// Simple JSON value.
#[derive(Clone, Debug)]
pub enum JsonValue {
    Null,
    Bool(bool),
    Number(f64),
    String(String),
    Array(Vec<JsonValue>),
    Object(HashMap<String, JsonValue>),
}

impl JsonValue {
    pub fn as_str(&self) -> Option<&str> {
        match self {
            JsonValue::String(s) => Some(s),
            _ => None,
        }
    }
    
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            JsonValue::Number(n) => Some(*n),
            _ => None,
        }
    }
    
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            JsonValue::Bool(b) => Some(*b),
            _ => None,
        }
    }
    
    pub fn as_array(&self) -> Option<&Vec<JsonValue>> {
        match self {
            JsonValue::Array(a) => Some(a),
            _ => None,
        }
    }
    
    pub fn as_object(&self) -> Option<&HashMap<String, JsonValue>> {
        match self {
            JsonValue::Object(o) => Some(o),
            _ => None,
        }
    }
    
    pub fn get(&self, key: &str) -> Option<&JsonValue> {
        match self {
            JsonValue::Object(o) => o.get(key),
            _ => None,
        }
    }
}

// =============================================================================
// Static File Serving
// =============================================================================

/// Create a handler for serving static files.
pub fn static_files(directory: &str) -> Handler {
    let dir = directory.to_string();
    
    Box::new(move |req: Request| {
        let path = req.path.trim_start_matches('/');
        let full_path = format!("{}/{}", dir, path);
        
        // Security: prevent directory traversal
        if path.contains("..") {
            return Response::new(403).text("Forbidden");
        }
        
        match std::fs::read(&full_path) {
            Ok(content) => {
                let ct = content_type_for(&full_path);
                Response::ok()
                    .content_type(ct)
                    .body_bytes(content)
            }
            Err(_) => Response::not_found(),
        }
    })
}

fn content_type_for(path: &str) -> &'static str {
    if path.ends_with(".html") || path.ends_with(".htm") {
        "text/html; charset=utf-8"
    } else if path.ends_with(".css") {
        "text/css"
    } else if path.ends_with(".js") {
        "application/javascript"
    } else if path.ends_with(".json") {
        "application/json"
    } else if path.ends_with(".png") {
        "image/png"
    } else if path.ends_with(".jpg") || path.ends_with(".jpeg") {
        "image/jpeg"
    } else if path.ends_with(".gif") {
        "image/gif"
    } else if path.ends_with(".svg") {
        "image/svg+xml"
    } else if path.ends_with(".ico") {
        "image/x-icon"
    } else if path.ends_with(".txt") {
        "text/plain"
    } else if path.ends_with(".xml") {
        "application/xml"
    } else if path.ends_with(".pdf") {
        "application/pdf"
    } else {
        "application/octet-stream"
    }
}

// =============================================================================
// Cookie Support
// =============================================================================

/// Parse cookies from a request.
pub fn parse_cookies(request: &Request) -> HashMap<String, String> {
    let mut cookies = HashMap::new();
    
    if let Some(cookie_header) = request.header("cookie") {
        for part in cookie_header.split(';') {
            let part = part.trim();
            if let Some((name, value)) = part.split_once('=') {
                cookies.insert(name.trim().to_string(), value.trim().to_string());
            }
        }
    }
    
    cookies
}

/// Create a Set-Cookie header value.
pub fn set_cookie(
    name: &str,
    value: &str,
    max_age: Option<u64>,
    path: Option<&str>,
    http_only: bool,
    secure: bool,
) -> String {
    let mut cookie = format!("{}={}", name, value);
    
    if let Some(age) = max_age {
        cookie.push_str(&format!("; Max-Age={}", age));
    }
    
    if let Some(p) = path {
        cookie.push_str(&format!("; Path={}", p));
    }
    
    if http_only {
        cookie.push_str("; HttpOnly");
    }
    
    if secure {
        cookie.push_str("; Secure");
    }
    
    cookie
}

// =============================================================================
// Session Management
// =============================================================================

use std::sync::RwLock;
use std::time::{Duration, Instant};

/// Session data store.
#[derive(Clone, Debug)]
pub struct Session {
    pub id: String,
    pub data: HashMap<String, String>,
    pub created_at: u64,
    pub expires_at: u64,
}

impl Session {
    /// Create a new session.
    pub fn new() -> Self {
        let id = generate_session_id();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        
        Self {
            id,
            data: HashMap::new(),
            created_at: now,
            expires_at: now + 3600, // 1 hour default
        }
    }
    
    /// Create with custom expiry.
    pub fn with_expiry(expiry_secs: u64) -> Self {
        let mut session = Self::new();
        session.expires_at = session.created_at + expiry_secs;
        session
    }
    
    /// Get a value.
    pub fn get(&self, key: &str) -> Option<&str> {
        self.data.get(key).map(|s| s.as_str())
    }
    
    /// Set a value.
    pub fn set(&mut self, key: &str, value: &str) {
        self.data.insert(key.to_string(), value.to_string());
    }
    
    /// Remove a value.
    pub fn remove(&mut self, key: &str) -> Option<String> {
        self.data.remove(key)
    }
    
    /// Check if expired.
    pub fn is_expired(&self) -> bool {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        now >= self.expires_at
    }
    
    /// Refresh expiry.
    pub fn refresh(&mut self, expiry_secs: u64) {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        self.expires_at = now + expiry_secs;
    }
}

impl Default for Session {
    fn default() -> Self {
        Self::new()
    }
}

fn generate_session_id() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    
    // Simple hex-based session ID
    format!("{:032x}", timestamp ^ (std::process::id() as u128) << 64)
}

/// In-memory session store.
pub struct SessionStore {
    sessions: RwLock<HashMap<String, Session>>,
    expiry: u64,
}

impl SessionStore {
    /// Create a new session store.
    pub fn new() -> Self {
        Self {
            sessions: RwLock::new(HashMap::new()),
            expiry: 3600,
        }
    }
    
    /// Create with custom default expiry.
    pub fn with_expiry(expiry_secs: u64) -> Self {
        Self {
            sessions: RwLock::new(HashMap::new()),
            expiry: expiry_secs,
        }
    }
    
    /// Get or create a session.
    pub fn get_or_create(&self, session_id: Option<&str>) -> Session {
        if let Some(id) = session_id {
            if let Some(session) = self.get(id) {
                if !session.is_expired() {
                    return session;
                }
            }
        }
        
        // Create new session
        let session = Session::with_expiry(self.expiry);
        let mut sessions = self.sessions.write().unwrap();
        sessions.insert(session.id.clone(), session.clone());
        session
    }
    
    /// Get a session by ID.
    pub fn get(&self, id: &str) -> Option<Session> {
        let sessions = self.sessions.read().unwrap();
        sessions.get(id).cloned()
    }
    
    /// Save a session.
    pub fn save(&self, session: Session) {
        let mut sessions = self.sessions.write().unwrap();
        sessions.insert(session.id.clone(), session);
    }
    
    /// Delete a session.
    pub fn delete(&self, id: &str) {
        let mut sessions = self.sessions.write().unwrap();
        sessions.remove(id);
    }
    
    /// Clean up expired sessions.
    pub fn cleanup(&self) -> usize {
        let mut sessions = self.sessions.write().unwrap();
        let before = sessions.len();
        sessions.retain(|_, s| !s.is_expired());
        before - sessions.len()
    }
}

impl Default for SessionStore {
    fn default() -> Self {
        Self::new()
    }
}

// =============================================================================
// WebSocket Support
// =============================================================================

/// WebSocket opcode.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WsOpcode {
    Continuation = 0x0,
    Text = 0x1,
    Binary = 0x2,
    Close = 0x8,
    Ping = 0x9,
    Pong = 0xA,
}

impl WsOpcode {
    pub fn from_u8(v: u8) -> Option<Self> {
        match v {
            0x0 => Some(WsOpcode::Continuation),
            0x1 => Some(WsOpcode::Text),
            0x2 => Some(WsOpcode::Binary),
            0x8 => Some(WsOpcode::Close),
            0x9 => Some(WsOpcode::Ping),
            0xA => Some(WsOpcode::Pong),
            _ => None,
        }
    }
}

/// WebSocket frame.
#[derive(Clone, Debug)]
pub struct WsFrame {
    pub fin: bool,
    pub opcode: WsOpcode,
    pub mask: Option<[u8; 4]>,
    pub payload: Vec<u8>,
}

impl WsFrame {
    /// Create a text frame.
    pub fn text(data: &str) -> Self {
        Self {
            fin: true,
            opcode: WsOpcode::Text,
            mask: None,
            payload: data.as_bytes().to_vec(),
        }
    }
    
    /// Create a binary frame.
    pub fn binary(data: Vec<u8>) -> Self {
        Self {
            fin: true,
            opcode: WsOpcode::Binary,
            mask: None,
            payload: data,
        }
    }
    
    /// Create a close frame.
    pub fn close(code: u16, reason: &str) -> Self {
        let mut payload = Vec::new();
        payload.extend_from_slice(&code.to_be_bytes());
        payload.extend_from_slice(reason.as_bytes());
        
        Self {
            fin: true,
            opcode: WsOpcode::Close,
            mask: None,
            payload,
        }
    }
    
    /// Create a ping frame.
    pub fn ping(data: Vec<u8>) -> Self {
        Self {
            fin: true,
            opcode: WsOpcode::Ping,
            mask: None,
            payload: data,
        }
    }
    
    /// Create a pong frame.
    pub fn pong(data: Vec<u8>) -> Self {
        Self {
            fin: true,
            opcode: WsOpcode::Pong,
            mask: None,
            payload: data,
        }
    }
    
    /// Encode frame to bytes.
    pub fn encode(&self) -> Vec<u8> {
        let mut result = Vec::new();
        
        // First byte: FIN + opcode
        let first = (if self.fin { 0x80 } else { 0x00 }) | (self.opcode as u8);
        result.push(first);
        
        // Second byte: MASK + payload length
        let mask_bit = if self.mask.is_some() { 0x80 } else { 0x00 };
        let len = self.payload.len();
        
        if len < 126 {
            result.push(mask_bit | (len as u8));
        } else if len < 65536 {
            result.push(mask_bit | 126);
            result.extend_from_slice(&(len as u16).to_be_bytes());
        } else {
            result.push(mask_bit | 127);
            result.extend_from_slice(&(len as u64).to_be_bytes());
        }
        
        // Mask key (if present)
        if let Some(mask) = self.mask {
            result.extend_from_slice(&mask);
        }
        
        // Payload (masked if mask is present)
        if let Some(mask) = self.mask {
            for (i, byte) in self.payload.iter().enumerate() {
                result.push(byte ^ mask[i % 4]);
            }
        } else {
            result.extend_from_slice(&self.payload);
        }
        
        result
    }
    
    /// Decode frame from bytes.
    pub fn decode(data: &[u8]) -> Result<(Self, usize), &'static str> {
        if data.len() < 2 {
            return Err("Frame too short");
        }
        
        let fin = (data[0] & 0x80) != 0;
        let opcode = WsOpcode::from_u8(data[0] & 0x0F).ok_or("Invalid opcode")?;
        let masked = (data[1] & 0x80) != 0;
        let mut payload_len = (data[1] & 0x7F) as usize;
        
        let mut offset = 2;
        
        if payload_len == 126 {
            if data.len() < 4 {
                return Err("Frame too short for extended length");
            }
            payload_len = u16::from_be_bytes([data[2], data[3]]) as usize;
            offset = 4;
        } else if payload_len == 127 {
            if data.len() < 10 {
                return Err("Frame too short for extended length");
            }
            payload_len = u64::from_be_bytes([
                data[2], data[3], data[4], data[5],
                data[6], data[7], data[8], data[9],
            ]) as usize;
            offset = 10;
        }
        
        let mask = if masked {
            if data.len() < offset + 4 {
                return Err("Frame too short for mask");
            }
            let m = [data[offset], data[offset + 1], data[offset + 2], data[offset + 3]];
            offset += 4;
            Some(m)
        } else {
            None
        };
        
        if data.len() < offset + payload_len {
            return Err("Frame too short for payload");
        }
        
        let mut payload = data[offset..offset + payload_len].to_vec();
        
        // Unmask payload
        if let Some(m) = mask {
            for (i, byte) in payload.iter_mut().enumerate() {
                *byte ^= m[i % 4];
            }
        }
        
        Ok((
            Self {
                fin,
                opcode,
                mask,
                payload,
            },
            offset + payload_len,
        ))
    }
    
    /// Get payload as text.
    pub fn as_text(&self) -> Option<&str> {
        std::str::from_utf8(&self.payload).ok()
    }
}

/// WebSocket connection.
pub struct WebSocket {
    stream: TcpStream,
}

impl WebSocket {
    /// Accept a WebSocket upgrade from an HTTP request.
    pub fn accept(stream: TcpStream, request: &Request) -> Result<Self, &'static str> {
        // Check upgrade headers
        let upgrade = request.header("upgrade").ok_or("Missing Upgrade header")?;
        if upgrade.to_lowercase() != "websocket" {
            return Err("Not a WebSocket upgrade");
        }
        
        let key = request.header("sec-websocket-key").ok_or("Missing Sec-WebSocket-Key")?;
        
        // Compute accept key
        let accept_key = compute_accept_key(key);
        
        // Send upgrade response
        let response = format!(
            "HTTP/1.1 101 Switching Protocols\r\n\
             Upgrade: websocket\r\n\
             Connection: Upgrade\r\n\
             Sec-WebSocket-Accept: {}\r\n\
             \r\n",
            accept_key
        );
        
        let mut s = stream;
        s.write_all(response.as_bytes()).map_err(|_| "Failed to send upgrade")?;
        s.flush().map_err(|_| "Failed to flush")?;
        
        Ok(Self { stream: s })
    }
    
    /// Send a text message.
    pub fn send_text(&mut self, message: &str) -> std::io::Result<()> {
        let frame = WsFrame::text(message);
        self.stream.write_all(&frame.encode())?;
        self.stream.flush()
    }
    
    /// Send a binary message.
    pub fn send_binary(&mut self, data: &[u8]) -> std::io::Result<()> {
        let frame = WsFrame::binary(data.to_vec());
        self.stream.write_all(&frame.encode())?;
        self.stream.flush()
    }
    
    /// Send a ping.
    pub fn ping(&mut self, data: &[u8]) -> std::io::Result<()> {
        let frame = WsFrame::ping(data.to_vec());
        self.stream.write_all(&frame.encode())?;
        self.stream.flush()
    }
    
    /// Close the connection.
    pub fn close(&mut self, code: u16, reason: &str) -> std::io::Result<()> {
        let frame = WsFrame::close(code, reason);
        self.stream.write_all(&frame.encode())?;
        self.stream.flush()
    }
    
    /// Receive a frame.
    pub fn recv(&mut self) -> std::io::Result<WsFrame> {
        let mut buffer = vec![0u8; 4096];
        let n = self.stream.read(&mut buffer)?;
        
        if n == 0 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::UnexpectedEof,
                "Connection closed",
            ));
        }
        
        match WsFrame::decode(&buffer[..n]) {
            Ok((frame, _)) => Ok(frame),
            Err(e) => Err(std::io::Error::new(std::io::ErrorKind::InvalidData, e)),
        }
    }
    
    /// Check if this is a WebSocket upgrade request.
    pub fn is_upgrade_request(request: &Request) -> bool {
        request.header("upgrade")
            .map(|h| h.to_lowercase() == "websocket")
            .unwrap_or(false)
    }
}

/// Compute WebSocket accept key.
fn compute_accept_key(key: &str) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    
    // Proper implementation would use SHA-1 + Base64
    // This is a simplified version for demonstration
    const MAGIC: &str = "258EAFA5-E914-47DA-95CA-C5AB0DC85B11";
    
    let combined = format!("{}{}", key, MAGIC);
    
    // Use a simple hash as placeholder (real impl needs SHA-1)
    let mut hasher = DefaultHasher::new();
    combined.hash(&mut hasher);
    let hash = hasher.finish();
    
    // Convert to base64-like string
    base64_encode(&hash.to_be_bytes())
}

fn base64_encode(data: &[u8]) -> String {
    const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut result = String::new();
    
    for chunk in data.chunks(3) {
        let mut n = (chunk[0] as u32) << 16;
        if chunk.len() > 1 {
            n |= (chunk[1] as u32) << 8;
        }
        if chunk.len() > 2 {
            n |= chunk[2] as u32;
        }
        
        result.push(CHARS[((n >> 18) & 0x3F) as usize] as char);
        result.push(CHARS[((n >> 12) & 0x3F) as usize] as char);
        
        if chunk.len() > 1 {
            result.push(CHARS[((n >> 6) & 0x3F) as usize] as char);
        } else {
            result.push('=');
        }
        
        if chunk.len() > 2 {
            result.push(CHARS[(n & 0x3F) as usize] as char);
        } else {
            result.push('=');
        }
    }
    
    result
}

/// WebSocket message handler type.
pub type WsHandler = Box<dyn Fn(&mut WebSocket, WsFrame) + Send + Sync>;

/// WebSocket server that can be added to an App.
pub struct WsServer {
    handlers: HashMap<String, WsHandler>,
}

impl WsServer {
    /// Create a new WebSocket server.
    pub fn new() -> Self {
        Self {
            handlers: HashMap::new(),
        }
    }
    
    /// Register a handler for a path.
    pub fn on(mut self, path: &str, handler: WsHandler) -> Self {
        self.handlers.insert(path.to_string(), handler);
        self
    }
    
    /// Handle a WebSocket connection.
    pub fn handle(&self, path: &str, ws: &mut WebSocket, frame: WsFrame) {
        if let Some(handler) = self.handlers.get(path) {
            handler(ws, frame);
        }
    }
}

impl Default for WsServer {
    fn default() -> Self {
        Self::new()
    }
}

// =============================================================================
// Flash Messages
// =============================================================================

/// Flash message for one-time notifications.
#[derive(Clone, Debug)]
pub struct FlashMessage {
    pub category: String,
    pub message: String,
}

impl FlashMessage {
    pub fn new(category: &str, message: &str) -> Self {
        Self {
            category: category.to_string(),
            message: message.to_string(),
        }
    }
    
    pub fn info(message: &str) -> Self {
        Self::new("info", message)
    }
    
    pub fn success(message: &str) -> Self {
        Self::new("success", message)
    }
    
    pub fn warning(message: &str) -> Self {
        Self::new("warning", message)
    }
    
    pub fn error(message: &str) -> Self {
        Self::new("error", message)
    }
}

/// Store flash messages in session.
pub fn flash(session: &mut Session, message: FlashMessage) {
    let key = format!("_flash_{}", session.data.len());
    session.set(&key, &format!("{}:{}", message.category, message.message));
}

/// Get and consume flash messages from session.
pub fn get_flashed_messages(session: &mut Session) -> Vec<FlashMessage> {
    let mut messages = Vec::new();
    let keys: Vec<_> = session.data.keys()
        .filter(|k| k.starts_with("_flash_"))
        .cloned()
        .collect();
    
    for key in keys {
        if let Some(value) = session.remove(&key) {
            if let Some((category, message)) = value.split_once(':') {
                messages.push(FlashMessage::new(category, message));
            }
        }
    }
    
    messages
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_response() {
        let response = Response::ok().text("Hello");
        assert_eq!(response.status, 200);
        assert_eq!(response.body, b"Hello");
    }
    
    #[test]
    fn test_router() {
        let router = Router::new()
            .get("/", Box::new(|_| Response::ok().text("Home")))
            .get("/users/<id>", Box::new(|req| {
                let id = req.param("id").unwrap_or("?");
                Response::ok().text(&format!("User: {}", id))
            }));
        
        let (route, params) = router.match_route(Method::Get, "/").unwrap();
        assert!(params.is_empty());
        
        let (route, params) = router.match_route(Method::Get, "/users/123").unwrap();
        assert_eq!(params.get("id"), Some(&"123".to_string()));
        
        assert!(router.match_route(Method::Get, "/notfound").is_none());
    }
    
    #[test]
    fn test_template() {
        let template = Template::new("<h1>Hello, {{name}}!</h1>");
        let mut vars = HashMap::new();
        vars.insert("name".to_string(), "World".to_string());
        
        let result = template.render(&vars);
        assert_eq!(result, "<h1>Hello, World!</h1>");
    }
    
    #[test]
    fn test_url_encoding() {
        assert_eq!(urlencoding::encode("hello world"), "hello+world");
        assert_eq!(urlencoding::decode("hello+world").unwrap(), "hello world");
        assert_eq!(urlencoding::decode("hello%20world").unwrap(), "hello world");
    }
    
    #[test]
    fn test_cookies() {
        let cookie = set_cookie("session", "abc123", Some(3600), Some("/"), true, false);
        assert!(cookie.contains("session=abc123"));
        assert!(cookie.contains("Max-Age=3600"));
        assert!(cookie.contains("Path=/"));
        assert!(cookie.contains("HttpOnly"));
    }
    
    #[test]
    fn test_session() {
        let mut session = Session::new();
        assert!(!session.is_expired());
        
        session.set("user", "alice");
        assert_eq!(session.get("user"), Some("alice"));
        
        session.remove("user");
        assert_eq!(session.get("user"), None);
    }
    
    #[test]
    fn test_session_store() {
        let store = SessionStore::new();
        
        let session = store.get_or_create(None);
        let id = session.id.clone();
        
        let same_session = store.get_or_create(Some(&id));
        assert_eq!(session.id, same_session.id);
    }
    
    #[test]
    fn test_ws_frame_encode_decode() {
        let original = WsFrame::text("Hello, WebSocket!");
        let encoded = original.encode();
        let (decoded, _) = WsFrame::decode(&encoded).unwrap();
        
        assert_eq!(decoded.opcode, WsOpcode::Text);
        assert_eq!(decoded.as_text(), Some("Hello, WebSocket!"));
    }
    
    #[test]
    fn test_flash_messages() {
        let mut session = Session::new();
        
        flash(&mut session, FlashMessage::info("Welcome!"));
        flash(&mut session, FlashMessage::error("Something went wrong"));
        
        let messages = get_flashed_messages(&mut session);
        assert_eq!(messages.len(), 2);
        
        // Messages should be consumed
        let messages2 = get_flashed_messages(&mut session);
        assert_eq!(messages2.len(), 0);
    }
}

