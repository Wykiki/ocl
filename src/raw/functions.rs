//! All the functions.
//!
//! ### Redundant Casts
//!
//! Redundant casts are temporary for development and will be removed.
//!
//! POSSIBLE TODO: Break this file up
//!

#![allow(dead_code)]

use std::ptr;
use std::mem;
use std::io::Read;
use std::ffi::CString;
use std::iter;
use libc::{size_t, c_void};
use num::{FromPrimitive};

use cl_h::{self, Status, cl_bool, cl_int, cl_uint, cl_platform_id, cl_device_id, cl_device_type, cl_device_info, cl_platform_info, cl_context, cl_context_info, cl_context_properties, cl_image_format, cl_image_desc, cl_kernel, cl_program_build_info, cl_mem, cl_mem_info, cl_mem_flags, cl_event, cl_program, cl_addressing_mode, cl_filter_mode, cl_command_queue_info, cl_command_queue, cl_image_info, cl_sampler, cl_sampler_info, cl_program_info, cl_kernel_info, cl_kernel_arg_info, cl_kernel_work_group_info, cl_event_info, cl_profiling_info};

use error::{Error as OclError, Result as OclResult};
use raw::{self, DEVICES_MAX, PlatformIdRaw, DeviceIdRaw, ContextRaw, ContextProperties, ContextInfo, ContextInfoResult,  MemFlags, CommandQueueRaw, MemRaw, ProgramRaw, KernelRaw, EventRaw, SamplerRaw, KernelArg, DeviceType, ImageFormat, ImageDescriptor, CommandExecutionStatus, AddressingMode, FilterMode, PlatformInfo, PlatformInfoResult, DeviceInfo, DeviceInfoResult, CommandQueueInfo, CommandQueueInfoResult, MemInfo, MemInfoResult, ImageInfo, ImageInfoResult, SamplerInfo, SamplerInfoResult, ProgramInfo, ProgramInfoResult, ProgramBuildInfo, ProgramBuildInfoResult, KernelInfo, KernelInfoResult, KernelArgInfo, KernelArgInfoResult, KernelWorkGroupInfo, KernelWorkGroupInfoResult, EventInfo, EventInfoResult, ProfilingInfo, ProfilingInfoResult};
use util;

//============================================================================
//============================================================================
//=========================== SUPPORT FUNCTIONS ==============================
//============================================================================
//============================================================================

/// Converts the `cl_int` errcode into a string containing the associated
/// constant name.
fn errcode_string(errcode: cl_int) -> String {
    match Status::from_i32(errcode) {
        Some(cls) => format!("{:?}", cls),
        None => format!("[Unknown Error Code: {}]", errcode as i64),
    }
}

/// Evaluates `errcode` and returns an `Err` with a failure message if it is
/// not 0.
///
/// [NAME?]: Is this an idiomatic name for this function?
///
/// TODO: Possibly convert this to a macro of some sort.
fn errcode_try(message: &str, errcode: cl_int) -> OclResult<()> {
    if errcode != cl_h::Status::CL_SUCCESS as cl_int {
        OclError::errcode(errcode, 
            format!("\n\nOPENCL ERROR: {} failed with code [{}]: {}\n\n", 
                message, errcode, errcode_string(errcode))
        )
    } else {
        Ok(())
    }
}

/// Evaluates `errcode` and panics with a failure message if it is not 0.
fn errcode_assert(message: &str, errcode: cl_int) {
    errcode_try(message, errcode).unwrap();
}

/// Maps options of slices to pointers and a length.
fn resolve_event_opts(wait_list: Option<&[EventRaw]>, new_event: Option<&mut EventRaw>)
            -> OclResult<(cl_uint, *const cl_event, *mut cl_event)> {
    // If the wait list is empty or if its containing option is none, map to (0, null),
    // otherwise map to the length and pointer (driver doesn't want an empty list):    
    let (wait_list_len, wait_list_ptr) = match wait_list {
        Some(wl) => {
            if wl.len() > 0 {
                (wl.len() as cl_uint, wl.as_ptr() as *const cl_event)
            } else {
                (0, ptr::null_mut() as *const cl_event)
            }
        },
        None => (0, ptr::null_mut() as *const cl_event),
    };

    let new_event_ptr = match new_event {
        Some(ne) => ne as *mut _ as *mut cl_event,
        None => ptr::null_mut() as *mut cl_event,
    };

    Ok((wait_list_len, wait_list_ptr, new_event_ptr))
}

/// Converts an array option reference into a pointer to the contained array.
fn resolve_work_dims(work_dims: &Option<[usize; 3]>) -> *const size_t {
    match work_dims {
        &Some(ref w) => w as *const [usize; 3] as *const size_t,
        &None => 0 as *const size_t,
    }
}



/// If the program pointed to by `cl_program` for any of the devices listed in 
/// `device_ids` has a build log of any length, it will be returned as an 
/// errcode result.
///
pub fn program_build_err(program: ProgramRaw, device_ids: &[DeviceIdRaw]) -> OclResult<()> {
    let mut size = 0 as size_t;

    for &device_id in device_ids.iter() {
        unsafe {
            let name = cl_h::CL_PROGRAM_BUILD_LOG as cl_program_build_info;

            let mut errcode = cl_h::clGetProgramBuildInfo(
                program.as_ptr(),
                device_id.as_ptr(),
                name,
                0,
                ptr::null_mut(),
                &mut size,
            );
            errcode_assert("clGetProgramBuildInfo(size)", errcode);

            let mut pbi: Vec<u8> = iter::repeat(32u8).take(size as usize).collect();

            errcode = cl_h::clGetProgramBuildInfo(
                program.as_ptr(),
                device_id.as_ptr(),
                name,
                size,
                pbi.as_mut_ptr() as *mut c_void,
                ptr::null_mut(),
            );
            errcode_assert("clGetProgramBuildInfo()", errcode);

            if size > 1 {
                let pbi_nonull = try!(String::from_utf8(pbi));
                let pbi_errcode_string = format!(
                    "\n\n\
                    ###################### OPENCL PROGRAM BUILD DEBUG OUTPUT ######################\
                    \n\n{}\n\
                    ###############################################################################\
                    \n\n",
                    pbi_nonull);

                return OclError::err(pbi_errcode_string);
            }
        }
    }

    Ok(())
}


//============================================================================
//============================================================================
//======================= OPENCL FUNCTION WRAPPERS ===========================
//============================================================================
//============================================================================

//============================================================================
//============================= Platform API =================================
//============================================================================

/// Returns a list of available platforms as 'raw' objects.
// TODO: Get rid of manual vec allocation now that PlatformIdRaw implements Clone.
pub fn get_platform_ids() -> OclResult<Vec<PlatformIdRaw>> {
    let mut num_platforms = 0 as cl_uint;
    
    // Get a count of available platforms:
    let mut errcode: cl_int = unsafe { 
        cl_h::clGetPlatformIDs(0, ptr::null_mut(), &mut num_platforms) 
    };
    try!(errcode_try("clGetPlatformIDs()", errcode));

    // Create a vec with the appropriate size:
    let mut null_vec: Vec<usize> = iter::repeat(0).take(num_platforms as usize).collect();
    let (ptr, len, cap) = (null_vec.as_mut_ptr(), null_vec.len(), null_vec.capacity());

    // Steal the vec's soul:
    let mut platforms: Vec<PlatformIdRaw> = unsafe {
        mem::forget(null_vec);
        Vec::from_raw_parts(ptr as *mut PlatformIdRaw, len, cap)
    };

    errcode = unsafe {
        cl_h::clGetPlatformIDs(
            num_platforms, 
            platforms.as_mut_ptr() as *mut cl_platform_id, 
            ptr::null_mut()
        )
    };
    try!(errcode_try("clGetPlatformIDs()", errcode));
    
    Ok(platforms)
}

/// [UNTESTED]
/// Returns platform information of the requested type.
pub fn get_platform_info(platform: PlatformIdRaw, request_param: PlatformInfo,
            ) -> OclResult<PlatformInfoResult> {
    // cl_h::clGetPlatformInfo(platform: cl_platform_id,
    //                              param_name: cl_platform_info,
    //                              param_value_size: size_t,
    //                              param_value: *mut c_void,
    //                              param_value_size_ret: *mut size_t) -> cl_int;

    let mut size = 0 as size_t;

    unsafe {
        try!(errcode_try("clGetPlatformInfo()", cl_h::clGetPlatformInfo(
            platform.as_ptr() as cl_platform_id,
            request_param as cl_platform_info,
            0 as size_t,
            ptr::null_mut(),
            &mut size as *mut size_t,
        )));
    }
        
    let mut requested_value: Vec<u8> = iter::repeat(32u8).take(size as usize).collect();

    unsafe {
        try!(errcode_try("clGetPlatformInfo()", cl_h::clGetPlatformInfo(
            platform.as_ptr() as cl_platform_id,
            request_param as cl_platform_info,
            size as size_t,
            requested_value.as_mut_ptr() as *mut c_void,
            ptr::null_mut() as *mut size_t,
        )));
    }

    PlatformInfoResult::new(request_param, requested_value)
}

//============================================================================
//============================= Device APIs  =================================
//============================================================================

/// Returns a list of available devices for a particular platform.
pub fn get_device_ids(
            platform: PlatformIdRaw, 
            // device_types_opt: Option<cl_device_type>)
            device_types_opt: Option<DeviceType>)
            -> OclResult<Vec<DeviceIdRaw>> {
    let device_type = device_types_opt.unwrap_or(raw::DEVICE_TYPE_DEFAULT);
    let mut devices_available: cl_uint = 0;

    let mut device_ids: Vec<DeviceIdRaw> = iter::repeat(DeviceIdRaw::null())
        .take(DEVICES_MAX as usize).collect();

    let errcode = unsafe { cl_h::clGetDeviceIDs(
        platform.as_ptr(), 
        device_type.bits() as cl_device_type,
        DEVICES_MAX, 
        device_ids.as_mut_ptr() as *mut cl_device_id,
        &mut devices_available,
    ) };
    try!(errcode_try("clGetDeviceIDs()", errcode));

    // Trim vec len:
    unsafe { device_ids.set_len(devices_available as usize); }
    device_ids.shrink_to_fit();

    Ok(device_ids)
}

