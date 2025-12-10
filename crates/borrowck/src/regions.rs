//! Region inference for lifetimes.
//!
//! Regions represent the liveness scope of references.

use crate::lifetime::LifetimeId;
use crate::loans::Location;
use bitflags::bitflags;
use petgraph::graph::{DiGraph, NodeIndex};
use rustc_hash::FxHashMap;
use std::fmt;

/// Unique identifier for a region.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct RegionId(pub u32);

impl RegionId {
    pub const STATIC: RegionId = RegionId(0);

    pub fn new(id: u32) -> Self {
        Self(id)
    }

    pub fn is_static(&self) -> bool {
        *self == Self::STATIC
    }
}

impl fmt::Display for RegionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.is_static() {
            write!(f, "'static")
        } else {
            write!(f, "'r{}", self.0)
        }
    }
}

/// A region represents the liveness scope of a reference.
#[derive(Clone, Debug)]
pub struct Region {
    pub id: RegionId,
    /// The points where this region is live.
    pub points: Vec<Location>,
    /// Regions that this region outlives.
    pub outlives: Vec<RegionId>,
    /// The origin of this region (what created it).
    pub origin: RegionOrigin,
}

impl Region {
    pub fn new(id: RegionId, origin: RegionOrigin) -> Self {
        Self {
            id,
            points: Vec::new(),
            outlives: Vec::new(),
            origin,
        }
    }

    pub fn static_region() -> Self {
        Self {
            id: RegionId::STATIC,
            points: Vec::new(),
            outlives: Vec::new(),
            origin: RegionOrigin::Static,
        }
    }

    /// Adds a point to this region.
    pub fn add_point(&mut self, location: Location) {
        if !self.points.contains(&location) {
            self.points.push(location);
        }
    }

    /// Adds an outlives constraint.
    pub fn add_outlives(&mut self, other: RegionId) {
        if !self.outlives.contains(&other) {
            self.outlives.push(other);
        }
    }

    /// Checks if this region contains a point.
    pub fn contains(&self, location: Location) -> bool {
        self.id.is_static() || self.points.contains(&location)
    }
}

/// The origin of a region.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RegionOrigin {
    /// The static region.
    Static,
    /// A user-specified lifetime parameter.
    UserLifetime(LifetimeId),
    /// An anonymous lifetime from a reference.
    Anonymous(Location),
    /// An inferred lifetime from control flow.
    Inferred,
}

/// Region constraint kinds.
#[derive(Clone, Debug)]
pub enum RegionConstraint {
    /// Region 'a outlives region 'b.
    Outlives {
        sup: RegionId,
        sub: RegionId,
        origin: ConstraintOrigin,
    },
    /// Region 'a must contain location.
    LiveAt {
        region: RegionId,
        location: Location,
    },
    /// Region 'a must equal region 'b.
    Equal {
        a: RegionId,
        b: RegionId,
        origin: ConstraintOrigin,
    },
}

/// Origin of a constraint (for error reporting).
#[derive(Clone, Debug)]
pub struct ConstraintOrigin {
    pub kind: ConstraintKind,
    pub span: roast_common::Span,
}

/// Kind of constraint.
#[derive(Clone, Copy, Debug)]
pub enum ConstraintKind {
    /// From a borrow expression.
    Borrow,
    /// From a reborrow.
    Reborrow,
    /// From function argument.
    Argument,
    /// From function return.
    Return,
    /// From assignment.
    Assignment,
    /// From calling a function.
    Call,
    /// From a type annotation.
    Annotation,
}

/// Region inference context.
pub struct RegionInference {
    /// All regions.
    regions: Vec<Region>,
    /// Constraints to solve.
    constraints: Vec<RegionConstraint>,
    /// Graph of region outlives relationships.
    outlives_graph: DiGraph<RegionId, ()>,
    /// Map from region ID to graph node.
    region_nodes: FxHashMap<RegionId, NodeIndex>,
    /// Next region ID.
    next_id: u32,
}

impl RegionInference {
    pub fn new() -> Self {
        let mut ri = Self {
            regions: Vec::new(),
            constraints: Vec::new(),
            outlives_graph: DiGraph::new(),
            region_nodes: FxHashMap::default(),
            next_id: 1,
        };
        // Add static region
        let static_region = Region::static_region();
        let node = ri.outlives_graph.add_node(RegionId::STATIC);
        ri.region_nodes.insert(RegionId::STATIC, node);
        ri.regions.push(static_region);
        ri
    }

    /// Creates a new region.
    pub fn new_region(&mut self, origin: RegionOrigin) -> RegionId {
        let id = RegionId::new(self.next_id);
        self.next_id += 1;
        let region = Region::new(id, origin);
        self.regions.push(region);
        let node = self.outlives_graph.add_node(id);
        self.region_nodes.insert(id, node);
        id
    }

    /// Creates a new anonymous region.
    pub fn new_anonymous(&mut self, location: Location) -> RegionId {
        self.new_region(RegionOrigin::Anonymous(location))
    }

    /// Creates a new inferred region.
    pub fn new_inferred(&mut self) -> RegionId {
        self.new_region(RegionOrigin::Inferred)
    }

