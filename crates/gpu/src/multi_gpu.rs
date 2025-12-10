//! Multi-GPU support: peer-to-peer, NCCL, and distributed operations.

use std::sync::Arc;
use std::ptr;
use libloading::{Library, Symbol};
use crate::device::{Device, DeviceInfo, DeviceType};
use crate::tensor::Tensor;
use crate::error::{GpuError, GpuResult};

// =============================================================================
// NCCL Types
// =============================================================================

/// NCCL communicator handle.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct NcclComm {
    _opaque: *mut std::ffi::c_void,
}

impl NcclComm {
    pub fn null() -> Self {
        Self { _opaque: ptr::null_mut() }
    }
    
    pub fn is_null(&self) -> bool {
        self._opaque.is_null()
    }
}

/// NCCL unique ID for initialization.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct NcclUniqueId {
    internal: [u8; 128],
}

impl NcclUniqueId {
    pub fn new() -> Self {
        Self { internal: [0u8; 128] }
    }
}

impl Default for NcclUniqueId {
    fn default() -> Self {
        Self::new()
    }
}

/// NCCL result code.
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NcclResult {
    Success = 0,
    UnhandledCudaError = 1,
    SystemError = 2,
    InternalError = 3,
    InvalidArgument = 4,
    InvalidUsage = 5,
    RemoteError = 6,
    InProgress = 7,
    NumResults = 8,
}

impl NcclResult {
    pub fn from_code(code: i32) -> Self {
        match code {
            0 => Self::Success,
            1 => Self::UnhandledCudaError,
            2 => Self::SystemError,
            3 => Self::InternalError,
            4 => Self::InvalidArgument,
            5 => Self::InvalidUsage,
            6 => Self::RemoteError,
            7 => Self::InProgress,
            _ => Self::InternalError,
        }
    }
    
    pub fn is_success(&self) -> bool {
        *self == Self::Success
    }
    
    pub fn to_error(&self) -> GpuError {
        GpuError::Cuda(format!("NCCL error: {:?}", self))
    }
}

/// NCCL data type.
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NcclDataType {
    Int8 = 0,
    Uint8 = 1,
    Int32 = 2,
    Uint32 = 3,
    Int64 = 4,
    Uint64 = 5,
    Float16 = 6,
    Float32 = 7,
    Float64 = 8,
    Bfloat16 = 9,
}

/// NCCL reduction operation.
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NcclRedOp {
    Sum = 0,
    Prod = 1,
    Max = 2,
    Min = 3,
    Avg = 4,
}

// =============================================================================
// NCCL Function Types
// =============================================================================

type NcclGetVersion = unsafe extern "C" fn(*mut i32) -> i32;
type NcclGetUniqueId = unsafe extern "C" fn(*mut NcclUniqueId) -> i32;
type NcclCommInitRank = unsafe extern "C" fn(*mut NcclComm, i32, NcclUniqueId, i32) -> i32;
type NcclCommInitAll = unsafe extern "C" fn(*mut NcclComm, i32, *const i32) -> i32;
type NcclCommDestroy = unsafe extern "C" fn(NcclComm) -> i32;
type NcclCommCount = unsafe extern "C" fn(NcclComm, *mut i32) -> i32;
type NcclCommCuDevice = unsafe extern "C" fn(NcclComm, *mut i32) -> i32;
type NcclCommUserRank = unsafe extern "C" fn(NcclComm, *mut i32) -> i32;

// Collective operations
type NcclAllReduce = unsafe extern "C" fn(
    *const std::ffi::c_void, *mut std::ffi::c_void,
    usize, i32, i32, NcclComm, usize,
) -> i32;
type NcclBroadcast = unsafe extern "C" fn(
    *const std::ffi::c_void, *mut std::ffi::c_void,
    usize, i32, i32, NcclComm, usize,
) -> i32;
type NcclReduce = unsafe extern "C" fn(
    *const std::ffi::c_void, *mut std::ffi::c_void,
    usize, i32, i32, i32, NcclComm, usize,
) -> i32;
type NcclAllGather = unsafe extern "C" fn(
    *const std::ffi::c_void, *mut std::ffi::c_void,
    usize, i32, NcclComm, usize,
) -> i32;
type NcclReduceScatter = unsafe extern "C" fn(
    *const std::ffi::c_void, *mut std::ffi::c_void,
    usize, i32, i32, NcclComm, usize,
) -> i32;
type NcclSend = unsafe extern "C" fn(
    *const std::ffi::c_void, usize, i32, i32, NcclComm, usize,
) -> i32;
type NcclRecv = unsafe extern "C" fn(
    *mut std::ffi::c_void, usize, i32, i32, NcclComm, usize,
) -> i32;
type NcclGroupStart = unsafe extern "C" fn() -> i32;
type NcclGroupEnd = unsafe extern "C" fn() -> i32;

