//! Queue data structures.
//!
//! FIFO queues, deques, and ring buffers.

use std::collections::VecDeque;

/// A simple FIFO queue.
pub struct Queue<T> {
    data: VecDeque<T>,
}

impl<T> Queue<T> {
    /// Create an empty queue.
    pub fn new() -> Self {
        Self { data: VecDeque::new() }
    }
    
    /// Create a queue with capacity.
    pub fn with_capacity(capacity: usize) -> Self {
        Self { data: VecDeque::with_capacity(capacity) }
    }
    
    /// Enqueue a value.
    pub fn enqueue(&mut self, value: T) {
        self.data.push_back(value);
    }
    
    /// Dequeue a value.
    pub fn dequeue(&mut self) -> Option<T> {
        self.data.pop_front()
    }
    
    /// Peek at the front.
    pub fn front(&self) -> Option<&T> {
        self.data.front()
    }
    
    /// Peek at the back.
    pub fn back(&self) -> Option<&T> {
        self.data.back()
    }
    
    /// Get length.
    pub fn len(&self) -> usize {
        self.data.len()
    }
    
    /// Check if empty.
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }
    
    /// Clear the queue.
    pub fn clear(&mut self) {
        self.data.clear();
    }
    
    /// Iterate over values.
    pub fn iter(&self) -> impl Iterator<Item = &T> {
        self.data.iter()
    }
}

impl<T> Default for Queue<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> FromIterator<T> for Queue<T> {
    fn from_iter<I: IntoIterator<Item = T>>(iter: I) -> Self {
        Self { data: iter.into_iter().collect() }
    }
}

/// A double-ended queue.
pub struct Deque<T> {
    data: VecDeque<T>,
}

impl<T> Deque<T> {
    /// Create an empty deque.
    pub fn new() -> Self {
        Self { data: VecDeque::new() }
    }
    
    /// Push to the front.
    pub fn push_front(&mut self, value: T) {
        self.data.push_front(value);
    }
    
    /// Push to the back.
    pub fn push_back(&mut self, value: T) {
        self.data.push_back(value);
    }
    
    /// Pop from the front.
    pub fn pop_front(&mut self) -> Option<T> {
        self.data.pop_front()
    }
    
    /// Pop from the back.
    pub fn pop_back(&mut self) -> Option<T> {
        self.data.pop_back()
    }
    
    /// Peek at the front.
    pub fn front(&self) -> Option<&T> {
        self.data.front()
    }
    
    /// Peek at the back.
    pub fn back(&self) -> Option<&T> {
        self.data.back()
    }
    
    /// Get length.
    pub fn len(&self) -> usize {
        self.data.len()
    }
    
    /// Check if empty.
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }
    
    /// Clear the deque.
    pub fn clear(&mut self) {
        self.data.clear();
    }
    
    /// Get by index.
    pub fn get(&self, index: usize) -> Option<&T> {
        self.data.get(index)
    }
    
    /// Rotate left by n positions.
    pub fn rotate_left(&mut self, n: usize) {
        self.data.rotate_left(n);
    }
    
    /// Rotate right by n positions.
    pub fn rotate_right(&mut self, n: usize) {
        self.data.rotate_right(n);
    }
}

impl<T> Default for Deque<T> {
    fn default() -> Self {
        Self::new()
    }
}

/// A fixed-size ring buffer.
pub struct RingBuffer<T> {
    data: Vec<Option<T>>,
    head: usize,
    tail: usize,
    len: usize,
}

impl<T> RingBuffer<T> {
    /// Create a ring buffer with capacity.
    pub fn new(capacity: usize) -> Self {
        let mut data = Vec::with_capacity(capacity);
        for _ in 0..capacity {
            data.push(None);
        }
        
        Self {
            data,
            head: 0,
            tail: 0,
            len: 0,
        }
    }
    
    /// Push a value. Returns evicted value if buffer was full.
    pub fn push(&mut self, value: T) -> Option<T> {
        let evicted = if self.len == self.data.len() {
            // Buffer full, evict oldest
            self.pop()
        } else {
            None
        };
        
        self.data[self.tail] = Some(value);
        self.tail = (self.tail + 1) % self.data.len();
        self.len += 1;
        
        evicted
    }
    
    /// Pop the oldest value.
    pub fn pop(&mut self) -> Option<T> {
        if self.len == 0 {
            return None;
        }
        
        let value = self.data[self.head].take();
        self.head = (self.head + 1) % self.data.len();
        self.len -= 1;
        
        value
    }
    
    /// Peek at the oldest value.
    pub fn front(&self) -> Option<&T> {
        if self.len == 0 {
            None
        } else {
            self.data[self.head].as_ref()
        }
    }
    
    /// Peek at the newest value.
    pub fn back(&self) -> Option<&T> {
        if self.len == 0 {
            None
        } else {
            let idx = if self.tail == 0 { self.data.len() - 1 } else { self.tail - 1 };
            self.data[idx].as_ref()
        }
    }
    
    /// Get length.
    pub fn len(&self) -> usize {
        self.len
    }
    
    /// Check if empty.
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
    
    /// Check if full.
    pub fn is_full(&self) -> bool {
        self.len == self.data.len()
    }
    
    /// Get capacity.
    pub fn capacity(&self) -> usize {
        self.data.len()
    }
    
    /// Clear the buffer.
    pub fn clear(&mut self) {
        for slot in &mut self.data {
            *slot = None;
        }
        self.head = 0;
        self.tail = 0;
        self.len = 0;
    }
}

/// A stack (LIFO).
pub struct Stack<T> {
    data: Vec<T>,
}

impl<T> Stack<T> {
    /// Create an empty stack.
    pub fn new() -> Self {
        Self { data: Vec::new() }
    }
    
    /// Create a stack with capacity.
    pub fn with_capacity(capacity: usize) -> Self {
        Self { data: Vec::with_capacity(capacity) }
    }
    
    /// Push a value.
    pub fn push(&mut self, value: T) {
        self.data.push(value);
    }
    
    /// Pop a value.
    pub fn pop(&mut self) -> Option<T> {
        self.data.pop()
    }
    
    /// Peek at the top.
    pub fn top(&self) -> Option<&T> {
        self.data.last()
    }
    
    /// Get length.
    pub fn len(&self) -> usize {
        self.data.len()
    }
    
    /// Check if empty.
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }
    
    /// Clear the stack.
    pub fn clear(&mut self) {
        self.data.clear();
    }
}

impl<T> Default for Stack<T> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_queue() {
        let mut q = Queue::new();
        q.enqueue(1);
        q.enqueue(2);
        q.enqueue(3);
        
        assert_eq!(q.dequeue(), Some(1));
        assert_eq!(q.dequeue(), Some(2));
        assert_eq!(q.dequeue(), Some(3));
        assert_eq!(q.dequeue(), None);
    }
    
    #[test]
    fn test_ring_buffer() {
        let mut rb = RingBuffer::new(3);
        
        assert_eq!(rb.push(1), None);
        assert_eq!(rb.push(2), None);
        assert_eq!(rb.push(3), None);
        assert_eq!(rb.push(4), Some(1)); // Evicts 1
        
        assert_eq!(rb.pop(), Some(2));
        assert_eq!(rb.pop(), Some(3));
        assert_eq!(rb.pop(), Some(4));
    }
    
    #[test]
    fn test_stack() {
        let mut s = Stack::new();
        s.push(1);
        s.push(2);
        s.push(3);
        
        assert_eq!(s.pop(), Some(3));
        assert_eq!(s.pop(), Some(2));
        assert_eq!(s.pop(), Some(1));
    }
}

