//! GPU tensor operations.

use crate::tensor::{Tensor, Shape, DType};
use crate::device::Device;
use crate::kernel::{LaunchConfig, cuda_kernels};
use crate::backend::KernelArg;
use crate::error::{GpuError, GpuResult};

// =============================================================================
// Element-wise operations
// =============================================================================

/// Add two tensors element-wise.
pub fn add(a: &Tensor, b: &Tensor) -> GpuResult<Tensor> {
    check_same_shape(a, b)?;
    check_same_device(a, b)?;
    
    let result = Tensor::zeros(a.shape().clone(), a.dtype(), a.device())?;
    
    // Launch add kernel
    let n = a.numel() as u32;
    let config = LaunchConfig::linear(n, 256);
    
    let args = [
        KernelArg::Ptr(a.data_ptr().ptr),
        KernelArg::Ptr(b.data_ptr().ptr),
        KernelArg::Ptr(result.data_ptr().ptr),
        KernelArg::I32(n as i32),
    ];
    
    // TODO: Actually launch kernel
    // For now, do CPU fallback
    let a_data: Vec<f32> = a.to_cpu()?;
    let b_data: Vec<f32> = b.to_cpu()?;
    let c_data: Vec<f32> = a_data.iter().zip(b_data.iter()).map(|(x, y)| x + y).collect();
    
    Tensor::from_slice(&c_data, a.shape().clone(), a.device())
}

/// Subtract two tensors element-wise.
pub fn sub(a: &Tensor, b: &Tensor) -> GpuResult<Tensor> {
    check_same_shape(a, b)?;
    check_same_device(a, b)?;
    
    let a_data: Vec<f32> = a.to_cpu()?;
    let b_data: Vec<f32> = b.to_cpu()?;
    let c_data: Vec<f32> = a_data.iter().zip(b_data.iter()).map(|(x, y)| x - y).collect();
    
    Tensor::from_slice(&c_data, a.shape().clone(), a.device())
}

/// Multiply two tensors element-wise.
pub fn mul(a: &Tensor, b: &Tensor) -> GpuResult<Tensor> {
    check_same_shape(a, b)?;
    check_same_device(a, b)?;
    
    let a_data: Vec<f32> = a.to_cpu()?;
    let b_data: Vec<f32> = b.to_cpu()?;
    let c_data: Vec<f32> = a_data.iter().zip(b_data.iter()).map(|(x, y)| x * y).collect();
    
    Tensor::from_slice(&c_data, a.shape().clone(), a.device())
}

/// Divide two tensors element-wise.
pub fn div(a: &Tensor, b: &Tensor) -> GpuResult<Tensor> {
    check_same_shape(a, b)?;
    check_same_device(a, b)?;
    
    let a_data: Vec<f32> = a.to_cpu()?;
    let b_data: Vec<f32> = b.to_cpu()?;
    let c_data: Vec<f32> = a_data.iter().zip(b_data.iter()).map(|(x, y)| x / y).collect();
    
    Tensor::from_slice(&c_data, a.shape().clone(), a.device())
}

/// Add scalar to tensor.
pub fn add_scalar(a: &Tensor, scalar: f32) -> GpuResult<Tensor> {
    let a_data: Vec<f32> = a.to_cpu()?;
    let c_data: Vec<f32> = a_data.iter().map(|x| x + scalar).collect();
    
    Tensor::from_slice(&c_data, a.shape().clone(), a.device())
}

/// Multiply tensor by scalar.
pub fn mul_scalar(a: &Tensor, scalar: f32) -> GpuResult<Tensor> {
    let a_data: Vec<f32> = a.to_cpu()?;
    let c_data: Vec<f32> = a_data.iter().map(|x| x * scalar).collect();
    
    Tensor::from_slice(&c_data, a.shape().clone(), a.device())
}

/// Negate tensor.
pub fn neg(a: &Tensor) -> GpuResult<Tensor> {
    mul_scalar(a, -1.0)
}

/// Absolute value.
pub fn abs(a: &Tensor) -> GpuResult<Tensor> {
    let a_data: Vec<f32> = a.to_cpu()?;
    let c_data: Vec<f32> = a_data.iter().map(|x| x.abs()).collect();
    
    Tensor::from_slice(&c_data, a.shape().clone(), a.device())
}

/// Square root.
pub fn sqrt(a: &Tensor) -> GpuResult<Tensor> {
    let a_data: Vec<f32> = a.to_cpu()?;
    let c_data: Vec<f32> = a_data.iter().map(|x| x.sqrt()).collect();
    
    Tensor::from_slice(&c_data, a.shape().clone(), a.device())
}