// =============================================================================
// NCCL Library
// =============================================================================

/// NCCL library handle.
pub struct NcclLib {
    _lib: Library,
    get_version: Symbol<'static, NcclGetVersion>,
    get_unique_id: Symbol<'static, NcclGetUniqueId>,
    comm_init_rank: Symbol<'static, NcclCommInitRank>,
    comm_init_all: Symbol<'static, NcclCommInitAll>,
    comm_destroy: Symbol<'static, NcclCommDestroy>,
    comm_count: Symbol<'static, NcclCommCount>,
    comm_cu_device: Symbol<'static, NcclCommCuDevice>,
    comm_user_rank: Symbol<'static, NcclCommUserRank>,
    all_reduce: Symbol<'static, NcclAllReduce>,
    broadcast: Symbol<'static, NcclBroadcast>,
    reduce: Symbol<'static, NcclReduce>,
    all_gather: Symbol<'static, NcclAllGather>,
    reduce_scatter: Symbol<'static, NcclReduceScatter>,
    send: Symbol<'static, NcclSend>,
    recv: Symbol<'static, NcclRecv>,
    group_start: Symbol<'static, NcclGroupStart>,
    group_end: Symbol<'static, NcclGroupEnd>,
}

impl NcclLib {
    /// Load the NCCL library.
    pub fn load() -> GpuResult<Self> {
        let lib_names = if cfg!(windows) {
            vec!["nccl.dll"]
        } else if cfg!(target_os = "macos") {
            vec!["libnccl.dylib"]
        } else {
            vec!["libnccl.so.2", "libnccl.so"]
        };
        
        let mut last_error = None;
        for name in lib_names {
            match unsafe { Library::new(name) } {
                Ok(lib) => {
                    return Self::from_library(lib);
                }
                Err(e) => {
                    last_error = Some(e);
                }
            }
        }
        
        Err(GpuError::DriverNotFound(format!(
            "NCCL library not found: {:?}",
            last_error
        )))
    }
    
    fn from_library(lib: Library) -> GpuResult<Self> {
        unsafe {
            let lib = Box::leak(Box::new(lib));
            
            macro_rules! load_symbol {
                ($name:ident, $ty:ty, $sym:expr) => {
                    let $name: Symbol<$ty> = lib
                        .get($sym)
                        .map_err(|e| GpuError::DriverNotFound(e.to_string()))?;
                    let $name: Symbol<'static, $ty> = std::mem::transmute($name);
                };
            }
            
            load_symbol!(get_version, NcclGetVersion, b"ncclGetVersion\0");
            load_symbol!(get_unique_id, NcclGetUniqueId, b"ncclGetUniqueId\0");
            load_symbol!(comm_init_rank, NcclCommInitRank, b"ncclCommInitRank\0");
            load_symbol!(comm_init_all, NcclCommInitAll, b"ncclCommInitAll\0");
            load_symbol!(comm_destroy, NcclCommDestroy, b"ncclCommDestroy\0");
            load_symbol!(comm_count, NcclCommCount, b"ncclCommCount\0");
            load_symbol!(comm_cu_device, NcclCommCuDevice, b"ncclCommCuDevice\0");
            load_symbol!(comm_user_rank, NcclCommUserRank, b"ncclCommUserRank\0");
            load_symbol!(all_reduce, NcclAllReduce, b"ncclAllReduce\0");
            load_symbol!(broadcast, NcclBroadcast, b"ncclBroadcast\0");
            load_symbol!(reduce, NcclReduce, b"ncclReduce\0");
            load_symbol!(all_gather, NcclAllGather, b"ncclAllGather\0");
            load_symbol!(reduce_scatter, NcclReduceScatter, b"ncclReduceScatter\0");
            load_symbol!(send, NcclSend, b"ncclSend\0");
            load_symbol!(recv, NcclRecv, b"ncclRecv\0");
            load_symbol!(group_start, NcclGroupStart, b"ncclGroupStart\0");
            load_symbol!(group_end, NcclGroupEnd, b"ncclGroupEnd\0");
            
            let lib_ref = &*(lib as *const Library);
            
            Ok(Self {
                _lib: ptr::read(lib_ref),
                get_version, get_unique_id, comm_init_rank, comm_init_all,
                comm_destroy, comm_count, comm_cu_device, comm_user_rank,
                all_reduce, broadcast, reduce, all_gather, reduce_scatter,
                send, recv, group_start, group_end,
            })
        }
    }
    
