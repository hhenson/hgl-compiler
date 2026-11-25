# libhglrt, the C++ runtime library of the HGL compiler's C++ target
# (docs/architecture.md, "The C++ runtime library"), and its unit tests.
#
# Built once for each hgraph SDK, against hgraph's runtime and wiring
# libraries alone. The SDK and its dependencies are found through
#
#   HGL_HGRAPH_SDK   the SDK's install prefix (scripts/provision-hgraph-sdk.sh)
#   HGL_HGRAPH_DEPS  the prefix of the SDK's dependencies
#
# given as cache variables or, failing that, in the environment. Options:
#
#   BUILD_TESTING    the unit tests (default ON), which need doctest
#   HGLRT_SANITIZE   the library and its tests built with the address and
#                    undefined-behaviour sanitizers (default OFF)
#
# The compiler is GCC 15 or later on Linux and Apple clang 21 or later on
# macOS, as the SDK's is (scripts/provision-hgraph-sdk.sh).
#
# What it gives: the shared library `libhglrt.so` (`libhglrt.dylib` on
# macOS; target `hglrt`, also `hglrt::hglrt`), whose one header for
# generated code is <hglrt/hglrt.h>, and a CMake package of it, in the
# build tree and where it is installed.
cmake_minimum_required(VERSION 3.25)

project(hglrt VERSION 0.1.0 LANGUAGES CXX)

set(HGL_HGRAPH_SDK "$ENV{HGL_HGRAPH_SDK}" CACHE PATH "The hgraph SDK's install prefix")
set(HGL_HGRAPH_DEPS "$ENV{HGL_HGRAPH_DEPS}" CACHE PATH "The prefix of the hgraph SDK's dependencies")
option(HGLRT_SANITIZE "Build with the address and undefined-behaviour sanitizers" OFF)

if(NOT HGL_HGRAPH_SDK OR NOT HGL_HGRAPH_DEPS)
    message(FATAL_ERROR "HGL_HGRAPH_SDK and HGL_HGRAPH_DEPS must name the hgraph SDK and its dependencies")
endif()
if(CMAKE_CXX_COMPILER_ID STREQUAL "GNU" AND CMAKE_CXX_COMPILER_VERSION VERSION_LESS 15)
    message(FATAL_ERROR "libhglrt needs GCC 15 or later; ${CMAKE_CXX_COMPILER} is GCC ${CMAKE_CXX_COMPILER_VERSION}")
endif()
if(CMAKE_CXX_COMPILER_ID STREQUAL "AppleClang" AND CMAKE_CXX_COMPILER_VERSION VERSION_LESS 21)
    message(FATAL_ERROR
        "libhglrt needs Apple clang 21 or later; ${CMAKE_CXX_COMPILER} is Apple clang ${CMAKE_CXX_COMPILER_VERSION}")
endif()

# hgraph's package finds fmt, spdlog and simdjson in the dependency
# prefix, and would fetch a missing one: none may be missing.
list(PREPEND CMAKE_PREFIX_PATH "${HGL_HGRAPH_SDK}" "${HGL_HGRAPH_DEPS}")
set(HGRAPH_FETCH_MISSING_DEPENDENCIES OFF CACHE BOOL "hgraph's dependencies come from HGL_HGRAPH_DEPS")
find_package(hgraph CONFIG REQUIRED)

include(GNUInstallDirs)

