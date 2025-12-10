//! GPU Tensor types.

use std::sync::Arc;
use std::marker::PhantomData;
use crate::device::{Device, DevicePtr};
use crate::error::{GpuError, GpuResult};

/// Data type for tensors.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DType {
    /// 32-bit float.
    Float32,
    /// 64-bit float.
    Float64,
    /// 16-bit float (half precision).
    Float16,
    /// BFloat16.
    BFloat16,
    /// 8-bit signed integer.
    Int8,
    /// 16-bit signed integer.
    Int16,
    /// 32-bit signed integer.
    Int32,
    /// 64-bit signed integer.
    Int64,
    /// 8-bit unsigned integer.
    UInt8,
    /// 16-bit unsigned integer.
    UInt16,
    /// 32-bit unsigned integer.
    UInt32,
    /// 64-bit unsigned integer.
    UInt64,
    /// Boolean.
    Bool,
    /// Complex 64 (two f32).
    Complex64,
    /// Complex 128 (two f64).
    Complex128,
}

impl DType {
    /// Get size in bytes.
    pub fn size(&self) -> usize {
        match self {
            Self::Float32 | Self::Int32 | Self::UInt32 => 4,
            Self::Float64 | Self::Int64 | Self::UInt64 | Self::Complex64 => 8,
            Self::Float16 | Self::BFloat16 | Self::Int16 | Self::UInt16 => 2,
            Self::Int8 | Self::UInt8 | Self::Bool => 1,
            Self::Complex128 => 16,
        }
    }
    
    /// Get name.
    pub fn name(&self) -> &'static str {
        match self {
            Self::Float32 => "float32",
            Self::Float64 => "float64",
            Self::Float16 => "float16",
            Self::BFloat16 => "bfloat16",
            Self::Int8 => "int8",
            Self::Int16 => "int16",
            Self::Int32 => "int32",
            Self::Int64 => "int64",
            Self::UInt8 => "uint8",
            Self::UInt16 => "uint16",
            Self::UInt32 => "uint32",
            Self::UInt64 => "uint64",
            Self::Bool => "bool",
            Self::Complex64 => "complex64",
            Self::Complex128 => "complex128",
        }
    }
    
    /// Check if floating point.
    pub fn is_float(&self) -> bool {
        matches!(self, Self::Float32 | Self::Float64 | Self::Float16 | Self::BFloat16)
    }
    
    /// Check if integer.
    pub fn is_int(&self) -> bool {
        matches!(
            self,
            Self::Int8 | Self::Int16 | Self::Int32 | Self::Int64 |
            Self::UInt8 | Self::UInt16 | Self::UInt32 | Self::UInt64
        )
    }
    
    /// Check if complex.
    pub fn is_complex(&self) -> bool {
        matches!(self, Self::Complex64 | Self::Complex128)
    }
}

/// Tensor shape.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Shape {
    dims: Vec<usize>,
}

impl Shape {
    /// Create a new shape.
    pub fn new(dims: Vec<usize>) -> Self {
        Self { dims }
    }
    
    /// Scalar shape.
    pub fn scalar() -> Self {
        Self { dims: vec![] }
    }
    
    /// 1D shape.
    pub fn d1(n: usize) -> Self {
        Self { dims: vec![n] }
    }
    
    /// 2D shape.
    pub fn d2(rows: usize, cols: usize) -> Self {
        Self { dims: vec![rows, cols] }
    }
    
    /// 3D shape.
    pub fn d3(d0: usize, d1: usize, d2: usize) -> Self {
        Self { dims: vec![d0, d1, d2] }
    }
    
    /// 4D shape.
    pub fn d4(d0: usize, d1: usize, d2: usize, d3: usize) -> Self {
        Self { dims: vec![d0, d1, d2, d3] }
    }
    
    /// Number of dimensions.
    pub fn ndim(&self) -> usize {
        self.dims.len()
    }
    
    /// Get dimension.
    pub fn dim(&self, i: usize) -> usize {
        self.dims.get(i).copied().unwrap_or(1)
    }
    
