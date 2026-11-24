#!/usr/bin/env bash
# Checks a release's two archives as a user has them (docs/releases.md):
# the release alone, and the release with the hgl-debug archive unpacked into
# it. Run from the repository's root, whose fixtures it builds, on the
# platform the archives are for, with CMake, Ninja, the platform's compiler
# and its debugger: gdb on Linux, lldb on macOS.
#
# Usage: scripts/check-debug-release.sh DIRECTORY
#
#   DIRECTORY  holds hgl-VERSION-PLATFORM.tar.gz and
#              hgl-debug-VERSION-PLATFORM.tar.gz, each with its .sha256.
#
# 1. The release alone: a debug build links the optimised libraries, says
#    why, and runs; a debugger names an engine function and finds no line.
# 2. Both in one tree: the build tool verifies the debug tree. A debug
#    build links the debug libraries beside hgl, says
#    so, and runs; a debugger stops in an engine frame of it with its file,
#    line and locals. The launch's source map makes the engine source readable.
#    The release's optimised engine and fmt libraries
#    resolve functions to files and lines through their symbol files. On
#    Linux the engine's line is also checked in a live process and core file.
#
# The debuggers look for symbol files beside the libraries alone, never in a
# directory or service of the machine's: gdb is given an empty debug file
# directory and no debuginfod; lldb is told not to look elsewhere
# (`symbols.enable-external-lookup`), which on macOS would find a dSYM of the
# same UUID anywhere Spotlight has seen one, in a release cache say. What
# they find is what the two trees hold.
set -euo pipefail

fail() {
    echo "check-debug-release: $*" >&2
    exit 1
}

[[ $# -eq 1 ]] || fail "usage: $0 DIRECTORY"
archives=$(cd -- "$1" && pwd -P)
source scripts/hgraph-dependencies.sh
fmt_major=${FMT_VERSION%%.*}
case "$(uname -s)-$(uname -m)" in
    Darwin-arm64)
        platform=macos-arm64 target=aarch64-apple-darwin suffix=dylib checksum=(shasum -a 256)
        fmt_library=libfmt.$fmt_major.dylib
        ;;
    Linux-x86_64)
        platform=linux-x86_64 target=x86_64-unknown-linux-gnu suffix=so checksum=(sha256sum)
        fmt_library=libfmt.so.$fmt_major
        ;;
    *) fail "runs on Apple silicon macOS or x86-64 Linux" ;;
esac
build_tool=(cargo run --locked --quiet -p hgl_build_tool --)
version=$("${build_tool[@]}" version)
name=hgl-$version-$platform
debug_name=hgl-debug-$version-$platform
fixtures=$PWD/crates/hgl_cpp/tests/fixtures
program=("$fixtures/demo.apps.hgl" "$fixtures/demo.apps.backtest.hgl")
fmt_function=fmt::v${fmt_major}::assert_fail

scratch=$(mktemp -d "${TMPDIR:-/tmp}/hgl-check-debug.XXXXXX")
scratch=$(cd -- "$scratch" && pwd -P)
trap 'rm -rf -- "$scratch"' EXIT
(
    cd -- "$archives"
    "${checksum[@]}" -c "$name.tar.gz.sha256" "$debug_name.tar.gz.sha256"
)
# Each tree is unpacked under a path with a space, as a user's may be. The
# second is unpacked once the first is checked: a debugger on macOS may find a
# dSYM anywhere by its UUID.
alone="$scratch/release alone"
both="$scratch/release with hgl-debug"
mkdir -p -- "$alone" "$both"

unset HGL_HGRAPH_SDK HGL_HGRAPH_DEPS HGL_HGRAPH_DEBUG HGL_CPP_FLAGS LD_LIBRARY_PATH DYLD_LIBRARY_PATH
export HGL_NATIVE_CACHE="$scratch/native"
# gdb asks no server for symbols, and looks in no directory of the machine's.
export DEBUGINFOD_URLS=
nowhere=$scratch/no-symbols
mkdir -p -- "$nowhere"
gdb=(gdb -batch -nx -iex "set debug-file-directory $nowhere")
lldb=(xcrun lldb --batch -O 'settings set symbols.enable-external-lookup false')

# A debug build of the fixtures with the hgl of release tree $1, into $2:
# its JSON description in $2.json and what hgl said in $2.said.
debug_build() {
    "$1/$name/bin/hgl" run "${program[@]}" --debug-build "$2" >"$2.json" 2>"$2.said"
    cat -- "$2.said"
}

# What the platform's debugger says of function $2 in shared library $1.
lookup_symbol() {
    if [[ $platform == linux-x86_64 ]]; then
        "${gdb[@]}" -ex "info line $2" "$1" 2>&1
    else
        "${lldb[@]}" -o "target create '$1'" \
            -o "image lookup -v -n $2" 2>&1
    fi
}

