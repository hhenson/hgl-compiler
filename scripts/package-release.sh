#!/usr/bin/env bash
# Build a relocatable release on a supported target. See docs/releases.md.
set -euo pipefail

fail() {
    echo "package-release: $*" >&2
    exit 1
}

[[ $# -le 2 ]] || fail "usage: $0 [output-directory [version-tag]]"
case "$(uname -s)-$(uname -m)" in
    Darwin-arm64)
        platform=macos-arm64
        target=aarch64-apple-darwin
        export CXX=${CXX:-/usr/bin/clang++}
        export MACOSX_DEPLOYMENT_TARGET=26.0
        library_origin=@loader_path
        install_name=(-DCMAKE_INSTALL_NAME_DIR=@rpath)
        checksum=(shasum -a 256)
        ;;
    Linux-x86_64)
        grep -qx 'ID=ubuntu' /etc/os-release && grep -qx 'VERSION_ID="26.04"' /etc/os-release ||
            fail "build the Linux release on Ubuntu 26.04"
        [[ $(getconf GNU_LIBC_VERSION) == 'glibc 2.43' ]] || fail "requires glibc 2.43"
        platform=linux-x86_64
        target=x86_64-unknown-linux-gnu
        export CC=${CC:-gcc-15} CXX=${CXX:-g++-15}
        library_origin='$ORIGIN'
        install_name=()
        checksum=(sha256sum)
        ;;
    *) fail "requires Apple silicon macOS or x86-64 Ubuntu 26.04" ;;
esac
root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)
destination=${1:-$root/target/release-bundles}
version_tag=${2:-}
cd -- "$root"
# The submodules are judged after they are updated, below: a checkout kept
# from an earlier build holds them at the commits that build pinned, which
# is no change of this tree's.
[[ -z $(git status --porcelain --ignore-submodules=all) ]] || fail "requires a clean checkout"

version=$(python3 scripts/release-bundle.py version)
[[ -z $version_tag || $version_tag == "$version" ]] ||
    fail "tag $version_tag does not match workspace version $version"
name=hgl-$version-$platform
read -r hgraph_tag < HGRAPH_TAG
source scripts/hgraph-dependencies.sh
source scripts/gather-dsyms.sh

# All build paths are disposable, including the compiler probe.
work=$(mktemp -d "${TMPDIR:-/tmp}/hgl-release.XXXXXX")
work=$(cd -- "$work" && pwd -P)
trap 'rm -rf -- "$work"' EXIT

probe_toolchain() {
    local compiler_path compiler
    compiler_path=$(command -v "$CXX") || compiler_path=$CXX
    compiler=$("$CXX" --version 2>/dev/null) || compiler=
    cat >"$work/toolchain-probe.cpp" <<'CPP'
#include <algorithm>
#include <type_traits>
int main() { static_assert(std::is_same_v<decltype(std::max(1, 2)), const int&>); return std::max(1, 2) != 2; }
CPP
    if [[ $platform == linux-x86_64 ]]; then
        local version defines
        version=$("$CXX" -dumpversion 2>/dev/null) || version=
        defines=$("$CXX" -dM -E -x c++ /dev/null 2>/dev/null) || defines=
        [[ ${version%%.*} == 15 && $defines == *__GNUC__* && $defines != *__clang__* ]] &&
            "$CXX" -std=c++23 "$work/toolchain-probe.cpp" -o "$work/toolchain-probe" &&
            "$work/toolchain-probe" || fail "requires the Ubuntu 26.04 GCC 15 toolchain"
        return
    fi
    if [[ $compiler =~ Apple[[:space:]]clang[[:space:]]version[[:space:]]([0-9]+) ]] &&
        [[ ${BASH_REMATCH[1]} -ge 21 ]] &&
        "$CXX" -std=c++23 -arch arm64 "-mmacosx-version-min=$MACOSX_DEPLOYMENT_TARGET" \
            "$work/toolchain-probe.cpp" -o "$work/toolchain-probe" >/dev/null 2>&1 &&
        "$work/toolchain-probe" >/dev/null 2>&1; then
        printf '%s\n' "$compiler" >&2
        return 0
    fi
    compiler=${compiler%%$'\n'*}
    fail "C++23 toolchain check failed with $compiler_path (${compiler:-version unavailable}).
Apple clang 21 or later and usable C++ standard library headers are required.
DEVELOPER_DIR=${DEVELOPER_DIR-<unset>} SDKROOT=${SDKROOT-<unset>} CC=${CC-<unset>} CXX=$CXX
These settings must name one macOS toolchain."
}

probe_toolchain
git submodule update --init --recursive
[[ -z $(git status --porcelain) ]] || fail "requires clean submodules"

