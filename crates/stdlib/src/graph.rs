//! Graph data structures and algorithms.

use std::collections::{HashMap, HashSet, VecDeque, BinaryHeap};
use std::cmp::Ordering;

/// A directed graph with weighted edges.
pub struct Graph<V, E> {
    nodes: HashMap<usize, V>,
    edges: HashMap<usize, Vec<(usize, E)>>,
    next_id: usize,
}

impl<V, E> Graph<V, E> {
    /// Create an empty graph.
    pub fn new() -> Self {
        Self {
            nodes: HashMap::new(),
            edges: HashMap::new(),
            next_id: 0,
        }
    }
    
    /// Add a node and return its ID.
    pub fn add_node(&mut self, value: V) -> usize {
        let id = self.next_id;
        self.next_id += 1;
        self.nodes.insert(id, value);
        self.edges.insert(id, Vec::new());
        id
    }
    
    /// Add an edge between two nodes.
    pub fn add_edge(&mut self, from: usize, to: usize, weight: E) {
        if let Some(edges) = self.edges.get_mut(&from) {
            edges.push((to, weight));
        }
    }
    
    /// Get a node's value.
    pub fn get_node(&self, id: usize) -> Option<&V> {
        self.nodes.get(&id)
    }
    
    /// Get edges from a node.
    pub fn get_edges(&self, id: usize) -> Option<&[(usize, E)]> {
        self.edges.get(&id).map(|e| e.as_slice())
    }
    
    /// Get number of nodes.
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }
    
    /// Get all node IDs.
    pub fn node_ids(&self) -> impl Iterator<Item = usize> + '_ {
        self.nodes.keys().copied()
    }
    
    /// Remove a node.
    pub fn remove_node(&mut self, id: usize) -> Option<V> {
        self.edges.remove(&id);
        // Remove edges pointing to this node
        for edges in self.edges.values_mut() {
            edges.retain(|(to, _)| *to != id);
        }
        self.nodes.remove(&id)
    }
}

impl<V, E> Default for Graph<V, E> {
    fn default() -> Self {
        Self::new()
    }
}

impl<V, E: Clone> Graph<V, E> {
    /// Breadth-first search.
    pub fn bfs(&self, start: usize) -> Vec<usize> {
        let mut visited = HashSet::new();
        let mut queue = VecDeque::new();
        let mut order = Vec::new();
        
        queue.push_back(start);
        visited.insert(start);
        
        while let Some(node) = queue.pop_front() {
            order.push(node);
            
            if let Some(edges) = self.edges.get(&node) {
                for (neighbor, _) in edges {
                    if visited.insert(*neighbor) {
                        queue.push_back(*neighbor);
                    }
                }
            }
        }
        
        order
    }
    
    /// Depth-first search.
    pub fn dfs(&self, start: usize) -> Vec<usize> {
        let mut visited = HashSet::new();
        let mut order = Vec::new();
        
        self.dfs_recursive(start, &mut visited, &mut order);
        
        order
    }
    
    fn dfs_recursive(&self, node: usize, visited: &mut HashSet<usize>, order: &mut Vec<usize>) {
        if !visited.insert(node) {
            return;
        }
        
        order.push(node);
        
        if let Some(edges) = self.edges.get(&node) {
            for (neighbor, _) in edges {
                self.dfs_recursive(*neighbor, visited, order);
            }
        }
    }
    
    /// Check if path exists between two nodes.
    pub fn has_path(&self, from: usize, to: usize) -> bool {
        let visited = self.bfs(from);
        visited.contains(&to)
    }
}

impl<V> Graph<V, i64> {
    /// Dijkstra's shortest path algorithm.
    pub fn dijkstra(&self, start: usize) -> HashMap<usize, i64> {
        let mut distances: HashMap<usize, i64> = HashMap::new();
        let mut heap = BinaryHeap::new();
        
        distances.insert(start, 0);
        heap.push(DijkstraNode { cost: 0, node: start });
        
        while let Some(DijkstraNode { cost, node }) = heap.pop() {
            if let Some(&d) = distances.get(&node) {
                if cost > d {
                    continue;
                }
            }
            
            if let Some(edges) = self.edges.get(&node) {
                for (neighbor, weight) in edges {
                    let new_cost = cost + weight;
                    
                    let is_shorter = distances.get(neighbor)
                        .map_or(true, |&d| new_cost < d);
                    
                    if is_shorter {
                        distances.insert(*neighbor, new_cost);
                        heap.push(DijkstraNode { cost: new_cost, node: *neighbor });
                    }
                }
            }
        }
        
        distances
    }
    
