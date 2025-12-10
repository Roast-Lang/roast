//! Heap queue algorithm (priority queue).
//!
//! Python-compatible heapq module.

use std::cmp::Ordering;

// =============================================================================
// Heap Operations
// =============================================================================

/// Push an item onto the heap, maintaining heap invariant.
pub fn heappush<T: Ord>(heap: &mut Vec<T>, item: T) {
    heap.push(item);
    sift_up(heap, heap.len() - 1);
}

/// Pop the smallest item from the heap, maintaining heap invariant.
pub fn heappop<T: Ord>(heap: &mut Vec<T>) -> Option<T> {
    if heap.is_empty() {
        return None;
    }
    
    let last = heap.len() - 1;
    heap.swap(0, last);
    let result = heap.pop();
    
    if !heap.is_empty() {
        sift_down(heap, 0);
    }
    
    result
}

/// Pop the smallest item and push a new item, more efficient than separate operations.
pub fn heapreplace<T: Ord>(heap: &mut Vec<T>, item: T) -> Option<T> {
    if heap.is_empty() {
        heap.push(item);
        return None;
    }
    
    let result = std::mem::replace(&mut heap[0], item);
    sift_down(heap, 0);
    Some(result)
}

/// Push item on the heap, then pop and return the smallest item.
pub fn heappushpop<T: Ord>(heap: &mut Vec<T>, item: T) -> T {
    if heap.is_empty() || item <= heap[0] {
        return item;
    }
    
    let result = std::mem::replace(&mut heap[0], item);
    sift_down(heap, 0);
    result
}

/// Transform a list into a heap, in-place, in O(n) time.
pub fn heapify<T: Ord>(heap: &mut Vec<T>) {
    let n = heap.len();
    for i in (0..n / 2).rev() {
        sift_down(heap, i);
    }
}

/// Return the nth smallest element (0-indexed).
pub fn nsmallest<T: Ord + Clone>(n: usize, items: &[T]) -> Vec<T> {
    if n == 0 || items.is_empty() {
        return vec![];
    }
    
    if n >= items.len() {
        let mut result = items.to_vec();
        result.sort();
        return result;
    }
    
    // Use a max-heap of size n
    let mut heap: Vec<T> = items.iter().take(n).cloned().collect();
    
    // Build max-heap
    for i in (0..heap.len() / 2).rev() {
        sift_down_max(&mut heap, i);
    }
    
    // Process remaining items
    for item in items.iter().skip(n) {
        if *item < heap[0] {
            heap[0] = item.clone();
            sift_down_max(&mut heap, 0);
        }
    }
    
    // Sort the result
    heap.sort();
    heap
}

/// Return the nth largest element (0-indexed).
pub fn nlargest<T: Ord + Clone>(n: usize, items: &[T]) -> Vec<T> {
    if n == 0 || items.is_empty() {
        return vec![];
    }
    
    if n >= items.len() {
        let mut result = items.to_vec();
        result.sort_by(|a, b| b.cmp(a));
        return result;
    }
    
    // Use a min-heap of size n
    let mut heap: Vec<T> = items.iter().take(n).cloned().collect();
    heapify(&mut heap);
    
    // Process remaining items
    for item in items.iter().skip(n) {
        if *item > heap[0] {
            heap[0] = item.clone();
            sift_down(&mut heap, 0);
        }
    }
    
    // Sort the result in descending order
    heap.sort_by(|a, b| b.cmp(a));
    heap
}

