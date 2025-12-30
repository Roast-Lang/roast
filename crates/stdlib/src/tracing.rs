//! Distributed tracing.
//!
//! Provides W3C Trace Context compatible tracing:
//! - Spans with timing and attributes
//! - Trace propagation via headers
//! - Span events and status

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

// =============================================================================
// IDs
// =============================================================================

/// Generate a random trace ID (128 bits as hex).
pub fn generate_trace_id() -> String {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos() as u64;
    let counter = COUNTER.fetch_add(1, Ordering::Relaxed);
    let random = (timestamp.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407)) ^ counter;
    format!("{:016x}{:016x}", timestamp, random)
}

/// Generate a random span ID (64 bits as hex).
pub fn generate_span_id() -> String {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_micros() as u64;
    let counter = COUNTER.fetch_add(1, Ordering::Relaxed);
    let id = timestamp.wrapping_mul(6364136223846793005).wrapping_add(counter);
    format!("{:016x}", id)
}

// =============================================================================
// Span Context
// =============================================================================

/// Span context for trace propagation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SpanContext {
    /// Trace ID (32 hex characters).
    pub trace_id: String,
    /// Span ID (16 hex characters).
    pub span_id: String,
    /// Trace flags (e.g., sampled).
    pub trace_flags: u8,
    /// Trace state (vendor-specific data).
    pub trace_state: String,
}

impl SpanContext {
    /// Create a new root span context.
    pub fn new() -> Self {
        Self {
            trace_id: generate_trace_id(),
            span_id: generate_span_id(),
            trace_flags: 0x01, // Sampled by default
            trace_state: String::new(),
        }
    }
    
    /// Create a child context.
    pub fn child(&self) -> Self {
        Self {
            trace_id: self.trace_id.clone(),
            span_id: generate_span_id(),
            trace_flags: self.trace_flags,
            trace_state: self.trace_state.clone(),
        }
    }
    
    /// Check if the span is sampled.
    pub fn is_sampled(&self) -> bool {
        self.trace_flags & 0x01 != 0
    }
    
    /// Parse from W3C traceparent header.
    pub fn from_traceparent(header: &str) -> Option<Self> {
        let parts: Vec<&str> = header.split('-').collect();
        if parts.len() < 4 || parts[0] != "00" {
            return None;
        }
        
        let trace_id = parts[1].to_string();
        let span_id = parts[2].to_string();
        let trace_flags = u8::from_str_radix(parts[3], 16).ok()?;
        
        if trace_id.len() != 32 || span_id.len() != 16 {
            return None;
        }
        
        Some(Self {
            trace_id,
            span_id,
            trace_flags,
            trace_state: String::new(),
        })
    }
    
    /// Format as W3C traceparent header.
    pub fn to_traceparent(&self) -> String {
        format!("00-{}-{}-{:02x}", self.trace_id, self.span_id, self.trace_flags)
    }
}

impl Default for SpanContext {
    fn default() -> Self {
        Self::new()
    }
}

// =============================================================================
// Span Kind and Status
// =============================================================================

/// Span kind.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpanKind {
    /// Internal operation.
    Internal,
    /// Server handling a request.
    Server,
    /// Client making a request.
    Client,
    /// Producer sending a message.
    Producer,
    /// Consumer receiving a message.
    Consumer,
}

impl Default for SpanKind {
    fn default() -> Self {
        SpanKind::Internal
    }
}

/// Span status.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SpanStatus {
    /// Unset status.
    Unset,
    /// OK status.
    Ok,
    /// Error status with message.
    Error(String),
}

impl Default for SpanStatus {
    fn default() -> Self {
        SpanStatus::Unset
    }
}

// =============================================================================
// Span Event
// =============================================================================

/// An event within a span.
#[derive(Clone, Debug)]
pub struct SpanEvent {
    /// Event name.
    pub name: String,
    /// Event timestamp.
    pub timestamp: SystemTime,
    /// Event attributes.
    pub attributes: HashMap<String, AttributeValue>,
}