    /// Get all dimensions.
    pub fn dims(&self) -> &[usize] {
        &self.dims
    }
    
    /// Total number of elements.
    pub fn numel(&self) -> usize {
        self.dims.iter().product()
    }
    
    /// Check if shapes are broadcastable.
    pub fn is_broadcastable_with(&self, other: &Shape) -> bool {
        let max_ndim = self.ndim().max(other.ndim());
        
        for i in 0..max_ndim {
            let d1 = self.dims.get(self.ndim().saturating_sub(i + 1)).copied().unwrap_or(1);
            let d2 = other.dims.get(other.ndim().saturating_sub(i + 1)).copied().unwrap_or(1);
            
            if d1 != d2 && d1 != 1 && d2 != 1 {
                return false;
            }
        }
        
        true
    }
    
    /// Broadcast two shapes.
    pub fn broadcast(a: &Shape, b: &Shape) -> GpuResult<Shape> {
        if !a.is_broadcastable_with(b) {
            return Err(GpuError::DimensionMismatch(
                format!("Cannot broadcast {:?} with {:?}", a.dims, b.dims)
            ));
        }
        
        let max_ndim = a.ndim().max(b.ndim());
        let mut result = vec![1; max_ndim];
        
        for i in 0..max_ndim {
            let d1 = a.dims.get(a.ndim().saturating_sub(i + 1)).copied().unwrap_or(1);
            let d2 = b.dims.get(b.ndim().saturating_sub(i + 1)).copied().unwrap_or(1);
            result[max_ndim - i - 1] = d1.max(d2);
        }
        
        Ok(Shape::new(result))
    }
    
    /// Calculate strides for contiguous layout.
    pub fn strides(&self) -> Vec<usize> {
        let mut strides = vec![1; self.ndim()];
        for i in (0..self.ndim().saturating_sub(1)).rev() {
            strides[i] = strides[i + 1] * self.dims[i + 1];
        }
        strides
    }
}

impl std::fmt::Display for Shape {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "(")?;
        for (i, d) in self.dims.iter().enumerate() {
            if i > 0 {
                write!(f, ", ")?;
            }
            write!(f, "{}", d)?;
        }
        if self.dims.len() == 1 {
            write!(f, ",")?;
        }
        write!(f, ")")
    }
}

/// GPU Tensor.
pub struct Tensor {
    /// Underlying device memory.
    data: DevicePtr,
    
    /// Shape.
    shape: Shape,
    
    /// Data type.
    dtype: DType,
    
    /// Strides.
    strides: Vec<usize>,
    
    /// Offset into data.
    offset: usize,
    
    /// Device.
    device: Device,
    
    /// Is contiguous.
    contiguous: bool,
}

impl Tensor {
    /// Create a new tensor from shape.
    pub fn zeros(shape: Shape, dtype: DType, device: &Device) -> GpuResult<Self> {
        let size = shape.numel() * dtype.size();
        let data = device.alloc(size)?;
        let strides = shape.strides();
        
        // Zero initialize
        let zeros = vec![0u8; size];
        device.backend().copy_htod(zeros.as_ptr(), data.ptr, size)?;
        
        Ok(Self {
            data,
            shape,
            dtype,
            strides,
            offset: 0,
            device: device.clone(),
            contiguous: true,
        })
    }
    
    /// Create a tensor filled with ones.
    pub fn ones(shape: Shape, dtype: DType, device: &Device) -> GpuResult<Self> {
        let tensor = Self::zeros(shape, dtype, device)?;
        // TODO: Fill with ones kernel
        Ok(tensor)
    }
    
    /// Create a tensor from host data.
    pub fn from_slice<T: Copy>(
        data: &[T],
        shape: Shape,
        device: &Device,
    ) -> GpuResult<Self> {
        let dtype = dtype_from_type::<T>()?;
        
        if data.len() != shape.numel() {
            return Err(GpuError::InvalidShape(
                format!("Data length {} doesn't match shape {:?}", data.len(), shape.dims())
            ));
        }
        
        let ptr = device.copy_to_device(data)?;
        let strides = shape.strides();
        
        Ok(Self {
            data: ptr,
            shape,
            dtype,
            strides,
            offset: 0,
            device: device.clone(),
            contiguous: true,
        })
    }
    