/// Exponential.
pub fn exp(a: &Tensor) -> GpuResult<Tensor> {
    let a_data: Vec<f32> = a.to_cpu()?;
    let c_data: Vec<f32> = a_data.iter().map(|x| x.exp()).collect();
    
    Tensor::from_slice(&c_data, a.shape().clone(), a.device())
}

/// Natural logarithm.
pub fn log(a: &Tensor) -> GpuResult<Tensor> {
    let a_data: Vec<f32> = a.to_cpu()?;
    let c_data: Vec<f32> = a_data.iter().map(|x| x.ln()).collect();
    
    Tensor::from_slice(&c_data, a.shape().clone(), a.device())
}

/// Power.
pub fn pow(a: &Tensor, exponent: f32) -> GpuResult<Tensor> {
    let a_data: Vec<f32> = a.to_cpu()?;
    let c_data: Vec<f32> = a_data.iter().map(|x| x.powf(exponent)).collect();
    
    Tensor::from_slice(&c_data, a.shape().clone(), a.device())
}

/// Sine.
pub fn sin(a: &Tensor) -> GpuResult<Tensor> {
    let a_data: Vec<f32> = a.to_cpu()?;
    let c_data: Vec<f32> = a_data.iter().map(|x| x.sin()).collect();
    
    Tensor::from_slice(&c_data, a.shape().clone(), a.device())
}

/// Cosine.
pub fn cos(a: &Tensor) -> GpuResult<Tensor> {
    let a_data: Vec<f32> = a.to_cpu()?;
    let c_data: Vec<f32> = a_data.iter().map(|x| x.cos()).collect();
    
    Tensor::from_slice(&c_data, a.shape().clone(), a.device())
}

/// Tangent.
pub fn tan(a: &Tensor) -> GpuResult<Tensor> {
    let a_data: Vec<f32> = a.to_cpu()?;
    let c_data: Vec<f32> = a_data.iter().map(|x| x.tan()).collect();
    
    Tensor::from_slice(&c_data, a.shape().clone(), a.device())
}

// =============================================================================
// Reduction operations
// =============================================================================

/// Sum all elements.
pub fn sum(a: &Tensor) -> GpuResult<f32> {
    let a_data: Vec<f32> = a.to_cpu()?;
    Ok(a_data.iter().sum())
}

/// Mean of all elements.
pub fn mean(a: &Tensor) -> GpuResult<f32> {
    let s = sum(a)?;
    Ok(s / a.numel() as f32)
}

/// Maximum element.
pub fn max(a: &Tensor) -> GpuResult<f32> {
    let a_data: Vec<f32> = a.to_cpu()?;
    Ok(a_data.iter().cloned().fold(f32::NEG_INFINITY, f32::max))
}

/// Minimum element.
pub fn min(a: &Tensor) -> GpuResult<f32> {
    let a_data: Vec<f32> = a.to_cpu()?;
    Ok(a_data.iter().cloned().fold(f32::INFINITY, f32::min))
}

/// Standard deviation.
pub fn std(a: &Tensor) -> GpuResult<f32> {
    let m = mean(a)?;
    let a_data: Vec<f32> = a.to_cpu()?;
    let variance: f32 = a_data.iter().map(|x| (x - m).powi(2)).sum::<f32>() / a.numel() as f32;
    Ok(variance.sqrt())
}

/// Variance.
pub fn var(a: &Tensor) -> GpuResult<f32> {
    let m = mean(a)?;
    let a_data: Vec<f32> = a.to_cpu()?;
    Ok(a_data.iter().map(|x| (x - m).powi(2)).sum::<f32>() / a.numel() as f32)
}

/// Sum along dimension.
pub fn sum_dim(a: &Tensor, dim: usize) -> GpuResult<Tensor> {
    // TODO: Implement proper reduction
    Err(GpuError::Unsupported("sum_dim not implemented".to_string()))
}

/// Mean along dimension.
pub fn mean_dim(a: &Tensor, dim: usize) -> GpuResult<Tensor> {
    Err(GpuError::Unsupported("mean_dim not implemented".to_string()))
}

// =============================================================================
// Matrix operations
// =============================================================================

