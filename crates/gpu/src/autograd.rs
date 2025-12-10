//! Automatic Differentiation (Autograd) for GPU tensors.
//!
//! This module provides automatic differentiation capabilities
//! for computing gradients of tensor operations.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, RwLock, Weak};
use std::cell::RefCell;
use crate::tensor::{Tensor, Shape, DType};
use crate::device::Device;
use crate::error::{GpuError, GpuResult};

// =============================================================================
// Gradient Function
// =============================================================================

/// Gradient function type.
pub type GradFn = Arc<dyn Fn(&[&Tensor], &Tensor) -> GpuResult<Vec<Tensor>> + Send + Sync>;

/// Backward operation for a node.
pub struct BackwardFn {
    /// Gradient function.
    pub grad_fn: GradFn,
    
    /// Operation name.
    pub name: String,
    
    /// Saved tensors for backward.
    pub saved_tensors: Vec<Tensor>,
    
    /// Input nodes.
    pub inputs: Vec<Weak<RwLock<GradNode>>>,
}

impl std::fmt::Debug for BackwardFn {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BackwardFn")
            .field("name", &self.name)
            .field("saved_tensors", &self.saved_tensors.len())
            .finish()
    }
}

// =============================================================================
// Gradient Node (Computation Graph)
// =============================================================================

/// A node in the computation graph.
#[derive(Debug)]
pub struct GradNode {
    /// Unique ID.
    pub id: usize,
    
    /// Accumulated gradient.
    pub grad: Option<Tensor>,
    
    /// Backward function.
    pub backward_fn: Option<BackwardFn>,
    
    /// Requires gradient.
    pub requires_grad: bool,
    
    /// Is leaf (user-created tensor).
    pub is_leaf: bool,
    
    /// Retain graph after backward.
    pub retain_grad: bool,
}

impl GradNode {
    /// Create a new gradient node.
    pub fn new(id: usize, requires_grad: bool, is_leaf: bool) -> Self {
        Self {
            id,
            grad: None,
            backward_fn: None,
            requires_grad,
            is_leaf,
            retain_grad: false,
        }
    }
    
    /// Set backward function.
    pub fn set_backward(&mut self, backward: BackwardFn) {
        self.backward_fn = Some(backward);
    }
}

// =============================================================================
// Variable (Tensor with Gradient)
// =============================================================================

static NEXT_NODE_ID: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(1);

fn next_node_id() -> usize {
    NEXT_NODE_ID.fetch_add(1, std::sync::atomic::Ordering::SeqCst)
}

/// A tensor that tracks gradients.
pub struct Variable {
    /// Underlying tensor data.
    pub data: Tensor,
    
    /// Gradient node.
    pub node: Arc<RwLock<GradNode>>,
}

impl Variable {
    /// Create a new variable.
    pub fn new(data: Tensor, requires_grad: bool) -> Self {
        let id = next_node_id();
        let node = Arc::new(RwLock::new(GradNode::new(id, requires_grad, true)));
        
        Self { data, node }
    }
    
    /// Create a variable that doesn't require gradients.
    pub fn no_grad(data: Tensor) -> Self {
        Self::new(data, false)
    }
    
    /// Create a variable that requires gradients.
    pub fn requires_grad(data: Tensor) -> Self {
        Self::new(data, true)
    }
    
    /// Get the tensor data.
    pub fn data(&self) -> &Tensor {
        &self.data
    }
    
    /// Get the gradient.
    pub fn grad(&self) -> Option<Tensor> {
        let node = self.node.read().unwrap();
        node.grad.as_ref().and_then(|g| g.clone_tensor().ok())
    }
    
    /// Check if requires gradient.
    pub fn requires_grad_flag(&self) -> bool {
        let node = self.node.read().unwrap();
        node.requires_grad
    }
    
    /// Check if is leaf.
    pub fn is_leaf(&self) -> bool {
        let node = self.node.read().unwrap();
        node.is_leaf
    }
    
    /// Zero the gradient.
    pub fn zero_grad(&self) {
        let mut node = self.node.write().unwrap();
        node.grad = None;
    }
    
    /// Retain gradient (for non-leaf nodes).
    pub fn retain_grad(&self) {
        let mut node = self.node.write().unwrap();
        node.retain_grad = true;
    }
    