/// [INCOMPLETE][WORK IN PROGRESS] Returns information about a device.
///
/// ### Stability (or lack thereof)
///
/// Currently returning only one (temporary) variant.
///
#[allow(unused_variables)]
pub fn get_device_info(device: DeviceIdRaw, info_request: DeviceInfo,
            ) -> OclResult<(DeviceInfoResult)> {
    // cl_h::clGetDeviceInfo(device: cl_device_id,
    //                    param_name: cl_device_info,
    //                    param_value_size: size_t,
    //                    param_value: *mut c_void,
    //                    param_value_size_ret: *mut size_t) -> cl_int;

    let mut info_value_size: size_t = 0;

    let errcode = unsafe { cl_h::clGetDeviceInfo(
        device.as_ptr() as cl_device_id,
        info_request as cl_device_info,
        0 as size_t,
        0 as *mut c_void,
        &mut info_value_size as *mut size_t,
    ) };
    try!(errcode_try("clGetDeviceInfo", errcode));

    let mut result: Vec<u8> = iter::repeat(0u8).take(info_value_size).collect();

    let errcode = unsafe { cl_h::clGetDeviceInfo(
        device.as_ptr() as cl_device_id,
        info_request as cl_device_info,
        info_value_size  as size_t,
        result.as_mut_ptr() as *mut _ as *mut c_void,
        0 as *mut size_t,
    ) };

    // println!("GET_DEVICE_INFO(): errcode: {}, result: {:?}", errcode, result);
    errcode_try("clGetDeviceInfo", errcode)
        .and(Ok(DeviceInfoResult::TemporaryPlaceholderVariant(result)))
}

/// [UNIMPLEMENTED][PLACEHOLDER]
pub fn create_sub_devices() -> OclResult<()> {
    // clCreateSubDevices(in_device: cl_device_id,
    //                    properties: *const cl_device_partition_property,
    //                    num_devices: cl_uint,
    //                    out_devices: *mut cl_device_id,
    //                    num_devices_ret: *mut cl_uint) -> cl_int;
    unimplemented!();
}

/// [UNTESTED] Increments the reference count of a device.
pub fn retain_device(device: DeviceIdRaw) -> OclResult<()> {
    // clRetainDevice(device: cl_device_id) -> cl_int;
    unsafe { errcode_try("clRetainDevice", cl_h::clRetainDevice(device.as_ptr())) }
}

/// [UNTESTED] Decrements the reference count of a device.
pub fn release_device(device: DeviceIdRaw) -> OclResult<()> {
    // clReleaseDevice(device: cl_device_id ) -> cl_int;
    unsafe { errcode_try("clReleaseDevice", cl_h::clReleaseDevice(device.as_ptr())) }
}

//============================================================================
//============================= Context APIs  ================================
//============================================================================

/// [INCOMPLETE] Returns a new context pointer valid for all devices in 
/// `device_ids`.
///
/// [FIXME]: Incomplete implementation. Callback and userdata unimplemented.
///
//
// [NOTE]: Leave commented print statements intact until more `ContextProperties 
// variants are implemented.
pub fn create_context(properties: Option<ContextProperties>, device_ids: &Vec<DeviceIdRaw>,
            pfn_notify: Option<fn()>, user_data: Option<*mut c_void>) -> OclResult<ContextRaw> {
    if device_ids.len() == 0 {
        return OclError::err("ocl::raw::create_context: No devices specified.");
    }

    // println!("CREATE_CONTEXT: ORIGINAL: properties: {:?}", properties);

    let properties_bytes: Vec<u8> = match properties {
        Some(p) => p.into_bytes(),
        None => Vec::<u8>::with_capacity(0),
    };

    // print!("CREATE_CONTEXT: BYTES: ");
    // util::print_bytes_as_hex(&properties_bytes);
    // print!("\n");

    let properties_ptr = if properties_bytes.len() == 0 { 
        ptr::null() 
    } else {
        properties_bytes.as_ptr()
        // ptr::null() 
    };
    
    // println!("CREATE_CONTEXT: POINTER: {:?}", properties_ptr);

    let mut errcode: cl_int = 0;

    // [FIXME]: Callback function and data unimplemented.
    let context = unsafe { ContextRaw::new(cl_h::clCreateContext(
        properties_ptr as *const cl_context_properties, 
        device_ids.len() as cl_uint, 
        device_ids.as_ptr()  as *const cl_device_id,
        mem::transmute(ptr::null::<fn()>()), 
        ptr::null_mut(), 
        &mut errcode,
    )) };
    errcode_try("clCreateContext()", errcode).and(Ok(context))
}

/// [UNIMPLEMENTED][PLACEHOLDER]
pub fn create_context_from_type() -> OclResult<()> {
    // cl_h::clCreateContextFromType(properties: *mut cl_context_properties,
    //                            device_type: cl_device_type,
    //                            pfn_notify: extern fn (*mut c_char, *mut c_void, size_t, *mut c_void),
    //                            user_data: *mut c_void,
    //                            errcode_ret: *mut cl_int) -> cl_context;
    unimplemented!();
}

/// [UNTESTED] Increments the reference count of a context.
pub fn retain_context(context: ContextRaw) -> OclResult<()> {
    // cl_h::clRetainContext(context: cl_context) -> cl_int;
    unsafe { errcode_try("clRetainContext", cl_h::clRetainContext(context.as_ptr())) }
}

/// [UNTESTED] Decrements reference count of a context.
///
/// [FIXME]: Return result
pub fn release_context(context: ContextRaw) {
    unsafe { errcode_assert("clReleaseContext", cl_h::clReleaseContext(context.as_ptr())); }
}

/// Returns various kinds of context information.
///
/// [SDK Reference](https://www.khronos.org/registry/cl/sdk/1.2/docs/man/xhtml/clGetContextInfo.html)
///
/// # Errors
///
/// Returns an error result for all the reasons listed in the SDK in addition 
/// to an additional error when called with `CL_CONTEXT_DEVICES` as described
/// in in the `verify_context()` documentation below.
pub fn get_context_info(context: ContextRaw, request_param: ContextInfo)
            -> OclResult<(ContextInfoResult)> {
    // cl_h::clGetContextInfo(context: cl_context,
    //                     param_name: cl_context_info,
    //                     param_value_size: size_t,
    //                     param_value: *mut c_void,
    //                     param_value_size_ret: *mut size_t) -> cl_int;

   let mut result_size: size_t = 0;

    // let request_param: cl_context_info = cl_h::CL_CONTEXT_PROPERTIES;
    let errcode = unsafe { cl_h::clGetContextInfo(   
        context.as_ptr() as cl_context,
        request_param as cl_context_info,
        0 as size_t,
        0 as *mut c_void,
        &mut result_size as *mut usize,
    ) };
    // println!("context_info(): errcode: {}, result_size: {}", errcode, result_size);
    try!(errcode_try("clGetContextInfo", errcode));

    // Check for invalid context pointer (a potentially hard to track down bug)
    // using ridiculous and probably platform-specific logic [if the `Devices` 
    // variant is passed and we're not in the release config]:
    if !cfg!(release) {
        let err_if_zero_result_size = request_param as cl_context_info == cl_h::CL_CONTEXT_DEVICES;

        if result_size > 10000 || (result_size == 0 && err_if_zero_result_size) {
            return OclError::err("\n\nocl::raw::context_info(): Possible invalid context detected. \n\
                Context info result size is either '> 10k bytes' or '== 0'. Almost certainly an \n\
                invalid context object. If not, please file an issue at: \n\
                https://github.com/cogciprocate/ocl/issues.\n\n");
        }
    }

    let mut result: Vec<u8> = iter::repeat(0).take(result_size).collect();

    let errcode = unsafe { cl_h::clGetContextInfo(   
        context.as_ptr() as cl_context,
        request_param as cl_context_info,
        result_size as size_t,
        result.as_mut_ptr() as *mut c_void,
        0 as *mut usize,
    ) };
    // println!("GET_CONTEXT_INFO(): errcode: {}, result: {:?}", errcode, result);
    errcode_try("clGetContextInfo", errcode).and(
        ContextInfoResult::new(request_param, result))
}

//============================================================================
//========================== Command Queue APIs ==============================
//============================================================================

/// Returns a new command queue pointer.
pub fn create_command_queue(
            context: ContextRaw, 
            device: DeviceIdRaw)
            -> OclResult<CommandQueueRaw> {
    // Verify that the context is valid:
    try!(verify_context(context));

    let mut errcode: cl_int = 0;

    let cq = unsafe { CommandQueueRaw::new(cl_h::clCreateCommandQueue(
        context.as_ptr(), 
        device.as_ptr(),
        cl_h::CL_QUEUE_PROFILING_ENABLE, 
        &mut errcode
    )) };
    errcode_try("clCreateCommandQueue()", errcode).and(Ok(cq))
}

/// [UNTESTED]
/// Increments the reference count of a command queue.
pub fn retain_command_queue(queue: CommandQueueRaw) -> OclResult<()> {
    // cl_h::clRetainCommandQueue(command_queue: cl_command_queue) -> cl_int;
    unsafe { errcode_try("clRetainCommandQueue", cl_h::clRetainCommandQueue(queue.as_ptr())) }
}

/// Decrements the reference count of a command queue.
///
/// [FIXME]: Return result
pub fn release_command_queue(queue: CommandQueueRaw) -> OclResult<()> {
    unsafe { errcode_try("clReleaseCommandQueue", 
        cl_h::clReleaseCommandQueue(queue.as_ptr())) }
}

/// [UNTESTED] Returns information about a command queue
pub fn get_command_queue_info(queue: CommandQueueRaw, info_request: CommandQueueInfo,
            ) -> OclResult<(CommandQueueInfoResult)> {
    // cl_h::clGetCommandQueueInfo(command_queue: cl_command_queue,
    //                          param_name: cl_command_queue_info,
    //                          param_value_size: size_t,
    //                          param_value: *mut c_void,
    //                          param_value_size_ret: *mut size_t) -> cl_int;

    let mut info_value_size: size_t = 0;

    let errcode = unsafe { cl_h::clGetCommandQueueInfo(
        queue.as_ptr() as cl_command_queue,
        info_request as cl_command_queue_info,
        0 as size_t,
        0 as *mut c_void,
        &mut info_value_size as *mut size_t,
    ) };
    try!(errcode_try("clGetCommandQueueInfo", errcode));

    let mut result: Vec<u8> = iter::repeat(0u8).take(info_value_size).collect();

    let errcode = unsafe { cl_h::clGetCommandQueueInfo(
        queue.as_ptr() as cl_command_queue,
        info_request as cl_command_queue_info,
        info_value_size,
        result.as_mut_ptr() as *mut _ as *mut c_void,
        0 as *mut size_t,
    ) };    
    // println!("GET_COMMAND_QUEUE_INFO(): errcode: {}, result: {:?}", errcode, result);
    errcode_try("clGetCommandQueueInfo", errcode)
        .and(Ok(CommandQueueInfoResult::TemporaryPlaceholderVariant(result)))
}

//============================================================================
//========================== Memory Object APIs ==============================
//============================================================================

