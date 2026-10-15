//! Retained allocator block bytes, including allocator size-class rounding.
//! Only linked into the separate memory executable. There is no safe standard
//! API for observing allocator block sizes; the wrapper delegates every request
//! to System unchanged and observes live blocks through the platform allocator.
#![allow(unsafe_code)]

use std::{
    alloc::{GlobalAlloc, Layout, System},
    sync::atomic::{AtomicUsize, Ordering},
};

static LIVE: AtomicUsize = AtomicUsize::new(0);

#[cfg(target_os = "macos")]
unsafe extern "C" {
    fn malloc_size(pointer: *const std::ffi::c_void) -> usize;
}
#[cfg(target_os = "linux")]
unsafe extern "C" {
    fn malloc_usable_size(pointer: *mut std::ffi::c_void) -> usize;
}
// SAFETY: callers pass a live, non-null System allocation; both platform
// functions accept these malloc/posix_memalign blocks without modifying them.
unsafe fn block_size(pointer: *mut u8) -> usize {
    #[cfg(target_os = "macos")]
    unsafe {
        malloc_size(pointer.cast())
    }
    #[cfg(target_os = "linux")]
    unsafe {
        malloc_usable_size(pointer.cast())
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        let _ = pointer;
        0
    }
}

pub struct CountingAllocator;

// SAFETY: System performs allocation and deallocation with the original Layout.
// Observation does not modify pointers, layouts, contents or lifetimes; relaxed
// atomics only count bytes and never control memory access. No counter operation
// allocates or unwinds. Failed allocation/reallocation leaves counts unchanged.
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: forwarded GlobalAlloc contract; inspect only a successful block.
        unsafe {
            let pointer = System.alloc(layout);
            if !pointer.is_null() {
                LIVE.fetch_add(block_size(pointer), Ordering::Relaxed);
            }
            pointer
        }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        // SAFETY: same forwarding and live-block invariant as alloc.
        unsafe {
            let pointer = System.alloc_zeroed(layout);
            if !pointer.is_null() {
                LIVE.fetch_add(block_size(pointer), Ordering::Relaxed);
            }
            pointer
        }
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        // SAFETY: the caller supplies a live block and its original layout;
        // inspect it before freeing, and never touch it after System.dealloc.
        unsafe {
            LIVE.fetch_sub(block_size(pointer), Ordering::Relaxed);
            System.dealloc(pointer, layout);
        }
    }

    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        // SAFETY: inspect the original live block before realloc can invalidate
        // it; on failure it remains allocated, and on success inspect only new.
        unsafe {
            let previous = block_size(pointer);
            let result = System.realloc(pointer, layout, size);
            if !result.is_null() {
                LIVE.fetch_sub(previous, Ordering::Relaxed);
                LIVE.fetch_add(block_size(result), Ordering::Relaxed);
            }
            result
        }
    }
}

pub fn live_bytes() -> usize {
    LIVE.load(Ordering::Relaxed)
}

#[cfg(all(test, any(target_os = "macos", target_os = "linux")))]
mod tests {
    use super::*;

    #[test]
    fn allocation_alignment_zeroing_reallocation_and_release() {
        // Use an independent local allocator instance; assert block properties
        // rather than global totals, since the test harness allocates in parallel.
        for alignment in [8, 16, 64, 4096] {
            let layout = Layout::from_size_align(128, alignment).unwrap();
            // SAFETY: matching layouts, non-zero sizes, bounds-checked accesses,
            // and successful realloc pointers replace their invalidated originals.
            unsafe {
                let pointer = CountingAllocator.alloc_zeroed(layout);
                assert!(!pointer.is_null());
                assert_eq!(pointer as usize % alignment, 0);
                assert!(block_size(pointer) >= 128);
                assert!(
                    std::slice::from_raw_parts(pointer, 128)
                        .iter()
                        .all(|&b| b == 0)
                );
                pointer.write(73);
                let pointer = CountingAllocator.realloc(pointer, layout, 1024);
                assert!(!pointer.is_null());
                assert_eq!(pointer as usize % alignment, 0);
                assert_eq!(pointer.read(), 73);
                assert!(block_size(pointer) >= 1024);
                CountingAllocator
                    .dealloc(pointer, Layout::from_size_align(1024, alignment).unwrap());
            }
        }
    }
}