    /// Create a tensor with random values.
    pub fn rand(shape: Shape, dtype: DType, device: &Device) -> GpuResult<Self> {
        // For now, create zeros and fill with random on CPU
        // TODO: GPU random kernel
        let numel = shape.numel();
        let mut host_data = vec![0f32; numel];
        
        use std::time::{SystemTime, UNIX_EPOCH};
        let seed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos() as u64;
        
        // Simple LCG random
        let mut state = seed;
        for i in 0..numel {
            state = state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            host_data[i] = (state as f32) / (u64::MAX as f32);
        }
        
        Self::from_slice(&host_data, shape, device)
    }
    
    /// Create a tensor with uniform random values in range.
    pub fn uniform(
        shape: Shape,
        low: f32,
        high: f32,
        dtype: DType,
        device: &Device,
    ) -> GpuResult<Self> {
        let tensor = Self::rand(shape, dtype, device)?;
        // TODO: Scale to range [low, high]
        Ok(tensor)
    }
    
    /// Create a tensor with normal random values.
    pub fn randn(shape: Shape, dtype: DType, device: &Device) -> GpuResult<Self> {
        // Box-Muller transform for normal distribution
        let numel = shape.numel();
        let mut host_data = vec![0f32; numel];
        
        use std::time::{SystemTime, UNIX_EPOCH};
        let seed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos() as u64;
        
        let mut state = seed;
        for i in (0..numel).step_by(2) {
            state = state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            let u1 = (state as f64) / (u64::MAX as f64);
            state = state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            let u2 = (state as f64) / (u64::MAX as f64);
            
            let r = (-2.0 * u1.max(1e-10).ln()).sqrt();
            let theta = 2.0 * std::f64::consts::PI * u2;
            
            host_data[i] = (r * theta.cos()) as f32;
            if i + 1 < numel {
                host_data[i + 1] = (r * theta.sin()) as f32;
            }
        }
        
        Self::from_slice(&host_data, shape, device)
    }
    
    /// Create identity matrix.
    pub fn eye(n: usize, dtype: DType, device: &Device) -> GpuResult<Self> {
        let shape = Shape::d2(n, n);
        let mut host_data = vec![0f32; n * n];
        for i in 0..n {
            host_data[i * n + i] = 1.0;
        }
        Self::from_slice(&host_data, shape, device)
    }
    
    /// Create tensor with given value.
    pub fn full(shape: Shape, value: f32, dtype: DType, device: &Device) -> GpuResult<Self> {
        let numel = shape.numel();
        let host_data = vec![value; numel];
        Self::from_slice(&host_data, shape, device)
    }
    
    /// Create range tensor.
    pub fn arange(start: f32, end: f32, step: f32, device: &Device) -> GpuResult<Self> {
        let numel = ((end - start) / step).ceil() as usize;
        let mut host_data = Vec::with_capacity(numel);
        let mut val = start;
        while val < end {
            host_data.push(val);
            val += step;
        }
        let shape = Shape::d1(host_data.len());
        Self::from_slice(&host_data, shape, device)
    }
    
    /// Create linspace tensor.
    pub fn linspace(start: f32, end: f32, steps: usize, device: &Device) -> GpuResult<Self> {
        let mut host_data = Vec::with_capacity(steps);
        let step = (end - start) / (steps - 1) as f32;
        for i in 0..steps {
            host_data.push(start + step * i as f32);
        }
        let shape = Shape::d1(steps);
        Self::from_slice(&host_data, shape, device)
    }
    
    /// Get shape.
    pub fn shape(&self) -> &Shape {
        &self.shape
    }
    
    /// Get dtype.
    pub fn dtype(&self) -> DType {
        self.dtype
    }
    
    /// Get device.
    pub fn device(&self) -> &Device {
        &self.device
    }
    