# The SDK and libraries use load paths relative to their installed location.
bundle=$work/$name
deps=$bundle/libexec/hgraph-deps
sdk=$bundle/libexec/hgraph-sdk
# The hgl-debug archive's tree, which unpacks where the release does: hgraph's
# libraries built for debugging in libexec/hgraph-debug, laid out as libexec
# is, and the symbol files of the release's own libraries beside them.
debug_name=hgl-debug-$version-$platform
debug_tree=$work/debug/$name
debug=$debug_tree/libexec/hgraph-debug
jobs=${HGL_BUILD_JOBS:-$(getconf _NPROCESSORS_ONLN)}
mkdir -p -- "$bundle/bin" "$bundle/licenses" "$debug/licenses"
native_cache=${HGL_RELEASE_NATIVE_CACHE:-}
if [[ -n $native_cache && $native_cache != /* ]]; then
    native_cache=$root/$native_cache
fi

clone() {
    git -c advice.detachedHead=false clone --quiet --depth 1 --branch "$3" "$2" "$work/$1"
}

# Builds dependency NAME, cloned in $work/NAME, as BUILD (CMake's build type)
# into PREFIX, with its licence in LICENSES/NAME.
build_dependency() {
    local name=$1 build=$2 prefix=$3 licenses=$4
    shift 4
    cmake -S "$work/$name" -B "$work/$name-$build" -G Ninja \
        -DCMAKE_BUILD_TYPE="$build" -DCMAKE_INSTALL_PREFIX="$prefix" -DCMAKE_INSTALL_LIBDIR=lib \
        -DCMAKE_PREFIX_PATH="$prefix" ${install_name[@]+"${install_name[@]}"} \
        -DCMAKE_INSTALL_RPATH="$library_origin" -DBUILD_SHARED_LIBS=ON -DBUILD_TESTING=OFF "$@"
    cmake --build "$work/$name-$build" --parallel "$jobs"
    cmake --install "$work/$name-$build"
    [[ $platform == linux-x86_64 ]] || gather_dsyms "$prefix/lib"
    mkdir -p -- "$licenses/$name"
    for notice in "$work/$name"/LICENSE*; do
        [[ -f $notice ]] || fail "$name has no license"
        cp -- "$notice" "$licenses/$name/"
    done
}

# Builds the three dependencies and hgraph's SDK as BUILD into LIBEXEC's
# hgraph-deps and hgraph-sdk, with the dependencies' licences in LICENSES.
build_libraries() {
    local build=$1 libexec=$2 licenses=$3
    local prefix=$libexec/hgraph-deps
    build_dependency fmt "$build" "$prefix" "$licenses" -DFMT_TEST=OFF -DFMT_DOC=OFF
    build_dependency spdlog "$build" "$prefix" "$licenses" \
        -DSPDLOG_FMT_EXTERNAL=ON -DSPDLOG_BUILD_SHARED=ON -DSPDLOG_BUILD_TESTS=OFF
    build_dependency simdjson "$build" "$prefix" "$licenses" -DSIMDJSON_DEVELOPER_MODE=OFF
    # Keep only headers, shared libraries and CMake packages from the dependencies.
    rm -rf -- "$prefix/lib/pkgconfig" "$prefix/share"
    TMPDIR=$work HGL_HGRAPH_WORK_DIR=$work/hgraph-build HGL_HGRAPH_DEPS=$prefix \
        HGL_HGRAPH_SOURCE=$work/hgraph HGL_HGRAPH_BUILD_TYPE=$build \
        HGL_HGRAPH_INSTALL_RPATH="$library_origin/../../hgraph-deps/lib" HGL_BUILD_JOBS=$jobs \
        scripts/provision-hgraph-sdk.sh "$hgraph_tag" "$libexec/hgraph-sdk"
    sed 's|^dependencies=.*|dependencies=../hgraph-deps|' "$libexec/hgraph-sdk/hgl-sdk.txt" >"$work/sdk-stamp"
    mv -- "$work/sdk-stamp" "$libexec/hgraph-sdk/hgl-sdk.txt"
}

# Moves the debug information of each shared library in LIBRARIES out into a
# symbol file in SYMBOLS, the same directory of the debug tree, matched to the
# library by its build ID: on Linux LIB.debug in .debug, its debug sections
# compressed, which the library's debug link names and a debugger looks for
# beside it; on macOS the dSYM bundle gather_dsyms made beside it. A Linux
# library keeps all else, its symbol table whole (source file names too).
split_symbols() {
    local libraries=$1 symbols=$2 library file
    mkdir -p -- "$symbols"
    if [[ $platform == macos-arm64 ]]; then
        for file in "$libraries"/*.dSYM; do
            if [[ -d $file ]]; then
                mv -- "$file" "$symbols/"
            fi
        done
        return 0
    fi
    mkdir -p -- "$symbols/.debug"
    for library in "$libraries"/*.so*; do
        [[ -f $library && ! -L $library ]] || continue
        file=$symbols/.debug/${library##*/}.debug
        objcopy --only-keep-debug --compress-debug-sections=zlib "$library" "$file"
        objcopy --strip-debug --keep-file-symbols --add-gnu-debuglink="$file" "$library"
    done
}