    /// Detach from computation graph.
    pub fn detach(&self) -> Variable {
        Variable::no_grad(self.data.clone_tensor().unwrap())
    }
    
    /// Backward pass to compute gradients.
    pub fn backward(&self) -> GpuResult<()> {
        // Create initial gradient (ones with same shape)
        let grad = Tensor::full(
            self.data.shape().clone(),
            1.0,
            self.data.dtype(),
            self.data.device(),
        )?;
        
        self.backward_with_grad(&grad)
    }
    
    /// Backward pass with custom gradient.
    pub fn backward_with_grad(&self, grad: &Tensor) -> GpuResult<()> {
        // Topological sort
        let order = topological_sort(&self.node);
        
        // Initialize gradient for this node
        {
            let mut node = self.node.write().unwrap();
            node.grad = Some(grad.clone_tensor()?);
        }
        
        // Backward through each node
        for node_ref in order {
        let (backward_fn, current_grad) = {
            let node = node_ref.read().unwrap();
            
            if !node.requires_grad {
                continue;
            }
            
            let grad = match &node.grad {
                Some(g) => g.clone_tensor()?,
                None => continue,
            };
            
            match &node.backward_fn {
                Some(bf) => {
                    let saved: Vec<Tensor> = bf.saved_tensors.iter()
                        .filter_map(|t| t.clone_tensor().ok())
                        .collect();
                    let inputs: Vec<_> = bf.inputs.clone();
                    (Some((bf.grad_fn.clone(), saved, inputs)), grad)
                }
                None => (None, grad),
            }
        };
            
        if let Some((grad_fn, saved_tensors, inputs)) = backward_fn {
            let saved_refs: Vec<&Tensor> = saved_tensors.iter().collect();
            let input_grads = (*grad_fn)(&saved_refs, &current_grad)?;
                
                // Accumulate gradients to inputs
                for (input_weak, input_grad) in inputs.iter().zip(input_grads.iter()) {
                    if let Some(input_node) = input_weak.upgrade() {
                        let mut input = input_node.write().unwrap();
                        
                        if !input.requires_grad {
                            continue;
                        }
                        
                        if input.is_leaf || input.retain_grad {
                            match &mut input.grad {
                                Some(existing) => {
                                    // Accumulate gradient
                                    let acc = crate::ops::add(existing, input_grad)?;
                                    *existing = acc;
                                }
                                None => {
                                    input.grad = Some(input_grad.clone_tensor()?);
                                }
                            }
                        } else {
                            // Non-leaf: just set (will be used in next backward step)
                            input.grad = Some(input_grad.clone_tensor()?);
                        }
                    }
                }
            }
        }
        
        Ok(())
    }
}

impl Clone for Variable {
    fn clone(&self) -> Self {
        Self {
            data: self.data.clone_tensor().unwrap(),
            node: self.node.clone(),
        }
    }
}

impl std::fmt::Debug for Variable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Variable")
            .field("data", &self.data)
            .field("requires_grad", &self.requires_grad_flag())
            .field("is_leaf", &self.is_leaf())
            .finish()
    }
}

// =============================================================================
// Topological Sort
// =============================================================================

fn topological_sort(root: &Arc<RwLock<GradNode>>) -> Vec<Arc<RwLock<GradNode>>> {
    let mut visited = HashSet::new();
    let mut order = Vec::new();
    
    fn visit(
        node: &Arc<RwLock<GradNode>>,
        visited: &mut HashSet<usize>,
        order: &mut Vec<Arc<RwLock<GradNode>>>,
    ) {
        let id = node.read().unwrap().id;
        
        if visited.contains(&id) {
            return;
        }
        
        visited.insert(id);
        
        // Visit inputs first
            {
                let node_guard = node.read().unwrap();
                if let Some(backward) = &node_guard.backward_fn {
                    let inputs: Vec<_> = backward.inputs.iter().cloned().collect();
                    drop(node_guard);
                    for input in inputs {
                        if let Some(input_node) = input.upgrade() {
                            visit(&input_node, visited, order);
                        }
                    }
                }
            }
        
        order.push(node.clone());
    }
    
    visit(root, &mut visited, &mut order);
    
    // Reverse for backward order
    order.reverse();
    order
}

