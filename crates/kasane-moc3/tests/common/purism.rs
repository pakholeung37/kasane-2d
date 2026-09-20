#![allow(dead_code)]

use std::alloc::{alloc, dealloc, Layout as MemLayout};
use std::ffi::CStr;
use std::os::raw::{c_char, c_float, c_int, c_uchar, c_uint, c_ushort, c_void};

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct CsmVector2 {
    pub x: c_float,
    pub y: c_float,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct CsmVector4 {
    pub x: c_float,
    pub y: c_float,
    pub z: c_float,
    pub w: c_float,
}

extern "C" {
    pub fn csmGetVersion() -> c_uint;
    pub fn csmHasMocConsistency(address: *mut c_void, size: c_uint) -> c_int;
    pub fn csmReviveMocInPlace(address: *mut c_void, size: c_uint) -> *mut c_void;
    pub fn csmGetSizeofModel(moc: *const c_void) -> c_uint;
    pub fn csmInitializeModelInPlace(
        moc: *const c_void,
        address: *mut c_void,
        size: c_uint,
    ) -> *mut c_void;
    pub fn csmUpdateModel(model: *mut c_void);
    pub fn csmReadCanvasInfo(
        model: *const c_void,
        out_size: *mut CsmVector2,
        out_origin: *mut CsmVector2,
        out_ppu: *mut c_float,
    );

    pub fn csmGetParameterCount(model: *const c_void) -> c_int;
    pub fn csmGetParameterIds(model: *const c_void) -> *const *const c_char;
    pub fn csmGetParameterMinimumValues(model: *const c_void) -> *const c_float;
    pub fn csmGetParameterMaximumValues(model: *const c_void) -> *const c_float;
    pub fn csmGetParameterDefaultValues(model: *const c_void) -> *const c_float;
    pub fn csmGetParameterValues(model: *mut c_void) -> *mut c_float;

    pub fn csmGetPartCount(model: *const c_void) -> c_int;
    pub fn csmGetPartIds(model: *const c_void) -> *const *const c_char;
    pub fn csmGetPartParentPartIndices(model: *const c_void) -> *const c_int;

    pub fn csmGetDrawableCount(model: *const c_void) -> c_int;
    pub fn csmGetDrawableIds(model: *const c_void) -> *const *const c_char;
    pub fn csmGetDrawableConstantFlags(model: *const c_void) -> *const c_uchar;
    pub fn csmGetDrawableDynamicFlags(model: *const c_void) -> *const c_uchar;
    pub fn csmGetDrawableTextureIndices(model: *const c_void) -> *const c_int;
    pub fn csmGetDrawableDrawOrders(model: *const c_void) -> *const c_int;
    pub fn csmGetRenderOrders(model: *const c_void) -> *const c_int;
    pub fn csmGetDrawableOpacities(model: *const c_void) -> *const c_float;
    pub fn csmGetDrawableMaskCounts(model: *const c_void) -> *const c_int;
    pub fn csmGetDrawableMasks(model: *const c_void) -> *const *const c_int;
    pub fn csmGetDrawableVertexCounts(model: *const c_void) -> *const c_int;
    pub fn csmGetDrawableVertexPositions(model: *const c_void) -> *const *const CsmVector2;
    pub fn csmGetDrawableVertexUvs(model: *const c_void) -> *const *const CsmVector2;
    pub fn csmGetDrawableIndexCounts(model: *const c_void) -> *const c_int;
    pub fn csmGetDrawableIndices(model: *const c_void) -> *const *const c_ushort;
    pub fn csmGetDrawableMultiplyColors(model: *const c_void) -> *const CsmVector4;
    pub fn csmGetDrawableScreenColors(model: *const c_void) -> *const CsmVector4;
    pub fn csmGetDrawableParentPartIndices(model: *const c_void) -> *const c_int;
}

pub struct AlignedBuffer {
    ptr: *mut u8,
    layout: MemLayout,
    pub size: usize,
}

impl AlignedBuffer {
    pub fn new(size: usize, align: usize) -> Self {
        let rounded = (size + align - 1) & !(align - 1);
        let layout = MemLayout::from_size_align(rounded.max(align), align).unwrap();
        let ptr = unsafe { alloc(layout) };
        assert!(!ptr.is_null(), "Allocation failed");
        unsafe { std::ptr::write_bytes(ptr, 0, rounded) };
        Self {
            ptr,
            layout,
            size: rounded,
        }
    }

    pub fn as_ptr(&self) -> *const u8 {
        self.ptr
    }

    pub fn as_mut_ptr(&mut self) -> *mut u8 {
        self.ptr
    }
}

impl Drop for AlignedBuffer {
    fn drop(&mut self) {
        unsafe {
            dealloc(self.ptr, self.layout);
        }
    }
}

pub unsafe fn c_str_to_str<'a>(ptr: *const c_char) -> &'a str {
    if ptr.is_null() {
        ""
    } else {
        CStr::from_ptr(ptr).to_str().unwrap()
    }
}