# Print engine source through the launch description's installed source path.
list_engine_source() {
    local library=$1 source=$2 function=hgraph::NodeView::global_state
    if [[ $platform == linux-x86_64 ]]; then
        "${gdb[@]}" -ex "set substitute-path /hgl-release/hgraph \"$source\"" \
            -ex "list $function" "$library" 2>&1
    else
        "${lldb[@]}" -o "target create '$library'" \
            -o "settings set target.source-map /hgl-release/hgraph \"$source\"" \
            -o "source list -n $function" 2>&1
    fi
}

# What the platform's debugger shows of a run of the command in file $1 (one
# word to a line), stopped where the engine's runtime library gives a node its
# store: the library the process loaded, the frame and its arguments, and on
# Linux the locals of the engine's loop over the nodes; or, where $2 is given, a core file of the process
# written there in place of the locals, which gdb 17 cannot always show of an
# optimised build.
stop_in_engine() {
    local -a command=()
    local word
    while IFS= read -r word; do
        command+=("$word")
    done <"$1"
    if [[ $platform == macos-arm64 ]]; then
        "${lldb[@]}" \
            -o 'breakpoint set --shlib libhgraph_runtime.dylib --name hgraph::NodeView::global_state' \
            -o 'process launch --stdin /dev/null' -o 'frame info' -o 'frame variable' \
            -o 'image list libhgraph_runtime.dylib' -o 'process kill' \
            -- "${command[@]}" 2>&1 || true
        return
    fi
    local follow
    if [[ -n ${2:-} ]]; then
        follow="generate-core-file $2"
    else
        follow=$(cat <<'GDB'
echo engine stack:\n
frame apply all -s info locals
GDB
)
    fi
    # The source-qualified breakpoint ignores the executable's PLT stub.
    cat >"$scratch/stop.gdb" <<GDB
set pagination off
set confirm off
set print max-depth 1
start
break -source node.cpp -function hgraph::NodeView::global_state
continue
echo engine symbol:\n
info symbol \$pc
echo engine top frame:\n
frame 0
$follow
kill
GDB
    "${gdb[@]}" -x "$scratch/stop.gdb" --args "${command[@]}" </dev/null 2>&1 |
        awk '
            /^engine symbol:$/ {
                getline
                if (sub(/^.* in section [^ ]+ of /, "")) print "engine library: " $0
                next
            }
            /^engine top frame:$/ {
                getline
                if (/^#0 .*hgraph::NodeView::global_state / && sub(/^.* at /, ""))
                    print "engine frame: hgraph::NodeView::global_state at " $0
                next
            }
            /^engine stack:$/ { stack = 1; next }
            stack && /^#[0-9]+ / {
                frame = !found && /evaluate_nodes</ && !/annotate_on_exception/ && /\/hgl-release\/hgraph\//
                if (frame) found = 1
                next
            }
            frame && /^[[:alnum:]_]+ = / { ++locals; next }
            /^Saved corefile/ { print }
            END { if (locals) print "engine loop locals: " locals }
        ' || true
}

# Whether what the debugger said, $1, has the process load the engine's
# runtime library from directory $2.
loaded() {
    local library=$2/libhgraph_runtime.$suffix
    if [[ $platform == linux-x86_64 ]]; then
        grep -qxF "engine library: $library" <<<"$1"
    else
        grep -qF " $library" <<<"$1"
    fi
}

# What gdb says of the newest frame in the core file $2 of executable $1.
core_frame() {
    "${gdb[@]}" -ex 'frame 0' "$1" "$2" 2>&1 | grep -E '^#0' || true
}

echo "== the release alone"
tar -xzf "$archives/$name.tar.gz" -C "$alone"
"${build_tool[@]}" verify --target "$target" "$alone/$name"
said=$(debug_build "$alone" "$scratch/alone")
grep -qx "hgl: a debug build, with hgraph's optimised libraries in .*: the \`hgl-debug\` package is not installed beside hgl, and HGL_HGRAPH_DEBUG is not set" <<<"$said" ||
    fail "the release alone said: $said"
[[ $("${build_tool[@]}" launch-field "$scratch/alone.json" hgraph libraries) == optimised ]] || fail "the release alone did not link the optimised libraries"
if "${build_tool[@]}" launch-field "$scratch/alone.json" sourceMap /hgl-release/hgraph >/dev/null 2>&1; then
    fail "the release alone reported an engine source map"
fi
"${build_tool[@]}" run-described "$scratch/alone.json"
looked=$(lookup_symbol "$alone/$name/libexec/hgraph-sdk/lib/libhgraph_runtime.$suffix" hgraph::NodeView::global_state)
printf '%s\n' "$looked"
! grep -q 'node\.cpp' <<<"$looked" || fail "the release alone found a line of the engine"
# Open the install-name link that a program loads, especially on macOS
# where the sibling dSYM is named for that link.
looked=$(lookup_symbol "$alone/$name/libexec/hgraph-deps/lib/$fmt_library" "$fmt_function")
printf '%s\n' "$looked"
grep -q 'assert_fail' <<<"$looked" || fail "the release alone found no fmt symbol"
! grep -Eq '^Line [0-9]+ of |^[[:space:]]*LineEntry:' <<<"$looked" ||
    fail "the release alone found a line of fmt"

