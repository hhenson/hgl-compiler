#!/usr/bin/env bash
# Builds hgraph's shared C++ SDK at a release tag and installs it into a
# directory: the SDK that runtime/cpp (libhglrt) and the C++ target's
# generated programs are built against (docs/architecture.md, "The C++
# runtime library"). It is built without Arrow: hgraph's graph, node, type
# and runtime layers, its runtime and wiring libraries, and nothing of its
# standard library, which is all the C++ target takes from hgraph
# (docs/cpp-target.md). It runs on Linux and on macOS, under bash 3.2 or
# later (macOS's own), and builds with the platform's compiler: GCC 15 or
# later on Linux, Apple clang 21 or later on macOS.
#
# Usage: scripts/provision-hgraph-sdk.sh TAG DIR
#
#   TAG  an hgraph release tag, 0.9.12 or later (the first that builds
#        without Arrow)
#   DIR  where the SDK is installed: the value of HGL_HGRAPH_SDK for what
#        is built against it. It must not exist yet, or be empty, or hold
#        an SDK this script installed for TAG against the same dependency
#        prefix, which is then kept as it is.
#
# Environment:
#
#   HGL_HGRAPH_DEPS    (required) the dependency prefix: fmt, spdlog and
#                      simdjson, with CMake and Ninja. Where it does not
#                      exist it is made, with micromamba, from the
#                      conda-forge packages DEPENDENCIES lists. A prefix that
#                      holds more, Arrow among it, does: nothing more is used.
#   HGL_HGRAPH_SOURCE  a checkout of hgraph at TAG to build. Without it, TAG
#                      is cloned from GitHub into a temporary directory.
#   CXX                the C++ compiler (default: g++ on Linux, clang++ on
#                      macOS).
#   HGL_HGRAPH_INSTALL_RPATH  where SDK libraries look for dependencies.
#                      Defaults to HGL_HGRAPH_DEPS/lib.
#   HGL_BUILD_JOBS     how many jobs the build runs at once (default: every
#                      processor).
#   MAMBA_EXE          micromamba, where it is not `micromamba` on the path.
#                      Needed only to make the dependency prefix.
#
# The SDK is configured as hgraph's own CI builds its runtime without Arrow
# (the step "Build and test the native runtime without Arrow" of
# `.github/workflows/native-cpp.yml` at the tag): a Release build of shared
# libraries with `HGRAPH_WITH_ARROW=OFF`, which leaves out Frame, Series,
# the table codecs, the standard library, Python and the extensions, and
# with Arrow's packages kept from being found at all. The date/tz library
# is always hgraph's own pinned release, fetched and built into its runtime
# library, and never a package of the prefix: what the SDK is does not turn
# on what else a prefix holds. hgraph's tests and warnings are hgraph's, so
# they are left out. The SDK's libraries are given the prefix's library
# directory as their own run path (CMAKE_INSTALL_RPATH, which hgraph puts
# before its `$ORIGIN`, `@loader_path` on macOS), so that a program built
# against the SDK finds fmt and spdlog with no setting of its own.
#
# On macOS conda-forge's fmt, spdlog and simdjson link conda-forge's own
# libc++, which the prefix then holds, where the SDK and what is built
# against it link the system's. That libc++ re-exports the system's
# libc++abi, so a process has one runtime of exceptions and type
# information, and an exception thrown in one library is caught in another.
#
# DIR/hgl-sdk.txt says what was built: the tag, its commit, how it was
# configured (`arrow=off`), the compiler and the dependency prefix. It is
# written last, so that a DIR without it holds no whole SDK, and an SDK of
# the tag built otherwise, with Arrow, is not taken for this one.

set -euo pipefail

source "$(dirname -- "${BASH_SOURCE[0]}")/hgraph-dependencies.sh"

# The dependency prefix made where there is none: what hgraph's build
# without Arrow finds as packages (fmt 11 or later, spdlog 1.15 or later,
# simdjson 4.5 or later), and CMake and Ninja to build it, each at the
# version the SDK was first built with. The date/tz library is not among
# them: hgraph fetches its own.
DEPENDENCIES=(
    "fmt=$FMT_VERSION"
    "spdlog=$SPDLOG_VERSION"
    "simdjson=$SIMDJSON_VERSION"
    "cmake=4.2.3"
    "ninja=1.13.2"
)

# How the SDK is configured, as hgl-sdk.txt says it.
CONFIGURATION="arrow=off"

