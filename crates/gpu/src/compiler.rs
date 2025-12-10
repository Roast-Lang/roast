//! GPU kernel compiler.

use std::collections::HashMap;
use std::path::PathBuf;
use crate::backend::BackendType;
use crate::error::{GpuError, GpuResult};

/// Kernel source language.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceLanguage {
    /// CUDA C/C++.
    Cuda,
    /// PTX assembly.
    Ptx,
    /// OpenCL C.
    OpenCL,
    /// SPIR-V.
    SpirV,
    /// Metal Shading Language.
    Metal,
    /// HLSL.
    Hlsl,
    /// Roast kernel DSL.
    Roast,
}

/// Compilation options.
#[derive(Debug, Clone)]
pub struct CompileOptions {
    /// Optimization level (0-3).
    pub opt_level: u32,
    
    /// Generate debug info.
    pub debug: bool,
    
    /// Fast math.
    pub fast_math: bool,
    
    /// Target architecture (e.g., "sm_86" for CUDA).
    pub target_arch: Option<String>,
    
    /// Include paths.
    pub include_paths: Vec<PathBuf>,
    
    /// Preprocessor defines.
    pub defines: HashMap<String, String>,
    
    /// Extra compiler flags.
    pub extra_flags: Vec<String>,
    
    /// Verbose output.
    pub verbose: bool,
}

impl Default for CompileOptions {
    fn default() -> Self {
        Self {
            opt_level: 2,
            debug: false,
            fast_math: true,
            target_arch: None,
            include_paths: Vec::new(),
            defines: HashMap::new(),
            extra_flags: Vec::new(),
            verbose: false,
        }
    }
}

/// Compiled kernel output.
#[derive(Debug)]
pub struct CompiledOutput {
    /// Binary code.
    pub binary: Vec<u8>,
    
    /// Source language.
    pub source_lang: SourceLanguage,
    
    /// Target backend.
    pub backend: BackendType,
    
    /// Kernel entry points.
    pub entry_points: Vec<String>,
    
    /// Compilation log.
    pub log: String,
    
    /// Resource usage (registers, shared memory, etc.).
    pub resources: KernelResources,
}

/// Kernel resource usage.
#[derive(Debug, Default)]
pub struct KernelResources {
    /// Registers per thread.
    pub registers: u32,
    
    /// Shared memory per block (bytes).
    pub shared_mem: u32,
    
    /// Local memory per thread (bytes).
    pub local_mem: u32,
    
    /// Constant memory (bytes).
    pub const_mem: u32,
    
    /// Max threads per block.
    pub max_threads: u32,
}

/// Kernel compiler.
pub struct KernelCompiler {
    /// Options.
    options: CompileOptions,
    
    /// Include headers.
    headers: HashMap<String, String>,
}

impl KernelCompiler {
    /// Create a new compiler.
    pub fn new(options: CompileOptions) -> Self {
        Self {
            options,
            headers: HashMap::new(),
        }
    }
    
    /// Add a header.
    pub fn add_header(&mut self, name: &str, content: &str) {
        self.headers.insert(name.to_string(), content.to_string());
    }
    
    /// Compile CUDA source to PTX.
    pub fn compile_cuda(&self, source: &str) -> GpuResult<CompiledOutput> {
        // This would use NVRTC
        // For now, return stub
        
        let ptx = self.generate_stub_ptx(source)?;
        
        Ok(CompiledOutput {
            binary: ptx.into_bytes(),
            source_lang: SourceLanguage::Cuda,
            backend: BackendType::Cuda,
            entry_points: extract_kernel_names(source),
            log: String::new(),
            resources: KernelResources::default(),
        })
    }
    
    /// Compile OpenCL source.
    pub fn compile_opencl(&self, source: &str) -> GpuResult<CompiledOutput> {
        Ok(CompiledOutput {
            binary: source.as_bytes().to_vec(),
            source_lang: SourceLanguage::OpenCL,
            backend: BackendType::OpenCL,
            entry_points: extract_kernel_names(source),
            log: String::new(),
            resources: KernelResources::default(),
        })
    }
    
    /// Compile Roast kernel DSL to target backend.
    pub fn compile_roast(&self, source: &str, backend: BackendType) -> GpuResult<CompiledOutput> {
        match backend {
            BackendType::Cuda => {
                let cuda_source = self.translate_roast_to_cuda(source)?;
                self.compile_cuda(&cuda_source)
            }
            BackendType::OpenCL => {
                let opencl_source = self.translate_roast_to_opencl(source)?;
                self.compile_opencl(&opencl_source)
            }
            _ => Err(GpuError::Unsupported(
                format!("Roast to {:?} not supported", backend)
            )),
        }
    }
    