    /// Get NCCL version.
    pub fn version(&self) -> GpuResult<i32> {
        let mut version = 0;
        let result = unsafe { (self.get_version)(&mut version) };
        
        if !NcclResult::from_code(result).is_success() {
            return Err(NcclResult::from_code(result).to_error());
        }
        
        Ok(version)
    }
    
    /// Get a unique ID for initialization.
    pub fn get_unique_id(&self) -> GpuResult<NcclUniqueId> {
        let mut id = NcclUniqueId::new();
        let result = unsafe { (self.get_unique_id)(&mut id) };
        
        if !NcclResult::from_code(result).is_success() {
            return Err(NcclResult::from_code(result).to_error());
        }
        
        Ok(id)
    }
}

// =============================================================================
// Multi-GPU Manager
// =============================================================================

/// Multi-GPU device manager.
pub struct MultiGpu {
    /// Available devices.
    devices: Vec<DeviceInfo>,
    
    /// Active device indices.
    active_devices: Vec<usize>,
    
    /// NCCL library (optional).
    nccl: Option<Arc<NcclLib>>,
    
    /// NCCL communicators.
    comms: Vec<NcclComm>,
}

impl MultiGpu {
    /// Create a new multi-GPU manager.
    pub fn new() -> GpuResult<Self> {
        let devices = Device::list_all()?;
        let nccl = NcclLib::load().ok().map(Arc::new);
        
        Ok(Self {
            devices,
            active_devices: Vec::new(),
            nccl,
            comms: Vec::new(),
        })
    }
    
    /// Get available devices.
    pub fn devices(&self) -> &[DeviceInfo] {
        &self.devices
    }
    
    /// Get number of GPUs.
    pub fn device_count(&self) -> usize {
        self.devices.iter()
            .filter(|d| d.device_type == DeviceType::Cuda)
            .count()
    }
    
    /// Check if NCCL is available.
    pub fn has_nccl(&self) -> bool {
        self.nccl.is_some()
    }
    
    /// Initialize NCCL for all GPUs.
    pub fn init_nccl(&mut self, device_ids: &[i32]) -> GpuResult<()> {
        let nccl = self.nccl.as_ref().ok_or_else(|| {
            GpuError::DriverNotFound("NCCL not available".to_string())
        })?;
        
        let n_devices = device_ids.len();
        let mut comms = vec![NcclComm::null(); n_devices];
        
        let result = unsafe {
            (nccl.comm_init_all)(
                comms.as_mut_ptr(),
                n_devices as i32,
                device_ids.as_ptr(),
            )
        };
        
        if !NcclResult::from_code(result).is_success() {
            return Err(NcclResult::from_code(result).to_error());
        }
        
        self.comms = comms;
        self.active_devices = device_ids.iter().map(|&i| i as usize).collect();
        
        Ok(())
    }
    