/// Matrix multiplication.
pub fn matmul(a: &Tensor, b: &Tensor) -> GpuResult<Tensor> {
    if a.ndim() != 2 || b.ndim() != 2 {
        return Err(GpuError::InvalidShape(
            format!("matmul requires 2D tensors, got {}D and {}D", a.ndim(), b.ndim())
        ));
    }
    
    let m = a.shape().dim(0);
    let k = a.shape().dim(1);
    let n = b.shape().dim(1);
    
    if k != b.shape().dim(0) {
        return Err(GpuError::DimensionMismatch(
            format!("matmul: inner dimensions don't match: {} vs {}", k, b.shape().dim(0))
        ));
    }
    
    check_same_device(a, b)?;
    
    // CPU fallback
    let a_data: Vec<f32> = a.to_cpu()?;
    let b_data: Vec<f32> = b.to_cpu()?;
    let mut c_data = vec![0.0f32; m * n];
    
    for i in 0..m {
        for j in 0..n {
            let mut sum = 0.0;
            for l in 0..k {
                sum += a_data[i * k + l] * b_data[l * n + j];
            }
            c_data[i * n + j] = sum;
        }
    }
    
    Tensor::from_slice(&c_data, Shape::d2(m, n), a.device())
}

/// Batched matrix multiplication.
pub fn bmm(a: &Tensor, b: &Tensor) -> GpuResult<Tensor> {
    if a.ndim() != 3 || b.ndim() != 3 {
        return Err(GpuError::InvalidShape(
            format!("bmm requires 3D tensors, got {}D and {}D", a.ndim(), b.ndim())
        ));
    }
    
    // TODO: Implement
    Err(GpuError::Unsupported("bmm not implemented".to_string()))
}

/// Vector dot product.
pub fn dot(a: &Tensor, b: &Tensor) -> GpuResult<f32> {
    check_same_shape(a, b)?;
    
    let a_data: Vec<f32> = a.to_cpu()?;
    let b_data: Vec<f32> = b.to_cpu()?;
    
    Ok(a_data.iter().zip(b_data.iter()).map(|(x, y)| x * y).sum())
}

/// Outer product.
pub fn outer(a: &Tensor, b: &Tensor) -> GpuResult<Tensor> {
    if a.ndim() != 1 || b.ndim() != 1 {
        return Err(GpuError::InvalidShape("outer requires 1D tensors".to_string()));
    }
    
    let m = a.numel();
    let n = b.numel();
    
    let a_data: Vec<f32> = a.to_cpu()?;
    let b_data: Vec<f32> = b.to_cpu()?;
    
    let mut c_data = Vec::with_capacity(m * n);
    for i in 0..m {
        for j in 0..n {
            c_data.push(a_data[i] * b_data[j]);
        }
    }
    
    Tensor::from_slice(&c_data, Shape::d2(m, n), a.device())
}

// =============================================================================
// Neural network operations
// =============================================================================

/// ReLU activation.
pub fn relu(a: &Tensor) -> GpuResult<Tensor> {
    let a_data: Vec<f32> = a.to_cpu()?;
    let c_data: Vec<f32> = a_data.iter().map(|x| x.max(0.0)).collect();
    
    Tensor::from_slice(&c_data, a.shape().clone(), a.device())
}

/// Sigmoid activation.
pub fn sigmoid(a: &Tensor) -> GpuResult<Tensor> {
    let a_data: Vec<f32> = a.to_cpu()?;
    let c_data: Vec<f32> = a_data.iter().map(|x| 1.0 / (1.0 + (-x).exp())).collect();
    
    Tensor::from_slice(&c_data, a.shape().clone(), a.device())
}

/// Tanh activation.
pub fn tanh(a: &Tensor) -> GpuResult<Tensor> {
    let a_data: Vec<f32> = a.to_cpu()?;
    let c_data: Vec<f32> = a_data.iter().map(|x| x.tanh()).collect();
    
    Tensor::from_slice(&c_data, a.shape().clone(), a.device())
}

/// Softmax.
pub fn softmax(a: &Tensor, dim: Option<usize>) -> GpuResult<Tensor> {
    let a_data: Vec<f32> = a.to_cpu()?;
    
    // For 1D, apply across all elements
    let max_val: f32 = a_data.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
    let exp_data: Vec<f32> = a_data.iter().map(|x| (x - max_val).exp()).collect();
    let sum: f32 = exp_data.iter().sum();
    let c_data: Vec<f32> = exp_data.iter().map(|x| x / sum).collect();
    
    Tensor::from_slice(&c_data, a.shape().clone(), a.device())
}

/// Log softmax.
pub fn log_softmax(a: &Tensor, dim: Option<usize>) -> GpuResult<Tensor> {
    let sm = softmax(a, dim)?;
    log(&sm)
}

