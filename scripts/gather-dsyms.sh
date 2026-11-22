# Sourced by the SDK's provisioning and the release's packaging, on macOS.
#
# gather_dsyms DIR: on macOS a library's debug information stays in the
# objects it was linked from, which go with the build tree. For each shared
# library in DIR linked from objects with debug information, it is gathered
# into a dSYM bundle beside the library, named as the loader names the library
# (its install name's last part), and the library keeps the names of its
# functions and its UUID, which the dSYM is matched by. A library changed so
# is signed again, ad hoc, where its signature no longer holds.
#
# A dSYM keeps what a debugger reads, the DWARF in Contents/Resources/DWARF
# and Contents/Info.plist. dsymutil also writes Contents/Resources/Relocations,
# a map of the objects' relocations naming the build tree's paths, which only
# a later dsymutil re-linking the DWARF reads: it is left out.
gather_dsyms() {
    local library name
    for library in "$1"/*.dylib; do
        [[ -f $library && ! -L $library ]] || continue
        # grep reads the whole listing: `grep -q` would end at the first
        # match, and nm's broken pipe fail the test under pipefail.
        nm -ap "$library" | grep ' OSO ' >/dev/null || continue
        name=$(otool -D "$library" | tail -n 1)
        name=${name##*/}
        [[ -e $1/$name ]] || name=${library##*/}
        echo "gather-dsyms: $name"
        dsymutil "$1/$name" -o "$1/$name.dSYM"
        rm -rf -- "${1:?}/${name:?}.dSYM/Contents/Resources/Relocations"
        strip -S "$library"
        codesign --verify "$library" 2>/dev/null || codesign --force --sign - "$library"
    done
}
