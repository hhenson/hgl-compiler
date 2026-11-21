# Fails where libhglrt takes anything of hgraph's standard library or of
# Arrow, which the C++ target takes nothing from (docs/cpp-target.md), or
# where its public headers bring in hgraph's full wiring front end: the
# test `hglrt_takes_no_stdlib` runs it, with
#
#   PREPROCESSED  the include set of the library's public headers,
#                 preprocessed (tests/include_set.cpp): every header they
#                 bring in, at any depth, is named in it
#   SOURCE_DIR    runtime/cpp, whose sources are read for an include of one
#   READELF       readelf, or on macOS OTOOL, otool; and BINARIES the
#                 library, its tests and hgraph's runtime and wiring
#                 libraries they link, whose dynamic sections (load
#                 commands, on macOS) are read for a library they need: an
#                 SDK built with Arrow fails it
#                 (scripts/provision-hgraph-sdk.sh)
#
# Arrow's headers are under `arrow/`, hgraph's standard library's under
# `hgraph/lib/`, and their shared objects are `libarrow*`, `libparquet*` and
# `libhgraph_stdlib*`. The public headers take hgraph's front end for
# generated code (`hgraph/types/generated.h`) and not its full wiring front
# end, `graph_wiring.h`, which `subgraph_wiring.h` brings in: a unit pays
# for what it includes, and generated code uses neither.
file(STRINGS "${PREPROCESSED}" markers REGEX "^# [0-9]+ \"")
foreach(marker IN LISTS markers)
    if(marker MATCHES "\"[^\"]*(hgraph/lib/|/arrow/|hgraph/types/graph_wiring\\.h|hgraph/types/subgraph_wiring\\.h)[^\"]*\"")
        message(FATAL_ERROR "libhglrt's public headers bring in ${CMAKE_MATCH_0}")
    endif()
endforeach()

file(GLOB_RECURSE sources "${SOURCE_DIR}/include/*" "${SOURCE_DIR}/src/*" "${SOURCE_DIR}/tests/*")
foreach(source IN LISTS sources)
    file(STRINGS "${source}" includes REGEX "^[ \t]*#[ \t]*include[ \t]*[<\"]")
    foreach(include IN LISTS includes)
        if(include MATCHES "[<\"](hgraph/lib/|arrow/)")
            message(FATAL_ERROR "${source} includes ${include}")
        endif()
    endforeach()
endforeach()

foreach(binary IN LISTS BINARIES)
    if(OTOOL)
        # `otool -L`: the binary's own name, then each library it loads.
        execute_process(COMMAND "${OTOOL}" -L "${binary}" OUTPUT_VARIABLE needed COMMAND_ERROR_IS_FATAL ANY)
        set(needs "[^\n]*(libarrow|libparquet|libhgraph_stdlib)[^\n]*")
    else()
        execute_process(COMMAND "${READELF}" -d "${binary}" OUTPUT_VARIABLE needed COMMAND_ERROR_IS_FATAL ANY)
        set(needs "NEEDED[^\n]*(libarrow|libparquet|libhgraph_stdlib)[^\n]*")
    endif()
    if(needed MATCHES "${needs}")
        message(FATAL_ERROR "${binary} needs ${CMAKE_MATCH_0}")
    endif()
endforeach()