    /// Number of elements.
    pub fn numel(&self) -> usize {
        self.shape.numel()
    }
    
    /// Number of dimensions.
    pub fn ndim(&self) -> usize {
        self.shape.ndim()
    }
    
    /// Size in bytes.
    pub fn size_bytes(&self) -> usize {
        self.numel() * self.dtype.size()
    }
    
    /// Is contiguous.
    pub fn is_contiguous(&self) -> bool {
        self.contiguous
    }
    
    /// Get raw device pointer.
    pub fn data_ptr(&self) -> DevicePtr {
        self.data.offset(self.offset * self.dtype.size())
    }
    
    /// Copy to host.
    pub fn to_cpu<T: Copy + Default>(&self) -> GpuResult<Vec<T>> {
        let numel = self.numel();
        let mut data = vec![T::default(); numel];
        self.device.copy_to_host(&self.data, &mut data)?;
        Ok(data)
    }
    
    /// Copy to another device.
    pub fn to_device(&self, device: &Device) -> GpuResult<Tensor> {
        // If same device, just clone reference
        // TODO: Actually copy
        let size = self.size_bytes();
        let new_ptr = device.alloc(size)?;
        
        // Copy via host (TODO: peer-to-peer if supported)
        let host_data: Vec<u8> = self.to_cpu()?;
        device.backend().copy_htod(host_data.as_ptr(), new_ptr.ptr, size)?;
        
        Ok(Tensor {
            data: new_ptr,
            shape: self.shape.clone(),
            dtype: self.dtype,
            strides: self.strides.clone(),
            offset: 0,
            device: device.clone(),
            contiguous: true,
        })
    }
    
    /// Reshape tensor.
    pub fn reshape(&self, new_shape: Shape) -> GpuResult<Tensor> {
        if new_shape.numel() != self.numel() {
            return Err(GpuError::InvalidShape(
                format!("Cannot reshape {:?} to {:?}", self.shape.dims(), new_shape.dims())
            ));
        }
        
        if !self.contiguous {
            // Need to make contiguous first
            return Err(GpuError::Unsupported(
                "Reshape of non-contiguous tensor".to_string()
            ));
        }
        
        Ok(Tensor {
            data: self.data,
            shape: new_shape.clone(),
            dtype: self.dtype,
            strides: new_shape.strides(),
            offset: self.offset,
            device: self.device.clone(),
            contiguous: true,
        })
    }
    
    /// View tensor with new shape.
    pub fn view(&self, new_shape: Shape) -> GpuResult<TensorView> {
        if new_shape.numel() != self.numel() {
            return Err(GpuError::InvalidShape(
                format!("Cannot view {:?} as {:?}", self.shape.dims(), new_shape.dims())
            ));
        }
        
        Ok(TensorView {
            tensor: self,
            shape: new_shape.clone(),
            strides: new_shape.strides(),
            offset: self.offset,
        })
    }
    
    /// Transpose.
    pub fn transpose(&self, dim0: usize, dim1: usize) -> GpuResult<Tensor> {
        if dim0 >= self.ndim() || dim1 >= self.ndim() {
            return Err(GpuError::InvalidShape(
                format!("Transpose dims {} and {} out of range for {}D tensor",
                    dim0, dim1, self.ndim())
            ));
        }
        
        let mut new_shape = self.shape.dims().to_vec();
        let mut new_strides = self.strides.clone();
        
        new_shape.swap(dim0, dim1);
        new_strides.swap(dim0, dim1);
        
        Ok(Tensor {
            data: self.data,
            shape: Shape::new(new_shape),
            dtype: self.dtype,
            strides: new_strides,
            offset: self.offset,
            device: self.device.clone(),
            contiguous: false,
        })
    }
    
    /// Matrix transpose (for 2D).
    pub fn t(&self) -> GpuResult<Tensor> {
        if self.ndim() != 2 {
            return Err(GpuError::InvalidShape(
                format!("t() only works on 2D tensors, got {}D", self.ndim())
            ));
        }
        self.transpose(0, 1)
    }
    