/// Returns a new buffer pointer with size (bytes): `len` * sizeof(T).
pub fn create_buffer<T>(
            context: ContextRaw,
            flags: MemFlags,
            len: usize,
            data: Option<&[T]>)
            -> OclResult<MemRaw> {
    // Verify that the context is valid:
    try!(verify_context(context));

    let mut errcode: cl_int = 0;

    let host_ptr = match data {
        Some(d) => {
            if d.len() != len { 
                return OclError::err("ocl::create_buffer(): Data length mismatch.");
            }
            d.as_ptr() as cl_mem
        },
        None => ptr::null_mut(),
    };

    let buf = unsafe { MemRaw::new(cl_h::clCreateBuffer(
        context.as_ptr(), 
        flags.bits() as cl_mem_flags,
        len * mem::size_of::<T>(),
        host_ptr, 
        &mut errcode,
    )) };
    errcode_assert("create_buffer", errcode);

    Ok(buf)
}

/// [UNIMPLEMENTED][PLACEHOLDER]
pub fn create_sub_buffer() -> OclResult<()> {
    // cl_h::clCreateSubBuffer(buffer: cl_mem,
    //                     flags: cl_mem_flags,
    //                     buffer_create_type: cl_buffer_create_type,
    //                     buffer_create_info: *mut c_void,
    //                     errcode_ret: *mut cl_int) -> cl_mem;
    unimplemented!();
}

/// Returns a new image (mem) pointer.
// [WORK IN PROGRESS]
pub fn create_image<T>(
            context: ContextRaw,
            flags: MemFlags,
            // format: &cl_image_format,
            // desc: &cl_image_desc,
            format: ImageFormat,
            desc: ImageDescriptor,
            data: Option<&[T]>)
            -> OclResult<MemRaw> {
    // Verify that the context is valid:
    try!(verify_context(context));

    let mut errcode: cl_int = 0;
    
    let data_ptr = match data {
        Some(d) => {
            // [FIXME]: CALCULATE CORRECT IMAGE SIZE AND COMPARE
            // assert!(d.len() == len, "ocl::create_image(): Data length mismatch.");
            d.as_ptr() as cl_mem
        },
        None => ptr::null_mut(),
    };

    let image_ptr = unsafe { MemRaw::new(cl_h::clCreateImage(
        context.as_ptr(),
        flags.bits() as cl_mem_flags,
        &format.as_raw() as *const cl_image_format,
        &desc.as_raw() as *const cl_image_desc,
        data_ptr,
        &mut errcode as *mut cl_int,
    )) }; 
    errcode_assert("create_image", errcode);

    assert!(!image_ptr.as_ptr().is_null());

    Ok(image_ptr)
}

/// [UNTESTED]
/// Increments the reference counter of a mem object.
pub fn retain_mem_object(mem: MemRaw) -> OclResult<()> {
    // cl_h::clRetainMemObject(memobj: cl_mem) -> cl_int;
    unsafe { errcode_try("clRetainMemObject", cl_h::clRetainMemObject(mem.as_ptr())) }
}

/// Decrements the reference counter of a mem object.
pub fn release_mem_object(mem: MemRaw) -> OclResult<()> {
    unsafe { errcode_try("clReleaseMemObject", cl_h::clReleaseMemObject(mem.as_ptr())) }
}

/// [UNIMPLEMENTED][PLACEHOLDER]
pub fn get_supported_image_formats() -> OclResult<()> {
    // cl_h::clGetSupportedImageFormats(context: cl_context,
    //                               flags: cl_mem_flags,
    //                               image_type: cl_mem_object_type,
    //                               num_entries: cl_uint,
    //                               image_formats: *mut cl_image_format,
    //                               num_image_formats: *mut cl_uint) -> cl_int;
    unimplemented!();
}







//============================================================================
//============================================================================
//============================================================================
//============================================================================
//============================================================================
//============================================================================
//============================================================================
//=========================== WORK IN PROGRESS ===============================
//============================================================================
//============================================================================
//============================================================================
//============================================================================
//============================================================================
//============================================================================
//============================================================================

/// [UNIMPLEMENTED][PLACEHOLDER]
pub fn get_mem_object_info(obj: MemRaw, info_request: MemInfo,
            ) -> OclResult<(MemInfoResult)> {
    // cl_h::clGetMemObjectInfo(memobj: cl_mem,
    //                       param_name: cl_mem_info,
    //                       param_value_size: size_t,
    //                       param_value: *mut c_void,
    //                       param_value_size_ret: *mut size_t) -> cl_int;

    let mut info_value_size: size_t = 0;

    let errcode = unsafe { cl_h::clGetMemObjectInfo(
        obj.as_ptr() as cl_mem,
        info_request as cl_mem_info,
        0 as size_t,
        0 as *mut c_void,
        &mut info_value_size as *mut size_t,
    ) };
    try!(errcode_try("clGetMemObjectInfo", errcode));

    let mut result: Vec<u8> = iter::repeat(0u8).take(info_value_size).collect();

    let errcode = unsafe { cl_h::clGetMemObjectInfo(
        obj.as_ptr() as cl_mem,
        info_request as cl_mem_info,
        info_value_size,
        result.as_mut_ptr() as *mut _ as *mut c_void,
        0 as *mut size_t,
    ) };    
    // println!("GET_COMMAND_QUEUE_INFO(): errcode: {}, result: {:?}", errcode, result);
    errcode_try("clGetMemObjectInfo", errcode)
        .and(Ok(MemInfoResult::TemporaryPlaceholderVariant(result)))
}









//============================================================================
//============================================================================
//============================================================================
//============================================================================
//============================================================================
//============================================================================
//============================================================================
//=========================== WORK IN PROGRESS ===============================
//============================================================================
//============================================================================
//============================================================================
//============================================================================
//============================================================================
//============================================================================
//============================================================================


/// [UNIMPLEMENTED][PLACEHOLDER]
pub fn get_image_info(obj: MemRaw, info_request: ImageInfo,
            ) -> OclResult<(ImageInfoResult)> {
    // cl_h::clGetImageInfo(image: cl_mem,
    //                   param_name: cl_image_info,
    //                   param_value_size: size_t,
    //                   param_value: *mut c_void,
    //                   param_value_size_ret: *mut size_t) -> cl_int;

    let mut info_value_size: size_t = 0;

    let errcode = unsafe { cl_h::clGetImageInfo(
        obj.as_ptr() as cl_mem,
        info_request as cl_image_info,
        0 as size_t,
        0 as *mut c_void,
        &mut info_value_size as *mut size_t,
    ) };
    try!(errcode_try("clGetImageInfo", errcode));

    let mut result: Vec<u8> = iter::repeat(0u8).take(info_value_size).collect();

    let errcode = unsafe { cl_h::clGetImageInfo(
        obj.as_ptr() as cl_mem,
        info_request as cl_image_info,
        info_value_size,
        result.as_mut_ptr() as *mut _ as *mut c_void,
        0 as *mut size_t,
    ) };    
    // println!("GET_COMMAND_QUEUE_INFO(): errcode: {}, result: {:?}", errcode, result);
    errcode_try("clGetImageInfo", errcode)
        .and(Ok(ImageInfoResult::TemporaryPlaceholderVariant(result)))
}

/// [UNIMPLEMENTED][PLACEHOLDER]
pub fn set_mem_object_destructor_callback() -> OclResult<()> {
    // cl_h::clSetMemObjectDestructorCallback(memobj: cl_mem,
    //                                     pfn_notify: extern fn (cl_mem, *mut c_void),
    //                                     user_data: *mut c_void) -> cl_int;
    unimplemented!();
}

//============================================================================
//============================= Sampler APIs =================================
//============================================================================

/// [UNTESTED]
/// Returns a new sampler.
pub fn create_sampler(context: ContextRaw, normalize_coords: bool, addressing_mode: AddressingMode,
            filter_mode: FilterMode) -> OclResult<(SamplerRaw)> {
    // cl_h::clCreateSampler(context: cl_context,
    //                    normalize_coords: cl_bool,
    //                    addressing_mode: cl_addressing_mode,
    //                    filter_mode: cl_filter_mode,
    //                    errcode_ret: *mut cl_int) -> cl_sampler;

    let mut errcode = 0;

    let sampler = unsafe { SamplerRaw::new(cl_h::clCreateSampler(
        context.as_ptr(),
        normalize_coords as cl_bool,
        addressing_mode as cl_addressing_mode,
        filter_mode as cl_filter_mode,
        &mut errcode,
    )) };

    errcode_try("clCreateSampler", errcode).and(Ok(sampler))
}

/// [UNTESTED]
/// Increments a sampler reference counter.
pub fn retain_sampler(sampler: SamplerRaw) -> OclResult<()> {
    // cl_h::clRetainSampler(sampler: cl_sampler) -> cl_int;
    unsafe { errcode_try("clRetainSampler", cl_h::clRetainSampler(sampler.as_ptr())) }
}

/// [UNTESTED]
/// Decrements a sampler reference counter.
pub fn release_sampler(sampler: SamplerRaw) -> OclResult<()> {
    // cl_h::clReleaseSampler(sampler: cl_sampler) ->cl_int;
    unsafe { errcode_try("clReleaseSampler", cl_h::clReleaseSampler(sampler.as_ptr())) }
}












//============================================================================
//============================================================================
//============================================================================
//============================================================================
//============================================================================
//============================================================================
//============================================================================
//=========================== WORK IN PROGRESS ===============================
//============================================================================
//============================================================================
//============================================================================
//============================================================================
//============================================================================
//============================================================================
//============================================================================



/// [UNIMPLEMENTED][PLACEHOLDER]
pub fn get_sampler_info(obj: SamplerRaw, info_request: SamplerInfo,
            ) -> OclResult<(SamplerInfoResult)> {
    // cl_h::clGetSamplerInfo(sampler: cl_sampler,
    //                     param_name: cl_sampler_info,
    //                     param_value_size: size_t,
    //                     param_value: *mut c_void,
    //                     param_value_size_ret: *mut size_t) -> cl_int;

    let mut info_value_size: size_t = 0;

    let errcode = unsafe { cl_h::clGetSamplerInfo(
        obj.as_ptr() as cl_sampler,
        info_request as cl_sampler_info,
        0 as size_t,
        0 as *mut c_void,
        &mut info_value_size as *mut size_t,
    ) };
    try!(errcode_try("clGetSamplerInfo", errcode));

    let mut result: Vec<u8> = iter::repeat(0u8).take(info_value_size).collect();

    let errcode = unsafe { cl_h::clGetSamplerInfo(
        obj.as_ptr() as cl_sampler,
        info_request as cl_sampler_info,
        info_value_size,
        result.as_mut_ptr() as *mut _ as *mut c_void,
        0 as *mut size_t,
    ) };    
    // println!("GET_COMMAND_QUEUE_INFO(): errcode: {}, result: {:?}", errcode, result);
    errcode_try("clGetSamplerInfo", errcode)
        .and(Ok(SamplerInfoResult::TemporaryPlaceholderVariant(result)))
}

//============================================================================
//========================== Program Object APIs =============================
//============================================================================