    /// Initialize NCCL for distributed training.
    pub fn init_nccl_rank(
        &mut self,
        n_ranks: i32,
        unique_id: NcclUniqueId,
        rank: i32,
    ) -> GpuResult<()> {
        let nccl = self.nccl.as_ref().ok_or_else(|| {
            GpuError::DriverNotFound("NCCL not available".to_string())
        })?;
        
        let mut comm = NcclComm::null();
        let result = unsafe {
            (nccl.comm_init_rank)(&mut comm, n_ranks, unique_id, rank)
        };
        
        if !NcclResult::from_code(result).is_success() {
            return Err(NcclResult::from_code(result).to_error());
        }
        
        self.comms = vec![comm];
        
        Ok(())
    }
    
    /// Destroy NCCL communicators.
    pub fn destroy_nccl(&mut self) -> GpuResult<()> {
        if let Some(nccl) = &self.nccl {
            for comm in &self.comms {
                if !comm.is_null() {
                    let result = unsafe { (nccl.comm_destroy)(*comm) };
                    if !NcclResult::from_code(result).is_success() {
                        return Err(NcclResult::from_code(result).to_error());
                    }
                }
            }
        }
        
        self.comms.clear();
        Ok(())
    }
    
    /// Get NCCL unique ID (for distributed init).
    pub fn get_nccl_unique_id(&self) -> GpuResult<NcclUniqueId> {
        let nccl = self.nccl.as_ref().ok_or_else(|| {
            GpuError::DriverNotFound("NCCL not available".to_string())
        })?;
        
        nccl.get_unique_id()
    }
}

impl Drop for MultiGpu {
    fn drop(&mut self) {
        let _ = self.destroy_nccl();
    }
}

// =============================================================================
// NCCL Collective Operations
// =============================================================================

/// NCCL collective operations context.
pub struct NcclOps {
    lib: Arc<NcclLib>,
    comms: Vec<NcclComm>,
}

impl NcclOps {
    /// Create from existing communicators.
    pub fn new(lib: Arc<NcclLib>, comms: Vec<NcclComm>) -> Self {
        Self { lib, comms }
    }
    
    /// AllReduce: sum across all GPUs.
    pub fn all_reduce_sum(
        &self,
        send_buff: usize,
        recv_buff: usize,
        count: usize,
        comm_idx: usize,
        stream: usize,
    ) -> GpuResult<()> {
        if comm_idx >= self.comms.len() {
            return Err(GpuError::InvalidConfig("Invalid communicator index".to_string()));
        }
        
        let result = unsafe {
            (self.lib.all_reduce)(
                send_buff as *const std::ffi::c_void,
                recv_buff as *mut std::ffi::c_void,
                count,
                NcclDataType::Float32 as i32,
                NcclRedOp::Sum as i32,
                self.comms[comm_idx],
                stream,
            )
        };
        
        if !NcclResult::from_code(result).is_success() {
            return Err(NcclResult::from_code(result).to_error());
        }
        
        Ok(())
    }
    
    /// Broadcast from root to all GPUs.
    pub fn broadcast(
        &self,
        send_buff: usize,
        recv_buff: usize,
        count: usize,
        root: i32,
        comm_idx: usize,
        stream: usize,
    ) -> GpuResult<()> {
        if comm_idx >= self.comms.len() {
            return Err(GpuError::InvalidConfig("Invalid communicator index".to_string()));
        }
        
        let result = unsafe {
            (self.lib.broadcast)(
                send_buff as *const std::ffi::c_void,
                recv_buff as *mut std::ffi::c_void,
                count,
                NcclDataType::Float32 as i32,
                root,
                self.comms[comm_idx],
                stream,
            )
        };
        
        if !NcclResult::from_code(result).is_success() {
            return Err(NcclResult::from_code(result).to_error());
        }
        
        Ok(())
    }
    
    /// AllGather: gather from all, distribute to all.
    pub fn all_gather(
        &self,
        send_buff: usize,
        recv_buff: usize,
        send_count: usize,
        comm_idx: usize,
        stream: usize,
    ) -> GpuResult<()> {
        if comm_idx >= self.comms.len() {
            return Err(GpuError::InvalidConfig("Invalid communicator index".to_string()));
        }
        
        let result = unsafe {
            (self.lib.all_gather)(
                send_buff as *const std::ffi::c_void,
                recv_buff as *mut std::ffi::c_void,
                send_count,
                NcclDataType::Float32 as i32,
                self.comms[comm_idx],
                stream,
            )
        };
        
        if !NcclResult::from_code(result).is_success() {
            return Err(NcclResult::from_code(result).to_error());
        }
        
        Ok(())
    }
    