    /// Squeeze dimensions of size 1.
    pub fn squeeze(&self, dim: Option<usize>) -> Tensor {
        let new_dims: Vec<usize> = if let Some(d) = dim {
            self.shape.dims().iter().enumerate()
                .filter(|(i, &s)| *i != d || s != 1)
                .map(|(_, &s)| s)
                .collect()
        } else {
            self.shape.dims().iter()
                .filter(|&&s| s != 1)
                .copied()
                .collect()
        };
        
        let new_shape = Shape::new(if new_dims.is_empty() { vec![1] } else { new_dims });
        
        Tensor {
            data: self.data,
            shape: new_shape.clone(),
            dtype: self.dtype,
            strides: new_shape.strides(),
            offset: self.offset,
            device: self.device.clone(),
            contiguous: self.contiguous,
        }
    }
    
    /// Unsqueeze - add dimension.
    pub fn unsqueeze(&self, dim: usize) -> GpuResult<Tensor> {
        if dim > self.ndim() {
            return Err(GpuError::InvalidShape(
                format!("Unsqueeze dim {} out of range for {}D tensor", dim, self.ndim())
            ));
        }
        
        let mut new_dims = self.shape.dims().to_vec();
        new_dims.insert(dim, 1);
        let new_shape = Shape::new(new_dims);
        
        Ok(Tensor {
            data: self.data,
            shape: new_shape.clone(),
            dtype: self.dtype,
            strides: new_shape.strides(),
            offset: self.offset,
            device: self.device.clone(),
            contiguous: self.contiguous,
        })
    }
    
    /// Clone tensor.
    pub fn clone_tensor(&self) -> GpuResult<Tensor> {
        let size = self.size_bytes();
        let new_ptr = self.device.alloc(size)?;
        self.device.copy_device_to_device(&self.data, &new_ptr, size)?;
        
        Ok(Tensor {
            data: new_ptr,
            shape: self.shape.clone(),
            dtype: self.dtype,
            strides: self.strides.clone(),
            offset: 0,
            device: self.device.clone(),
            contiguous: true,
        })
    }
}

impl std::fmt::Debug for Tensor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Tensor(shape={}, dtype={}, device={})",
            self.shape, self.dtype.name(), self.device.name())
    }
}

/// Tensor view (non-owning reference).
pub struct TensorView<'a> {
    tensor: &'a Tensor,
    shape: Shape,
    strides: Vec<usize>,
    offset: usize,
}

impl<'a> TensorView<'a> {
    pub fn shape(&self) -> &Shape {
        &self.shape
    }
    
    pub fn dtype(&self) -> DType {
        self.tensor.dtype
    }
    
    pub fn numel(&self) -> usize {
        self.shape.numel()
    }
}

/// Mutable tensor view.
pub struct TensorMut<'a> {
    tensor: &'a mut Tensor,
    shape: Shape,
    strides: Vec<usize>,
    offset: usize,
}

impl<'a> TensorMut<'a> {
    pub fn shape(&self) -> &Shape {
        &self.shape
    }
    
    pub fn dtype(&self) -> DType {
        self.tensor.dtype
    }
}

/// Get DType from Rust type.
fn dtype_from_type<T: Copy>() -> GpuResult<DType> {
    let type_name = std::any::type_name::<T>();
    
    match type_name {
        "f32" => Ok(DType::Float32),
        "f64" => Ok(DType::Float64),
        "i8" => Ok(DType::Int8),
        "i16" => Ok(DType::Int16),
        "i32" => Ok(DType::Int32),
        "i64" => Ok(DType::Int64),
        "u8" => Ok(DType::UInt8),
        "u16" => Ok(DType::UInt16),
        "u32" => Ok(DType::UInt32),
        "u64" => Ok(DType::UInt64),
        "bool" => Ok(DType::Bool),
        _ => Err(GpuError::TypeMismatch {
            expected: "numeric type".to_string(),
            got: type_name.to_string(),
        }),
    }
}