/// Creates a new program.
pub fn create_program_with_source(
            context: ContextRaw, 
            src_strings: Vec<CString>)
            // cmplr_opts: CString,
            // device_ids: &Vec<DeviceIdRaw>)
            -> OclResult<ProgramRaw> {
    // cl_h::clCreateProgramWithSource(context: cl_context,
    //                              count: cl_uint,
    //                              strings: *const *const c_char,
    //                              lengths: *const size_t,
    //                              errcode_ret: *mut cl_int) -> cl_program;

    // Verify that the context is valid:
    try!(verify_context(context));

    // Lengths (not including \0 terminator) of each string:
    let ks_lens: Vec<usize> = src_strings.iter().map(|cs| cs.as_bytes().len()).collect();  

    // Pointers to each string:
    let kern_string_ptrs: Vec<*const i8> = src_strings.iter().map(|cs| cs.as_ptr()).collect();

    let mut errcode: cl_int = 0;
    
    let program = unsafe { ProgramRaw::new(cl_h::clCreateProgramWithSource(
        context.as_ptr(), 
        kern_string_ptrs.len() as cl_uint,
        kern_string_ptrs.as_ptr() as *const *const i8,
        ks_lens.as_ptr() as *const usize,
        &mut errcode,
    )) };

    errcode_try("clCreateProgramWithSource()", errcode).and(Ok(program))
}

/// [UNIMPLEMENTED][PLACEHOLDER]
pub fn create_program_with_binary() -> OclResult<()> {
    // cl_h::clCreateProgramWithBinary(context: cl_context,
    //                              num_devices: cl_uint,
    //                              device_list: *const cl_device_id,
    //                              lengths: *const size_t,
    //                              binaries: *const *const c_uchar,
    //                              binary_status: *mut cl_int,
    //                              errcode_ret: *mut cl_int) -> cl_program;
    unimplemented!();
}

/// [UNIMPLEMENTED][PLACEHOLDER]
pub fn create_program_with_built_in_kernels() -> OclResult<()> {
    // clCreateProgramWithBuiltInKernels(context: cl_context,
    //                                  num_devices: cl_uint,
    //                                  device_list: *const cl_device_id,
    //                                  kernel_names: *mut char,
    //                                  errcode_ret: *mut cl_int) -> cl_program;
    unimplemented!();
}

/// [UNTESTED]
/// Increments a program reference counter.
pub fn retain_program(program: ProgramRaw) -> OclResult<()> {
    // cl_h::clRetainProgram(program: cl_program) -> cl_int;
    unsafe { errcode_try("clRetainProgram", cl_h::clRetainProgram(program.as_ptr())) }
}

/// Decrements a program reference counter.
pub fn release_program(program: ProgramRaw) -> OclResult<()> {
    unsafe { errcode_try("clReleaseKernel", cl_h::clReleaseProgram(program.as_ptr())) }
}

pub struct UserDataPh(usize);

impl UserDataPh {
    fn unwrapped(&self) -> *mut c_void {
        ptr::null_mut()
    }
}

/// Builds a program.
///
/// Callback functions are not yet supported.
pub fn build_program(
            program: ProgramRaw,
            devices: &[DeviceIdRaw],
            options: CString,
            pfn_notify: Option<extern "C" fn(*mut c_void, *mut c_void)>,
            user_data: Option<Box<UserDataPh>>)
            -> OclResult<()> {
    assert!(pfn_notify.is_none() && user_data.is_none(),
        "ocl::raw::build_program(): Callback functions not yet implemented.");
    // cl_h::clBuildProgram(program: cl_program,
    //                   num_devices: cl_uint,
    //                   device_list: *const cl_device_id,
    //                   options: *const c_char,
    //                   pfn_notify: extern fn (cl_program, *mut c_void),
    //                   user_data: *mut c_void) -> cl_int;

            // src_strings: Vec<CString>,
            // cmplr_opts: CString,
            // context: ContextRaw, 
            // device_ids: &Vec<DeviceIdRaw>)
            // -> OclResult<ProgramRaw> {
    let user_data = match user_data {
        Some(ud) => ud.unwrapped(),
        None => ptr::null_mut(),
    };

    let errcode = unsafe { cl_h::clBuildProgram(
        program.as_ptr() as cl_program,
        devices.len() as cl_uint,
        devices.as_ptr() as *const cl_device_id, 
        options.as_ptr() as *const i8,
        // mem::transmute(ptr::null::<fn()>()), 
        pfn_notify.unwrap_or(mem::transmute(ptr::null::<fn()>())),
        user_data,
    ) };  

    if errcode < 0 {
        program_build_err(program, devices)
    } else {
        try!(errcode_try("clBuildProgram()", errcode));
        Ok(()) 
    }
}

/// [UNIMPLEMENTED][PLACEHOLDER]
pub fn compile_program() -> OclResult<()> {
    // clCompileProgram(program: cl_program,
    //                 num_devices: cl_uint,
    //                 device_list: *const cl_device_id,
    //                 options: *const c_char, 
    //                 num_input_headers: cl_uint,
    //                 input_headers: *const cl_program,
    //                 header_include_names: *const *const c_char,
    //                 pfn_notify: extern fn (program: cl_program, user_data: *mut c_void),
    //                 user_data: *mut c_void) -> cl_int;
    unimplemented!();
}

/// [UNIMPLEMENTED][PLACEHOLDER]
pub fn link_program() -> OclResult<()> {
    // clLinkProgram(context: cl_context,
    //               num_devices: cl_uint,
    //               device_list: *const cl_device_id,
    //               options: *const c_char, 
    //               num_input_programs: cl_uint,
    //               input_programs: *const cl_program,
    //               pfn_notify: extern fn (program: cl_program, user_data: *mut c_void),
    //               user_data: *mut c_void,
    //               errcode_ret: *mut cl_int) -> cl_program;
    unimplemented!();
}

/// [UNTESTED]
/// Unloads a platform compiler.
pub fn unload_platform_compiler(platform: PlatformIdRaw) -> OclResult<()> {
    // pub fn clUnloadPlatformCompiler(platform: cl_platform_id) -> cl_int;
    unsafe { errcode_try("clUnloadPlatformCompiler", 
        cl_h::clUnloadPlatformCompiler(platform.as_ptr())) }
}










//============================================================================
//============================================================================
//============================================================================
//============================================================================
//============================================================================
//============================================================================
//============================================================================
//=========================== WORK IN PROGRESS ===============================
//============================================================================
//============================================================================
//============================================================================
//============================================================================
//============================================================================
//============================================================================
//============================================================================


/// [UNIMPLEMENTED][PLACEHOLDER]
// (partial implementation in 'derived' section)
pub fn get_program_info(obj: ProgramRaw, info_request: ProgramInfo,
            ) -> OclResult<(ProgramInfoResult)> {
    // cl_h::clGetProgramInfo(program: cl_program,
    //                     param_name: cl_program_info,
    //                     param_value_size: size_t,
    //                     param_value: *mut c_void,
    //                     param_value_size_ret: *mut size_t) -> cl_int;

    let mut info_value_size: size_t = 0;

    let errcode = unsafe { cl_h::clGetProgramInfo(
        obj.as_ptr() as cl_program,
        info_request as cl_program_info,
        0 as size_t,
        0 as *mut c_void,
        &mut info_value_size as *mut size_t,
    ) };
    try!(errcode_try("clGetProgramInfo", errcode));

    let mut result: Vec<u8> = iter::repeat(0u8).take(info_value_size).collect();

    let errcode = unsafe { cl_h::clGetProgramInfo(
        obj.as_ptr() as cl_program,
        info_request as cl_program_info,
        info_value_size,
        result.as_mut_ptr() as *mut _ as *mut c_void,
        0 as *mut size_t,
    ) };    
    // println!("GET_COMMAND_QUEUE_INFO(): errcode: {}, result: {:?}", errcode, result);
    errcode_try("clGetProgramInfo", errcode)
        .and(Ok(ProgramInfoResult::TemporaryPlaceholderVariant(result)))
}











//============================================================================
//============================================================================
//============================================================================
//============================================================================
//============================================================================
//============================================================================
//============================================================================
//=========================== WORK IN PROGRESS ===============================
//============================================================================
//============================================================================
//============================================================================
//============================================================================
//============================================================================
//============================================================================
//============================================================================

/// [UNIMPLEMENTED][PLACEHOLDER]
// (partial implementation in 'derived' section)
pub fn get_program_build_info(obj: ProgramRaw, device_obj: DeviceIdRaw, info_request: ProgramBuildInfo,
            ) -> OclResult<(ProgramBuildInfoResult)> {
    // cl_h::clGetProgramBuildInfo(program: cl_program,
    //                          device: cl_device_id,
    //                          param_name: cl_program_build_info,
    //                          param_value_size: size_t,
    //                          param_value: *mut c_void,
    //                          param_value_size_ret: *mut size_t) -> cl_int;

    let mut info_value_size: size_t = 0;

    let errcode = unsafe { cl_h::clGetProgramBuildInfo(
        obj.as_ptr() as cl_program,
        device_obj.as_ptr() as cl_device_id,
        info_request as cl_program_build_info,
        0 as size_t,
        0 as *mut c_void,
        &mut info_value_size as *mut size_t,
    ) };
    try!(errcode_try("clGetProgramBuildInfo", errcode));

    let mut result: Vec<u8> = iter::repeat(0u8).take(info_value_size).collect();

    let errcode = unsafe { cl_h::clGetProgramBuildInfo(
        obj.as_ptr() as cl_program,
        device_obj.as_ptr() as cl_device_id,
        info_request as cl_program_build_info,
        info_value_size as size_t,
        result.as_mut_ptr() as *mut _ as *mut c_void,
        0 as *mut size_t,
    ) };    
    // println!("GET_COMMAND_QUEUE_INFO(): errcode: {}, result: {:?}", errcode, result);
    errcode_try("clGetProgramBuildInfo", errcode)
        .and(Ok(ProgramBuildInfoResult::TemporaryPlaceholderVariant(result)))
}

//============================================================================
//========================== Kernel Object APIs ==============================
//============================================================================

/// Returns a new kernel pointer.
pub fn create_kernel(
            program: ProgramRaw, 
            name: &str)
            -> OclResult<KernelRaw> {
    let mut err: cl_int = 0;

    let kernel_ptr = unsafe { KernelRaw::new(cl_h::clCreateKernel(
        program.as_ptr(),
        try!(CString::new(name.as_bytes())).as_ptr(), 
        &mut err,
    )) };
    errcode_try(&format!("clCreateKernel('{}'):", &name), err).and(Ok(kernel_ptr))
}

/// [UNIMPLEMENTED][PLACEHOLDER]
pub fn create_kernels_in_program() -> OclResult<()> {
    // cl_h::clCreateKernelsInProgram(program: cl_program,
    //                             num_kernels: cl_uint,
    //                             kernels: *mut cl_kernel,
    //                             num_kernels_ret: *mut cl_uint) -> cl_int;
    unimplemented!();
}