impl SpanEvent {
    /// Create a new event.
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            timestamp: SystemTime::now(),
            attributes: HashMap::new(),
        }
    }
    
    /// Add an attribute.
    pub fn with_attribute(mut self, key: &str, value: impl Into<AttributeValue>) -> Self {
        self.attributes.insert(key.to_string(), value.into());
        self
    }
}

// =============================================================================
// Attribute Value
// =============================================================================

/// Attribute value types.
#[derive(Clone, Debug)]
pub enum AttributeValue {
    String(String),
    Int(i64),
    Float(f64),
    Bool(bool),
    StringArray(Vec<String>),
    IntArray(Vec<i64>),
}

impl From<&str> for AttributeValue {
    fn from(s: &str) -> Self {
        AttributeValue::String(s.to_string())
    }
}

impl From<String> for AttributeValue {
    fn from(s: String) -> Self {
        AttributeValue::String(s)
    }
}

impl From<i64> for AttributeValue {
    fn from(n: i64) -> Self {
        AttributeValue::Int(n)
    }
}

impl From<i32> for AttributeValue {
    fn from(n: i32) -> Self {
        AttributeValue::Int(n as i64)
    }
}

impl From<f64> for AttributeValue {
    fn from(n: f64) -> Self {
        AttributeValue::Float(n)
    }
}

impl From<bool> for AttributeValue {
    fn from(b: bool) -> Self {
        AttributeValue::Bool(b)
    }
}

// =============================================================================
// Span
// =============================================================================

/// A span representing a unit of work.
pub struct Span {
    /// Span name.
    pub name: String,
    /// Span context.
    pub context: SpanContext,
    /// Parent span ID.
    pub parent_span_id: Option<String>,
    /// Span kind.
    pub kind: SpanKind,
    /// Start time.
    pub start_time: Instant,
    /// Start timestamp.
    pub start_timestamp: SystemTime,
    /// End time (set when span ends).
    end_time: Option<Instant>,
    /// Span status.
    pub status: SpanStatus,
    /// Span attributes.
    pub attributes: HashMap<String, AttributeValue>,
    /// Span events.
    pub events: Vec<SpanEvent>,
    /// Whether the span has ended.
    ended: bool,
}

impl Span {
    /// Create a new root span.
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            context: SpanContext::new(),
            parent_span_id: None,
            kind: SpanKind::Internal,
            start_time: Instant::now(),
            start_timestamp: SystemTime::now(),
            end_time: None,
            status: SpanStatus::Unset,
            attributes: HashMap::new(),
            events: Vec::new(),
            ended: false,
        }
    }
    
    /// Create a child span.
    pub fn child(&self, name: &str) -> Self {
        Self {
            name: name.to_string(),
            context: self.context.child(),
            parent_span_id: Some(self.context.span_id.clone()),
            kind: SpanKind::Internal,
            start_time: Instant::now(),
            start_timestamp: SystemTime::now(),
            end_time: None,
            status: SpanStatus::Unset,
            attributes: HashMap::new(),
            events: Vec::new(),
            ended: false,
        }
    }
    
    /// Set span kind.
    pub fn with_kind(mut self, kind: SpanKind) -> Self {
        self.kind = kind;
        self
    }
    
    /// Set an attribute.
    pub fn set_attribute(&mut self, key: &str, value: impl Into<AttributeValue>) {
        self.attributes.insert(key.to_string(), value.into());
    }
    
    /// Add an attribute (builder pattern).
    pub fn with_attribute(mut self, key: &str, value: impl Into<AttributeValue>) -> Self {
        self.set_attribute(key, value);
        self
    }
    
    /// Add an event.
    pub fn add_event(&mut self, event: SpanEvent) {
        self.events.push(event);
    }
    
    /// Record an exception.
    pub fn record_exception(&mut self, error: &dyn std::error::Error) {
        let mut event = SpanEvent::new("exception");
        event.attributes.insert("exception.type".to_string(), 
            AttributeValue::String(std::any::type_name_of_val(error).to_string()));
        event.attributes.insert("exception.message".to_string(), 
            AttributeValue::String(error.to_string()));
        self.events.push(event);
        self.status = SpanStatus::Error(error.to_string());
    }
    
    /// Set span status.
    pub fn set_status(&mut self, status: SpanStatus) {
        self.status = status;
    }
    
    /// End the span.
    pub fn end(&mut self) {
        if !self.ended {
            self.end_time = Some(Instant::now());
            self.ended = true;
        }
    }
    
    /// Get span duration.
    pub fn duration(&self) -> Duration {
        let end = self.end_time.unwrap_or_else(Instant::now);
        end.duration_since(self.start_time)
    }
    
    /// Check if span has ended.
    pub fn is_ended(&self) -> bool {
        self.ended
    }
}

