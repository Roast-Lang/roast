//! Heap data structures.
//!
//! Binary heap (priority queue) implementations.

use std::cmp::Ordering;

/// A binary max-heap.
pub struct MaxHeap<T> {
    data: Vec<T>,
}

impl<T: Ord> MaxHeap<T> {
    /// Create an empty heap.
    pub fn new() -> Self {
        Self { data: Vec::new() }
    }
    
    /// Create a heap with capacity.
    pub fn with_capacity(capacity: usize) -> Self {
        Self { data: Vec::with_capacity(capacity) }
    }
    
    /// Create a heap from a vector.
    pub fn from_vec(mut vec: Vec<T>) -> Self {
        let len = vec.len();
        let mut heap = Self { data: vec };
        
        // Heapify
        for i in (0..len / 2).rev() {
            heap.sift_down(i);
        }
        
        heap
    }
    
    /// Push a value onto the heap.
    pub fn push(&mut self, value: T) {
        self.data.push(value);
        self.sift_up(self.data.len() - 1);
    }
    
    /// Pop the maximum value.
    pub fn pop(&mut self) -> Option<T> {
        if self.data.is_empty() {
            return None;
        }
        
        let last = self.data.len() - 1;
        self.data.swap(0, last);
        let max = self.data.pop();
        
        if !self.data.is_empty() {
            self.sift_down(0);
        }
        
        max
    }
    
    /// Peek at the maximum value.
    pub fn peek(&self) -> Option<&T> {
        self.data.first()
    }
    
    /// Get the length.
    pub fn len(&self) -> usize {
        self.data.len()
    }
    
    /// Check if empty.
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }
    
    /// Clear the heap.
    pub fn clear(&mut self) {
        self.data.clear();
    }
    
    fn sift_up(&mut self, mut idx: usize) {
        while idx > 0 {
            let parent = (idx - 1) / 2;
            if self.data[idx] <= self.data[parent] {
                break;
            }
            self.data.swap(idx, parent);
            idx = parent;
        }
    }
    
    fn sift_down(&mut self, mut idx: usize) {
        let len = self.data.len();
        
        loop {
            let left = 2 * idx + 1;
            let right = 2 * idx + 2;
            let mut largest = idx;
            
            if left < len && self.data[left] > self.data[largest] {
                largest = left;
            }
            if right < len && self.data[right] > self.data[largest] {
                largest = right;
            }
            
            if largest == idx {
                break;
            }
            
            self.data.swap(idx, largest);
            idx = largest;
        }
    }
}

impl<T: Ord> Default for MaxHeap<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: Ord> FromIterator<T> for MaxHeap<T> {
    fn from_iter<I: IntoIterator<Item = T>>(iter: I) -> Self {
        Self::from_vec(iter.into_iter().collect())
    }
}

/// A binary min-heap.
pub struct MinHeap<T> {
    data: Vec<T>,
}

impl<T: Ord> MinHeap<T> {
    /// Create an empty heap.
    pub fn new() -> Self {
        Self { data: Vec::new() }
    }
    
    /// Create a heap with capacity.
    pub fn with_capacity(capacity: usize) -> Self {
        Self { data: Vec::with_capacity(capacity) }
    }
    
    /// Create a heap from a vector.
    pub fn from_vec(mut vec: Vec<T>) -> Self {
        let len = vec.len();
        let mut heap = Self { data: vec };
        
        for i in (0..len / 2).rev() {
            heap.sift_down(i);
        }
        
        heap
    }
    
    /// Push a value onto the heap.
    pub fn push(&mut self, value: T) {
        self.data.push(value);
        self.sift_up(self.data.len() - 1);
    }
    
    /// Pop the minimum value.
    pub fn pop(&mut self) -> Option<T> {
        if self.data.is_empty() {
            return None;
        }
        
        let last = self.data.len() - 1;
        self.data.swap(0, last);
        let min = self.data.pop();
        
        if !self.data.is_empty() {
            self.sift_down(0);
        }
        
        min
    }
    
    /// Peek at the minimum value.
    pub fn peek(&self) -> Option<&T> {
        self.data.first()
    }
    
    /// Get the length.
    pub fn len(&self) -> usize {
        self.data.len()
    }
    