// =============================================================================
// Autograd Operations
// =============================================================================

/// Create a new variable from an operation.
fn new_variable_from_op(
    data: Tensor,
    inputs: &[&Variable],
    name: &str,
    grad_fn: GradFn,
    saved_tensors: Vec<Tensor>,
) -> Variable {
    let requires_grad = inputs.iter().any(|v| v.requires_grad_flag());
    
    let id = next_node_id();
    let node = Arc::new(RwLock::new(GradNode::new(id, requires_grad, false)));
    
    if requires_grad {
        let inputs: Vec<Weak<RwLock<GradNode>>> = inputs
            .iter()
            .map(|v| Arc::downgrade(&v.node))
            .collect();
        
        let mut node_mut = node.write().unwrap();
        node_mut.backward_fn = Some(BackwardFn {
            grad_fn,
            name: name.to_string(),
            saved_tensors,
            inputs,
        });
    }
    
    Variable { data, node }
}

/// Addition with gradient tracking.
pub fn add(a: &Variable, b: &Variable) -> GpuResult<Variable> {
    let data = crate::ops::add(a.data(), b.data())?;
    
    let grad_fn: GradFn = Arc::new(|_saved, grad_output| {
        // d(a+b)/da = 1, d(a+b)/db = 1
        Ok(vec![
            grad_output.clone_tensor()?,
            grad_output.clone_tensor()?,
        ])
    });
    
    Ok(new_variable_from_op(data, &[a, b], "add", grad_fn, vec![]))
}

/// Subtraction with gradient tracking.
pub fn sub(a: &Variable, b: &Variable) -> GpuResult<Variable> {
    let data = crate::ops::sub(a.data(), b.data())?;
    
    let grad_fn: GradFn = Arc::new(|_saved, grad_output| {
        // d(a-b)/da = 1, d(a-b)/db = -1
        let neg_grad = crate::ops::neg(grad_output)?;
        Ok(vec![
            grad_output.clone_tensor()?,
            neg_grad,
        ])
    });
    
    Ok(new_variable_from_op(data, &[a, b], "sub", grad_fn, vec![]))
}

/// Multiplication with gradient tracking.
pub fn mul(a: &Variable, b: &Variable) -> GpuResult<Variable> {
    let data = crate::ops::mul(a.data(), b.data())?;
    
    let a_data = a.data().clone_tensor()?;
    let b_data = b.data().clone_tensor()?;
    
    let grad_fn: GradFn = Arc::new(move |saved, grad_output| {
        let a = &saved[0];
        let b = &saved[1];
        
        // d(a*b)/da = b, d(a*b)/db = a
        let grad_a = crate::ops::mul(grad_output, b)?;
        let grad_b = crate::ops::mul(grad_output, a)?;
        
        Ok(vec![grad_a, grad_b])
    });
    
    Ok(new_variable_from_op(data, &[a, b], "mul", grad_fn, vec![a_data, b_data]))
}

/// Division with gradient tracking.
pub fn div(a: &Variable, b: &Variable) -> GpuResult<Variable> {
    let data = crate::ops::div(a.data(), b.data())?;
    
    let a_data = a.data().clone_tensor()?;
    let b_data = b.data().clone_tensor()?;
    
    let grad_fn: GradFn = Arc::new(move |saved, grad_output| {
        let a = &saved[0];
        let b = &saved[1];
        
        // d(a/b)/da = 1/b
        // d(a/b)/db = -a/b^2
        let grad_a = crate::ops::div(grad_output, b)?;
        
        let neg_a = crate::ops::neg(a)?;
        let b_sq = crate::ops::mul(b, b)?;
        let neg_a_div_b_sq = crate::ops::div(&neg_a, &b_sq)?;
        let grad_b = crate::ops::mul(grad_output, &neg_a_div_b_sq)?;
        
        Ok(vec![grad_a, grad_b])
    });
    
    Ok(new_variable_from_op(data, &[a, b], "div", grad_fn, vec![a_data, b_data]))
}