/// [UNTESTED]
/// Increments a kernel reference counter.
pub fn retain_kernel(kernel: KernelRaw) -> OclResult<()> {
    // cl_h::clRetainKernel(kernel: cl_kernel) -> cl_int;
    unsafe { errcode_try("clRetainKernel", cl_h::clRetainKernel(kernel.as_ptr())) }
}

/// Decrements a kernel reference counter.
pub fn release_kernel(kernel: KernelRaw) -> OclResult<()> {
    unsafe { errcode_try("clReleaseKernel", cl_h::clReleaseKernel(kernel.as_ptr())) }
}

/// Modifies or creates a kernel argument.
///
/// `kernel_name` is for error reporting and is optional but highly recommended.
///
pub fn set_kernel_arg<T>(kernel: KernelRaw, arg_index: u32, arg: KernelArg<T>,
            kernel_name: Option<&str>
            ) -> OclResult<()> {
    let (arg_size, arg_value) = match arg {
        KernelArg::Mem(mem_obj) => {
            (mem::size_of::<MemRaw>() as size_t, 
            (&mem_obj.as_ptr() as *const *mut c_void) as *const c_void)
        },
        KernelArg::Sampler(smplr) => {
            (mem::size_of::<SamplerRaw>() as size_t, 
            (&smplr.as_ptr() as *const *mut c_void) as *const c_void)
        },
        KernelArg::Scalar(scalar) => {
            (mem::size_of::<T>() as size_t, 
            scalar as *const _ as *const c_void)
        },
        KernelArg::Vector(vector)=> {
            ((mem::size_of::<T>() * vector.len()) as size_t,
            vector as *const _ as *const c_void)
        },
        KernelArg::Local(length) => {
            ((mem::size_of::<T>() * length) as size_t,
            ptr::null())
        },
        KernelArg::Other { size, value } => (size, value),
    };

    let err = unsafe { cl_h::clSetKernelArg(
            kernel.as_ptr(), 
            arg_index,
            arg_size, 
            arg_value,
    ) };
    let err_pre = format!("clSetKernelArg('{}'):", kernel_name.unwrap_or(""));
    errcode_try(&err_pre, err)
} 











//============================================================================
//============================================================================
//============================================================================
//============================================================================
//============================================================================
//============================================================================
//============================================================================
//=========================== WORK IN PROGRESS ===============================
//============================================================================
//============================================================================
//============================================================================
//============================================================================
//============================================================================
//============================================================================
//============================================================================

/// [UNIMPLEMENTED][PLACEHOLDER]
pub fn get_kernel_info(obj: KernelRaw, info_request: KernelInfo,
            ) -> OclResult<(KernelInfoResult)> {
    // cl_h::clGetKernelInfo(kernel: cl_kernel,
    //                    param_name: cl_kernel_info,
    //                    param_value_size: size_t,
    //                    param_value: *mut c_void,
    //                    param_value_size_ret: *mut size_t) -> cl_int;

    let mut info_value_size: size_t = 0;

    let errcode = unsafe { cl_h::clGetKernelInfo(
        obj.as_ptr() as cl_kernel,
        info_request as cl_kernel_info,
        0 as size_t,
        0 as *mut c_void,
        &mut info_value_size as *mut size_t,
    ) };
    try!(errcode_try("clGetKernelInfo", errcode));

    let mut result: Vec<u8> = iter::repeat(0u8).take(info_value_size).collect();

    let errcode = unsafe { cl_h::clGetKernelInfo(
        obj.as_ptr() as cl_kernel,
        info_request as cl_kernel_info,
        info_value_size,
        result.as_mut_ptr() as *mut _ as *mut c_void,
        0 as *mut size_t,
    ) };    
    // println!("GET_COMMAND_QUEUE_INFO(): errcode: {}, result: {:?}", errcode, result);
    errcode_try("clGetKernelInfo", errcode)
        .and(Ok(KernelInfoResult::TemporaryPlaceholderVariant(result)))
}










//============================================================================
//============================================================================
//============================================================================
//============================================================================
//============================================================================
//============================================================================
//============================================================================
//=========================== WORK IN PROGRESS ===============================
//============================================================================
//============================================================================
//============================================================================
//============================================================================
//============================================================================
//============================================================================
//============================================================================

/// [UNIMPLEMENTED][PLACEHOLDER]
pub fn get_kernel_arg_info(obj: KernelRaw, arg_index: u32, info_request: KernelArgInfo,
            ) -> OclResult<(KernelArgInfoResult)> {
    // clGetKernelArgInfo(kernel: cl_kernel,
    //                   arg_indx: cl_uint,
    //                   param_name: cl_kernel_arg_info,
    //                   param_value_size: size_t,
    //                   param_value: *mut c_void,
    //                   param_value_size_ret: *mut size_t) -> cl_int;

    let mut info_value_size: size_t = 0;

    let errcode = unsafe { cl_h::clGetKernelArgInfo(
        obj.as_ptr() as cl_kernel,
        arg_index as cl_uint,
        info_request as cl_kernel_arg_info,
        0 as size_t,
        0 as *mut c_void,
        &mut info_value_size as *mut size_t,
    ) };
    try!(errcode_try("clGetKernelArgInfo", errcode));

    let mut result: Vec<u8> = iter::repeat(0u8).take(info_value_size).collect();

    let errcode = unsafe { cl_h::clGetKernelArgInfo(
        obj.as_ptr() as cl_kernel,
        arg_index as cl_uint,
        info_request as cl_kernel_arg_info,
        info_value_size,
        result.as_mut_ptr() as *mut _ as *mut c_void,
        0 as *mut size_t,
    ) };    
    // println!("GET_COMMAND_QUEUE_INFO(): errcode: {}, result: {:?}", errcode, result);
    errcode_try("clGetKernelArgInfo", errcode)
        .and(Ok(KernelArgInfoResult::TemporaryPlaceholderVariant(result)))
}












//============================================================================
//============================================================================
//============================================================================
//============================================================================
//============================================================================
//============================================================================
//============================================================================
//=========================== WORK IN PROGRESS ===============================
//============================================================================
//============================================================================
//============================================================================
//============================================================================
//============================================================================
//============================================================================
//============================================================================

/// [UNIMPLEMENTED][PLACEHOLDER]
pub fn get_kernel_work_group_info(obj: KernelRaw, device_obj: DeviceIdRaw, info_request: KernelWorkGroupInfo,
            ) -> OclResult<(KernelWorkGroupInfoResult)> {
    // cl_h::clGetKernelWorkGroupInfo(kernel: cl_kernel,
    //                             device: cl_device_id,
    //                             param_name: cl_kernel_work_group_info,
    //                             param_value_size: size_t,
    //                             param_value: *mut c_void,
    //                             param_value_size_ret: *mut size_t) -> cl_int;

    let mut info_value_size: size_t = 0;

    let errcode = unsafe { cl_h::clGetKernelWorkGroupInfo(
        obj.as_ptr() as cl_kernel,
        device_obj.as_ptr() as cl_device_id,
        info_request as cl_kernel_work_group_info,
        0 as size_t,
        0 as *mut c_void,
        &mut info_value_size as *mut size_t,
    ) };
    try!(errcode_try("clGetKernelWorkGroupInfo", errcode));

    let mut result: Vec<u8> = iter::repeat(0u8).take(info_value_size).collect();

    let errcode = unsafe { cl_h::clGetKernelWorkGroupInfo(
        obj.as_ptr() as cl_kernel,
        device_obj.as_ptr() as cl_device_id,
        info_request as cl_kernel_work_group_info,
        info_value_size,
        result.as_mut_ptr() as *mut _ as *mut c_void,
        0 as *mut size_t,
    ) };    
    // println!("GET_COMMAND_QUEUE_INFO(): errcode: {}, result: {:?}", errcode, result);
    errcode_try("clGetKernelWorkGroupInfo", errcode)
        .and(Ok(KernelWorkGroupInfoResult::TemporaryPlaceholderVariant(result)))
}

//============================================================================
//========================== Event Object APIs ===============================
//============================================================================

pub fn wait_for_events(count: cl_uint, event_list: &[EventRaw]) {
    let errcode = unsafe {
        cl_h::clWaitForEvents(count, &(*event_list.as_ptr()).as_ptr())
    };

    errcode_assert("clWaitForEvents", errcode);
}










//============================================================================
//============================================================================
//============================================================================
//============================================================================
//============================================================================
//============================================================================
//============================================================================
//=========================== WORK IN PROGRESS ===============================
//============================================================================
//============================================================================
//============================================================================
//============================================================================
//============================================================================
//============================================================================
//============================================================================

/// [UNIMPLEMENTED][PLACEHOLDER]
pub fn get_event_info(obj: EventRaw, info_request: EventInfo,
            ) -> OclResult<(EventInfoResult)> {
    // cl_h::clGetEventInfo(event: cl_event,
    //                   param_name: cl_event_info,
    //                   param_value_size: size_t,
    //                   param_value: *mut c_void,
    //                   param_value_size_ret: *mut size_t) -> cl_int;

    let mut info_value_size: size_t = 0;

    let errcode = unsafe { cl_h::clGetEventInfo(
        obj.as_ptr() as cl_event,
        info_request as cl_event_info,
        0 as size_t,
        0 as *mut c_void,
        &mut info_value_size as *mut size_t,
    ) };
    try!(errcode_try("clGetEventInfo", errcode));

    let mut result: Vec<u8> = iter::repeat(0u8).take(info_value_size).collect();

    let errcode = unsafe { cl_h::clGetEventInfo(
        obj.as_ptr() as cl_event,
        info_request as cl_event_info,
        info_value_size,
        result.as_mut_ptr() as *mut _ as *mut c_void,
        0 as *mut size_t,
    ) };    
    // println!("GET_COMMAND_QUEUE_INFO(): errcode: {}, result: {:?}", errcode, result);
    errcode_try("clGetEventInfo", errcode)
        .and(Ok(EventInfoResult::TemporaryPlaceholderVariant(result)))
}

/// [UNTESTED]
/// Creates an event not already associated with any command.
pub fn create_user_event(context: ContextRaw) -> OclResult<(EventRaw)> {
    // cl_h::clCreateUserEvent(context: cl_context,
    //                      errcode_ret: *mut cl_int) -> cl_event;
    let mut errcode = 0;
    let event = unsafe { EventRaw::new(cl_h::clCreateUserEvent(context.as_ptr(), &mut errcode)) };
    errcode_try("clCreateUserEvent", errcode).and(Ok(event))
}

/// [UNTESTED]
/// Increments an event's reference counter.
pub fn retain_event(event: EventRaw) -> OclResult<()> {
    // cl_h::clRetainEvent(event: cl_event) -> cl_int;
    unsafe { errcode_try("clRetainEvent", cl_h::clRetainEvent(event.as_ptr())) }
}

/// Decrements an event's reference counter.
pub fn release_event(event: EventRaw) -> OclResult<()> {
    unsafe { errcode_try("clReleaseEvent", cl_h::clReleaseEvent(event.as_ptr())) }
}