/// GELU activation.
pub fn gelu(a: &Tensor) -> GpuResult<Tensor> {
    // GELU(x) = x * Φ(x) ≈ 0.5 * x * (1 + tanh(sqrt(2/π) * (x + 0.044715 * x^3)))
    let a_data: Vec<f32> = a.to_cpu()?;
    let sqrt_2_over_pi: f32 = (2.0 / std::f32::consts::PI).sqrt();
    
    let c_data: Vec<f32> = a_data.iter().map(|x| {
        0.5 * x * (1.0 + (sqrt_2_over_pi * (x + 0.044715 * x.powi(3))).tanh())
    }).collect();
    
    Tensor::from_slice(&c_data, a.shape().clone(), a.device())
}

/// Leaky ReLU.
pub fn leaky_relu(a: &Tensor, negative_slope: f32) -> GpuResult<Tensor> {
    let a_data: Vec<f32> = a.to_cpu()?;
    let c_data: Vec<f32> = a_data.iter().map(|x| {
        if *x > 0.0 { *x } else { negative_slope * x }
    }).collect();
    
    Tensor::from_slice(&c_data, a.shape().clone(), a.device())
}

/// ELU activation.
pub fn elu(a: &Tensor, alpha: f32) -> GpuResult<Tensor> {
    let a_data: Vec<f32> = a.to_cpu()?;
    let c_data: Vec<f32> = a_data.iter().map(|x| {
        if *x > 0.0 { *x } else { alpha * (x.exp() - 1.0) }
    }).collect();
    
    Tensor::from_slice(&c_data, a.shape().clone(), a.device())
}

// =============================================================================
// Comparison operations
// =============================================================================

/// Element-wise equality.
pub fn eq(a: &Tensor, b: &Tensor) -> GpuResult<Tensor> {
    check_same_shape(a, b)?;
    
    let a_data: Vec<f32> = a.to_cpu()?;
    let b_data: Vec<f32> = b.to_cpu()?;
    let c_data: Vec<f32> = a_data.iter().zip(b_data.iter())
        .map(|(x, y)| if (x - y).abs() < 1e-6 { 1.0 } else { 0.0 })
        .collect();
    
    Tensor::from_slice(&c_data, a.shape().clone(), a.device())
}

/// Element-wise greater than.
pub fn gt(a: &Tensor, b: &Tensor) -> GpuResult<Tensor> {
    check_same_shape(a, b)?;
    
    let a_data: Vec<f32> = a.to_cpu()?;
    let b_data: Vec<f32> = b.to_cpu()?;
    let c_data: Vec<f32> = a_data.iter().zip(b_data.iter())
        .map(|(x, y)| if x > y { 1.0 } else { 0.0 })
        .collect();
    
    Tensor::from_slice(&c_data, a.shape().clone(), a.device())
}

/// Element-wise less than.
pub fn lt(a: &Tensor, b: &Tensor) -> GpuResult<Tensor> {
    check_same_shape(a, b)?;
    
    let a_data: Vec<f32> = a.to_cpu()?;
    let b_data: Vec<f32> = b.to_cpu()?;
    let c_data: Vec<f32> = a_data.iter().zip(b_data.iter())
        .map(|(x, y)| if x < y { 1.0 } else { 0.0 })
        .collect();
    
    Tensor::from_slice(&c_data, a.shape().clone(), a.device())
}

/// Where/select.
pub fn where_cond(cond: &Tensor, a: &Tensor, b: &Tensor) -> GpuResult<Tensor> {
    check_same_shape(a, b)?;
    check_same_shape(cond, a)?;
    
    let cond_data: Vec<f32> = cond.to_cpu()?;
    let a_data: Vec<f32> = a.to_cpu()?;
    let b_data: Vec<f32> = b.to_cpu()?;
    
    let c_data: Vec<f32> = cond_data.iter()
        .zip(a_data.iter().zip(b_data.iter()))
        .map(|(c, (x, y))| if *c != 0.0 { *x } else { *y })
        .collect();
    
    Tensor::from_slice(&c_data, a.shape().clone(), a.device())
}

// =============================================================================
// Utility functions
// =============================================================================

fn check_same_shape(a: &Tensor, b: &Tensor) -> GpuResult<()> {
    if a.shape() != b.shape() {
        return Err(GpuError::DimensionMismatch(
            format!("Shape mismatch: {:?} vs {:?}", a.shape().dims(), b.shape().dims())
        ));
    }
    Ok(())
}

fn check_same_device(a: &Tensor, b: &Tensor) -> GpuResult<()> {
    if a.device().device_type() != b.device().device_type() {
        return Err(GpuError::Runtime(
            format!("Device mismatch: {:?} vs {:?}",
                a.device().device_type(), b.device().device_type())
        ));
    }
    Ok(())
}