/// Matrix multiplication with gradient tracking.
pub fn matmul(a: &Variable, b: &Variable) -> GpuResult<Variable> {
    let data = crate::ops::matmul(a.data(), b.data())?;
    
    let a_data = a.data().clone_tensor()?;
    let b_data = b.data().clone_tensor()?;
    
    let grad_fn: GradFn = Arc::new(move |saved, grad_output| {
        let a = &saved[0];
        let b = &saved[1];
        
        // grad_a = grad_output @ b.T
        // grad_b = a.T @ grad_output
        let b_t = b.t()?;
        let a_t = a.t()?;
        
        let grad_a = crate::ops::matmul(grad_output, &b_t)?;
        let grad_b = crate::ops::matmul(&a_t, grad_output)?;
        
        Ok(vec![grad_a, grad_b])
    });
    
    Ok(new_variable_from_op(data, &[a, b], "matmul", grad_fn, vec![a_data, b_data]))
}

/// Sum with gradient tracking.
pub fn sum(a: &Variable) -> GpuResult<Variable> {
    let sum_val = crate::ops::sum(a.data())?;
    let data = Tensor::full(
        Shape::scalar(),
        sum_val,
        a.data().dtype(),
        a.data().device(),
    )?;
    
    let input_shape = a.data().shape().clone();
    let device = a.data().device().clone();
    let dtype = a.data().dtype();
    
    let grad_fn: GradFn = Arc::new(move |_saved, grad_output| {
        // Broadcast scalar gradient to input shape
        let grad_val = crate::ops::sum(grad_output)?;
        let grad = Tensor::full(input_shape.clone(), grad_val, dtype, &device)?;
        Ok(vec![grad])
    });
    
    Ok(new_variable_from_op(data, &[a], "sum", grad_fn, vec![]))
}

/// Mean with gradient tracking.
pub fn mean(a: &Variable) -> GpuResult<Variable> {
    let mean_val = crate::ops::mean(a.data())?;
    let data = Tensor::full(
        Shape::scalar(),
        mean_val,
        a.data().dtype(),
        a.data().device(),
    )?;
    
    let numel = a.data().numel() as f32;
    let input_shape = a.data().shape().clone();
    let device = a.data().device().clone();
    let dtype = a.data().dtype();
    
    let grad_fn: GradFn = Arc::new(move |_saved, grad_output| {
        let grad_val = crate::ops::sum(grad_output)? / numel;
        let grad = Tensor::full(input_shape.clone(), grad_val, dtype, &device)?;
        Ok(vec![grad])
    });
    
    Ok(new_variable_from_op(data, &[a], "mean", grad_fn, vec![]))
}

/// ReLU with gradient tracking.
pub fn relu(a: &Variable) -> GpuResult<Variable> {
    let data = crate::ops::relu(a.data())?;
    
    let input_data = a.data().clone_tensor()?;
    
    let grad_fn: GradFn = Arc::new(move |saved, grad_output| {
        let input = &saved[0];
        // grad = grad_output * (input > 0)
        let mask = crate::ops::gt(input, &Tensor::full(
            input.shape().clone(),
            0.0,
            input.dtype(),
            input.device(),
        )?)?;
        let grad = crate::ops::mul(grad_output, &mask)?;
        Ok(vec![grad])
    });
    
    Ok(new_variable_from_op(data, &[a], "relu", grad_fn, vec![input_data]))
}

/// Sigmoid with gradient tracking.
pub fn sigmoid(a: &Variable) -> GpuResult<Variable> {
    let data = crate::ops::sigmoid(a.data())?;
    
    let output_data = data.clone_tensor()?;
    
    let grad_fn: GradFn = Arc::new(move |saved, grad_output| {
        let sigmoid_out = &saved[0];
        // grad = grad_output * sigmoid * (1 - sigmoid)
        let ones = Tensor::full(
            sigmoid_out.shape().clone(),
            1.0,
            sigmoid_out.dtype(),
            sigmoid_out.device(),
        )?;
        let one_minus_sigmoid = crate::ops::sub(&ones, sigmoid_out)?;
        let sigmoid_deriv = crate::ops::mul(sigmoid_out, &one_minus_sigmoid)?;
        let grad = crate::ops::mul(grad_output, &sigmoid_deriv)?;
        Ok(vec![grad])
    });
    
    Ok(new_variable_from_op(data, &[a], "sigmoid", grad_fn, vec![output_data]))
}

