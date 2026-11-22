#!/usr/bin/env bash
# Run SDK cases concurrently while dividing a budget of Ninja jobs.
# The SDK's environment is the same one that hgl test reads.
set -euo pipefail

if (( $# < 2 )) || [[ ! $1 =~ ^[1-9][0-9]*$ || ! $2 =~ ^[1-9][0-9]*$ ]] || (( $2 > $1 )); then
    echo "usage: check-cpp-sdk.sh JOBS CASES [cargo options]" >&2
    echo "JOBS and CASES must be positive, and CASES must fit within JOBS." >&2
    exit 2
fi
jobs=$1
cases=$2
shift 2
export CMAKE_BUILD_PARALLEL_LEVEL=$((jobs / cases))
exec cargo test --locked -p hgl_cli --test cpp "$@" -- --ignored --test-threads="$cases"
