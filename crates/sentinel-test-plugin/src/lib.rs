//! A minimal native plugin implementing the Sentinel ABI (see
//! `sentinel-dll-host`'s `abi` module). It uppercases its input — a trivial but
//! real transform that proves the host can pass bytes in, run native code, and
//! get bytes back across the C ABI boundary.

use std::os::raw::c_int;

/// Process `in_len` bytes at `in_ptr`, returning an uppercased copy via the
/// out-parameters. Returns 0 on success.
///
/// # Safety
/// `in_ptr` must be valid for `in_len` bytes; `out_ptr` and `out_len` must be
/// valid, writable pointers. The returned buffer must be freed with
/// [`sentinel_free`]. This is the host-facing ABI contract.
#[no_mangle]
pub unsafe extern "C" fn sentinel_invoke(
    in_ptr: *const u8,
    in_len: usize,
    out_ptr: *mut *mut u8,
    out_len: *mut usize,
) -> c_int {
    if in_ptr.is_null() || out_ptr.is_null() || out_len.is_null() {
        return 1;
    }
    // SAFETY: caller guarantees `in_ptr` is valid for `in_len` bytes per the ABI.
    let input = unsafe { std::slice::from_raw_parts(in_ptr, in_len) };
    let output = input.to_ascii_uppercase().into_boxed_slice();
    let len = output.len();
    let ptr = Box::into_raw(output) as *mut u8;
    // SAFETY: caller guarantees the out-pointers are valid and writable.
    unsafe {
        *out_ptr = ptr;
        *out_len = len;
    }
    0
}

/// Free a buffer previously returned by [`sentinel_invoke`].
///
/// # Safety
/// `ptr`/`len` must be exactly what a prior [`sentinel_invoke`] produced, freed
/// at most once.
#[no_mangle]
pub unsafe extern "C" fn sentinel_free(ptr: *mut u8, len: usize) {
    if ptr.is_null() {
        return;
    }
    // SAFETY: reconstruct the same Box<[u8]> that `sentinel_invoke` leaked.
    unsafe {
        let _ = Box::from_raw(std::ptr::slice_from_raw_parts_mut(ptr, len));
    }
}