/// [UNIMPLEMENTED][PLACEHOLDER]
pub fn set_user_event_status(event: EventRaw, execution_status: CommandExecutionStatus) 
        -> OclResult<()> {
    // cl_h::clSetUserEventStatus(event: cl_event,
    //                         execution_status: cl_int) -> cl_int;
    unsafe { errcode_try("clSetUserEventStatus", cl_h::clSetUserEventStatus(
        event.as_ptr(), execution_status as cl_int)) }
}

pub unsafe fn set_event_callback(
            event: EventRaw, 
            callback_trigger: i32, 
            callback_receiver: extern fn (cl_event, cl_int, *mut c_void),
            user_data: *mut c_void,
            ) {
    let errcode = cl_h::clSetEventCallback(event.as_ptr(), callback_trigger, 
        callback_receiver, user_data);

    errcode_assert("clSetEventCallback", errcode);
}

//============================================================================
//============================ Profiling APIs ================================
//============================================================================











//============================================================================
//============================================================================
//============================================================================
//============================================================================
//============================================================================
//============================================================================
//============================================================================
//=========================== WORK IN PROGRESS ===============================
//============================================================================
//============================================================================
//============================================================================
//============================================================================
//============================================================================
//============================================================================
//============================================================================

/// [UNIMPLEMENTED][PLACEHOLDER]
pub fn get_event_profiling_info(obj: EventRaw, info_request: ProfilingInfo,
            ) -> OclResult<(ProfilingInfoResult)> {
    // cl_h::clGetEventProfilingInfo(event: cl_event,
    //                            param_name: cl_profiling_info,
    //                            param_value_size: size_t,
    //                            param_value: *mut c_void,
    //                            param_value_size_ret: *mut size_t) -> cl_int;


    let mut info_value_size: size_t = 0;

    let errcode = unsafe { cl_h::clGetEventProfilingInfo(
        obj.as_ptr() as cl_event,
        info_request as cl_profiling_info,
        0 as size_t,
        0 as *mut c_void,
        &mut info_value_size as *mut size_t,
    ) };
    try!(errcode_try("clGetEventProfilingInfo", errcode));

    let mut result: Vec<u8> = iter::repeat(0u8).take(info_value_size).collect();

    let errcode = unsafe { cl_h::clGetEventProfilingInfo(
        obj.as_ptr() as cl_event,
        info_request as cl_profiling_info,
        info_value_size,
        result.as_mut_ptr() as *mut _ as *mut c_void,
        0 as *mut size_t,
    ) };    
    // println!("GET_COMMAND_QUEUE_INFO(): errcode: {}, result: {:?}", errcode, result);
    errcode_try("clGetEventProfilingInfo", errcode)
        .and(Ok(ProfilingInfoResult::TemporaryPlaceholderVariant(result)))
}

//============================================================================
//========================= Flush and Finish APIs ============================
//============================================================================

/// [UNTESTED]
/// Flushes a command queue.
///
/// Issues all previously queued OpenCL commands in a command-queue to the 
/// device associated with the command-queue.
pub fn flush(command_queue: CommandQueueRaw) -> OclResult<()> {
    // cl_h::clFlush(command_queue: cl_command_queue) -> cl_int;
    unsafe { errcode_try("clFlush", cl_h::clFlush(command_queue.as_ptr())) }
}

/// Waits for a queue to finish.
///
/// Blocks until all previously queued OpenCL commands in a command-queue are 
/// issued to the associated device and have completed.
pub fn finish(command_queue: CommandQueueRaw) -> OclResult<()> {
    unsafe { 
        let errcode = cl_h::clFinish(command_queue.as_ptr());
        errcode_try("clFinish()", errcode)
    }
}

//============================================================================
//======================= Enqueued Commands APIs =============================
//============================================================================

/// Enqueues a read from device memory referred to by `buffer` to device memory,
/// `data`.
///
/// # Safety
///
/// It's complicated. Short version: make sure the memory pointed to by the 
/// slice, `data`, doesn't get reallocated before `new_event` is complete.
///
/// [FIXME]: Add a proper explanation of all the ins and outs. 
///
/// [FIXME]: Return result
pub unsafe fn enqueue_read_buffer<T>(
            command_queue: CommandQueueRaw,
            buffer: &MemRaw, 
            block: bool,
            data: &[T],
            offset: usize,
            wait_list: Option<&[EventRaw]>, 
            new_event: Option<&mut EventRaw>)
            -> OclResult<()> {
    let (wait_list_len, wait_list_ptr, new_event_ptr) = 
        resolve_event_opts(wait_list, new_event).expect("[FIXME]: enqueue_read_buffer()");

    let errcode = cl_h::clEnqueueReadBuffer(
        command_queue.as_ptr(), 
        buffer.as_ptr(), 
        block as cl_uint, 
        offset, 
        (data.len() * mem::size_of::<T>()) as size_t, 
        data.as_ptr() as cl_mem, 
        wait_list_len,
        wait_list_ptr,
        new_event_ptr,
    );

    errcode_try("clEnqueueReadBuffer()", errcode)
}

/// [UNIMPLEMENTED][PLACEHOLDER]
/// Enqueue commands to read from a rectangular region from a buffer object to host memory.
///
/// ## Official Documentation
///
/// [SDK]
///
/// [SDK]: https://www.khronos.org/registry/cl/sdk/1.2/docs/man/xhtml/clEnqueueReadBufferRect.html
pub fn enqueue_read_buffer_rect() -> OclResult<()> {
    // cl_h::clEnqueueReadBufferRect(command_queue: cl_command_queue,
    //                            buffer: cl_mem,
    //                            blocking_read: cl_bool,
    //                            buffer_origin: *mut size_t,
    //                            host_origin: *mut size_t,
    //                            region: *mut size_t,
    //                            buffer_slc_pitch: size_t,
    //                            buffer_slc_pitch: size_t,
    //                            host_slc_pitch: size_t,
    //                            host_slc_pitch: size_t,
    //                            ptr: *mut c_void,
    //                            num_events_in_wait_list: cl_uint,
    //                            event_wait_list: *const cl_event,
    //                            event: *mut cl_event) -> cl_int;
    unimplemented!();
}

/// Enqueues a write from host memory, `data`, to device memory referred to by
/// `buffer`.
///
/// [FIXME]: Return result
pub fn enqueue_write_buffer<T>(
            command_queue: CommandQueueRaw,
            buffer: &MemRaw, 
            block: bool,
            data: &[T],
            offset: usize,
            wait_list: Option<&[EventRaw]>, 
            new_event: Option<&mut EventRaw>)
            -> OclResult<()> {
    let (wait_list_len, wait_list_ptr, new_event_ptr) 
        = resolve_event_opts(wait_list, new_event)
            .expect("[FIXME: Return result]: enqueue_write_buffer()");

    // let wait_list_len = match &wait_list {
    //     &Some(ref wl) => wl.len() as u32,
    //     &None => 0,
    // };

    unsafe {
        // let wait_list_ptr = wait_list as *const *mut c_void;
        // let new_event_ptr = new_event as *mut *mut c_void;

        let errcode = cl_h::clEnqueueWriteBuffer(
                    command_queue.as_ptr(),
                    buffer.as_ptr(),
                    block as cl_uint,
                    offset,
                    (data.len() * mem::size_of::<T>()) as size_t,
                    data.as_ptr() as cl_mem,
                    wait_list_len,
                    wait_list_ptr,
                    new_event_ptr,
        );

        errcode_try("clEnqueueWriteBuffer()", errcode)
    }
}

/// [UNIMPLEMENTED][PLACEHOLDER]
pub fn enqueue_write_buffer_rect() -> OclResult<()> {
    // cl_h::clEnqueueWriteBufferRect(command_queue: cl_command_queue,
    //                             blocking_write: cl_bool,
    //                             buffer_origin: *mut size_t,
    //                             host_origin: *mut size_t,
    //                             region: *mut size_t,
    //                             buffer_slc_pitch: size_t,
    //                             buffer_slc_pitch: size_t,
    //                             host_slc_pitch: size_t,
    //                             host_slc_pitch: size_t,
    //                             ptr: *mut c_void,
    //                             num_events_in_wait_list: cl_uint,
    //                             event_wait_list: *const cl_event,
    //                             event: *mut cl_event) -> cl_int;
    unimplemented!();
}

/// [UNTESTED][UNUSED]
#[allow(dead_code)]
pub fn enqueue_copy_buffer(
            command_queue: CommandQueueRaw,
            src_buffer: MemRaw,
            dst_buffer: MemRaw,
            src_offset: usize,
            dst_offset: usize,
            len_copy_bytes: usize)
            -> OclResult<()> {
    let errcode = unsafe {
        cl_h::clEnqueueCopyBuffer(
        command_queue.as_ptr(),
        src_buffer.as_ptr(),
        dst_buffer.as_ptr(),
        src_offset,
        dst_offset,
        len_copy_bytes as usize,
        0,
        ptr::null(),
        ptr::null_mut(),
    ) };
    errcode_try("clEnqueueCopyBuffer()", errcode)
}

/// [UNIMPLEMENTED][PLACEHOLDER]
pub fn enqueue_fill_buffer() -> OclResult<()> {
    // clEnqueueFillBuffer(command_queue: cl_command_queue,
    //                 buffer: cl_mem, 
    //                 pattern: *const c_void, 
    //                 pattern_size: size_t, 
    //                 offset: size_t, 
    //                 size: size_t, 
    //                 num_events_in_wait_list: cl_uint, 
    //                 event_wait_list: *const cl_event, 
    //                 event: *mut cl_event) -> cl_int;
    unimplemented!();
}

/// [UNIMPLEMENTED][PLACEHOLDER]
pub fn enqueue_copy_buffer_rect() -> OclResult<()> {
    // cl_h::clEnqueueCopyBufferRect(command_queue: cl_command_queue,
    //                            src_buffer: cl_mem,
    //                            dst_buffer: cl_mem,
    //                            src_origin: *mut size_t,
    //                            dst_origin: *mut size_t,
    //                            region: *mut size_t,
    //                            src_slc_pitch: size_t,
    //                            src_slc_pitch: size_t,
    //                            dst_slc_pitch: size_t,
    //                            dst_slc_pitch: size_t,
    //                            num_events_in_wait_list: cl_uint,
    //                            event_wait_list: *const cl_event,
    //                            event: *mut cl_event) -> cl_int;
    unimplemented!();
}

/// [UNIMPLEMENTED][PLACEHOLDER]
pub fn enqueue_read_image() -> OclResult<()> {
    // cl_h::clEnqueueReadImage(command_queue: cl_command_queue,
    //                       image: cl_mem,
    //                       blocking_read: cl_bool,
    //                       origin: *mut size_t,
    //                       region: *mut size_t,
    //                       slc_pitch: size_t,
    //                       slc_pitch: size_t,
    //                       ptr: *mut c_void,
    //                       num_events_in_wait_list: cl_uint,
    //                       event_wait_list: *const cl_event,
    //                       event: *mut cl_event) -> cl_int;
    unimplemented!();
}

