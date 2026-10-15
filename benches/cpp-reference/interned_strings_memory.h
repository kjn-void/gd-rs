#pragma once

// Only enabled in the allocation-measurement executable, never timed builds.
// Count allocator block sizes, including size-class rounding, with no added
// allocation headers. Match the Rust System allocator observer on each host.
#ifdef GD_BENCH_MEMORY
#include <atomic>
#include <cstdlib>
#include <new>
#ifdef __APPLE__
#include <malloc/malloc.h>
#else
#include <malloc.h>
#endif

static std::atomic<std::size_t> live_allocation_bytes{0};

static std::size_t allocation_size(void* pointer) noexcept {
#ifdef __APPLE__
    return malloc_size(pointer);
#else
    return malloc_usable_size(pointer);
#endif
}

void* operator new(std::size_t size) {
    auto pointer = std::malloc(size ? size : 1);
    if (!pointer) throw std::bad_alloc();
    live_allocation_bytes.fetch_add(allocation_size(pointer), std::memory_order_relaxed);
    return pointer;
}
void* operator new[](std::size_t size) { return ::operator new(size); }
void operator delete(void* pointer) noexcept {
    if (!pointer) return;
    live_allocation_bytes.fetch_sub(allocation_size(pointer), std::memory_order_relaxed);
    std::free(pointer);
}
void operator delete[](void* pointer) noexcept { ::operator delete(pointer); }
void operator delete(void* pointer, std::size_t) noexcept { ::operator delete(pointer); }
void operator delete[](void* pointer, std::size_t) noexcept { ::operator delete(pointer); }
void* operator new(std::size_t size, std::align_val_t alignment) {
    void* pointer = nullptr;
    if (posix_memalign(&pointer, static_cast<std::size_t>(alignment), size ? size : 1))
        throw std::bad_alloc();
    live_allocation_bytes.fetch_add(allocation_size(pointer), std::memory_order_relaxed);
    return pointer;
}
void* operator new[](std::size_t size, std::align_val_t alignment) {
    return ::operator new(size, alignment);
}
void operator delete(void* pointer, std::align_val_t) noexcept { ::operator delete(pointer); }
void operator delete[](void* pointer, std::align_val_t) noexcept { ::operator delete(pointer); }
void operator delete(void* pointer, std::size_t, std::align_val_t) noexcept { ::operator delete(pointer); }
void operator delete[](void* pointer, std::size_t, std::align_val_t) noexcept { ::operator delete(pointer); }
static std::size_t live_bytes() { return live_allocation_bytes.load(std::memory_order_relaxed); }
#else
static std::size_t live_bytes() { return 0; }
#endif
