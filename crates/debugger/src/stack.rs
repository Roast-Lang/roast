//! Stack frame handling.

use std::path::PathBuf;

/// Frame ID.
pub type FrameId = i64;

/// Stack frame.
#[derive(Debug, Clone)]
pub struct Frame {
    /// Frame ID.
    pub id: FrameId,
    /// Function name.
    pub name: String,
    /// Source file.
    pub source: Option<PathBuf>,
    /// Line number.
    pub line: i64,
    /// Column.
    pub column: i64,
    /// End line.
    pub end_line: Option<i64>,
    /// End column.
    pub end_column: Option<i64>,
    /// Local variables reference.
    pub locals_ref: i64,
    /// Module name.
    pub module: Option<String>,
}

/// Call stack.
#[derive(Debug, Default)]
pub struct CallStack {
    /// Frames (top of stack first).
    frames: Vec<Frame>,
    /// Next frame ID.
    next_id: FrameId,
}

impl CallStack {
    /// Create a new call stack.
    pub fn new() -> Self {
        Self {
            frames: Vec::new(),
            next_id: 1,
        }
    }
    
    /// Push a frame.
    pub fn push(&mut self, name: String, source: Option<PathBuf>, line: i64, column: i64) -> FrameId {
        let id = self.next_id;
        self.next_id += 1;
        
        let frame = Frame {
            id,
            name,
            source,
            line,
            column,
            end_line: None,
            end_column: None,
            locals_ref: id, // Use frame ID as locals reference
            module: None,
        };
        
        self.frames.insert(0, frame); // Insert at top
        id
    }
    
    /// Pop a frame.
    pub fn pop(&mut self) -> Option<Frame> {
        if !self.frames.is_empty() {
            Some(self.frames.remove(0))
        } else {
            None
        }
    }
    
    /// Get frame by ID.
    pub fn get(&self, id: FrameId) -> Option<&Frame> {
        self.frames.iter().find(|f| f.id == id)
    }
    
    /// Get the top frame.
    pub fn top(&self) -> Option<&Frame> {
        self.frames.first()
    }
    
    /// Get all frames.
    pub fn frames(&self) -> &[Frame] {
        &self.frames
    }
    
    /// Get depth.
    pub fn depth(&self) -> usize {
        self.frames.len()
    }
    
    /// Clear the stack.
    pub fn clear(&mut self) {
        self.frames.clear();
    }
    
    /// Update top frame location.
    pub fn update_location(&mut self, line: i64, column: i64) {
        if let Some(frame) = self.frames.first_mut() {
            frame.line = line;
            frame.column = column;
        }
    }
}