/// Tanh with gradient tracking.
pub fn tanh(a: &Variable) -> GpuResult<Variable> {
    let data = crate::ops::tanh(a.data())?;
    
    let output_data = data.clone_tensor()?;
    
    let grad_fn: GradFn = Arc::new(move |saved, grad_output| {
        let tanh_out = &saved[0];
        // grad = grad_output * (1 - tanh^2)
        let tanh_sq = crate::ops::mul(tanh_out, tanh_out)?;
        let ones = Tensor::full(
            tanh_out.shape().clone(),
            1.0,
            tanh_out.dtype(),
            tanh_out.device(),
        )?;
        let one_minus_tanh_sq = crate::ops::sub(&ones, &tanh_sq)?;
        let grad = crate::ops::mul(grad_output, &one_minus_tanh_sq)?;
        Ok(vec![grad])
    });
    
    Ok(new_variable_from_op(data, &[a], "tanh", grad_fn, vec![output_data]))
}

/// Exp with gradient tracking.
pub fn exp(a: &Variable) -> GpuResult<Variable> {
    let data = crate::ops::exp(a.data())?;
    
    let output_data = data.clone_tensor()?;
    
    let grad_fn: GradFn = Arc::new(move |saved, grad_output| {
        let exp_out = &saved[0];
        // grad = grad_output * exp(x)
        let grad = crate::ops::mul(grad_output, exp_out)?;
        Ok(vec![grad])
    });
    
    Ok(new_variable_from_op(data, &[a], "exp", grad_fn, vec![output_data]))
}

/// Log with gradient tracking.
pub fn log(a: &Variable) -> GpuResult<Variable> {
    let data = crate::ops::log(a.data())?;
    
    let input_data = a.data().clone_tensor()?;
    
    let grad_fn: GradFn = Arc::new(move |saved, grad_output| {
        let input = &saved[0];
        // grad = grad_output / x
        let grad = crate::ops::div(grad_output, input)?;
        Ok(vec![grad])
    });
    
    Ok(new_variable_from_op(data, &[a], "log", grad_fn, vec![input_data]))
}

/// Pow with gradient tracking.
pub fn pow(a: &Variable, exponent: f32) -> GpuResult<Variable> {
    let data = crate::ops::pow(a.data(), exponent)?;
    
    let input_data = a.data().clone_tensor()?;
    
    let grad_fn: GradFn = Arc::new(move |saved, grad_output| {
        let input = &saved[0];
        // grad = grad_output * exponent * x^(exponent-1)
        let pow_minus_one = crate::ops::pow(input, exponent - 1.0)?;
        let scaled = crate::ops::mul_scalar(&pow_minus_one, exponent)?;
        let grad = crate::ops::mul(grad_output, &scaled)?;
        Ok(vec![grad])
    });
    
    Ok(new_variable_from_op(data, &[a], "pow", grad_fn, vec![input_data]))
}

/// Softmax with gradient tracking.
pub fn softmax(a: &Variable, _dim: Option<usize>) -> GpuResult<Variable> {
    let data = crate::ops::softmax(a.data(), None)?;
    
    let output_data = data.clone_tensor()?;
    
    let grad_fn: GradFn = Arc::new(move |saved, grad_output| {
        let softmax_out = &saved[0];
        // Jacobian of softmax is: diag(s) - s * s^T
        // grad = s * (grad_output - sum(grad_output * s))
        let gs = crate::ops::mul(grad_output, softmax_out)?;
        let sum_gs = crate::ops::sum(&gs)?;
        let sum_tensor = Tensor::full(
            softmax_out.shape().clone(),
            sum_gs,
            softmax_out.dtype(),
            softmax_out.device(),
        )?;
        let diff = crate::ops::sub(grad_output, &sum_tensor)?;
        let grad = crate::ops::mul(softmax_out, &diff)?;
        Ok(vec![grad])
    });
    
    Ok(new_variable_from_op(data, &[a], "softmax", grad_fn, vec![output_data]))
}

// =============================================================================
// No-grad Context
// =============================================================================

thread_local! {
    static GRAD_ENABLED: RefCell<bool> = RefCell::new(true);
}