    /// Check if empty.
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }
    
    /// Clear the heap.
    pub fn clear(&mut self) {
        self.data.clear();
    }
    
    fn sift_up(&mut self, mut idx: usize) {
        while idx > 0 {
            let parent = (idx - 1) / 2;
            if self.data[idx] >= self.data[parent] {
                break;
            }
            self.data.swap(idx, parent);
            idx = parent;
        }
    }
    
    fn sift_down(&mut self, mut idx: usize) {
        let len = self.data.len();
        
        loop {
            let left = 2 * idx + 1;
            let right = 2 * idx + 2;
            let mut smallest = idx;
            
            if left < len && self.data[left] < self.data[smallest] {
                smallest = left;
            }
            if right < len && self.data[right] < self.data[smallest] {
                smallest = right;
            }
            
            if smallest == idx {
                break;
            }
            
            self.data.swap(idx, smallest);
            idx = smallest;
        }
    }
}

impl<T: Ord> Default for MinHeap<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: Ord> FromIterator<T> for MinHeap<T> {
    fn from_iter<I: IntoIterator<Item = T>>(iter: I) -> Self {
        Self::from_vec(iter.into_iter().collect())
    }
}

/// Priority queue with custom priority.
pub struct PriorityQueue<T, P> {
    data: Vec<(T, P)>,
}

impl<T, P: Ord> PriorityQueue<T, P> {
    /// Create an empty priority queue.
    pub fn new() -> Self {
        Self { data: Vec::new() }
    }
    
    /// Push with priority (higher priority = dequeued first).
    pub fn push(&mut self, value: T, priority: P) {
        self.data.push((value, priority));
        self.sift_up(self.data.len() - 1);
    }
    
    /// Pop the highest priority item.
    pub fn pop(&mut self) -> Option<(T, P)> {
        if self.data.is_empty() {
            return None;
        }
        
        let last = self.data.len() - 1;
        self.data.swap(0, last);
        let item = self.data.pop();
        
        if !self.data.is_empty() {
            self.sift_down(0);
        }
        
        item
    }
    
    /// Peek at the highest priority item.
    pub fn peek(&self) -> Option<(&T, &P)> {
        self.data.first().map(|(t, p)| (t, p))
    }
    
    /// Get length.
    pub fn len(&self) -> usize {
        self.data.len()
    }
    
    /// Check if empty.
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }
    
    fn sift_up(&mut self, mut idx: usize) {
        while idx > 0 {
            let parent = (idx - 1) / 2;
            if self.data[idx].1 <= self.data[parent].1 {
                break;
            }
            self.data.swap(idx, parent);
            idx = parent;
        }
    }
    
    fn sift_down(&mut self, mut idx: usize) {
        let len = self.data.len();
        
        loop {
            let left = 2 * idx + 1;
            let right = 2 * idx + 2;
            let mut largest = idx;
            
            if left < len && self.data[left].1 > self.data[largest].1 {
                largest = left;
            }
            if right < len && self.data[right].1 > self.data[largest].1 {
                largest = right;
            }
            
            if largest == idx {
                break;
            }
            
            self.data.swap(idx, largest);
            idx = largest;
        }
    }
}

impl<T, P: Ord> Default for PriorityQueue<T, P> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_max_heap() {
        let mut heap = MaxHeap::new();
        heap.push(3);
        heap.push(1);
        heap.push(4);
        heap.push(1);
        heap.push(5);
        
        assert_eq!(heap.pop(), Some(5));
        assert_eq!(heap.pop(), Some(4));
        assert_eq!(heap.pop(), Some(3));
        assert_eq!(heap.pop(), Some(1));
        assert_eq!(heap.pop(), Some(1));
        assert_eq!(heap.pop(), None);
    }
    
    #[test]
    fn test_min_heap() {
        let mut heap = MinHeap::new();
        heap.push(3);
        heap.push(1);
        heap.push(4);
        heap.push(1);
        heap.push(5);
        
        assert_eq!(heap.pop(), Some(1));
        assert_eq!(heap.pop(), Some(1));
        assert_eq!(heap.pop(), Some(3));
    }
    
    #[test]
    fn test_priority_queue() {
        let mut pq = PriorityQueue::new();
        pq.push("low", 1);
        pq.push("high", 10);
        pq.push("medium", 5);
        
        assert_eq!(pq.pop().map(|(v, _)| v), Some("high"));
        assert_eq!(pq.pop().map(|(v, _)| v), Some("medium"));
        assert_eq!(pq.pop().map(|(v, _)| v), Some("low"));
    }
}

