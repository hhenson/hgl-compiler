# Sourced by the SDK's provisioning and the release's packaging, on macOS.
#
# gather_dsyms DIR: on macOS a library's debug information stays in the
# objects it was linked from, which go with the build tree. For each shared
# library in DIR linked from objects with debug information, it is gathered
# into a dSYM bundle beside the library, named as the loader names the library
# (its install name's last part), and the library keeps the names of its
# functions and its UUID, which the dSYM is matched by. A library changed so
# is signed again, ad hoc, where its signature no longer holds.
gather_dsyms() {
    local library name
    for library in "$1"/*.dylib; do
        [[ -f $library && ! -L $library ]] || continue
        nm -ap "$library" | grep -q ' OSO ' || continue
        name=$(otool -D "$library" | tail -n 1)
        name=${name##*/}
        [[ -e $1/$name ]] || name=${library##*/}
        dsymutil "$1/$name" -o "$1/$name.dSYM"
        strip -S "$library"
        codesign --verify "$library" 2>/dev/null || codesign --force --sign - "$library"
    done
}