/// Check if gradient computation is enabled.
pub fn is_grad_enabled() -> bool {
    GRAD_ENABLED.with(|g| *g.borrow())
}

/// Set gradient computation state.
pub fn set_grad_enabled(enabled: bool) {
    GRAD_ENABLED.with(|g| *g.borrow_mut() = enabled);
}

/// No-grad context guard.
pub struct NoGrad {
    prev_state: bool,
}

impl NoGrad {
    /// Enter no-grad context.
    pub fn new() -> Self {
        let prev_state = is_grad_enabled();
        set_grad_enabled(false);
        Self { prev_state }
    }
}

impl Drop for NoGrad {
    fn drop(&mut self) {
        set_grad_enabled(self.prev_state);
    }
}

impl Default for NoGrad {
    fn default() -> Self {
        Self::new()
    }
}

/// Execute a closure with gradient computation disabled.
pub fn no_grad<F, R>(f: F) -> R
where
    F: FnOnce() -> R,
{
    let _guard = NoGrad::new();
    f()
}

// =============================================================================
// Optimizer
// =============================================================================

/// Base trait for optimizers.
pub trait Optimizer {
    /// Perform one optimization step.
    fn step(&mut self) -> GpuResult<()>;
    
    /// Zero all gradients.
    fn zero_grad(&mut self);
}

/// Stochastic Gradient Descent optimizer.
pub struct SGD {
    /// Parameters to optimize.
    params: Vec<Variable>,
    
    /// Learning rate.
    lr: f32,
    
    /// Momentum factor.
    momentum: f32,
    
    /// Weight decay (L2 penalty).
    weight_decay: f32,
    
    /// Momentum buffers.
    momentum_buffers: Vec<Option<Tensor>>,
}

impl SGD {
    /// Create a new SGD optimizer.
    pub fn new(params: Vec<Variable>, lr: f32) -> Self {
        let n = params.len();
        let mut momentum_buffers = Vec::with_capacity(n);
        for _ in 0..n {
            momentum_buffers.push(None);
        }
        Self {
            params,
            lr,
            momentum: 0.0,
            weight_decay: 0.0,
            momentum_buffers,
        }
    }
    
    /// Set momentum.
    pub fn momentum(mut self, momentum: f32) -> Self {
        self.momentum = momentum;
        self
    }
    
    /// Set weight decay.
    pub fn weight_decay(mut self, weight_decay: f32) -> Self {
        self.weight_decay = weight_decay;
        self
    }
}

impl Optimizer for SGD {
    fn step(&mut self) -> GpuResult<()> {
        for (i, param) in self.params.iter().enumerate() {
            let grad = match param.grad() {
                Some(g) => g,
                None => continue,
            };
            
            // Apply weight decay
            let grad = if self.weight_decay != 0.0 {
                let decay = crate::ops::mul_scalar(param.data(), self.weight_decay)?;
                crate::ops::add(&grad, &decay)?
            } else {
                grad
            };
            
            // Apply momentum
            let update = if self.momentum != 0.0 {
                let buf = match &mut self.momentum_buffers[i] {
                    Some(b) => {
                        let scaled_buf = crate::ops::mul_scalar(b, self.momentum)?;
                        let new_buf = crate::ops::add(&scaled_buf, &grad)?;
                        *b = new_buf.clone_tensor()?;
                        new_buf
                    }
                    None => {
                        self.momentum_buffers[i] = Some(grad.clone_tensor()?);
                        grad
                    }
                };
                buf
            } else {
                grad
            };
            
            // Update: param = param - lr * update
            let scaled = crate::ops::mul_scalar(&update, -self.lr)?;
            let new_data = crate::ops::add(param.data(), &scaled)?;
            
            // TODO: Actually update param.data in place
            // This would require interior mutability
        }
        
        Ok(())
    }
    
    fn zero_grad(&mut self) {
        for param in &self.params {
            param.zero_grad();
        }
    }
}

/// Adam optimizer.
pub struct Adam {
    /// Parameters to optimize.
    params: Vec<Variable>,
    
    /// Learning rate.
    lr: f32,
    
    /// Beta1 (first moment decay).
    beta1: f32,
    
    /// Beta2 (second moment decay).
    beta2: f32,
    