    /// ReduceScatter: reduce and scatter results.
    pub fn reduce_scatter(
        &self,
        send_buff: usize,
        recv_buff: usize,
        recv_count: usize,
        comm_idx: usize,
        stream: usize,
    ) -> GpuResult<()> {
        if comm_idx >= self.comms.len() {
            return Err(GpuError::InvalidConfig("Invalid communicator index".to_string()));
        }
        
        let result = unsafe {
            (self.lib.reduce_scatter)(
                send_buff as *const std::ffi::c_void,
                recv_buff as *mut std::ffi::c_void,
                recv_count,
                NcclDataType::Float32 as i32,
                NcclRedOp::Sum as i32,
                self.comms[comm_idx],
                stream,
            )
        };
        
        if !NcclResult::from_code(result).is_success() {
            return Err(NcclResult::from_code(result).to_error());
        }
        
        Ok(())
    }
    
    /// Point-to-point send.
    pub fn send(
        &self,
        send_buff: usize,
        count: usize,
        peer: i32,
        comm_idx: usize,
        stream: usize,
    ) -> GpuResult<()> {
        if comm_idx >= self.comms.len() {
            return Err(GpuError::InvalidConfig("Invalid communicator index".to_string()));
        }
        
        let result = unsafe {
            (self.lib.send)(
                send_buff as *const std::ffi::c_void,
                count,
                NcclDataType::Float32 as i32,
                peer,
                self.comms[comm_idx],
                stream,
            )
        };
        
        if !NcclResult::from_code(result).is_success() {
            return Err(NcclResult::from_code(result).to_error());
        }
        
        Ok(())
    }
    
    /// Point-to-point receive.
    pub fn recv(
        &self,
        recv_buff: usize,
        count: usize,
        peer: i32,
        comm_idx: usize,
        stream: usize,
    ) -> GpuResult<()> {
        if comm_idx >= self.comms.len() {
            return Err(GpuError::InvalidConfig("Invalid communicator index".to_string()));
        }
        
        let result = unsafe {
            (self.lib.recv)(
                recv_buff as *mut std::ffi::c_void,
                count,
                NcclDataType::Float32 as i32,
                peer,
                self.comms[comm_idx],
                stream,
            )
        };
        
        if !NcclResult::from_code(result).is_success() {
            return Err(NcclResult::from_code(result).to_error());
        }
        
        Ok(())
    }
    
    /// Start a group of operations.
    pub fn group_start(&self) -> GpuResult<()> {
        let result = unsafe { (self.lib.group_start)() };
        
        if !NcclResult::from_code(result).is_success() {
            return Err(NcclResult::from_code(result).to_error());
        }
        
        Ok(())
    }
    
    /// End a group of operations.
    pub fn group_end(&self) -> GpuResult<()> {
        let result = unsafe { (self.lib.group_end)() };
        
        if !NcclResult::from_code(result).is_success() {
            return Err(NcclResult::from_code(result).to_error());
        }
        
        Ok(())
    }
}

// =============================================================================
// Peer-to-Peer Operations
// =============================================================================

/// Peer-to-peer GPU memory operations.
pub struct PeerToPeer {
    /// CUDA driver for P2P operations.
    enabled_pairs: Vec<(i32, i32)>,
}

impl PeerToPeer {
    /// Create a new P2P context.
    pub fn new() -> Self {
        Self {
            enabled_pairs: Vec::new(),
        }
    }
    
    /// Check if P2P is possible between two devices.
    pub fn can_access(device: i32, peer: i32) -> bool {
        // Would call cudaDeviceCanAccessPeer
        device != peer
    }
    
    /// Enable P2P access between devices.
    pub fn enable(&mut self, device: i32, peer: i32) -> GpuResult<()> {
        if !Self::can_access(device, peer) {
            return Err(GpuError::Unsupported(
                format!("P2P not supported between devices {} and {}", device, peer)
            ));
        }
        
        // Would call cudaDeviceEnablePeerAccess
        self.enabled_pairs.push((device, peer));
        
        Ok(())
    }
    