/// Merge multiple sorted inputs into a single sorted output.
pub fn merge<T: Ord + Clone>(iterables: Vec<Vec<T>>) -> Vec<T> {
    if iterables.is_empty() {
        return vec![];
    }
    
    // Use a min-heap of (value, source_index, item_index)
    let mut heap: Vec<(T, usize, usize)> = Vec::new();
    
    // Initialize heap with first element from each source
    for (i, items) in iterables.iter().enumerate() {
        if !items.is_empty() {
            heap.push((items[0].clone(), i, 0));
        }
    }
    
    // Build heap
    for i in (0..heap.len() / 2).rev() {
        sift_down_tuple(&mut heap, i);
    }
    
    let mut result = Vec::new();
    
    while !heap.is_empty() {
        // Pop smallest
        let last = heap.len() - 1;
        heap.swap(0, last);
        let (value, source_idx, item_idx) = heap.pop().unwrap();
        result.push(value);
        
        // Push next element from same source
        let next_idx = item_idx + 1;
        if next_idx < iterables[source_idx].len() {
            heap.push((iterables[source_idx][next_idx].clone(), source_idx, next_idx));
            let last = heap.len() - 1;
            sift_up_tuple(&mut heap, last);
        }
        
        if !heap.is_empty() {
            sift_down_tuple(&mut heap, 0);
        }
    }
    
    result
}

// =============================================================================
// Internal Helpers
// =============================================================================

fn sift_up<T: Ord>(heap: &mut Vec<T>, mut pos: usize) {
    while pos > 0 {
        let parent = (pos - 1) / 2;
        if heap[pos] < heap[parent] {
            heap.swap(pos, parent);
            pos = parent;
        } else {
            break;
        }
    }
}

fn sift_down<T: Ord>(heap: &mut Vec<T>, mut pos: usize) {
    let n = heap.len();
    loop {
        let left = 2 * pos + 1;
        let right = 2 * pos + 2;
        let mut smallest = pos;
        
        if left < n && heap[left] < heap[smallest] {
            smallest = left;
        }
        if right < n && heap[right] < heap[smallest] {
            smallest = right;
        }
        
        if smallest != pos {
            heap.swap(pos, smallest);
            pos = smallest;
        } else {
            break;
        }
    }
}

fn sift_down_max<T: Ord>(heap: &mut Vec<T>, mut pos: usize) {
    let n = heap.len();
    loop {
        let left = 2 * pos + 1;
        let right = 2 * pos + 2;
        let mut largest = pos;
        
        if left < n && heap[left] > heap[largest] {
            largest = left;
        }
        if right < n && heap[right] > heap[largest] {
            largest = right;
        }
        
        if largest != pos {
            heap.swap(pos, largest);
            pos = largest;
        } else {
            break;
        }
    }
}

fn sift_up_tuple<T: Ord>(heap: &mut Vec<(T, usize, usize)>, mut pos: usize) {
    while pos > 0 {
        let parent = (pos - 1) / 2;
        if heap[pos].0 < heap[parent].0 {
            heap.swap(pos, parent);
            pos = parent;
        } else {
            break;
        }
    }
}

fn sift_down_tuple<T: Ord>(heap: &mut Vec<(T, usize, usize)>, mut pos: usize) {
    let n = heap.len();
    loop {
        let left = 2 * pos + 1;
        let right = 2 * pos + 2;
        let mut smallest = pos;
        
        if left < n && heap[left].0 < heap[smallest].0 {
            smallest = left;
        }
        if right < n && heap[right].0 < heap[smallest].0 {
            smallest = right;
        }
        
        if smallest != pos {
            heap.swap(pos, smallest);
            pos = smallest;
        } else {
            break;
        }
    }
}

// =============================================================================
// Priority Queue
// =============================================================================

/// A priority queue with update capability.
#[derive(Clone, Debug)]
pub struct PriorityQueue<T> {
    heap: Vec<(i64, T)>,
    counter: i64,
}

impl<T: Ord + Clone> PriorityQueue<T> {
    /// Create a new priority queue.
    pub fn new() -> Self {
        Self {
            heap: Vec::new(),
            counter: 0,
        }
    }
    
    /// Check if empty.
    pub fn is_empty(&self) -> bool {
        self.heap.is_empty()
    }
    
    /// Get length.
    pub fn len(&self) -> usize {
        self.heap.len()
    }
    
    /// Push with priority.
    pub fn push(&mut self, item: T, priority: i64) {
        self.heap.push((priority, item));
        self.sift_up(self.heap.len() - 1);
        self.counter += 1;
    }
    