/// [UNIMPLEMENTED][PLACEHOLDER]
pub fn enqueue_write_image() -> OclResult<()> {
    // cl_h::clEnqueueWriteImage(command_queue: cl_command_queue,
    //                        image: cl_mem,
    //                        blocking_write: cl_bool,
    //                        origin: *mut size_t,
    //                        region: *mut size_t,
    //                        input_slc_pitch: size_t,
    //                        input_slc_pitch: size_t,
    //                        ptr: *mut c_void,
    //                        num_events_in_wait_list: cl_uint,
    //                        event_wait_list: *const cl_event,
    //                        event: *mut cl_event) -> cl_int;
    unimplemented!();
}

/// [UNIMPLEMENTED][PLACEHOLDER]
pub fn enqueue_fill_image() -> OclResult<()> {
    // clEnqueueFillImage(command_queue: cl_command_queue,
    //                   image: cl_mem, 
    //                   fill_color: *const c_void, 
    //                   origin: *const size_t, 
    //                   region: *const size_t, 
    //                   num_events_in_wait_list: cl_uint, 
    //                   event_wait_list: *const cl_event, 
    //                   event: *mut cl_event) -> cl_int;
    unimplemented!();
}

/// [UNIMPLEMENTED][PLACEHOLDER]
pub fn enqueue_copy_image() -> OclResult<()> {
    // cl_h::clEnqueueCopyImage(command_queue: cl_command_queue,
    //                       src_image: cl_mem,
    //                       dst_image: cl_mem,
    //                       src_origin: *mut size_t,
    //                       dst_origin: *mut size_t,
    //                       region: *mut size_t,
    //                       num_events_in_wait_list: cl_uint,
    //                       event_wait_list: *const cl_event,
    //                       event: *mut cl_event) -> cl_int;
    unimplemented!();
}

/// [UNIMPLEMENTED][PLACEHOLDER]
pub fn enqueue_copy_image_to_buffer() -> OclResult<()> {
    // cl_h::clEnqueueCopyImageToBuffer(command_queue: cl_command_queue,
    //                               src_image: cl_mem,
    //                               dst_buffer: cl_mem,
    //                               src_origin: *mut size_t,
    //                               region: *mut size_t,
    //                               dst_offset: size_t,
    //                               num_events_in_wait_list: cl_uint,
    //                               event_wait_list: *const cl_event,
    //                               event: *mut cl_event) -> cl_int;
    unimplemented!();
}

/// [UNIMPLEMENTED][PLACEHOLDER]
pub fn enqueue_copy_buffer_to_image() -> OclResult<()> {
    // cl_h::clEnqueueCopyBufferToImage(command_queue: cl_command_queue,
    //                               src_buffer: cl_mem,
    //                               dst_image: cl_mem,
    //                               src_offset: size_t,
    //                               dst_origin: *mut size_t,
    //                               region: *mut size_t,
    //                               num_events_in_wait_list: cl_uint,
    //                               event_wait_list: *const cl_event,
    //                               event: *mut cl_event) -> cl_int;
    unimplemented!();
}

/// [UNIMPLEMENTED][PLACEHOLDER]
pub fn enqueue_map_buffer() -> OclResult<()> {
    // cl_h::clEnqueueMapBuffer(command_queue: cl_command_queue,
    //                       buffer: cl_mem,
    //                       blocking_map: cl_bool,
    //                       map_flags: cl_map_flags,
    //                       offset: size_t,
    //                       cb: size_t,
    //                       num_events_in_wait_list: cl_uint,
    //                       event_wait_list: *const cl_event,
    //                       event: *mut cl_event,
    //                       errorcode_ret: *mut cl_int);
    unimplemented!();
}

/// [UNIMPLEMENTED][PLACEHOLDER]
pub fn enqueue_map_image() -> OclResult<()> {
    // cl_h::clEnqueueMapImage(command_queue: cl_command_queue,
    //                      image: cl_mem,
    //                      blocking_map: cl_bool,
    //                      map_flags: cl_map_flags,
    //                      origin: *mut size_t,
    //                      region: *mut size_t,
    //                      image_slc_pitch: size_t,
    //                      image_slc_pitch: size_t,
    //                      num_events_in_wait_list: cl_uint,
    //                      event_wait_list: *const cl_event,
    //                      event: *mut cl_event,
    //                      errorcode_ret: *mut cl_int);
    unimplemented!();
}

/// [UNIMPLEMENTED][PLACEHOLDER]
pub fn enqueue_unmap_mem_object() -> OclResult<()> {
    // cl_h::clEnqueueUnmapMemObject(command_queue: cl_command_queue,
    //                            memobj: cl_mem,
    //                            mapped_ptr: *mut c_void,
    //                            num_events_in_wait_list: cl_uint,
    //                            event_wait_list: *const cl_event,
    //                            event: *mut cl_event) -> cl_int;
    unimplemented!();
}

/// [UNIMPLEMENTED][PLACEHOLDER]
pub fn enqueue_migrate_mem_objects() -> OclResult<()> {
    // clEnqueueMigrateMemObjects(command_queue: cl_command_queue,
    //                           num_mem_objects: cl_uint,
    //                           mem_objects: *const cl_mem,
    //                           flags: cl_mem_migration_flags,
    //                           num_events_in_wait_list: cl_uint,
    //                           event_wait_list: *const cl_event,
    //                           event: *mut cl_event) -> cl_int;
    unimplemented!();
}

/// Enqueues a command to execute a kernel on a device.
///
/// # Stability
/// 
/// Work dimension/offset sizes *may* eventually be wrapped up in specialized types.
///
/// [SDK Docs](https://www.khronos.org/registry/cl/sdk/1.2/docs/man/xhtml/clEnqueueNDRangeKernel.html)
pub fn enqueue_n_d_range_kernel(
            command_queue: CommandQueueRaw,
            kernel: KernelRaw,
            work_dims: u32,
            global_work_offset: Option<[usize; 3]>,
            global_work_dims: [usize; 3],
            local_work_dims: Option<[usize; 3]>,
            wait_list: Option<&[EventRaw]>, 
            new_event: Option<&mut EventRaw>,
            kernel_name: Option<&str>
            ) -> OclResult<()> {
    let (wait_list_len, wait_list_ptr, new_event_ptr) = 
        try!(resolve_event_opts(wait_list, new_event));
    let gwo = resolve_work_dims(&global_work_offset);
    let gws = &global_work_dims as *const size_t;
    let lws = resolve_work_dims(&local_work_dims);

    unsafe {
        let errcode = cl_h::clEnqueueNDRangeKernel(
            command_queue.as_ptr(),
            kernel.as_ptr() as cl_kernel,
            work_dims,
            gwo,
            gws,
            lws,
            wait_list_len,
            wait_list_ptr,
            new_event_ptr,
        );

        let errcode_pre = format!("clEnqueueNDRangeKernel('{}'):", kernel_name.unwrap_or(""));
        errcode_try(&errcode_pre, errcode)
    }
}

/// [UNTESTED]
/// Enqueues a command to execute a kernel on a device.
///
/// The kernel is executed using a single work-item.
///
/// From [SDK]: clEnqueueTask is equivalent to calling clEnqueueNDRangeKernel 
/// with work_dim = 1, global_work_offset = NULL, global_work_size[0] set to 1,
/// and local_work_size[0] set to 1.
///
/// [SDK]: https://www.khronos.org/registry/cl/sdk/1.0/docs/man/xhtml/clEnqueueTask.html
pub fn enqueue_task(
            command_queue: CommandQueueRaw,
            kernel: KernelRaw,
            wait_list: Option<&[EventRaw]>, 
            new_event: Option<&mut EventRaw>,
            kernel_name: Option<&str>
            ) -> OclResult<()> {
    // cl_h::clEnqueueTask(command_queue: cl_command_queue,
    //                  kernel: cl_kernel,
    //                  num_events_in_wait_list: cl_uint,
    //                  event_wait_list: *const cl_event,
    //                  event: *mut cl_event) -> cl_int;

    let (wait_list_len, wait_list_ptr, new_event_ptr) = 
        try!(resolve_event_opts(wait_list, new_event));
    
    unsafe {
        let errcode = cl_h::clEnqueueTask(
            command_queue.as_ptr(),
            kernel.as_ptr() as cl_kernel,
            wait_list_len,
            wait_list_ptr,
            new_event_ptr,
        );

        let errcode_pre = format!("clEnqueueTask('{}'):", kernel_name.unwrap_or(""));
        errcode_try(&errcode_pre, errcode)
    }
}

/// [UNIMPLEMENTED][PLACEHOLDER]
pub fn enqueue_native_kernel() -> OclResult<()> {
    // cl_h::clEnqueueNativeKernel(command_queue: cl_command_queue,
    //                          user_func: extern fn (*mut c_void),
    //                          args: *mut c_void,
    //                          cb_args: size_t,
    //                          num_mem_objects: cl_uint,
    //                          mem_list: *const cl_mem,
    //                          args_mem_loc: *const *const c_void,
    //                          num_events_in_wait_list: cl_uint,
    //                          event_wait_list: *const cl_event,
    //                          event: *mut cl_event) -> cl_int;
    unimplemented!();
}

/// [UNIMPLEMENTED][PLACEHOLDER]
pub fn enqueue_marker_with_wait_list() -> OclResult<()> {
    // clEnqueueMarkerWithWaitList(command_queue: cl_command_queue,
    //          num_events_in_wait_list: cl_uint,
    //          event_wait_list: *const cl_event,
    //          event: *mut cl_event) -> cl_int;
    unimplemented!();
}

/// [UNIMPLEMENTED][PLACEHOLDER]
pub fn enqueue_barrier_with_wait_list() -> OclResult<()> {
    // clEnqueueBarrierWithWaitList(
    //          command_queue: cl_command_queue,
    //          num_events_in_wait_list: cl_uint,
    //          event_wait_list: *const cl_event,
    //          event: *mut cl_event) -> cl_int;
    unimplemented!();
}