    /// Epsilon for numerical stability.
    eps: f32,
    
    /// Weight decay.
    weight_decay: f32,
    
    /// Step count.
    step_count: i32,
    
    /// First moment estimates.
    m: Vec<Option<Tensor>>,
    
    /// Second moment estimates.
    v: Vec<Option<Tensor>>,
}

impl Adam {
    /// Create a new Adam optimizer.
    pub fn new(params: Vec<Variable>, lr: f32) -> Self {
        let n = params.len();
        let mut m = Vec::with_capacity(n);
        let mut v = Vec::with_capacity(n);
        for _ in 0..n {
            m.push(None);
            v.push(None);
        }
        Self {
            params,
            lr,
            beta1: 0.9,
            beta2: 0.999,
            eps: 1e-8,
            weight_decay: 0.0,
            step_count: 0,
            m,
            v,
        }
    }
    
    /// Set betas.
    pub fn betas(mut self, beta1: f32, beta2: f32) -> Self {
        self.beta1 = beta1;
        self.beta2 = beta2;
        self
    }
    
    /// Set epsilon.
    pub fn eps(mut self, eps: f32) -> Self {
        self.eps = eps;
        self
    }
    
    /// Set weight decay.
    pub fn weight_decay(mut self, weight_decay: f32) -> Self {
        self.weight_decay = weight_decay;
        self
    }
}

impl Optimizer for Adam {
    fn step(&mut self) -> GpuResult<()> {
        self.step_count += 1;
        
        let bias_correction1 = 1.0 - self.beta1.powi(self.step_count);
        let bias_correction2 = 1.0 - self.beta2.powi(self.step_count);
        
        for (i, param) in self.params.iter().enumerate() {
            let grad = match param.grad() {
                Some(g) => g,
                None => continue,
            };
            
            // Apply weight decay
            let grad = if self.weight_decay != 0.0 {
                let decay = crate::ops::mul_scalar(param.data(), self.weight_decay)?;
                crate::ops::add(&grad, &decay)?
            } else {
                grad
            };
            
            // Update first moment: m = beta1 * m + (1 - beta1) * grad
            let m = match &self.m[i] {
                Some(m_prev) => {
                    let scaled_m = crate::ops::mul_scalar(m_prev, self.beta1)?;
                    let scaled_grad = crate::ops::mul_scalar(&grad, 1.0 - self.beta1)?;
                    crate::ops::add(&scaled_m, &scaled_grad)?
                }
                None => {
                    crate::ops::mul_scalar(&grad, 1.0 - self.beta1)?
                }
            };
            self.m[i] = Some(m.clone_tensor()?);
            
            // Update second moment: v = beta2 * v + (1 - beta2) * grad^2
            let grad_sq = crate::ops::mul(&grad, &grad)?;
            let v = match &self.v[i] {
                Some(v_prev) => {
                    let scaled_v = crate::ops::mul_scalar(v_prev, self.beta2)?;
                    let scaled_grad_sq = crate::ops::mul_scalar(&grad_sq, 1.0 - self.beta2)?;
                    crate::ops::add(&scaled_v, &scaled_grad_sq)?
                }
                None => {
                    crate::ops::mul_scalar(&grad_sq, 1.0 - self.beta2)?
                }
            };
            self.v[i] = Some(v.clone_tensor()?);
            
            // Bias-corrected estimates
            let m_hat = crate::ops::mul_scalar(&m, 1.0 / bias_correction1)?;
            let v_hat = crate::ops::mul_scalar(&v, 1.0 / bias_correction2)?;
            
            // Update: param = param - lr * m_hat / (sqrt(v_hat) + eps)
            let v_sqrt = crate::ops::sqrt(&v_hat)?;
            let v_sqrt_eps = crate::ops::add_scalar(&v_sqrt, self.eps)?;
            let update = crate::ops::div(&m_hat, &v_sqrt_eps)?;
            let scaled_update = crate::ops::mul_scalar(&update, -self.lr)?;
            
            let _new_data = crate::ops::add(param.data(), &scaled_update)?;
            // TODO: Actually update param.data in place
        }
        
        Ok(())
    }
    
    fn zero_grad(&mut self) {
        for param in &self.params {
            param.zero_grad();
        }
    }
}

