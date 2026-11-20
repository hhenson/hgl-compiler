#!/usr/bin/env bash
# Builds hgraph's shared C++ SDK at a release tag and installs it into a
# directory: the SDK that runtime/cpp (libhglrt) and the C++ target's
# generated programs are built against (docs/architecture.md, "The C++
# runtime library").
#
# Usage: scripts/provision-hgraph-sdk.sh TAG DIR
#
#   TAG  an hgraph release tag, such as 0.9.8
#   DIR  where the SDK is installed: the value of HGL_HGRAPH_SDK for what
#        is built against it. It must not exist yet, or be empty, or hold
#        an SDK this script installed for TAG against the same dependency
#        prefix, which is then kept as it is.
#
# Environment:
#
#   HGL_HGRAPH_DEPS    (required) the dependency prefix: Arrow with its
#                      Compute and Acero libraries, fmt, spdlog and simdjson.
#                      Where it does not exist it is made, with micromamba,
#                      from the conda-forge packages DEPENDENCIES lists.
#   HGL_HGRAPH_SOURCE  a checkout of hgraph at TAG to build. Without it, TAG
#                      is cloned from GitHub into a temporary directory.
#   CXX                the C++ compiler, GCC 15 or later (default: g++).
#   HGL_BUILD_JOBS     how many jobs the build runs at once (default: every
#                      processor).
#   MAMBA_EXE          micromamba, where it is not `micromamba` on the path.
#                      Needed only to make the dependency prefix.
#
# The SDK is configured as hgraph's own CI builds its shared install (the
# job `native-shared-install` of `.github/workflows/native-cpp.yml` at the
# tag): a Release build of shared libraries against the dependency prefix,
# with the Python bindings, Python user nodes and IDE header hints off and
# the date/tz library fetched where no package has it. That job also builds
# hgraph's tests and extensions, with warnings as errors; the SDK needs
# neither, and hgraph's warnings are hgraph's to fix, so they are left out.
# PyArrow's Arrow is not used: the dependency prefix has Arrow. Where that
# job runs its consumers with the prefix on LD_LIBRARY_PATH, the SDK's
# libraries are given the prefix's library directory as their own run path
# (CMAKE_INSTALL_RPATH, which hgraph puts before its `$ORIGIN`), so that a
# program built against the SDK finds them with no setting of its own.
#
# DIR/hgl-sdk.txt says what was built: the tag, its commit, the compiler and
# the dependency prefix. It is written last, so that a DIR without it holds
# no whole SDK.

set -euo pipefail

# The dependency prefix made where there is none: what hgraph's CI job
# installs at 0.9.8 (`libarrow=25`, `libarrow-compute=25`, `libarrow-acero=25`,
# `fmt`, `spdlog>=1.15`, `simdjson>=4.5`, `cmake>=3.26`, `ninja`), each at the
# version the SDK was first built with.
DEPENDENCIES=(
    "libarrow=25.0.0"
    "libarrow-compute=25.0.0"
    "libarrow-acero=25.0.0"
    "fmt=12.1.0"
    "spdlog=1.17.0"
    "simdjson=4.6.4"
    "cmake=4.2.3"
    "ninja=1.13.2"
)

# The oldest GCC the SDK is built with: hgraph's headers need C++23, and the
# SDK's library needs GCC 15's libstdc++ (GLIBCXX_3.4.34) to link.
MINIMUM_GCC=15

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
    local tag=$1
    local dir deps
    dir=$(absolute "$2")
    deps=$(absolute "$HGL_HGRAPH_DEPS")
    local stamp=$dir/hgl-sdk.txt

    # An SDK this script installed for the tag is kept; anything else in
    # the way is not replaced.
    if [[ -f $stamp ]] && grep -qx "tag=$tag" "$stamp" && grep -qxF "dependencies=$deps" "$stamp"; then
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
        # A package cache of its own, so that nothing outside the prefix
        # and the temporary directory is written.
        "$mamba" create --yes --quiet --prefix "$deps" --override-channels \
            --channel conda-forge --root-prefix "$work/mamba" "${DEPENDENCIES[@]}"
    fi
    local cmake ninja
    cmake=$(tool "$deps" cmake)
    ninja=$(tool "$deps" ninja)

    local cxx defines version
    cxx=$(command -v "${CXX:-g++}") || fail "the C++ compiler ${CXX:-g++} was not found"
    # Clang defines __GNUC__ too, and answers -dumpversion with its own.
    defines=$("$cxx" -dM -E -x c++ /dev/null)
    [[ $defines == *__GNUC__* && $defines != *__clang__* ]] || fail "$cxx is not GCC"
    version=$("$cxx" -dumpfullversion -dumpversion)
    [[ ${version%%.*} -ge $MINIMUM_GCC ]] || fail "$cxx is GCC $version; GCC $MINIMUM_GCC or later is needed"

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

    local jobs=${HGL_BUILD_JOBS:-$(nproc)}
    say "building hgraph $tag ($commit) with GCC $version, $jobs jobs"
    local started=$SECONDS
    "$cmake" -S "$source" -B "$work/build" -G Ninja \
        -DCMAKE_MAKE_PROGRAM="$ninja" \
        -DCMAKE_CXX_COMPILER="$cxx" \
        -DCMAKE_BUILD_TYPE=Release \
        -DCMAKE_PREFIX_PATH="$deps" \
        -DCMAKE_INSTALL_PREFIX="$dir" \
        -DCMAKE_INSTALL_RPATH="$deps/lib" \
        -DBUILD_TESTING=OFF \
        -DHGRAPH_BUILD_SHARED=ON \
        -DHGRAPH_BUILD_PYTHON_BINDINGS=OFF \
        -DHGRAPH_ENABLE_PYTHON_USER_NODES=OFF \
        -DHGRAPH_ENABLE_IDE_PYTHON_HEADER_HINTS=OFF \
        -DHGRAPH_USE_PYARROW_ARROW=OFF \
        -DHGRAPH_FETCH_DATE=ON
    "$cmake" --build "$work/build" --parallel "$jobs"
    "$cmake" --install "$work/build"

    printf 'tag=%s\ncommit=%s\ncompiler=GCC %s\ndependencies=%s\n' "$tag" "$commit" "$version" "$deps" >"$stamp"
    say "installed hgraph $tag's SDK into $dir in $((SECONDS - started)) s"
}

main "$@"