add_library(hglrt SHARED
    src/application.cpp
    src/capabilities.cpp
    src/checked.cpp
    src/collections.cpp
    src/conversions.cpp
    src/failure.cpp
    src/formatting.cpp
    src/host.cpp
    src/key_sets.cpp
    src/list_endpoints.cpp
    src/output.cpp
    src/scalar.cpp
    src/temporal.cpp
    src/tenors.cpp
    src/testing.cpp
    src/text.cpp
    src/value_text.cpp
)
add_library(hglrt::hglrt ALIAS hglrt)
target_include_directories(hglrt PUBLIC
    "$<BUILD_INTERFACE:${CMAKE_CURRENT_SOURCE_DIR}/include>"
    "$<INSTALL_INTERFACE:${CMAKE_INSTALL_INCLUDEDIR}>"
)
target_compile_features(hglrt PUBLIC cxx_std_23)
# Only hgraph's runtime and wiring: `hgraph::core` would bring its standard
# library, which generated code does not use (docs/cpp-target.md).
target_link_libraries(hglrt PUBLIC hgraph::runtime hgraph::wiring fmt::fmt)
# Installed, the library finds the SDK and its dependencies where it was
# built against them. (The SDK's own libraries find theirs in the
# dependency prefix: the provisioning script gives them that path.)
set_target_properties(hglrt PROPERTIES INSTALL_RPATH_USE_LINK_PATH ON)

# This repository's own C++ has every warning, as errors.
set(HGLRT_WARNINGS -Wall -Wextra -Werror)
target_compile_options(hglrt PRIVATE ${HGLRT_WARNINGS})

if(HGLRT_SANITIZE)
    set(HGLRT_SANITIZERS -fsanitize=address,undefined -fno-sanitize-recover=all -fno-omit-frame-pointer)
    target_compile_options(hglrt PUBLIC ${HGLRT_SANITIZERS})
    target_link_options(hglrt PUBLIC ${HGLRT_SANITIZERS})
endif()

# The package of the library, for a build of generated code: from the build
# tree, `-Dhglrt_DIR=<build>`; where it is installed, under its prefix.
include(CMakePackageConfigHelpers)
configure_package_config_file(cmake/hglrtConfig.cmake.in "${CMAKE_CURRENT_BINARY_DIR}/hglrtConfig.cmake"
    INSTALL_DESTINATION "${CMAKE_INSTALL_LIBDIR}/cmake/hglrt"
)
write_basic_package_version_file("${CMAKE_CURRENT_BINARY_DIR}/hglrtConfigVersion.cmake"
    COMPATIBILITY ExactVersion
)
export(TARGETS hglrt NAMESPACE hglrt:: FILE "${CMAKE_CURRENT_BINARY_DIR}/hglrtTargets.cmake")
install(TARGETS hglrt EXPORT hglrtTargets)
install(DIRECTORY include/ TYPE INCLUDE)
install(EXPORT hglrtTargets NAMESPACE hglrt:: DESTINATION "${CMAKE_INSTALL_LIBDIR}/cmake/hglrt")
install(FILES
    "${CMAKE_CURRENT_BINARY_DIR}/hglrtConfig.cmake"
    "${CMAKE_CURRENT_BINARY_DIR}/hglrtConfigVersion.cmake"
    DESTINATION "${CMAKE_INSTALL_LIBDIR}/cmake/hglrt"
)