    /// Translate Roast kernel DSL to CUDA.
    fn translate_roast_to_cuda(&self, source: &str) -> GpuResult<String> {
        // Parse Roast kernel syntax and generate CUDA
        // For now, assume source is already valid CUDA-like syntax
        
        let mut output = String::new();
        
        // Add standard headers
        output.push_str("#include <cuda_runtime.h>\n");
        output.push_str("#include <math.h>\n\n");
        
        // Add user headers
        for (name, content) in &self.headers {
            output.push_str(&format!("// Header: {}\n{}\n", name, content));
        }
        
        // Translate the source
        // This is a simplified translation
        for line in source.lines() {
            let line = line.trim();
            
            if line.starts_with("@kernel") {
                // Skip decorator, next line is function def
                continue;
            }
            
            if line.starts_with("def ") && line.contains("->") {
                // Convert Python-style function to CUDA kernel
                let cuda_line = self.translate_function_def(line)?;
                output.push_str(&cuda_line);
                output.push('\n');
            } else if line.starts_with("idx = thread_idx()") {
                output.push_str("    int idx = blockIdx.x * blockDim.x + threadIdx.x;\n");
            } else if line.starts_with("if ") {
                let cuda_line = self.translate_if(line)?;
                output.push_str(&cuda_line);
                output.push('\n');
            } else {
                // General line translation
                let cuda_line = self.translate_statement(line)?;
                output.push_str(&cuda_line);
                output.push('\n');
            }
        }
        
        Ok(output)
    }
    
    fn translate_function_def(&self, line: &str) -> GpuResult<String> {
        // def kernel_name(a: Tensor[float], b: Tensor[float]) -> None:
        // becomes:
        // extern "C" __global__ void kernel_name(float *a, float *b) {
        
        let line = line.trim_start_matches("def ").trim_end_matches(':');
        let parts: Vec<&str> = line.split("->").collect();
        
        let sig = parts[0].trim();
        
        // Parse function name and params
        let paren_start = sig.find('(').ok_or_else(|| {
            GpuError::Compilation("Missing '(' in function def".to_string())
        })?;
        let paren_end = sig.rfind(')').ok_or_else(|| {
            GpuError::Compilation("Missing ')' in function def".to_string())
        })?;
        
        let name = &sig[..paren_start];
        let params_str = &sig[paren_start + 1..paren_end];
        
        // Convert parameters
        let mut cuda_params = Vec::new();
        for param in params_str.split(',') {
            let param = param.trim();
            if param.is_empty() {
                continue;
            }
            
            let cuda_param = self.translate_param(param)?;
            cuda_params.push(cuda_param);
        }
        
        Ok(format!(
            "extern \"C\" __global__ void {}({}) {{",
            name,
            cuda_params.join(", ")
        ))
    }
    
    fn translate_param(&self, param: &str) -> GpuResult<String> {
        // a: Tensor[float] -> float *a
        // n: int -> int n
        
        let parts: Vec<&str> = param.split(':').collect();
        if parts.len() != 2 {
            return Err(GpuError::Compilation(
                format!("Invalid parameter: {}", param)
            ));
        }
        
        let name = parts[0].trim();
        let type_str = parts[1].trim();
        
        if type_str.starts_with("Tensor[") {
            // Extract element type
            let elem_type = type_str
                .trim_start_matches("Tensor[")
                .trim_end_matches(']');
            let cuda_type = self.translate_type(elem_type)?;
            Ok(format!("{} *{}", cuda_type, name))
        } else {
            let cuda_type = self.translate_type(type_str)?;
            Ok(format!("{} {}", cuda_type, name))
        }
    }
    
    fn translate_type(&self, type_str: &str) -> GpuResult<String> {
        Ok(match type_str {
            "float" | "f32" => "float",
            "double" | "f64" => "double",
            "int" | "i32" => "int",
            "long" | "i64" => "long long",
            "uint" | "u32" => "unsigned int",
            "bool" => "bool",
            _ => "float", // Default
        }.to_string())
    }
    
    fn translate_if(&self, line: &str) -> GpuResult<String> {
        // if idx < len(a): -> if (idx < n) {
        let cond = line.trim_start_matches("if ")
            .trim_end_matches(':');
        
        // Replace Python-style comparisons
        let cuda_cond = cond
            .replace("len(", "/* len */ ")
            .replace(")", "");
        
        Ok(format!("    if ({}) {{", cuda_cond))
    }
    
    fn translate_statement(&self, line: &str) -> GpuResult<String> {
        if line.is_empty() {
            return Ok(String::new());
        }
        
        // c[idx] = a[idx] + b[idx] stays mostly the same
        let translated = line
            .replace("True", "true")
            .replace("False", "false")
            .replace("None", "nullptr")
            .replace("and", "&&")
            .replace("or", "||")
            .replace("not ", "!");
        
        if translated.contains('=') && !translated.contains("==") {
            Ok(format!("    {};", translated))
        } else {
            Ok(format!("    {}", translated))
        }
    }
    
    fn translate_roast_to_opencl(&self, source: &str) -> GpuResult<String> {
        // Similar to CUDA but with OpenCL syntax
        let mut output = String::new();
        
        for line in source.lines() {
            let line = line.trim();
            
            if line.starts_with("@kernel") {
                continue;
            }
            
            if line.starts_with("def ") {
                // Convert to OpenCL kernel
                output.push_str(&self.translate_function_def_opencl(line)?);
                output.push('\n');
            } else if line.starts_with("idx = thread_idx()") {
                output.push_str("    int idx = get_global_id(0);\n");
            } else {
                output.push_str(&self.translate_statement(line)?);
                output.push('\n');
            }
        }
        
        Ok(output)
    }
    