# The oldest compiler the SDK is built with. On Linux, GCC 15: hgraph's
# headers need C++23, and the SDK's library needs GCC 15's libstdc++
# (GLIBCXX_3.4.34) to link. On macOS, Apple clang 21, the oldest it has been
# built and tested with.
MINIMUM_GCC=15
MINIMUM_APPLE_CLANG=21

# hgraph's repository, where TAG is cloned from.
HGRAPH_REPOSITORY=https://github.com/hhenson/hgraph.git

fail() {
    echo "provision-hgraph-sdk: $*" >&2
    exit 1
}

say() {
    echo "provision-hgraph-sdk: $*"
}

# `$1` as an absolute path, as CMake's prefixes must be.
absolute() {
    case $1 in
        /*) printf '%s\n' "$1" ;;
        *) printf '%s/%s\n' "$PWD" "$1" ;;
    esac
}

# A tool of the dependency prefix `$1` where it has one, and otherwise the
# one on the path.
tool() {
    if [[ -x $1/bin/$2 ]]; then
        printf '%s\n' "$1/bin/$2"
    else
        command -v "$2" || fail "$2 is needed and was not found"
    fi
}

# The whole script is this function, which bash reads before it runs any of
# it: a change to the file while it runs does not change what it does.
main() {
    [[ $# -eq 2 && -n $1 && -n $2 ]] || fail "usage: $0 TAG DIR"
    [[ -n ${HGL_HGRAPH_DEPS:-} ]] || fail "HGL_HGRAPH_DEPS must name the dependency prefix"
    local platform
    platform=$(uname -s)
    case $platform in
        Linux | Darwin) ;;
        *) fail "the SDK is built on Linux and on macOS, not on $platform" ;;
    esac
    local tag=$1
    local dir deps
    dir=$(absolute "$2")
    deps=$(absolute "$HGL_HGRAPH_DEPS")
    local stamp=$dir/hgl-sdk.txt

    # An SDK this script installed for the tag, configured as it configures
    # one, is kept; anything else in the way is not replaced.
    local rpath_stamp="${HGL_HGRAPH_INSTALL_RPATH:+install-rpath=$HGL_HGRAPH_INSTALL_RPATH}"
    if [[ -f $stamp ]] && grep -qx "tag=$tag" "$stamp" && grep -qxF "$CONFIGURATION" "$stamp" &&
        grep -qxF "dependencies=$deps" "$stamp" &&
        [[ $(sed -n '/^install-rpath=/p' "$stamp") == "$rpath_stamp" ]]; then
        say "$dir already holds hgraph $tag's SDK"
        return 0
    fi
    if [[ -e $dir && -n $(ls -A -- "$dir") ]]; then
        fail "$dir holds no whole SDK of hgraph $tag; remove it or name another directory"
    fi

    work=$(mktemp -d)
    trap 'rm -rf -- "$work"' EXIT

    # The dependency prefix, made where it is not there.
    if [[ ! -d $deps ]]; then
        local mamba=${MAMBA_EXE:-micromamba}
        command -v "$mamba" >/dev/null || fail "$deps does not exist, and micromamba is needed to make it"
        say "making the dependency prefix $deps"
        mkdir -p -- "$(dirname -- "$deps")"
        # A package cache and a home directory of its own: micromamba
        # writes caches and a list of environments under the home
        # directory too. So nothing outside the prefix and the temporary
        # directory is written.
        mkdir -p -- "$work/home"
        HOME="$work/home" "$mamba" create --yes --quiet --prefix "$deps" --override-channels \
            --channel conda-forge --root-prefix "$work/mamba" "${DEPENDENCIES[@]}"
    fi
    local cmake ninja
    cmake=$(tool "$deps" cmake)
    ninja=$(tool "$deps" ninja)

    # The platform's compiler. Clang defines __GNUC__ too, and answers
    # -dumpversion with its own version; GCC's whole version is
    # -dumpfullversion's.
    local cxx defines version compiler default_cxx=g++
    [[ $platform == Darwin ]] && default_cxx=clang++
    cxx=$(command -v "${CXX:-$default_cxx}") || fail "the C++ compiler ${CXX:-$default_cxx} was not found"
    defines=$("$cxx" -dM -E -x c++ /dev/null 2>/dev/null) || fail "$cxx does not run as a C++ compiler"
    if [[ $platform == Darwin ]]; then
        [[ $defines == *__apple_build_version__* ]] || fail "$cxx is not Apple clang"
        version=$("$cxx" -dumpversion)
        [[ ${version%%.*} -ge $MINIMUM_APPLE_CLANG ]] ||
            fail "$cxx is Apple clang $version; Apple clang $MINIMUM_APPLE_CLANG or later is needed"
        compiler="Apple clang $version"
    else
        [[ $defines == *__GNUC__* && $defines != *__clang__* ]] || fail "$cxx is not GCC"
        version=$("$cxx" -dumpfullversion -dumpversion)
        [[ ${version%%.*} -ge $MINIMUM_GCC ]] || fail "$cxx is GCC $version; GCC $MINIMUM_GCC or later is needed"
        compiler="GCC $version"
    fi

    # The source at the tag.
    local source
    if [[ -n ${HGL_HGRAPH_SOURCE:-} ]]; then
        source=$(absolute "$HGL_HGRAPH_SOURCE")
    else
        source=$work/hgraph
        say "cloning hgraph $tag"
        git clone --quiet --depth 1 --branch "$tag" "$HGRAPH_REPOSITORY" "$source"
    fi
    local commit tagged
    commit=$(git -C "$source" rev-parse HEAD)
    tagged=$(git -C "$source" rev-parse "refs/tags/$tag^{commit}") || fail "$source has no tag $tag"
    [[ $commit == "$tagged" ]] || fail "$source is at $commit, not at the tag $tag ($tagged)"
    # A release with no such option builds Arrow in whatever it is told.
    grep -q 'option(HGRAPH_WITH_ARROW' "$source/CMakeLists.txt" ||
        fail "hgraph $tag cannot be built without Arrow: 0.9.12 or later is needed"

    # Every processor: `getconf` counts them on Linux and on macOS alike.
    local jobs=${HGL_BUILD_JOBS:-$(getconf _NPROCESSORS_ONLN)}
    say "building hgraph $tag ($commit) with $compiler, $jobs jobs"
    local started=$SECONDS
    "$cmake" -S "$source" -B "$work/build" -G Ninja \
        -DCMAKE_MAKE_PROGRAM="$ninja" \
        -DCMAKE_CXX_COMPILER="$cxx" \
        -DCMAKE_BUILD_TYPE=Release \
        -DCMAKE_PREFIX_PATH="$deps" \
        -DCMAKE_INSTALL_PREFIX="$dir" \
        -DCMAKE_INSTALL_RPATH="${HGL_HGRAPH_INSTALL_RPATH:-$deps/lib}" \
        -DBUILD_TESTING=OFF \
        -DHGRAPH_BUILD_SHARED=ON \
        -DHGRAPH_WITH_ARROW=OFF \
        -DCMAKE_DISABLE_FIND_PACKAGE_Arrow=ON \
        -DCMAKE_DISABLE_FIND_PACKAGE_ArrowCompute=ON \
        -DCMAKE_DISABLE_FIND_PACKAGE_ArrowAcero=ON \
        -DHGRAPH_BUILD_PYTHON_BINDINGS=OFF \
        -DHGRAPH_ENABLE_PYTHON_USER_NODES=OFF \
        -DHGRAPH_ENABLE_IDE_PYTHON_HEADER_HINTS=OFF \
        -DHGRAPH_FETCH_DATE=ON \
        -DCMAKE_DISABLE_FIND_PACKAGE_date=ON
    "$cmake" --build "$work/build" --parallel "$jobs"
    "$cmake" --install "$work/build"

    # date/tz can be built into the shared runtime and is not installed
    # separately. Keep its notice alongside hgraph's if it was fetched.
    local licenses=$dir/share/licenses/hgraph
    mkdir -p -- "$licenses"
    cp -- "$source/LICENSE" "$licenses/LICENSE"
    if [[ -f $work/build/_deps/date-src/LICENSE.txt ]]; then
        cp -- "$work/build/_deps/date-src/LICENSE.txt" "$licenses/date-LICENSE.txt"
    fi

    printf 'tag=%s\ncommit=%s\n%s\ncompiler=%s\ndependencies=%s\n' "$tag" "$commit" "$CONFIGURATION" "$compiler" \
        "$deps" >"$stamp"
    if [[ -n ${HGL_HGRAPH_INSTALL_RPATH:-} ]]; then
        printf 'install-rpath=%s
' "$HGL_HGRAPH_INSTALL_RPATH" >>"$stamp"
    fi
    say "installed hgraph $tag's SDK into $dir in $((SECONDS - started)) s"
}

main "$@"