echo "== the release with hgl-debug"
tar -xzf "$archives/$name.tar.gz" -C "$both"
tar -xzf "$archives/$debug_name.tar.gz" -C "$both"
stamp=$both/$name/libexec/hgraph-debug/hgl-debug.txt
[[ -f $stamp && $(<"$stamp") == "$version" ]] || fail "the debug archive has no stamp for hgl $version"
"${build_tool[@]}" verify-debug --target "$target" "$both/$name"
said=$(debug_build "$both" "$scratch/both")
grep -qx "hgl: a debug build, with hgraph's debug libraries in .*/libexec/hgraph-debug, installed beside hgl" <<<"$said" ||
    fail "the release with hgl-debug said: $said"
[[ $("${build_tool[@]}" launch-field "$scratch/both.json" hgraph libraries) == debug ]] || fail "the release with hgl-debug did not link the debug libraries"
mapped=$("${build_tool[@]}" launch-field "$scratch/both.json" sourceMap /hgl-release/hgraph)
[[ $mapped == "$both/$name/libexec/hgraph-debug/hgraph-source" ]] ||
    fail "the launch did not map engine source to the installed debug archive"
listed=$(list_engine_source "$both/$name/libexec/hgraph-debug/hgraph-sdk/lib/libhgraph_runtime.$suffix" "$mapped")
printf '%s\n' "$listed"
grep -qF 'return ops().global_state_view_impl(' <<<"$listed" ||
    fail "the debug archive and launch source map did not give engine source text"
"${build_tool[@]}" run-described "$scratch/both.json"
"${build_tool[@]}" launch-command "$scratch/both.json" >"$scratch/both.command"
stopped=$(stop_in_engine "$scratch/both.command")
printf '%s\n' "$stopped"
grep -Eq '(engine frame: hgraph::NodeView::global_state at .*/node\.cpp:[0-9]+|at node\.cpp:[0-9]+)' <<<"$stopped" ||
    fail "the debug build's engine frame has no line"
loaded "$stopped" "$both/$name/libexec/hgraph-debug/hgraph-sdk/lib" || fail "the debug build loaded another engine"
if [[ $platform == linux-x86_64 ]]; then
    grep -Eq '^engine loop locals: [0-9]+$' <<<"$stopped" || fail "the debug build's engine loop has no locals"
else
    grep -q 'this = ' <<<"$stopped" || fail "the debug build's engine frame has no arguments"
fi

# The release's optimised library, through its symbol file.
looked=$(lookup_symbol "$both/$name/libexec/hgraph-sdk/lib/libhgraph_runtime.$suffix" hgraph::NodeView::global_state)
printf '%s\n' "$looked"
grep -q 'node\.cpp' <<<"$looked" || fail "the release's library found no line of the engine with hgl-debug"
looked=$(lookup_symbol "$both/$name/libexec/hgraph-deps/lib/$fmt_library" "$fmt_function")
printf '%s\n' "$looked"
grep -Eq '^Line [1-9][0-9]* of "/hgl-release/fmt/[^\"]+"|^[[:space:]]*LineEntry: .* /hgl-release/fmt/[^:]+:[1-9][0-9]*' <<<"$looked" ||
    fail "the release's library found no line of fmt with hgl-debug"
if [[ $platform == linux-x86_64 ]]; then
    # An ordinary build, and its executable stopped in the engine with a core
    # file written, then the core file read.
    "$both/$name/bin/hgl" run "${program[@]}" --keep "$scratch/ordinary" >/dev/null
    executable=$scratch/ordinary/run/app-demo-apps-backtest
    printf '%s\n' "$executable" "$scratch/ordinary/results.txt" --debug-console >"$scratch/ordinary.command"
    stopped=$(stop_in_engine "$scratch/ordinary.command" "$scratch/ordinary.core")
    printf '%s\n' "$stopped"
    grep -Eq 'engine frame: hgraph::NodeView::global_state at .*/node\.cpp:[0-9]+' <<<"$stopped" ||
        fail "the ordinary build's engine frame has no line with hgl-debug"
    # An ordinary build never uses the debug libraries, as it runs too.
    loaded "$stopped" "$both/$name/libexec/hgraph-sdk/lib" || fail "the ordinary build loaded another engine"
    frame=$(core_frame "$executable" "$scratch/ordinary.core")
    printf '%s\n' "$frame"
    grep -Eq 'hgraph::NodeView::global_state .* at .*/node\.cpp:[0-9]+' <<<"$frame" ||
        fail "the core file's engine frame has no line with hgl-debug"
fi
echo "Checked $name and $debug_name"
