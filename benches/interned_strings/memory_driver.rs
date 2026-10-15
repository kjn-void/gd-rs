//! Separate allocation-instrumented executable, never used for timing results.
mod memory;
mod workload;

#[global_allocator]
static ALLOCATOR: memory::CountingAllocator = memory::CountingAllocator;

#[allow(clippy::assertions_on_constants)] // Keep a runtime message on unsupported hosts.
fn main() {
    assert!(
        cfg!(any(target_os = "macos", target_os = "linux")),
        "Memory measurements require the macOS or Linux system allocator"
    );
    workload::run(memory::live_bytes);
}
