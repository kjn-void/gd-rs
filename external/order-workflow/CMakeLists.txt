cmake_minimum_required(VERSION 3.24)
project(gd_order_workflow_debug LANGUAGES C CXX)
set(CMAKE_CXX_EXTENSIONS OFF)
set(CMAKE_INTERPROCEDURAL_OPTIMIZATION OFF)
if(MSVC)
    add_compile_options(/utf-8)
endif()

get_filename_component(GD_RS_ROOT "${CMAKE_CURRENT_LIST_DIR}/../.." ABSOLUTE)
set(GD_SOURCE_DIR "${GD_RS_ROOT}/external/gd" CACHE PATH "C++ GD source tree")
if(NOT EXISTS "${GD_SOURCE_DIR}/source/gd_table_column-buffer.cpp")
    message(FATAL_ERROR "Run git submodule update --init external/gd from the repository root")
endif()
include("${GD_RS_ROOT}/benches/cpp-reference/cmake/GdCore.cmake")
find_package(Threads REQUIRED)
find_package(Python3 REQUIRED COMPONENTS Interpreter)

# Use the same build-only correction as the benchmark; never edit external/gd.
set(GD_SIMD_GENERATED_DIR "${CMAKE_CURRENT_BINARY_DIR}/gd-simd-generated")
file(MAKE_DIRECTORY "${GD_SIMD_GENERATED_DIR}")
file(READ "${GD_SOURCE_DIR}/source/gd_table_simd.h" GD_SIMD_HEADER)
string(FIND "${GD_SIMD_HEADER}" "noexcept { ..." GD_SIMD_PLACEHOLDER)
if(GD_SIMD_PLACEHOLDER EQUAL -1)
    message(FATAL_ERROR "Expected gd_table_simd.h placeholder was not found")
endif()
string(REPLACE "noexcept { ..." "noexcept {" GD_SIMD_HEADER "${GD_SIMD_HEADER}")
file(GENERATE OUTPUT "${GD_SIMD_GENERATED_DIR}/gd_table_simd.h" CONTENT "${GD_SIMD_HEADER}")
configure_file("${GD_SOURCE_DIR}/source/gd_table_simd.cpp"
    "${GD_SIMD_GENERATED_DIR}/gd_table_simd.cpp" COPYONLY)

set(WORKFLOW_SOURCE_DIR "${GD_RS_ROOT}/benches/cpp-reference/order_workflow")
add_executable(gd_order_workflow "${WORKFLOW_SOURCE_DIR}/driver.cpp")
add_executable(gd_order_workflow_simd "${WORKFLOW_SOURCE_DIR}/driver.cpp"
    "${GD_SIMD_GENERATED_DIR}/gd_table_simd.cpp")
target_compile_definitions(gd_order_workflow_simd PRIVATE GD_WORKFLOW_SIMD)
target_include_directories(gd_order_workflow_simd BEFORE PRIVATE "${GD_SIMD_GENERATED_DIR}")

set(GD_WORKFLOW_DATABASE "${CMAKE_CURRENT_BINARY_DIR}/order-workflow-small.sqlite"
    CACHE FILEPATH "Shared SQLite fixture passed to the debugger")
set(GD_WORKFLOW_WORKERS "4" CACHE STRING "Worker count passed to the debugger")
set(GD_WORKFLOW_STAGE "verify" CACHE STRING "Stage passed to the debugger")
set_property(CACHE GD_WORKFLOW_STAGE PROPERTY STRINGS verify import prepare variants complete)

# The fixture generator refuses overwrites. An existing database is user-owned.
if(NOT EXISTS "${GD_WORKFLOW_DATABASE}")
    execute_process(COMMAND "${Python3_EXECUTABLE}"
        "${GD_RS_ROOT}/benches/order_workflow/fixture.py"
        "${GD_WORKFLOW_DATABASE}" --small
        COMMAND_ERROR_IS_FATAL ANY)
endif()

foreach(target gd_order_workflow gd_order_workflow_simd)
    target_compile_features(${target} PRIVATE cxx_std_20)
    target_link_libraries(${target} PRIVATE gd::core Threads::Threads)
    target_sources(${target} PRIVATE "${WORKFLOW_SOURCE_DIR}/workload.hpp"
        "${WORKFLOW_SOURCE_DIR}/pool.hpp")
    set_target_properties(${target} PROPERTIES
        VS_DEBUGGER_COMMAND_ARGUMENTS "\"${GD_WORKFLOW_DATABASE}\" ${GD_WORKFLOW_WORKERS} ${GD_WORKFLOW_STAGE} 1 native"
        VS_DEBUGGER_WORKING_DIRECTORY "${GD_RS_ROOT}")
endforeach()
target_sources(gd_order_workflow_simd PRIVATE "${WORKFLOW_SOURCE_DIR}/simd_table.hpp"
    "${GD_SIMD_GENERATED_DIR}/gd_table_simd.h")
set_property(DIRECTORY PROPERTY VS_STARTUP_PROJECT gd_order_workflow)

enable_testing()
foreach(target gd_order_workflow gd_order_workflow_simd)
    foreach(workers 1 4)
        add_test(NAME ${target}_verify_${workers}
            COMMAND ${target} "${GD_WORKFLOW_DATABASE}" ${workers} verify 1 native)
    endforeach()
endforeach()
