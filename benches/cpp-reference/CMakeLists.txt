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
