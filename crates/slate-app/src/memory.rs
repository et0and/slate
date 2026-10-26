//! Returning freed allocator pages after large project loads.

#[cfg(target_os = "macos")]
pub fn release_free_pages() {
    use std::ffi::c_void;

    unsafe extern "C" {
        fn malloc_zone_pressure_relief(zone: *mut c_void, goal: usize) -> usize;
    }

    // A null zone asks every zone to release free pages.
    unsafe {
        malloc_zone_pressure_relief(std::ptr::null_mut(), 0);
    }
}

#[cfg(all(target_os = "linux", target_env = "gnu"))]
pub fn release_free_pages() {
    unsafe extern "C" {
        fn malloc_trim(pad: usize) -> i32;
    }
    unsafe {
        malloc_trim(0);
    }
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
pub const fn release_free_pages() {}
