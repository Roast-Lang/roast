//! Debug Adapter Protocol handler.

use std::io::{BufRead, BufReader, Read, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::Arc;

use crate::protocol::*;
use crate::session::{DebugSession, StopReason};
use crate::watch::EvaluationResult;

/// Debug adapter.
pub struct DebugAdapter<R: Read, W: Write> {
    /// Input stream.
    input: BufReader<R>,
    /// Output stream.
    output: W,
    /// Debug session.
    session: Arc<DebugSession>,
    /// Sequence counter.
    seq: AtomicI64,
    /// Client capabilities.
    client_caps: Option<InitializeRequestArguments>,
}

impl<R: Read, W: Write> DebugAdapter<R, W> {
    /// Create a new adapter.
    pub fn new(input: R, output: W) -> Self {
        Self {
            input: BufReader::new(input),
            output,
            session: Arc::new(DebugSession::new()),
            seq: AtomicI64::new(1),
            client_caps: None,
        }
    }
    
    /// Run the adapter.
    pub fn run(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        loop {
            // Read message
            if let Some(msg) = self.read_message()? {
                if !self.handle_message(msg)? {
                    break;
                }
            }
        }
        Ok(())
    }
    
    /// Read a DAP message.
    fn read_message(&mut self) -> Result<Option<Request>, Box<dyn std::error::Error>> {
        // Read headers
        let mut headers = String::new();
        loop {
            let mut line = String::new();
            if self.input.read_line(&mut line)? == 0 {
                return Ok(None); // EOF
            }
            
            if line == "\r\n" || line == "\n" {
                break;
            }
            headers.push_str(&line);
        }
        
        // Parse Content-Length
        let content_length: usize = headers
            .lines()
            .find(|l| l.starts_with("Content-Length:"))
            .and_then(|l| l.split(':').nth(1))
            .and_then(|s| s.trim().parse().ok())
            .ok_or("Missing Content-Length")?;
        
        // Read body
        let mut body = vec![0u8; content_length];
        self.input.read_exact(&mut body)?;
        
        // Parse JSON
        let request: Request = serde_json::from_slice(&body)?;
        Ok(Some(request))
    }
    
    /// Send a response.
    fn send_response(&mut self, request: &Request, success: bool, body: Option<serde_json::Value>) {
        let response = Response {
            base: ProtocolMessage {
                seq: self.seq.fetch_add(1, Ordering::SeqCst),
                message_type: "response".to_string(),
            },
            request_seq: request.base.seq,
            success,
            command: request.command.clone(),
            message: None,
            body,
        };
        
        self.send_message(&response);
    }
    
    /// Send an error response.
    fn send_error(&mut self, request: &Request, message: &str) {
        let response = Response {
            base: ProtocolMessage {
                seq: self.seq.fetch_add(1, Ordering::SeqCst),
                message_type: "response".to_string(),
            },
            request_seq: request.base.seq,
            success: false,
            command: request.command.clone(),
            message: Some(message.to_string()),
            body: None,
        };
        
        self.send_message(&response);
    }
    
    /// Send an event.
    fn send_event(&mut self, event: &str, body: Option<serde_json::Value>) {
        let evt = Event {
            base: ProtocolMessage {
                seq: self.seq.fetch_add(1, Ordering::SeqCst),
                message_type: "event".to_string(),
            },
            event: event.to_string(),
            body,
        };
        
        self.send_message(&evt);
    }
    
    /// Send a message.
    fn send_message<T: serde::Serialize>(&mut self, msg: &T) {
        let body = serde_json::to_string(msg).unwrap();
        let header = format!("Content-Length: {}\r\n\r\n", body.len());
        
        let _ = self.output.write_all(header.as_bytes());
        let _ = self.output.write_all(body.as_bytes());
        let _ = self.output.flush();
    }
    
    /// Handle a request.
    fn handle_message(&mut self, request: Request) -> Result<bool, Box<dyn std::error::Error>> {
        match request.command.as_str() {
            "initialize" => {
                if let Some(args) = &request.arguments {
                    self.client_caps = serde_json::from_value(args.clone()).ok();
                }
                
                let caps = Capabilities {
                    supports_configuration_done_request: true,
                    supports_conditional_breakpoints: true,
                    supports_hit_conditional_breakpoints: true,
                    supports_evaluate_for_hovers: true,
                    supports_log_points: true,
                    supports_set_variable: true,
                    supports_terminate_request: true,
                    supports_stepping_granularity: true,
                    ..Default::default()
                };
                
                self.send_response(&request, true, Some(serde_json::to_value(caps)?));
            }
            
            "launch" => {
                if let Some(args) = &request.arguments {
                    let launch_args: LaunchRequestArguments = serde_json::from_value(args.clone())?;
                    
                    // Initialize session
                    self.session.start();
                    
                    self.send_response(&request, true, None);
                    
                    // Send initialized event
                    self.send_event("initialized", None);
                    
                    if launch_args.stop_on_entry {
                        self.session.stop(StopReason::Entry);
                        self.send_event("stopped", Some(serde_json::to_value(StoppedEventBody {
                            reason: "entry".to_string(),
                            description: Some("Stopped on entry".to_string()),
                            thread_id: Some(self.session.thread_id()),
                            preserve_focus_hint: false,
                            text: None,
                            all_threads_stopped: true,
                            hit_breakpoint_ids: None,
                        })?));
                    }
                }
            }
            
            "attach" => {
                self.send_response(&request, true, None);
                self.send_event("initialized", None);
            }
            
            "configurationDone" => {
                self.send_response(&request, true, None);
            }
            
            "setBreakpoints" => {
                if let Some(args) = &request.arguments {
                    let bp_args: SetBreakpointsArguments = serde_json::from_value(args.clone())?;
                    
                    let source_path = bp_args.source.path
                        .map(PathBuf::from)
                        .unwrap_or_default();
                    
                    let breakpoints = self.session.set_breakpoints(source_path, bp_args.breakpoints);
                    
                    self.send_response(&request, true, Some(serde_json::json!({
                        "breakpoints": breakpoints
                    })));
                }
            }
            
            "threads" => {
                self.send_response(&request, true, Some(serde_json::json!({
                    "threads": [
                        Thread {
                            id: self.session.thread_id(),
                            name: "Main Thread".to_string(),
                        }
                    ]
                })));
            }
            
            "stackTrace" => {
                if let Some(args) = &request.arguments {
                    let _st_args: StackTraceArguments = serde_json::from_value(args.clone())?;
                    let frames = self.session.stack_trace();
                    
                    self.send_response(&request, true, Some(serde_json::json!({
                        "stackFrames": frames,
                        "totalFrames": frames.len()
                    })));
                }
            }
            
            "scopes" => {
                if let Some(args) = &request.arguments {
                    let scope_args: ScopesArguments = serde_json::from_value(args.clone())?;
                    let scopes = self.session.scopes(scope_args.frame_id);
                    
                    self.send_response(&request, true, Some(serde_json::json!({
                        "scopes": scopes
                    })));
                }
            }
            
            "variables" => {
                if let Some(args) = &request.arguments {
                    let var_args: VariablesArguments = serde_json::from_value(args.clone())?;
                    let variables = self.session.variables(var_args.variables_reference);
                    
                    self.send_response(&request, true, Some(serde_json::json!({
                        "variables": variables
                    })));
                }
            }
            
            "continue" => {
                self.session.continue_execution();
                
                self.send_response(&request, true, Some(serde_json::json!({
                    "allThreadsContinued": true
                })));
                
                self.send_event("continued", Some(serde_json::to_value(ContinuedEventBody {
                    thread_id: self.session.thread_id(),
                    all_threads_continued: true,
                })?));
            }
            
            "next" | "stepOver" => {
                self.session.continue_execution();
                self.send_response(&request, true, None);
            }
            
            "stepIn" => {
                self.session.continue_execution();
                self.send_response(&request, true, None);
            }
            
            "stepOut" => {
                self.session.continue_execution();
                self.send_response(&request, true, None);
            }
            
            "pause" => {
                self.session.stop(StopReason::Pause);
                
                self.send_response(&request, true, None);
                
                self.send_event("stopped", Some(serde_json::to_value(StoppedEventBody {
                    reason: "pause".to_string(),
                    description: Some("Paused".to_string()),
                    thread_id: Some(self.session.thread_id()),
                    preserve_focus_hint: false,
                    text: None,
                    all_threads_stopped: true,
                    hit_breakpoint_ids: None,
                })?));
            }
            
            "evaluate" => {
                if let Some(args) = &request.arguments {
                    let eval_args: EvaluateArguments = serde_json::from_value(args.clone())?;
                    
                    // Get frame ID (default to first frame if not specified)
                    let frame_id = eval_args.frame_id.unwrap_or(1);
                    
                    // Evaluate the expression
                    let result = self.session.evaluate_expression(&eval_args.expression, frame_id);
                    
                    match result {
                        EvaluationResult::Success { value, variables_reference } => {
                            self.send_response(&request, true, Some(serde_json::json!({
                                "result": value.display(),
                                "type": value.type_name(),
                                "variablesReference": variables_reference
                            })));
                        }
                        EvaluationResult::Error { message } => {
                            self.send_response(&request, true, Some(serde_json::json!({
                                "result": format!("<error: {}>", message),
                                "variablesReference": 0
                            })));
                        }
                    }
                }
            }
            
            "disconnect" | "terminate" => {
                self.session.terminate();
                self.send_response(&request, true, None);
                
                self.send_event("terminated", Some(serde_json::to_value(TerminatedEventBody {
                    restart: None,
                })?));
                
                return Ok(false); // Exit loop
            }
            
            _ => {
                self.send_error(&request, &format!("Unknown command: {}", request.command));
            }
        }
        
        Ok(true)
    }
}