include(CTest)
if(BUILD_TESTING)
    # doctest, the unit-test framework: a package where one is installed,
    # otherwise its release, fetched.
    find_package(doctest 2.5 CONFIG QUIET)
    if(NOT doctest_FOUND)
        include(FetchContent)
        set(DOCTEST_NO_INSTALL ON CACHE BOOL "doctest is a test dependency of libhglrt")
        FetchContent_Declare(doctest
            URL https://github.com/doctest/doctest/archive/refs/tags/v2.5.3.tar.gz
            URL_HASH SHA256=174ebc4e769928959614789c5b4e9c3d0a0f81a62bb608756b127bfebfb21331
            SYSTEM
        )
        FetchContent_MakeAvailable(doctest)
    endif()

    add_executable(hglrt_tests
        tests/main.cpp
        tests/application_tests.cpp
        tests/capabilities_tests.cpp
        tests/capture_tests.cpp
        tests/checked_tests.cpp
        tests/collections_tests.cpp
        tests/control_tests.cpp
        tests/conversion_tests.cpp
        tests/endpoint_tests.cpp
        tests/failure_tests.cpp
        tests/generator_tests.cpp
        tests/host_tests.cpp
        tests/keyed_tests.cpp
        tests/list_endpoints_tests.cpp
        tests/references_tests.cpp
        tests/scalar_tests.cpp
        tests/scheduler_tests.cpp
        tests/structs_tests.cpp
        tests/sources_tests.cpp
        tests/store_tests.cpp
        tests/temporal_tests.cpp
        tests/tenor_tests.cpp
        tests/testing_tests.cpp
        tests/text_tests.cpp
        tests/timing_tests.cpp
        tests/traces_tests.cpp
        tests/window_tests.cpp
    )
    target_link_libraries(hglrt_tests PRIVATE hglrt doctest::doctest)
    target_compile_options(hglrt_tests PRIVATE ${HGLRT_WARNINGS})
    add_test(NAME hglrt_tests COMMAND hglrt_tests WORKING_DIRECTORY "${CMAKE_CURRENT_BINARY_DIR}")

    # hgraph alone, with no libhglrt: the read of a window in which HGL's
    # text leads hgraph's, which windows.h guards. It fails once hgraph reads
    # as HGL does (tests/hgraph_window_read.cpp).
    add_executable(hgraph_window_read tests/hgraph_window_read.cpp)
    target_link_libraries(hgraph_window_read PRIVATE hgraph::runtime hgraph::wiring)
    target_compile_features(hgraph_window_read PRIVATE cxx_std_23)
    target_compile_options(hgraph_window_read PRIVATE ${HGLRT_WARNINGS})
    if(HGLRT_SANITIZE)
        target_compile_options(hgraph_window_read PRIVATE ${HGLRT_SANITIZERS})
        target_link_options(hgraph_window_read PRIVATE ${HGLRT_SANITIZERS})
    endif()
    add_test(NAME hgraph_window_read COMMAND hgraph_window_read)

    # Nothing of hgraph's standard library and nothing of Arrow, included or
    # linked (docs/cpp-target.md). The include set of the public headers is
    # preprocessed, its "object" the preprocessed text, so that every header
    # it brings in at any depth is checked.
    add_library(hglrt_include_set OBJECT tests/include_set.cpp)
    target_link_libraries(hglrt_include_set PRIVATE hglrt)
    # Clang would warn that `-c` goes unused, as `-E` stops before compiling.
    target_compile_options(hglrt_include_set PRIVATE -E $<$<CXX_COMPILER_ID:AppleClang,Clang>:-Qunused-arguments>)
    # The libraries a binary needs are read with readelf from an ELF
    # binary's dynamic section, and with otool from a Mach-O binary's load
    # commands on macOS.
    if(APPLE)
        find_program(HGLRT_OTOOL otool)
        if(HGLRT_OTOOL)
            set(HGLRT_NEEDED_TOOL "-DOTOOL=${HGLRT_OTOOL}")
        endif()
    elseif(CMAKE_READELF)
        set(HGLRT_NEEDED_TOOL "-DREADELF=${CMAKE_READELF}")
    endif()
    if(HGLRT_NEEDED_TOOL)
        add_test(NAME hglrt_takes_no_stdlib
            COMMAND "${CMAKE_COMMAND}"
                "-DPREPROCESSED=$<TARGET_OBJECTS:hglrt_include_set>"
                "-DSOURCE_DIR=${CMAKE_CURRENT_SOURCE_DIR}"
                "${HGLRT_NEEDED_TOOL}"
                "-DBINARIES=$<TARGET_FILE:hglrt>$<SEMICOLON>$<TARGET_FILE:hglrt_tests>$<SEMICOLON>$<TARGET_FILE:hgraph::runtime>$<SEMICOLON>$<TARGET_FILE:hgraph::wiring>"
                -P "${CMAKE_CURRENT_SOURCE_DIR}/cmake/check_no_stdlib.cmake")
    endif()
endif()