    /// Pop item with lowest priority.
    pub fn pop(&mut self) -> Option<T> {
        if self.heap.is_empty() {
            return None;
        }
        
        let last = self.heap.len() - 1;
        self.heap.swap(0, last);
        let (_, item) = self.heap.pop().unwrap();
        
        if !self.heap.is_empty() {
            self.sift_down(0);
        }
        
        Some(item)
    }
    
    /// Peek at the minimum item.
    pub fn peek(&self) -> Option<&T> {
        self.heap.first().map(|(_, item)| item)
    }
    
    fn sift_up(&mut self, mut pos: usize) {
        while pos > 0 {
            let parent = (pos - 1) / 2;
            if self.heap[pos].0 < self.heap[parent].0 {
                self.heap.swap(pos, parent);
                pos = parent;
            } else {
                break;
            }
        }
    }
    
    fn sift_down(&mut self, mut pos: usize) {
        let n = self.heap.len();
        loop {
            let left = 2 * pos + 1;
            let right = 2 * pos + 2;
            let mut smallest = pos;
            
            if left < n && self.heap[left].0 < self.heap[smallest].0 {
                smallest = left;
            }
            if right < n && self.heap[right].0 < self.heap[smallest].0 {
                smallest = right;
            }
            
            if smallest != pos {
                self.heap.swap(pos, smallest);
                pos = smallest;
            } else {
                break;
            }
        }
    }
}

impl<T: Ord + Clone> Default for PriorityQueue<T> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_heappush_heappop() {
        let mut heap = vec![];
        heappush(&mut heap, 3);
        heappush(&mut heap, 1);
        heappush(&mut heap, 4);
        heappush(&mut heap, 1);
        heappush(&mut heap, 5);
        
        assert_eq!(heappop(&mut heap), Some(1));
        assert_eq!(heappop(&mut heap), Some(1));
        assert_eq!(heappop(&mut heap), Some(3));
        assert_eq!(heappop(&mut heap), Some(4));
        assert_eq!(heappop(&mut heap), Some(5));
        assert_eq!(heappop(&mut heap), None);
    }
    
    #[test]
    fn test_heapify() {
        let mut heap = vec![5, 3, 1, 4, 2];
        heapify(&mut heap);
        
        assert_eq!(heappop(&mut heap), Some(1));
        assert_eq!(heappop(&mut heap), Some(2));
    }
    
    #[test]
    fn test_nsmallest() {
        let items = vec![9, 4, 7, 1, 3, 8, 6, 2, 5];
        assert_eq!(nsmallest(3, &items), vec![1, 2, 3]);
        assert_eq!(nsmallest(1, &items), vec![1]);
    }
    
    #[test]
    fn test_nlargest() {
        let items = vec![9, 4, 7, 1, 3, 8, 6, 2, 5];
        assert_eq!(nlargest(3, &items), vec![9, 8, 7]);
        assert_eq!(nlargest(1, &items), vec![9]);
    }
    
    #[test]
    fn test_merge() {
        let a = vec![1, 3, 5, 7];
        let b = vec![2, 4, 6, 8];
        let c = vec![0, 9];
        
        let merged = merge(vec![a, b, c]);
        assert_eq!(merged, vec![0, 1, 2, 3, 4, 5, 6, 7, 8, 9]);
    }
    
    #[test]
    fn test_heapreplace() {
        let mut heap = vec![1, 3, 5];
        heapify(&mut heap);
        
        assert_eq!(heapreplace(&mut heap, 2), Some(1));
        assert_eq!(heappop(&mut heap), Some(2));
    }
    
    #[test]
    fn test_heappushpop() {
        let mut heap = vec![2, 4, 6];
        heapify(&mut heap);
        
        // Push smaller, get it back
        assert_eq!(heappushpop(&mut heap, 1), 1);
        
        // Push larger, get smallest from heap
        assert_eq!(heappushpop(&mut heap, 3), 2);
    }
    
    #[test]
    fn test_priority_queue() {
        let mut pq = PriorityQueue::new();
        pq.push("task1", 3);
        pq.push("task2", 1);
        pq.push("task3", 2);
        
        assert_eq!(pq.pop(), Some("task2"));
        assert_eq!(pq.pop(), Some("task3"));
        assert_eq!(pq.pop(), Some("task1"));
    }
}

