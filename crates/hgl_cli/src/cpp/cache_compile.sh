# A compiler invocation for a generated module. Its preprocessed text is
# also the exact input on a cache miss.
set -eu

cache=$1
pch=$2
output=$3
source=$4
cxx=$5
shift 5

temporary=$(mktemp -d "$output.XXXXXXXX")
shared=
trap 'rm -rf -- "$temporary"; if [ -n "$shared" ]; then rm -f -- "$shared"; fi' EXIT

"$cxx" "$@" -Winvalid-pch -include "$pch" -E -fpch-preprocess \
    -MD -MF "$output.d" -MQ "$output" "$source" -o "$temporary/unit.ii"
input_digest=$(sha256sum < "$temporary/unit.ii")
reference="$cache/${input_digest%% *}.ref"

read_object() {
    cp -- "$reference" "$temporary/reference" &&
        {
            IFS= read -r object_digest &&
                IFS= read -r origin
        } < "$temporary/reference" &&
        cp --reflink=auto -- "$origin" "$temporary/unit.o" &&
        copied_digest=$(sha256sum < "$temporary/unit.o") &&
        [ "${copied_digest%% *}" = "$object_digest" ] &&
        cmp -s -- "$reference" "$temporary/reference"
}

if ! { [ -f "$reference" ] && read_object; } 2>/dev/null; then
    "$cxx" "$@" -x c++-cpp-output -c "$temporary/unit.ii" -o "$temporary/unit.o"
    object_digest=$(sha256sum < "$temporary/unit.o")
    object_digest=${object_digest%% *}
fi
mv -f -- "$temporary/unit.o" "$output"
if shared=$(mktemp "$cache/reference.XXXXXXXX" 2>/dev/null); then
    printf '%s\n%s/%s\n' "$object_digest" "$(pwd -P)" "$output" > "$shared" &&
        mv -f -- "$shared" "$reference" || true
fi