impl Drop for Span {
    fn drop(&mut self) {
        if !self.ended {
            self.end();
        }
    }
}

// =============================================================================
// Tracer
// =============================================================================

/// Configuration for the tracer.
#[derive(Clone, Debug)]
pub struct TracerConfig {
    /// Service name.
    pub service_name: String,
    /// Whether to sample all traces.
    pub sample_all: bool,
    /// Maximum events per span.
    pub max_events_per_span: usize,
    /// Maximum attributes per span.
    pub max_attributes_per_span: usize,
}

impl Default for TracerConfig {
    fn default() -> Self {
        Self {
            service_name: "unknown".to_string(),
            sample_all: true,
            max_events_per_span: 128,
            max_attributes_per_span: 128,
        }
    }
}

/// A tracer for creating spans.
pub struct Tracer {
    config: TracerConfig,
    spans: RwLock<Vec<Arc<RwLock<Span>>>>,
}

impl Tracer {
    /// Create a new tracer.
    pub fn new(config: TracerConfig) -> Self {
        Self {
            config,
            spans: RwLock::new(Vec::new()),
        }
    }
    
    /// Create a new tracer with default config.
    pub fn default_tracer(service_name: &str) -> Self {
        Self::new(TracerConfig {
            service_name: service_name.to_string(),
            ..Default::default()
        })
    }
    
    /// Start a new span.
    pub fn start_span(&self, name: &str) -> Span {
        let mut span = Span::new(name);
        span.set_attribute("service.name", self.config.service_name.clone());
        span
    }
    
    /// Start a child span from an existing context.
    pub fn start_span_with_context(&self, name: &str, parent: &SpanContext) -> Span {
        let mut span = Span::new(name);
        span.context = parent.child();
        span.parent_span_id = Some(parent.span_id.clone());
        span.set_attribute("service.name", self.config.service_name.clone());
        span
    }
    
    /// Extract span context from headers.
    pub fn extract(&self, headers: &HashMap<String, String>) -> Option<SpanContext> {
        headers.get("traceparent")
            .and_then(|h| SpanContext::from_traceparent(h))
    }
    
    /// Inject span context into headers.
    pub fn inject(&self, context: &SpanContext, headers: &mut HashMap<String, String>) {
        headers.insert("traceparent".to_string(), context.to_traceparent());
        if !context.trace_state.is_empty() {
            headers.insert("tracestate".to_string(), context.trace_state.clone());
        }
    }
    
    /// Record a completed span.
    pub fn record(&self, span: Span) {
        let mut spans = self.spans.write().unwrap();
        spans.push(Arc::new(RwLock::new(span)));
    }
    
    /// Get all recorded spans.
    pub fn get_spans(&self) -> Vec<Arc<RwLock<Span>>> {
        self.spans.read().unwrap().clone()
    }
    
    /// Clear recorded spans.
    pub fn clear(&self) {
        self.spans.write().unwrap().clear();
    }
}

impl Default for Tracer {
    fn default() -> Self {
        Self::new(TracerConfig::default())
    }
}

// =============================================================================
// Global Tracer
// =============================================================================

lazy_static::lazy_static! {
    /// Global tracer instance.
    static ref GLOBAL_TRACER: RwLock<Option<Arc<Tracer>>> = RwLock::new(None);
}

/// Set the global tracer.
pub fn set_global_tracer(tracer: Tracer) {
    *GLOBAL_TRACER.write().unwrap() = Some(Arc::new(tracer));
}

