# Build the pinned GD sources without installing or modifying the submodule.
# This recipe and the installed-layout forwarding headers are owned by gd-rs.
include(FetchContent)

FetchContent_Declare(
    sqlite_amalgamation
    URL https://www.sqlite.org/2026/sqlite-amalgamation-3530200.zip
    URL_HASH SHA256=8a310d0a16c7a90cacd4c884e70faa51c902afed2a89f63aaa0126ab83558a32
    DOWNLOAD_EXTRACT_TIMESTAMP FALSE
)
FetchContent_MakeAvailable(sqlite_amalgamation)
add_library(gd_sqlite3 STATIC "${sqlite_amalgamation_SOURCE_DIR}/sqlite3.c")
target_include_directories(gd_sqlite3 PUBLIC "${sqlite_amalgamation_SOURCE_DIR}")

option(GD_ENABLE_SANITIZERS "Enable the sanitizers selected by GD_SANITIZERS" OFF)
set(GD_SANITIZERS "address,undefined" CACHE STRING
    "Comma-separated Clang/GCC sanitizers used when GD_ENABLE_SANITIZERS is enabled")

set(GD_CORE_SOURCES
    expression/gd_expression.cpp
    expression/gd_expression_code.cpp
    expression/gd_expression_method_01.cpp
    expression/gd_expression_parse_state.cpp
    expression/gd_expression_runtime.cpp
    expression/gd_expression_token.cpp
    expression/gd_expression_value.cpp
    gd_arguments.cpp
    gd_arguments_io.cpp
    gd_arguments_shared.cpp
    gd_binary.cpp
    gd_database_record.cpp
    gd_database_sqlite.cpp
    gd_parse.cpp
    gd_sql_value.cpp
    gd_table.cpp
    gd_table_arguments.cpp
    gd_table_column.cpp
    gd_table_column-buffer.cpp
    gd_table_index.cpp
    gd_table_io.cpp
    gd_table_table.cpp
    gd_types.cpp
    gd_utf8.cpp
    gd_utf8_2.cpp
    gd_variant.cpp
    gd_variant_view.cpp
    math/gd_math_string.cpp
)
list(TRANSFORM GD_CORE_SOURCES PREPEND "${GD_SOURCE_DIR}/source/")
add_library(gd_core STATIC ${GD_CORE_SOURCES})
add_library(gd::core ALIAS gd_core)
target_compile_features(gd_core PUBLIC cxx_std_20)
target_compile_definitions(gd_core PUBLIC GD_DATABASE_SQLITE_USE)

# A few upstream headers use installed-layout paths (gd/... and sqlite/...).
# Forward them from the build directory to the unmodified source dependencies.
set(GD_COMPAT_INCLUDE_DIR "${CMAKE_CURRENT_BINARY_DIR}/gd-compat-include")
file(MAKE_DIRECTORY "${GD_COMPAT_INCLUDE_DIR}/gd" "${GD_COMPAT_INCLUDE_DIR}/sqlite")
foreach(header gd_binary.h gd_compiler.h gd_table.h gd_table_column-buffer.h gd_types.h gd_utf8.h)
    file(GENERATE OUTPUT "${GD_COMPAT_INCLUDE_DIR}/gd/${header}"
        CONTENT "#pragma once\n#include <${header}>\n")
endforeach()
file(GENERATE OUTPUT "${GD_COMPAT_INCLUDE_DIR}/sqlite/sqlite3.h"
    CONTENT "#pragma once\n#include <sqlite3.h>\n")
target_include_directories(gd_core PUBLIC "${GD_SOURCE_DIR}/source" "${GD_COMPAT_INCLUDE_DIR}")
target_link_libraries(gd_core PUBLIC gd_sqlite3)

if(MSVC)
    target_compile_options(gd_core PRIVATE /W4)
else()
    target_compile_options(gd_core PRIVATE -Wall -Wextra -Wpedantic)
endif()

if(GD_ENABLE_SANITIZERS)
    if(NOT CMAKE_CXX_COMPILER_ID MATCHES "Clang|GNU")
        message(FATAL_ERROR "GD_ENABLE_SANITIZERS is unsupported by ${CMAKE_CXX_COMPILER_ID}")
    endif()
    add_library(gd_sanitizers INTERFACE)
    target_compile_options(gd_sanitizers INTERFACE
        -fno-omit-frame-pointer "-fsanitize=${GD_SANITIZERS}")
    target_link_options(gd_sanitizers INTERFACE
        -fno-omit-frame-pointer "-fsanitize=${GD_SANITIZERS}")
    target_link_libraries(gd_core PUBLIC gd_sanitizers)
endif()
