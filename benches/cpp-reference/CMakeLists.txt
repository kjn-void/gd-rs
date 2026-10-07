cmake_minimum_required(VERSION 3.24)

project(gd_cpp_reference_benchmarks LANGUAGES C CXX)
set(CMAKE_CXX_EXTENSIONS OFF)

include(FetchContent)

get_filename_component(
    GD_SOURCE_DIR_DEFAULT
    "${CMAKE_CURRENT_LIST_DIR}/../../external/gd"
    ABSOLUTE
)
set(GD_SOURCE_DIR "${GD_SOURCE_DIR_DEFAULT}" CACHE PATH
    "Path to the C++ GD source tree")

if(NOT EXISTS "${GD_SOURCE_DIR}/source/gd_table_column-buffer.cpp")
    message(FATAL_ERROR
        "GD source is missing at ${GD_SOURCE_DIR}; run git submodule update --init external/gd")
endif()

set(FETCHCONTENT_UPDATES_DISCONNECTED ON)

include(cmake/GdCore.cmake)

set(BENCHMARK_ENABLE_TESTING OFF CACHE BOOL "" FORCE)
set(BENCHMARK_ENABLE_INSTALL OFF CACHE BOOL "" FORCE)
set(BENCHMARK_ENABLE_GTEST_TESTS OFF CACHE BOOL "" FORCE)
FetchContent_Declare(
    googlebenchmark
    GIT_REPOSITORY https://github.com/google/benchmark.git
    GIT_TAG 192ef10025eb2c4cdd392bc502f0c852196baa48 # v1.9.5
    GIT_SHALLOW TRUE
)
FetchContent_MakeAvailable(googlebenchmark)

add_executable(gd_cpp_reference_benchmarks
    arguments_benchmark.cpp
    binary_benchmark.cpp
    expression_benchmark.cpp
    price_total_benchmark.cpp
    sqlite_benchmark.cpp
    sqlite_to_table_benchmark.cpp
    table_copy_benchmark.cpp
    table_column_buffer_benchmark.cpp
    table_index_benchmark.cpp
    utf8_benchmark.cpp
    variant_benchmark.cpp
    "${GD_SOURCE_DIR}/source/database/gd_database_io.cpp"
)
target_compile_features(gd_cpp_reference_benchmarks PRIVATE cxx_std_20)
target_link_libraries(gd_cpp_reference_benchmarks
    PRIVATE
        gd::core
        benchmark::benchmark_main
)

# gd_table_simd.h currently contains one literal placeholder in pack_set_values.
# Generate a benchmark-only corrected copy without modifying the C++ reference tree.
set(GD_SIMD_GENERATED_DIR "${CMAKE_CURRENT_BINARY_DIR}/gd-simd-generated")
file(MAKE_DIRECTORY "${GD_SIMD_GENERATED_DIR}")
file(READ "${GD_SOURCE_DIR}/source/gd_table_simd.h" GD_SIMD_HEADER)
string(FIND "${GD_SIMD_HEADER}" "noexcept { ..." GD_SIMD_PLACEHOLDER)
if(GD_SIMD_PLACEHOLDER EQUAL -1)
    message(FATAL_ERROR "Expected gd_table_simd.h placeholder was not found")
endif()
string(REPLACE "noexcept { ..." "noexcept {" GD_SIMD_HEADER "${GD_SIMD_HEADER}")
file(WRITE "${GD_SIMD_GENERATED_DIR}/gd_table_simd.h" "${GD_SIMD_HEADER}")
configure_file(
    "${GD_SOURCE_DIR}/source/gd_table_simd.cpp"
    "${GD_SIMD_GENERATED_DIR}/gd_table_simd.cpp"
    COPYONLY
)

add_executable(gd_cpp_reference_simd_comments
    simd_comments_benchmark.cpp
    "${GD_SIMD_GENERATED_DIR}/gd_table_simd.cpp"
)
target_compile_features(gd_cpp_reference_simd_comments PRIVATE cxx_std_20)
target_include_directories(gd_cpp_reference_simd_comments BEFORE PRIVATE
    "${GD_SIMD_GENERATED_DIR}"
)
target_link_libraries(gd_cpp_reference_simd_comments PRIVATE gd::core)

if(APPLE AND CMAKE_CXX_COMPILER_ID MATCHES "Clang")
    find_program(BREW_EXECUTABLE brew)
    if(BREW_EXECUTABLE)
        execute_process(
            COMMAND "${BREW_EXECUTABLE}" --prefix libomp
            OUTPUT_VARIABLE LIBOMP_PREFIX
            OUTPUT_STRIP_TRAILING_WHITESPACE
            RESULT_VARIABLE LIBOMP_PREFIX_RESULT
        )
    endif()
    if(NOT LIBOMP_PREFIX_RESULT EQUAL 0)
        set(LIBOMP_PREFIX "/opt/homebrew/opt/libomp")
    endif()
    find_path(LIBOMP_INCLUDE_DIR omp.h HINTS "${LIBOMP_PREFIX}/include")
    find_library(LIBOMP_LIBRARY omp HINTS "${LIBOMP_PREFIX}/lib")
    if(LIBOMP_INCLUDE_DIR AND LIBOMP_LIBRARY)
        add_library(gd_openmp INTERFACE)
        target_compile_options(gd_openmp INTERFACE -Xpreprocessor -fopenmp)
        target_include_directories(gd_openmp INTERFACE "${LIBOMP_INCLUDE_DIR}")
        target_link_libraries(gd_openmp INTERFACE "${LIBOMP_LIBRARY}")
    endif()
else()
    find_package(OpenMP QUIET COMPONENTS CXX)
    if(OpenMP_CXX_FOUND)
        add_library(gd_openmp INTERFACE)
        target_link_libraries(gd_openmp INTERFACE OpenMP::OpenMP_CXX)
    endif()
endif()

if(TARGET gd_openmp)
    add_executable(gd_cpp_reference_par_benchmark par_benchmark.cpp)
    target_compile_features(gd_cpp_reference_par_benchmark PRIVATE cxx_std_20)
    target_link_libraries(gd_cpp_reference_par_benchmark PRIVATE gd::core gd_openmp)
else()
    message(WARNING "OpenMP was not found; skipping gd_cpp_reference_par_benchmark")
endif()

# Standalone application for size and end-to-end comparisons (no benchmark framework).
find_package(Threads REQUIRED)
add_executable(gd_text_workflow text_workflow.cpp)
target_compile_features(gd_text_workflow PRIVATE cxx_std_20)
target_link_libraries(gd_text_workflow PRIVATE gd::core Threads::Threads)

add_executable(gd_order_workflow order_workflow/driver.cpp)
target_compile_features(gd_order_workflow PRIVATE cxx_std_20)
target_link_libraries(gd_order_workflow PRIVATE gd::core Threads::Threads)

add_executable(gd_order_workflow_simd
    order_workflow/driver.cpp
    "${GD_SIMD_GENERATED_DIR}/gd_table_simd.cpp"
)
target_compile_features(gd_order_workflow_simd PRIVATE cxx_std_20)
target_compile_definitions(gd_order_workflow_simd PRIVATE GD_WORKFLOW_SIMD)
target_include_directories(gd_order_workflow_simd BEFORE PRIVATE "${GD_SIMD_GENERATED_DIR}")
target_link_libraries(gd_order_workflow_simd PRIVATE gd::core Threads::Threads)

add_executable(gd_order_workflow_probes order_workflow/probes.cpp)
target_compile_features(gd_order_workflow_probes PRIVATE cxx_std_20)
target_link_libraries(gd_order_workflow_probes PRIVATE gd::core)