# Compresses the debug sections of each shared library in the directories
# given, on Linux: a debugger reads them as they are, in less room.
compress_debug() {
    [[ $platform == linux-x86_64 ]] || return 0
    local directory library
    for directory in "$@"; do
        for library in "$directory"/*.so*; do
            if [[ -f $library && ! -L $library ]]; then
                objcopy --compress-debug-sections=zlib "$library"
            fi
        done
    done
}

build_native() (
    export CXXFLAGS="${CXXFLAGS:+$CXXFLAGS }-ffile-prefix-map=$work=/hgl-release"
    clone fmt https://github.com/fmtlib/fmt.git "$FMT_VERSION"
    clone spdlog https://github.com/gabime/spdlog.git "v$SPDLOG_VERSION"
    clone simdjson https://github.com/simdjson/simdjson.git "v$SIMDJSON_VERSION"
    clone hgraph https://github.com/hhenson/hgraph.git "$hgraph_tag"
    # Build the pinned libraries against the system's C++ runtime: the
    # release's optimised, with debug information that is then split out of
    # them, and hgl-debug's for debugging.
    (
        export CXXFLAGS="$CXXFLAGS -g"
        build_libraries Release "$bundle/libexec" "$bundle/licenses"
    )
    split_symbols "$sdk/lib" "$debug_tree/libexec/hgraph-sdk/lib"
    split_symbols "$deps/lib" "$debug_tree/libexec/hgraph-deps/lib"
    build_libraries Debug "$debug" "$debug/licenses"
    compress_debug "$debug/hgraph-sdk/lib" "$debug/hgraph-deps/lib"
)

if [[ -n $native_cache && -d $native_cache/libexec && -d $native_cache/debug/libexec ]]; then
    python3 scripts/release-bundle.py copy-native "$native_cache" "$bundle" --debug "$debug_tree"
    grep -qxF "tag=$hgraph_tag" "$sdk/hgl-sdk.txt" &&
        grep -qxF "arrow=off" "$sdk/hgl-sdk.txt" ||
        fail "the cached SDK must be hgraph $hgraph_tag built without Arrow"
    echo "Using the cached hgraph SDK and dependency prefix"
else
    build_native
fi

# Runtime panic locations may carry source paths. Remap workspace and
# registry paths as well as stripping debug symbols from the release.
cargo_home=${CARGO_HOME:-$HOME/.cargo}
cargo_target=${CARGO_TARGET_DIR:-$work/cargo-target}
mkdir -p -- "$cargo_target"
cargo_target=$(cd -- "$cargo_target" && pwd -P)
export CARGO_ENCODED_RUSTFLAGS
CARGO_ENCODED_RUSTFLAGS=$(printf '%s\037%s\037%s' \
    "--remap-path-prefix=$root=." "--remap-path-prefix=$cargo_home=/cargo" \
    "--remap-path-prefix=$cargo_target=/cargo-target")
export CARGO_PROFILE_RELEASE_STRIP=symbols
cargo build --locked --release --package hgl_cli --target "$target" \
    --target-dir "$cargo_target" --jobs "$jobs"
cp -- "$cargo_target/$target/release/hgl" "$bundle/bin/hgl"
cp -- LICENSE "$bundle/licenses/hgl-compiler.txt"
cp -- spec/hgl_core/LICENSE "$bundle/licenses/hgl-core.txt"
cp -- crates/hgl_doc/vendor/katex/LICENSE "$bundle/licenses/katex.txt"
cp -- crates/hgl_doc/vendor/mermaid/LICENSE "$bundle/licenses/mermaid.txt"
cp -- crates/hgl_doc/vendor/mermaid/THIRD-PARTY-NOTICES.txt "$bundle/licenses/mermaid-third-party.txt"
python3 scripts/release-bundle.py licenses --target "$target" "$bundle/licenses"
audit=(--forbid "$root" --forbid "$work" --forbid "$HOME" --forbid "$cargo_target")
if [[ -n $native_cache ]]; then
    audit+=(--forbid "$native_cache")
fi
python3 scripts/release-bundle.py verify --target "$target" "$bundle" "${audit[@]}"
python3 scripts/release-bundle.py verify-debug --target "$target" "$debug_tree" --release "$bundle" \
    "${audit[@]}"
if [[ -n $native_cache && ! -d $native_cache/libexec ]]; then
    python3 scripts/release-bundle.py save-native "$bundle" "$native_cache" --debug "$debug_tree"
fi

mkdir -p -- "$destination"
destination=$(cd -- "$destination" && pwd -P)
# The debug archive's top directory is the release's, so that the two unpack
# into one tree.
COPYFILE_DISABLE=1 tar -czf "$work/$name.tar.gz" -C "$work" "$name"
COPYFILE_DISABLE=1 tar -czf "$work/$debug_name.tar.gz" -C "$work/debug" "$name"
mv -- "$work/$name.tar.gz" "$work/$debug_name.tar.gz" "$destination/"
(
    cd -- "$destination"
    for archive in "$name" "$debug_name"; do
        "${checksum[@]}" "$archive.tar.gz" >"$archive.tar.gz.sha256"
        printf 'Archive: %s\n' "$destination/$archive.tar.gz"
        cat "$archive.tar.gz.sha256"
    done
)
