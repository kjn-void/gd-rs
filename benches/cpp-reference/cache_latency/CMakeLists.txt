cmake_minimum_required(VERSION 3.24)

project(cache_latency LANGUAGES CXX)

if(NOT CMAKE_BUILD_TYPE AND NOT CMAKE_CONFIGURATION_TYPES)
    set(CMAKE_BUILD_TYPE Release CACHE STRING "Build configuration" FORCE)
endif()

set(L1_CACHE_BYTES "" CACHE STRING "Target L1 data-cache line size in bytes")
set(L1_CACHE_SHIFT "" CACHE STRING "Log2 of the target L1 data-cache line size")

if(L1_CACHE_BYTES STREQUAL "" AND L1_CACHE_SHIFT STREQUAL "")
    if(CMAKE_CROSSCOMPILING OR NOT CMAKE_SYSTEM_NAME STREQUAL "Linux")
        message(FATAL_ERROR
            "Set -DL1_CACHE_BYTES=<target bytes> or -DL1_CACHE_SHIFT=<target shift>.")
    endif()

    find_program(GETCONF_EXECUTABLE getconf REQUIRED)
    execute_process(
        COMMAND "${GETCONF_EXECUTABLE}" LEVEL1_DCACHE_LINESIZE
        OUTPUT_VARIABLE detected_line_bytes
        OUTPUT_STRIP_TRAILING_WHITESPACE
        RESULT_VARIABLE getconf_result
    )
    if(NOT getconf_result EQUAL 0 OR NOT detected_line_bytes MATCHES "^[0-9]+$")
        message(FATAL_ERROR
            "Cannot detect the L1 data-cache line size; set -DL1_CACHE_BYTES=<bytes>.")
    endif()
    set(L1_CACHE_BYTES "${detected_line_bytes}" CACHE STRING
        "Target L1 data-cache line size in bytes" FORCE)
endif()

set(cache_line_definitions)
foreach(name IN ITEMS L1_CACHE_BYTES L1_CACHE_SHIFT)
    if(NOT "${${name}}" STREQUAL "")
        if(NOT "${${name}}" MATCHES "^[0-9]+$")
            message(FATAL_ERROR "${name} must be a nonnegative integer.")
        endif()
        list(APPEND cache_line_definitions "${name}=${${name}}")
    endif()
endforeach()

if(NOT L1_CACHE_SHIFT STREQUAL "")
    if(L1_CACHE_SHIFT LESS 4 OR L1_CACHE_SHIFT GREATER 10)
        message(FATAL_ERROR "L1_CACHE_SHIFT must be between 4 and 10.")
    endif()
    math(EXPR shifted_line_bytes "1 << ${L1_CACHE_SHIFT}")
    if(NOT L1_CACHE_BYTES STREQUAL "" AND NOT L1_CACHE_BYTES EQUAL shifted_line_bytes)
        message(FATAL_ERROR "L1_CACHE_BYTES and L1_CACHE_SHIFT disagree.")
    endif()
    set(cache_line_bytes "${shifted_line_bytes}")
else()
    set(cache_line_bytes "${L1_CACHE_BYTES}")
endif()

math(EXPR line_size_remainder "${cache_line_bytes} & (${cache_line_bytes} - 1)")
if(cache_line_bytes LESS 16 OR cache_line_bytes GREATER 1024 OR line_size_remainder)
    message(FATAL_ERROR "The cache-line size must be a power of two from 16 to 1024 bytes.")
endif()
message(STATUS "L1 data-cache line size: ${cache_line_bytes} bytes")

add_executable(cache-latency cache_latency.cpp)
target_compile_features(cache-latency PRIVATE cxx_std_20)
set_target_properties(cache-latency PROPERTIES CXX_EXTENSIONS OFF)
target_compile_definitions(cache-latency PRIVATE ${cache_line_definitions})
target_compile_options(cache-latency PRIVATE
    -Wall -Wextra "$<$<CONFIG:Release>:-O3>")

include(CheckIPOSupported)
check_ipo_supported(LANGUAGES CXX)
set_property(TARGET cache-latency PROPERTY INTERPROCEDURAL_OPTIMIZATION_RELEASE TRUE)

include(CTest)
if(BUILD_TESTING)
    add_test(NAME cache-latency-verify COMMAND cache-latency --verify)
endif()
