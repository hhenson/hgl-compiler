#!/usr/bin/env bash
# A CMake linker launcher for the SDK's build on macOS, which the
# provisioning gives as CMAKE_CXX_LINKER_LAUNCHER. hgraph links its shared
# libraries with link-time optimisation in a Release build (ThinLTO with Apple
# clang): ld64 compiles a link's objects into temporary files and removes
# them, and with them the debug information that dsymutil gathers into a dSYM
# (gather-dsyms.sh). Told -object_path_lto, it keeps them: here in a
# directory beside the link's output, OUTPUT.lto, its own for each link, so
# that two libraries' objects, which ld64 numbers alike, never meet. The build
# tree that holds them goes when the provisioning ends, after gather_dsyms.
#
# Usage, as CMake runs a launcher: keep-lto-objects.sh LINKER ARGUMENTS...
set -eu

output=
previous=
for argument in "$@"; do
    if [[ $previous == -o ]]; then
        output=$argument
    fi
    previous=$argument
done
if [[ -z $output ]]; then
    exec "$@"
fi
# ld64 records the path as given, and dsymutil reads it from wherever it runs.
case $output in
    /*) ;;
    *) output=$PWD/$output ;;
esac
mkdir -p -- "$output.lto"
exec "$@" "-Wl,-object_path_lto,$output.lto"
