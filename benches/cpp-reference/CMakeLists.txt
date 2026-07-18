cmake_minimum_required(VERSION 3.24)

project(gd_cpp_reference_benchmarks LANGUAGES C CXX)

include(FetchContent)

get_filename_component(
    GD_SOURCE_DIR_DEFAULT
    "${CMAKE_CURRENT_LIST_DIR}/../../../gd"
    ABSOLUTE
)
set(GD_SOURCE_DIR "${GD_SOURCE_DIR_DEFAULT}" CACHE PATH
    "Path to the C++ GD source tree")

if(NOT EXISTS "${GD_SOURCE_DIR}/CMakeLists.txt")
    message(FATAL_ERROR
        "GD_SOURCE_DIR does not contain CMakeLists.txt: ${GD_SOURCE_DIR}")
endif()

set(GD_BUILD_TESTS OFF CACHE BOOL "" FORCE)
set(GD_BUILD_BENCHMARKS OFF CACHE BOOL "" FORCE)
set(FETCHCONTENT_UPDATES_DISCONNECTED ON)

add_subdirectory("${GD_SOURCE_DIR}" "${CMAKE_CURRENT_BINARY_DIR}/gd" EXCLUDE_FROM_ALL)

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
    table_column_buffer_benchmark.cpp
    table_index_benchmark.cpp
    utf8_benchmark.cpp
    variant_benchmark.cpp
)
target_compile_features(gd_cpp_reference_benchmarks PRIVATE cxx_std_20)
target_link_libraries(gd_cpp_reference_benchmarks
    PRIVATE
        gd::core
        benchmark::benchmark_main
)

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
