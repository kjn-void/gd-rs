cmake_minimum_required(VERSION 3.24)

project(index_sort LANGUAGES C CXX)
set(CMAKE_CXX_EXTENSIONS OFF)

if(NOT CMAKE_BUILD_TYPE AND NOT CMAKE_CONFIGURATION_TYPES)
    set(CMAKE_BUILD_TYPE Release CACHE STRING "Build configuration" FORCE)
endif()

include(FetchContent)

get_filename_component(
    GD_SOURCE_DIR_DEFAULT
    "${CMAKE_CURRENT_LIST_DIR}/../../../external/gd"
    ABSOLUTE
)
set(GD_SOURCE_DIR "${GD_SOURCE_DIR_DEFAULT}" CACHE PATH
    "Path to the C++ GD source tree")

if(NOT EXISTS "${GD_SOURCE_DIR}/source/gd_table_column-buffer.cpp")
    message(FATAL_ERROR
        "GD source is missing at ${GD_SOURCE_DIR}; run git submodule update --init external/gd")
endif()

set(FETCHCONTENT_UPDATES_DISCONNECTED ON)

# Reuse the gd-rs recipe that builds the pinned GD sources.
include("${CMAKE_CURRENT_LIST_DIR}/../../cpp-reference/cmake/GdCore.cmake")

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

add_executable(index_sort_benchmark index_sort_benchmark.cpp)
target_compile_features(index_sort_benchmark PRIVATE cxx_std_20)
target_link_libraries(index_sort_benchmark PRIVATE gd::core benchmark::benchmark)