    fn translate_function_def_opencl(&self, line: &str) -> GpuResult<String> {
        let line = line.trim_start_matches("def ").trim_end_matches(':');
        let parts: Vec<&str> = line.split("->").collect();
        let sig = parts[0].trim();
        
        let paren_start = sig.find('(').unwrap();
        let paren_end = sig.rfind(')').unwrap();
        
        let name = &sig[..paren_start];
        let params_str = &sig[paren_start + 1..paren_end];
        
        let mut opencl_params = Vec::new();
        for param in params_str.split(',') {
            let param = param.trim();
            if param.is_empty() {
                continue;
            }
            
            let parts: Vec<&str> = param.split(':').collect();
            if parts.len() == 2 {
                let pname = parts[0].trim();
                let ptype = parts[1].trim();
                
                if ptype.starts_with("Tensor[") {
                    let elem = ptype.trim_start_matches("Tensor[").trim_end_matches(']');
                    let ctype = self.translate_type(elem)?;
                    opencl_params.push(format!("__global {} *{}", ctype, pname));
                } else {
                    let ctype = self.translate_type(ptype)?;
                    opencl_params.push(format!("{} {}", ctype, pname));
                }
            }
        }
        
        Ok(format!(
            "__kernel void {}({}) {{",
            name,
            opencl_params.join(", ")
        ))
    }
    
    fn generate_stub_ptx(&self, _source: &str) -> GpuResult<String> {
        // Generate minimal valid PTX
        Ok(r#"
.version 7.0
.target sm_50
.address_size 64

.visible .entry stub_kernel()
{
    ret;
}
"#.to_string())
    }
}

/// Extract kernel names from source.
fn extract_kernel_names(source: &str) -> Vec<String> {
    let mut names = Vec::new();
    
    for line in source.lines() {
        let line = line.trim();
        
        // CUDA: __global__ void name
        if line.contains("__global__") {
            if let Some(name) = extract_function_name(line) {
                names.push(name);
            }
        }
        
        // OpenCL: __kernel void name
        if line.contains("__kernel") {
            if let Some(name) = extract_function_name(line) {
                names.push(name);
            }
        }
        
        // Roast: @kernel decorator
        if line.starts_with("def ") {
            let parts: Vec<&str> = line.split('(').collect();
            if !parts.is_empty() {
                let name = parts[0].trim_start_matches("def ").trim();
                names.push(name.to_string());
            }
        }
    }
    
    names
}

fn extract_function_name(line: &str) -> Option<String> {
    // Find 'void name(' pattern
    if let Some(void_pos) = line.find("void ") {
        let after_void = &line[void_pos + 5..];
        if let Some(paren_pos) = after_void.find('(') {
            let name = after_void[..paren_pos].trim();
            return Some(name.to_string());
        }
    }
    None
}

/// Built-in kernel library.
pub mod builtins {
    /// Common math functions.
    pub const MATH_HEADER: &str = r#"
#ifndef ROAST_MATH_H
#define ROAST_MATH_H

__device__ __forceinline__ float roast_sigmoid(float x) {
    return 1.0f / (1.0f + expf(-x));
}

__device__ __forceinline__ float roast_relu(float x) {
    return fmaxf(0.0f, x);
}

__device__ __forceinline__ float roast_gelu(float x) {
    const float c = 0.7978845608028654f; // sqrt(2/pi)
    return 0.5f * x * (1.0f + tanhf(c * (x + 0.044715f * x * x * x)));
}

__device__ __forceinline__ float roast_silu(float x) {
    return x * roast_sigmoid(x);
}

__device__ __forceinline__ float roast_softplus(float x) {
    return logf(1.0f + expf(x));
}

#endif
"#;

    /// Reduction utilities.
    pub const REDUCTION_HEADER: &str = r#"
#ifndef ROAST_REDUCTION_H
#define ROAST_REDUCTION_H

template<typename T, int BLOCK_SIZE>
__device__ __forceinline__ T block_reduce_sum(T val) {
    __shared__ T shared[BLOCK_SIZE];
    int tid = threadIdx.x;
    shared[tid] = val;
    __syncthreads();
    
    for (int s = BLOCK_SIZE / 2; s > 0; s >>= 1) {
        if (tid < s) {
            shared[tid] += shared[tid + s];
        }
        __syncthreads();
    }
    return shared[0];
}

template<typename T, int BLOCK_SIZE>
__device__ __forceinline__ T block_reduce_max(T val) {
    __shared__ T shared[BLOCK_SIZE];
    int tid = threadIdx.x;
    shared[tid] = val;
    __syncthreads();
    
    for (int s = BLOCK_SIZE / 2; s > 0; s >>= 1) {
        if (tid < s) {
            shared[tid] = max(shared[tid], shared[tid + s]);
        }
        __syncthreads();
    }
    return shared[0];
}

#endif
"#;
}