/// [UNTESTED]
/// Returns the address of the extension function named by `func_name` for 
/// a given platform.
///
/// The pointer returned should be cast to a function pointer type matching the extension
/// function's definition defined in the appropriate extension specification and
/// header file. 
///
///
/// A non-NULL return value does
/// not guarantee that an extension function is actually supported by the
/// platform. The application must also make a corresponding query using
/// `ocl::raw::get_platform_info(platform_raw, CL_PLATFORM_EXTENSIONS, ... )` or
/// `ocl::raw::get_device_info(device_raw, CL_DEVICE_EXTENSIONS, ... )` 
/// to determine if an extension is supported by the OpenCL implementation.
///
/// [FIXME]: Update enum names above to the wrapped types.
///
/// # Errors
/// 
/// Returns an error if:
///
/// - `func_name` cannot be converted to a `CString`.
/// - The specified function does not exist for the implementation.
/// - 'platform' is not a valid platform.
///
// Extension function access
//
// Returns the extension function address for the given function name,
// or NULL if a valid function can not be found. The client must
// check to make sure the address is not NULL, before using or
// or calling the returned function address.
//
// A non-NULL return value for clGetExtensionFunctionAddressForPlatform does
// not guarantee that an extension function is actually supported by the
// platform. The application must also make a corresponding query using
// clGetPlatformInfo (platform, CL_PLATFORM_EXTENSIONS, ... ) or
// clGetDeviceInfo (device,CL_DEVICE_EXTENSIONS, ... ) to determine if an
// extension is supported by the OpenCL implementation.
// 
// [FIXME]: Return a generic that implements `Fn` (or `FnMut/Once`?).
// TODO: Create another function which will handle the second check described 
// above in addition to calling this.
pub unsafe fn get_extension_function_address_for_platform(platform: PlatformIdRaw,
            func_name: &str) -> OclResult<*mut c_void> {
    // clGetExtensionFunctionAddressForPlatform(platform: cl_platform_id,
    //                    func_name: *const c_char) -> *mut c_void;
    let func_name_c = try!(CString::new(func_name));

    let ext_fn = cl_h::clGetExtensionFunctionAddressForPlatform(
        platform.as_ptr(),
        func_name_c.as_ptr(),
    );

    if ext_fn == 0 as *mut c_void { 
        OclError::err("The specified function does not exist for the implementation or 'platform' \
            is not a valid platform.")
    } else {
        Ok(ext_fn)
    }
}

//============================================================================
//============================================================================
//=========================== DERIVED FUNCTIONS ==============================
//============================================================================
//============================================================================
// MANY OF THESE NEED TO BE MORPHED INTO THE MORE GENERAL VERSIONS AND MOVED UP

/// Creates, builds, and returns a new program pointer from `src_strings`.
///
/// TODO: Break out create and build parts into requisite functions then call
/// from here.
pub fn create_build_program(
            context: ContextRaw, 
            src_strings: Vec<CString>,
            cmplr_opts: CString,
            device_ids: &[DeviceIdRaw])
            -> OclResult<ProgramRaw> {
    // // Verify that the context is valid:
    // try!(verify_context(context));

    // // Lengths (not including \0 terminator) of each string:
    // let ks_lens: Vec<usize> = src_strings.iter().map(|cs| cs.as_bytes().len()).collect();  
    // // Pointers to each string:
    // let kern_string_ptrs: Vec<*const i8> = src_strings.iter().map(|cs| cs.as_ptr()).collect();

    // let mut errcode: cl_int = 0;
    
    // let program = ProgramRaw::new(unsafe { cl_h::clCreateProgramWithSource(
    //             context.as_ptr(), 
    //             kern_string_ptrs.len() as cl_uint,
    //             kern_string_ptrs.as_ptr() as *const *const i8,
    //             ks_lens.as_ptr() as *const usize,
    //             &mut errcode,
    // ) });
    // errcode_assert("clCreateProgramWithSource()", errcode);

    // let errcode = unsafe { cl_h::clBuildProgram(
    //             program.as_ptr(),
    //             device_ids.len() as cl_uint,
    //             device_ids.as_ptr() as *const cl_device_id, 
    //             cmplr_opts.as_ptr() as *const i8,
    //             mem::transmute(ptr::null::<fn()>()), 
    //             ptr::null_mut(),
    // ) };  

    // if errcode < 0 {
    //     program_build_err(program, device_ids).map(|_| program)
    // } else {
    //     try!(errcode_try("clBuildProgram()", errcode));
    //     Ok(program) 
    // }

    let program = try!(create_program_with_source(context, src_strings));
    try!(build_program(program, device_ids, cmplr_opts, None, None));
    Ok(program)
}

pub fn get_max_work_group_size(device: DeviceIdRaw) -> usize {
    let mut max_work_group_size: usize = 0;

    let errcode = unsafe { cl_h::clGetDeviceInfo(
        device.as_ptr(),
        cl_h::CL_DEVICE_MAX_WORK_GROUP_SIZE,
        mem::size_of::<usize>() as usize,
        &mut max_work_group_size as *mut _ as *mut c_void,
        ptr::null_mut(),
    ) };

    errcode_assert("clGetDeviceInfo", errcode);

    max_work_group_size
}

#[allow(dead_code)]
/// [FIXME]: Why are we wrapping in this array? Fix this.
pub fn wait_for_event(event: EventRaw) {
    let event_array: [EventRaw; 1] = [event];

    let errcode = unsafe {
        cl_h::clWaitForEvents(1, &(*event_array.as_ptr()).as_ptr())
    };

    errcode_assert("clWaitForEvents", errcode);
}

/// Returns the status of `event`.
pub fn get_event_status(event: EventRaw) -> cl_int {
    let mut status: cl_int = 0;

    let errcode = unsafe { 
        cl_h::clGetEventInfo(
            event.as_ptr(),
            cl_h::CL_EVENT_COMMAND_EXECUTION_STATUS,
            mem::size_of::<cl_int>(),
            &mut status as *mut _ as *mut c_void,
            ptr::null_mut(),
        )
    };

    errcode_assert("clGetEventInfo", errcode);

    status
}

/// [UNTESTED] Returns the platform name.
///
/// TODO: DEPRICATE
pub fn platform_name(platform: PlatformIdRaw) -> OclResult<String> {
    // let mut size = 0 as size_t;

    // unsafe {
    //     let name = cl_h::CL_PLATFORM_NAME as cl_platform_info;
    //     let mut errcode = cl_h::clGetPlatformInfo(
    //                 platform.as_ptr(),
    //                 name,
    //                 0,
    //                 ptr::null_mut(),
    //                 &mut size,
    //     );
    //     errcode_assert("clGetPlatformInfo(size)", errcode);
        
    //     let mut param_value: Vec<u8> = iter::repeat(32u8).take(size as usize).collect();
    //     errcode = cl_h::clGetPlatformInfo(
    //                 platform.as_ptr(),
    //                 name,
    //                 size,
    //                 param_value.as_mut_ptr() as *mut c_void,
    //                 ptr::null_mut(),
    //     );
    //     errcode_assert("clGetPlatformInfo()", errcode);
    //     println!("*** Platform Name ({}): {}", name, String::from_utf8(param_value).unwrap());
    // }

    let info_result = try!(get_platform_info(platform, PlatformInfo::Name));
    Ok(info_result.into())
    // println!("*** Platform Name ({}): {}", name, String::from_utf8(param_value).unwrap());
}

/// Verifies that the `context` is in fact a context object pointer.
///
/// # Assumptions
///
/// Some (most?/all?) OpenCL implementations do not correctly error if non-context pointers are passed. This function relies on the fact that passing the `CL_CONTEXT_DEVICES` as the `param_name` to `clGetContextInfo` will (at least on my AMD implementation) often return a huge result size if `context` is not actually a `cl_context` pointer due to the fact that it's reading from some random memory location on non-context structs. Also checks for zero because a context must have at least one device (true?). Should probably choose a value lower than 10kB because it seems unlikely any result would be that big but w/e.
///
/// [UPDATE]: This function may no longer be necessary now that the raw pointers have wrappers but it still prevents a hard to track down bug so leaving it intact for now.
///
#[inline]
pub fn verify_context(context: ContextRaw) -> OclResult<()> {
    // context_info(context, cl_h::CL_CONTEXT_REFERENCE_COUNT)
    if cfg!(release) {
        Ok(())
    } else {
        get_context_info(context, ContextInfo::Devices).and(Ok(()))
    }
}

//============================================================================
//============================================================================
//====================== Wow, you made it this far? ==========================
//============================================================================
//============================================================================




// /// Returns a string containing requested information.
// ///
// /// Currently lazily assumes everything is a char[] and converts to a String. 
// /// Non-string info types need to be manually reconstructed from that. Yes this
// /// is retarded.
// ///
// /// [TODO (low priority)]: Needs to eventually be made more flexible and should return 
// /// an enum with a variant corresponding to the type of info requested. Could 
// /// alternatively return a generic type and just blindly cast to it.
// #[allow(dead_code, unused_variables)] 
// pub fn device_info(device_id: DeviceIdRaw, info_type: cl_device_info) -> String {
//     let mut info_value_size: usize = 0;

//     let errcode = unsafe { 
//         cl_h::clGetDeviceInfo(
//             device_id.as_ptr(),
//             cl_h::CL_DEVICE_MAX_WORK_GROUP_SIZE,
//             mem::size_of::<usize>() as usize,
//             0 as cl_device_id,
//             &mut info_value_size as *mut usize,
//         ) 
//     }; 

//     errcode_assert("clGetDeviceInfo", errcode);

//     String::new()
// }




// /// Returns context information.
// ///
// /// [SDK Reference](https://www.khronos.org/registry/cl/sdk/1.2/docs/man/xhtml/clGetContextInfo.html)
// ///
// /// # Errors
// ///
// /// Returns an error result for all the reasons listed in the SDK in addition 
// /// to an additional error when called with `CL_CONTEXT_DEVICES` as described
// /// in in the `verify_context()` documentation below.
// ///
// /// TODO: Finish wiring up full functionality. Return a 'ContextInfo' enum result.
// pub fn context_info(context: ContextRaw, request_param: cl_context_info)
//             -> OclResult<()> {
//     let mut result_size = 0;

//     // let request_param: cl_context_info = cl_h::CL_CONTEXT_PROPERTIES;
//     let errcode = unsafe { cl_h::clGetContextInfo(   
//         context.as_ptr(),
//         request_param,
//         0,(
//         0 as *mut c_void,
//         &mut result_size as *mut usize,
//     ) };
//     try!(errcode_try("clGetContextInfo", errcode));
//     // println!("context_info(): errcode: {}, result_size: {}", errcode, result_size);

//     let err_if_zero_result_size = request_param == cl_h::CL_CONTEXT_DEVICES;

//     if result_size > 10000 || (result_size == 0 && err_if_zero_result_size) {
//         return OclError::err("\n\nocl::raw::context_info(): Possible invalid context detected. \n\
//             Context info result size is either '> 10k bytes' or '== 0'. Almost certainly an \n\
//             invalid context object. If not, please file an issue at: \n\
//             https://github.com/cogciprocate/ocl/issues.\n\n");
//     }

//     let mut result: Vec<u8> = iter::repeat(0).take(result_size).collect();

//     let errcode = unsafe { cl_h::clGetContextInfo(   
//         context.as_ptr(),
//         request_param,
//         result_size,
//         result.as_mut_ptr() as *mut c_void,
//         0 as *mut usize,
//     ) };
//     try!(errcode_try("clGetContextInfo", errcode));
//     // println!("context_info(): errcode: {}, result: {:?}", errcode, result);

//     Ok(())
// }