    /// Disable P2P access.
    pub fn disable(&mut self, device: i32, peer: i32) -> GpuResult<()> {
        // Would call cudaDeviceDisablePeerAccess
        self.enabled_pairs.retain(|&(d, p)| !(d == device && p == peer));
        
        Ok(())
    }
    
    /// Copy data between devices.
    pub fn copy(
        &self,
        dst_device: i32,
        dst_ptr: usize,
        src_device: i32,
        src_ptr: usize,
        size: usize,
    ) -> GpuResult<()> {
        // Would call cudaMemcpyPeer
        Ok(())
    }
    
    /// Async copy between devices.
    pub fn copy_async(
        &self,
        dst_device: i32,
        dst_ptr: usize,
        src_device: i32,
        src_ptr: usize,
        size: usize,
        stream: usize,
    ) -> GpuResult<()> {
        // Would call cudaMemcpyPeerAsync
        Ok(())
    }
}

impl Default for PeerToPeer {
    fn default() -> Self {
        Self::new()
    }
}

// =============================================================================
// Data Parallelism
// =============================================================================

/// Data parallel wrapper for multi-GPU training.
pub struct DataParallel {
    /// Number of devices.
    n_devices: usize,
    
    /// Device IDs.
    device_ids: Vec<i32>,
    
    /// Multi-GPU manager.
    multi_gpu: MultiGpu,
}

impl DataParallel {
    /// Create a new data parallel context.
    pub fn new(device_ids: Vec<i32>) -> GpuResult<Self> {
        let n_devices = device_ids.len();
        let mut multi_gpu = MultiGpu::new()?;
        
        if multi_gpu.has_nccl() && n_devices > 1 {
            multi_gpu.init_nccl(&device_ids)?;
        }
        
        Ok(Self {
            n_devices,
            device_ids,
            multi_gpu,
        })
    }
    
    /// Get number of devices.
    pub fn n_devices(&self) -> usize {
        self.n_devices
    }
    
    /// Get device IDs.
    pub fn device_ids(&self) -> &[i32] {
        &self.device_ids
    }
    
    /// Scatter tensor across devices.
    pub fn scatter(&self, _tensor: &Tensor) -> GpuResult<Vec<Tensor>> {
        // Split tensor across batch dimension
        // TODO: Implement actual scatter
        Err(GpuError::Unsupported("scatter not implemented".to_string()))
    }
    
    /// Gather tensors from devices.
    pub fn gather(&self, _tensors: &[Tensor]) -> GpuResult<Tensor> {
        // Concatenate tensors along batch dimension
        // TODO: Implement actual gather
        Err(GpuError::Unsupported("gather not implemented".to_string()))
    }
    
    /// Replicate tensor to all devices.
    pub fn replicate(&self, _tensor: &Tensor) -> GpuResult<Vec<Tensor>> {
        // Copy tensor to all devices
        // TODO: Implement actual replicate
        Err(GpuError::Unsupported("replicate not implemented".to_string()))
    }
    
    /// Reduce gradients across devices (for training).
    pub fn reduce_gradients(&self, _grads: &mut [Tensor]) -> GpuResult<()> {
        // AllReduce gradients
        // TODO: Implement using NCCL
        Ok(())
    }
}

/// Model parallel utilities.
pub struct ModelParallel {
    /// Pipeline stages.
    n_stages: usize,
    
    /// Device assignment for each stage.
    stage_devices: Vec<i32>,
}

impl ModelParallel {
    /// Create pipeline parallel context.
    pub fn pipeline(n_stages: usize, devices: Vec<i32>) -> GpuResult<Self> {
        if devices.len() < n_stages {
            return Err(GpuError::InvalidConfig(
                format!("Need at least {} devices for {} stages", n_stages, n_stages)
            ));
        }
        
        Ok(Self {
            n_stages,
            stage_devices: devices,
        })
    }
    
    /// Get device for a stage.
    pub fn get_device(&self, stage: usize) -> i32 {
        self.stage_devices[stage % self.stage_devices.len()]
    }
    
    /// Number of stages.
    pub fn n_stages(&self) -> usize {
        self.n_stages
    }
}

