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
jobs=${HGL_BUILD_JOBS:-$(getconf _NPROCESSORS_ONLN)}
mkdir -p -- "$bundle/bin" "$bundle/licenses"
native_cache=${HGL_RELEASE_NATIVE_CACHE:-}
if [[ -n $native_cache && $native_cache != /* ]]; then
    native_cache=$root/$native_cache
fi

build_dependency() {
    local name=$1 repository=$2 tag=$3
    shift 3
    git -c advice.detachedHead=false clone --quiet --depth 1 --branch "$tag" "$repository" "$work/$name"
    cmake -S "$work/$name" -B "$work/$name-build" -G Ninja \
        -DCMAKE_BUILD_TYPE=Release -DCMAKE_INSTALL_PREFIX="$deps" -DCMAKE_INSTALL_LIBDIR=lib \
        -DCMAKE_PREFIX_PATH="$deps" ${install_name[@]+"${install_name[@]}"} \
        -DCMAKE_INSTALL_RPATH="$library_origin" -DBUILD_SHARED_LIBS=ON -DBUILD_TESTING=OFF "$@"
    cmake --build "$work/$name-build" --parallel "$jobs"
    cmake --install "$work/$name-build"
    mkdir -p -- "$bundle/licenses/$name"
    for notice in "$work/$name"/LICENSE*; do
        [[ -f $notice ]] || fail "$name has no license"
        cp -- "$notice" "$bundle/licenses/$name/"
    done
}

build_native() (
    export CXXFLAGS="${CXXFLAGS:+$CXXFLAGS }-ffile-prefix-map=$work=/hgl-release"
    # Build the pinned libraries against the system's C++ runtime.
    build_dependency fmt https://github.com/fmtlib/fmt.git "$FMT_VERSION" -DFMT_TEST=OFF -DFMT_DOC=OFF
    build_dependency spdlog https://github.com/gabime/spdlog.git "v$SPDLOG_VERSION" \
        -DSPDLOG_FMT_EXTERNAL=ON -DSPDLOG_BUILD_SHARED=ON -DSPDLOG_BUILD_TESTS=OFF
    build_dependency simdjson https://github.com/simdjson/simdjson.git "v$SIMDJSON_VERSION" \
        -DSIMDJSON_DEVELOPER_MODE=OFF
    # Keep only headers, shared libraries and CMake packages from the dependencies.
    rm -rf -- "$deps/lib/pkgconfig" "$deps/share"
    TMPDIR=$work HGL_HGRAPH_DEPS=$deps HGL_HGRAPH_INSTALL_RPATH="$library_origin/../../hgraph-deps/lib" \
        HGL_BUILD_JOBS=$jobs scripts/provision-hgraph-sdk.sh "$hgraph_tag" "$sdk"
    sed 's|^dependencies=.*|dependencies=../hgraph-deps|' "$sdk/hgl-sdk.txt" >"$work/sdk-stamp"
    mv -- "$work/sdk-stamp" "$sdk/hgl-sdk.txt"
)

if [[ -n $native_cache && -d $native_cache/libexec ]]; then
    python3 scripts/release-bundle.py copy-native "$native_cache" "$bundle"
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
python3 scripts/release-bundle.py licenses --target "$target" "$bundle/licenses"
audit=(--forbid "$root" --forbid "$work" --forbid "$HOME" --forbid "$cargo_target")
if [[ -n $native_cache ]]; then
    audit+=(--forbid "$native_cache")
fi
python3 scripts/release-bundle.py verify --target "$target" "$bundle" "${audit[@]}"
if [[ -n $native_cache && ! -d $native_cache/libexec ]]; then
    python3 scripts/release-bundle.py save-native "$bundle" "$native_cache"
fi

mkdir -p -- "$destination"
destination=$(cd -- "$destination" && pwd -P)
COPYFILE_DISABLE=1 tar -czf "$work/$name.tar.gz" -C "$work" "$name"
mv -- "$work/$name.tar.gz" "$destination/"
(
    cd -- "$destination"
    "${checksum[@]}" "$name.tar.gz" >"$name.tar.gz.sha256"
    printf 'Archive: %s\n' "$destination/$name.tar.gz"
    cat "$name.tar.gz.sha256"
)