    /// Find shortest path.
    pub fn shortest_path(&self, from: usize, to: usize) -> Option<(Vec<usize>, i64)> {
        let mut distances: HashMap<usize, i64> = HashMap::new();
        let mut previous: HashMap<usize, usize> = HashMap::new();
        let mut heap = BinaryHeap::new();
        
        distances.insert(from, 0);
        heap.push(DijkstraNode { cost: 0, node: from });
        
        while let Some(DijkstraNode { cost, node }) = heap.pop() {
            if node == to {
                // Reconstruct path
                let mut path = vec![to];
                let mut current = to;
                while let Some(&prev) = previous.get(&current) {
                    path.push(prev);
                    current = prev;
                }
                path.reverse();
                return Some((path, cost));
            }
            
            if let Some(&d) = distances.get(&node) {
                if cost > d {
                    continue;
                }
            }
            
            if let Some(edges) = self.edges.get(&node) {
                for (neighbor, weight) in edges {
                    let new_cost = cost + weight;
                    
                    let is_shorter = distances.get(neighbor)
                        .map_or(true, |&d| new_cost < d);
                    
                    if is_shorter {
                        distances.insert(*neighbor, new_cost);
                        previous.insert(*neighbor, node);
                        heap.push(DijkstraNode { cost: new_cost, node: *neighbor });
                    }
                }
            }
        }
        
        None
    }
}

#[derive(Eq, PartialEq)]
struct DijkstraNode {
    cost: i64,
    node: usize,
}

impl Ord for DijkstraNode {
    fn cmp(&self, other: &Self) -> Ordering {
        other.cost.cmp(&self.cost) // Reverse for min-heap
    }
}

impl PartialOrd for DijkstraNode {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// Union-Find (Disjoint Set Union) data structure.
pub struct UnionFind {
    parent: Vec<usize>,
    rank: Vec<usize>,
}

impl UnionFind {
    /// Create with n elements.
    pub fn new(n: usize) -> Self {
        Self {
            parent: (0..n).collect(),
            rank: vec![0; n],
        }
    }
    
    /// Find the root of an element.
    pub fn find(&mut self, x: usize) -> usize {
        if self.parent[x] != x {
            self.parent[x] = self.find(self.parent[x]); // Path compression
        }
        self.parent[x]
    }
    
    /// Union two sets.
    pub fn union(&mut self, x: usize, y: usize) {
        let root_x = self.find(x);
        let root_y = self.find(y);
        
        if root_x != root_y {
            // Union by rank
            match self.rank[root_x].cmp(&self.rank[root_y]) {
                Ordering::Less => self.parent[root_x] = root_y,
                Ordering::Greater => self.parent[root_y] = root_x,
                Ordering::Equal => {
                    self.parent[root_y] = root_x;
                    self.rank[root_x] += 1;
                }
            }
        }
    }
    
    /// Check if two elements are in the same set.
    pub fn connected(&mut self, x: usize, y: usize) -> bool {
        self.find(x) == self.find(y)
    }
}

/// Topological sort.
pub fn topological_sort<V: Clone, E: Clone>(graph: &Graph<V, E>) -> Option<Vec<usize>> {
    let mut in_degree: HashMap<usize, usize> = HashMap::new();
    
    // Initialize in-degrees
    for id in graph.node_ids() {
        in_degree.entry(id).or_insert(0);
        if let Some(edges) = graph.get_edges(id) {
            for (to, _) in edges {
                *in_degree.entry(*to).or_insert(0) += 1;
            }
        }
    }
    
    // Find nodes with no incoming edges
    let mut queue: VecDeque<usize> = in_degree.iter()
        .filter(|(_, &d)| d == 0)
        .map(|(&id, _)| id)
        .collect();
    
    let mut result = Vec::new();
    
    while let Some(node) = queue.pop_front() {
        result.push(node);
        
        if let Some(edges) = graph.get_edges(node) {
            for (neighbor, _) in edges {
                if let Some(d) = in_degree.get_mut(neighbor) {
                    *d -= 1;
                    if *d == 0 {
                        queue.push_back(*neighbor);
                    }
                }
            }
        }
    }
    
    if result.len() == graph.node_count() {
        Some(result)
    } else {
        None // Cycle detected
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_graph() {
        let mut graph = Graph::new();
        let a = graph.add_node("A");
        let b = graph.add_node("B");
        let c = graph.add_node("C");
        
        graph.add_edge(a, b, 1);
        graph.add_edge(b, c, 2);
        graph.add_edge(a, c, 4);
        
        assert!(graph.has_path(a, c));
    }
    
    #[test]
    fn test_dijkstra() {
        let mut graph: Graph<&str, i64> = Graph::new();
        let a = graph.add_node("A");
        let b = graph.add_node("B");
        let c = graph.add_node("C");
        
        graph.add_edge(a, b, 1);
        graph.add_edge(b, c, 2);
        graph.add_edge(a, c, 10);
        
        let (path, cost) = graph.shortest_path(a, c).unwrap();
        assert_eq!(cost, 3);
        assert_eq!(path, vec![a, b, c]);
    }
    
    #[test]
    fn test_union_find() {
        let mut uf = UnionFind::new(5);
        
        uf.union(0, 1);
        uf.union(2, 3);
        
        assert!(uf.connected(0, 1));
        assert!(uf.connected(2, 3));
        assert!(!uf.connected(0, 2));
        
        uf.union(1, 2);
        assert!(uf.connected(0, 3));
    }
}