    /// Gets a region by ID.
    pub fn get(&self, id: RegionId) -> Option<&Region> {
        self.regions.iter().find(|r| r.id == id)
    }

    /// Gets a mutable region by ID.
    pub fn get_mut(&mut self, id: RegionId) -> Option<&mut Region> {
        self.regions.iter_mut().find(|r| r.id == id)
    }

    /// Adds an outlives constraint: sup outlives sub.
    pub fn add_outlives(&mut self, sup: RegionId, sub: RegionId, origin: ConstraintOrigin) {
        self.constraints.push(RegionConstraint::Outlives { sup, sub, origin });

        // Add to graph
        let sup_node = self.region_nodes[&sup];
        let sub_node = self.region_nodes[&sub];
        self.outlives_graph.add_edge(sup_node, sub_node, ());
    }

    /// Adds a liveness constraint.
    pub fn add_live_at(&mut self, region: RegionId, location: Location) {
        self.constraints.push(RegionConstraint::LiveAt { region, location });
        if let Some(r) = self.get_mut(region) {
            r.add_point(location);
        }
    }

    /// Adds an equality constraint.
    pub fn add_equal(&mut self, a: RegionId, b: RegionId, origin: ConstraintOrigin) {
        self.constraints.push(RegionConstraint::Equal { a, b, origin });
    }

    /// Solves the region constraints.
    pub fn solve(&mut self) -> Result<(), RegionError> {
        // Propagate constraints until fixpoint
        let mut changed = true;
        let max_iterations = 1000;
        let mut iterations = 0;

        while changed && iterations < max_iterations {
            changed = false;
            iterations += 1;

            for constraint in self.constraints.clone() {
                match constraint {
                    RegionConstraint::Outlives { sup, sub, .. } => {
                        // sup outlives sub means sup contains all points in sub
                        if let (Some(sup_idx), Some(sub_idx)) = (
                            self.regions.iter().position(|r| r.id == sup),
                            self.regions.iter().position(|r| r.id == sub),
                        ) {
                            let sub_points = self.regions[sub_idx].points.clone();
                            for point in sub_points {
                                if !self.regions[sup_idx].points.contains(&point) {
                                    self.regions[sup_idx].points.push(point);
                                    changed = true;
                                }
                            }
                        }
                    }
                    RegionConstraint::LiveAt { region, location } => {
                        if let Some(r) = self.get_mut(region) {
                            if !r.points.contains(&location) {
                                r.add_point(location);
                                changed = true;
                            }
                        }
                    }
                    RegionConstraint::Equal { a, b, .. } => {
                        // Equal regions share all points
                        if let (Some(a_idx), Some(b_idx)) = (
                            self.regions.iter().position(|r| r.id == a),
                            self.regions.iter().position(|r| r.id == b),
                        ) {
                            let a_points = self.regions[a_idx].points.clone();
                            let b_points = self.regions[b_idx].points.clone();

                            for point in &b_points {
                                if !self.regions[a_idx].points.contains(point) {
                                    self.regions[a_idx].points.push(*point);
                                    changed = true;
                                }
                            }
                            for point in &a_points {
                                if !self.regions[b_idx].points.contains(point) {
                                    self.regions[b_idx].points.push(*point);
                                    changed = true;
                                }
                            }
                        }
                    }
                }
            }
        }

        if iterations >= max_iterations {
            return Err(RegionError::FixpointNotReached);
        }

        Ok(())
    }

    /// Checks if region 'a outlives region 'b.
    pub fn outlives(&self, a: RegionId, b: RegionId) -> bool {
        if a.is_static() {
            return true;
        }
        if a == b {
            return true;
        }

        // Check if there's a path from a to b in the outlives graph
        if let (Some(&a_node), Some(&b_node)) = (self.region_nodes.get(&a), self.region_nodes.get(&b)) {
            petgraph::algo::has_path_connecting(&self.outlives_graph, a_node, b_node, None)
        } else {
            false
        }
    }

    /// Returns all regions.
    pub fn regions(&self) -> &[Region] {
        &self.regions
    }

    /// Returns all constraints.
    pub fn constraints(&self) -> &[RegionConstraint] {
        &self.constraints
    }
}

impl Default for RegionInference {
    fn default() -> Self {
        Self::new()
    }
}

/// Region inference error.
#[derive(Clone, Debug)]
pub enum RegionError {
    /// Constraint solving did not reach fixpoint.
    FixpointNotReached,
    /// Region 'a does not outlive region 'b.
    DoesNotOutlive {
        sup: RegionId,
        sub: RegionId,
        origin: ConstraintOrigin,
    },
    /// Conflicting region constraints.
    Conflict {
        region: RegionId,
        constraints: Vec<RegionConstraint>,
    },
}

impl fmt::Display for RegionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RegionError::FixpointNotReached => {
                write!(f, "region inference did not converge")
            }
            RegionError::DoesNotOutlive { sup, sub, .. } => {
                write!(f, "region {} does not outlive {}", sup, sub)
            }
            RegionError::Conflict { region, .. } => {
                write!(f, "conflicting constraints for region {}", region)
            }
        }
    }
}