/// Get the global tracer.
pub fn global_tracer() -> Option<Arc<Tracer>> {
    GLOBAL_TRACER.read().unwrap().clone()
}

/// Start a span using the global tracer.
pub fn start_span(name: &str) -> Option<Span> {
    global_tracer().map(|t| t.start_span(name))
}

// =============================================================================
// Convenience Macros
// =============================================================================

/// Trace a block of code.
pub fn trace<F, T>(name: &str, f: F) -> T
where
    F: FnOnce(&mut Span) -> T,
{
    let tracer = global_tracer();
    if let Some(ref t) = tracer {
        let mut span = t.start_span(name);
        let result = f(&mut span);
        span.end();
        t.record(span);
        result
    } else {
        let mut span = Span::new(name);
        f(&mut span)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_generate_trace_id() {
        let id1 = generate_trace_id();
        let id2 = generate_trace_id();
        
        assert_eq!(id1.len(), 32);
        assert_eq!(id2.len(), 32);
        assert_ne!(id1, id2);
    }
    
    #[test]
    fn test_generate_span_id() {
        let id1 = generate_span_id();
        let id2 = generate_span_id();
        
        assert_eq!(id1.len(), 16);
        assert_eq!(id2.len(), 16);
        assert_ne!(id1, id2);
    }
    
    #[test]
    fn test_span_context() {
        let ctx = SpanContext::new();
        assert_eq!(ctx.trace_id.len(), 32);
        assert_eq!(ctx.span_id.len(), 16);
        assert!(ctx.is_sampled());
        
        let child = ctx.child();
        assert_eq!(child.trace_id, ctx.trace_id);
        assert_ne!(child.span_id, ctx.span_id);
    }
    
    #[test]
    fn test_traceparent() {
        let ctx = SpanContext {
            trace_id: "0af7651916cd43dd8448eb211c80319c".to_string(),
            span_id: "b7ad6b7169203331".to_string(),
            trace_flags: 0x01,
            trace_state: String::new(),
        };
        
        let header = ctx.to_traceparent();
        assert_eq!(header, "00-0af7651916cd43dd8448eb211c80319c-b7ad6b7169203331-01");
        
        let parsed = SpanContext::from_traceparent(&header).unwrap();
        assert_eq!(parsed.trace_id, ctx.trace_id);
        assert_eq!(parsed.span_id, ctx.span_id);
        assert_eq!(parsed.trace_flags, ctx.trace_flags);
    }
    
    #[test]
    fn test_span() {
        let mut span = Span::new("test_operation")
            .with_kind(SpanKind::Server)
            .with_attribute("http.method", "GET");
        
        span.set_attribute("http.status_code", 200i64);
        span.add_event(SpanEvent::new("cache_hit"));
        span.end();
        
        assert!(span.is_ended());
        assert!(span.duration() > Duration::ZERO);
        assert_eq!(span.attributes.len(), 2);
        assert_eq!(span.events.len(), 1);
    }
    
    #[test]
    fn test_child_span() {
        let parent = Span::new("parent");
        let child = parent.child("child");
        
        assert_eq!(child.context.trace_id, parent.context.trace_id);
        assert_eq!(child.parent_span_id, Some(parent.context.span_id.clone()));
    }
    
    #[test]
    fn test_tracer() {
        let tracer = Tracer::default_tracer("test-service");
        
        let mut span = tracer.start_span("operation");
        span.set_attribute("key", "value");
        span.end();
        
        tracer.record(span);
        
        let spans = tracer.get_spans();
        assert_eq!(spans.len(), 1);
    }
    
    #[test]
    fn test_header_injection() {
        let tracer = Tracer::default_tracer("test-service");
        let span = tracer.start_span("operation");
        
        let mut headers = HashMap::new();
        tracer.inject(&span.context, &mut headers);
        
        assert!(headers.contains_key("traceparent"));
        
        let extracted = tracer.extract(&headers).unwrap();
        assert_eq!(extracted.trace_id, span.context.trace_id);
    }
}
