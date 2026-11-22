//! Whether a directory of hgraph's libraries built for debugging can be
//! linked whole, before a debug build takes it ([`super::choose`]).
//!
//! A debug build configures from four CMake packages and links what they
//! import: hgraph's in the SDK, and fmt's, spdlog's and simdjson's in the
//! dependency prefix. A package that is missing, or a library one imports
//! that is missing or cut short, would fail the build, or let CMake find the
//! package in another prefix and link a second copy of a library beside the
//! first; a library that needs one of the others under a name neither
//! prefix holds, or holds as another library, would build a program that
//! cannot start, or starts with the wrong one. So every one of them is
//! looked at first, and the SDK's `hgl-sdk.txt` must say it was built for
//! debugging.

use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use object::read::elf::{ElfFile, FileHeader};
use object::read::macho::{LoadCommandVariant, MachHeader, MachOFile};
use object::{FileKind, Object, ObjectSection, ObjectSegment, ReadRef};

/// The CMake packages a debug build configures from, each with the prefix
/// of the directory it is in: `PREFIX/lib/cmake/NAME`.
const PACKAGES: [(&str, &str); 4] = [
    ("hgraph-sdk", "hgraph"),
    ("hgraph-deps", "fmt"),
    ("hgraph-deps", "spdlog"),
    ("hgraph-deps", "simdjson"),
];

/// The libraries of hgraph's own package that a build links.
const HGRAPH_LIBRARIES: [&str; 2] = ["libhgraph_runtime", "libhgraph_wiring"];

/// The start of the name of each library of the four packages, which one
/// of them needs from the two prefixes, whatever its build calls it
/// (`libfmtd` for a debug build of fmt).
const FAMILY: [&str; 4] = ["libhgraph", "libfmt", "libspdlog", "libsimdjson"];

/// The directories of the two prefixes the libraries are in.
const LIBRARY_DIRECTORIES: [&str; 2] = ["hgraph-sdk/lib", "hgraph-deps/lib"];

/// The hgl version the archive's debug libraries and symbol files belong to.
pub(super) const VERSION_STAMP: &str = "hgl-debug.txt";

/// Whether this package belongs to the release of hgl using it.
pub(super) fn version_matches(directory: &Path) -> Result<(), String> {
    let text = match fs::read_to_string(directory.join(VERSION_STAMP)) {
        Ok(text) => text,
        Err(error) if error.kind() == ErrorKind::NotFound => String::new(),
        Err(error) => return Err(format!("cannot read its `{VERSION_STAMP}`: {error}")),
    };
    let version = text.trim();
    let hgl = env!("CARGO_PKG_VERSION");
    if version.is_empty() {
        return Err(format!(
            "holds no `{VERSION_STAMP}` identifying its hgl version"
        ));
    }
    if version != hgl {
        return Err(format!(
            "is for hgl {}, and this hgl is {hgl}",
            version.escape_debug()
        ));
    }
    Ok(())
}

/// The lines of an SDK's `hgl-sdk.txt` (the provisioning script's stamp,
/// `scripts/provision-hgraph-sdk.sh`) that a debug build reads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Stamp {
    /// The hgraph tag it was built from.
    pub(super) tag: Option<String>,
    /// CMake's build type, which a Release build's stamp leaves out.
    pub(super) build_type: Option<String>,
}

/// The stamp of the SDK in `sdk`, where it has one.
pub(super) fn stamp(sdk: &Path) -> Option<Stamp> {
    let text = fs::read_to_string(sdk.join("hgl-sdk.txt")).ok()?;
    let value = |key: &str| {
        text.lines()
            .find_map(|line| line.strip_prefix(key)?.strip_prefix('='))
            .map(str::to_string)
    };
    Some(Stamp {
        tag: value("tag"),
        build_type: value("build-type"),
    })
}

/// Whether a debug build can take `directory` as hgraph's libraries built
/// for debugging: what the check could not read of it, each as a note for
/// the line `hgl` prints; or what is wrong with it, as words that follow
/// "which". The first wrong thing found is said, of a path relative to
/// `directory`.
pub(super) fn usable(directory: &Path) -> Result<Vec<String>, String> {
    if let Some(part) = super::DEBUG_HOLDS
        .iter()
        .find(|part| !directory.join(part).exists())
    {
        return Err(format!("holds no `{part}`"));
    }
    let Some(stamp) = stamp(&directory.join("hgraph-sdk")) else {
        return Err(
            "holds no `hgraph-sdk/hgl-sdk.txt`, which says how the SDK was built".to_string(),
        );
    };
    match stamp.build_type.as_deref() {
        Some("Debug") => {}
        Some(other) => {
            return Err(format!(
                "is not built for debugging: its `hgraph-sdk/hgl-sdk.txt` says `build-type={other}`"
            ));
        }
        None => {
            return Err(
                "is not built for debugging: its `hgraph-sdk/hgl-sdk.txt` names no build type, \
                 as an optimised build's does"
                    .to_string(),
            );
        }
    }
    // Every package and the files it imports are looked for before any
    // library is read: what is missing is said before what is cut short.
    let mut libraries = Vec::new();
    for (prefix, name) in PACKAGES {
        libraries.extend(package(directory, prefix, name)?);
    }
    let mut notes = Vec::new();
    for Imported { shown, path } in &libraries {
        let names = match read(path) {
            Read::Whole(names) => names,
            Read::Unread => {
                notes.push(format!("`{shown}` is of a form this check does not read"));
                continue;
            }
            Read::Broken(why) => {
                return Err(format!(
                    "holds `{shown}`, which is not a whole library: {why}"
                ));
            }
        };
        // A needed name's file in either prefix, and what reading it gave.
        let found = |name: &str| {
            LIBRARY_DIRECTORIES.iter().find_map(|directory_of| {
                let candidate = directory.join(directory_of).join(name);
                candidate
                    .is_file()
                    .then(|| (format!("{directory_of}/{name}"), read(&candidate)))
            })
        };
        if let Some(problem) = needs_problem(shown, &names, &found) {
            return Err(problem);
        }
    }
    Ok(notes)
}

/// Where a needed name's library is in one of the prefixes, as it is
/// shown, and what reading it gave.
type Found<'a> = &'a dyn Fn(&str) -> Option<(String, Read)>;

/// What is wrong with the names of the library `shown`, of `names`, among
/// the four packages' libraries: the first, of the name a program that
/// links it loads it by and then the names it needs, that neither prefix
/// holds, or that `found` gives a broken library or one of another name
/// for. `found` gives a file of a name's in one of the prefixes' library
/// directories, as it is shown, and what reading it gave: where it is
/// loaded under that name, it is the file that is loaded, whatever a
/// package imports.
fn needs_problem(shown: &str, names: &Names, found: Found) -> Option<String> {
    let loaded = names
        .own
        .iter()
        .map(|own| (own, format!("a program loads as `{own}`")));
    let needed = names
        .needed
        .iter()
        .map(|needed| (needed, format!("needs `{needed}`")));
    for (name, what) in loaded.chain(needed) {
        if !FAMILY.iter().any(|family| name.starts_with(family)) {
            continue;
        }
        let Some((found_shown, read)) = found(name) else {
            return Some(format!(
                "holds `{shown}`, which {what}, and neither `{}` nor `{}` holds it",
                LIBRARY_DIRECTORIES[0], LIBRARY_DIRECTORIES[1]
            ));
        };
        match read {
            Read::Broken(why) => {
                return Some(format!(
                    "holds `{shown}`, which {what}, and `{found_shown}` is not a whole library: \
                     {why}"
                ));
            }
            // A library known by another name: a link to another build's,
            // say.
            Read::Whole(Names { own: Some(own), .. }) if own != *name => {
                return Some(format!(
                    "holds `{shown}`, which {what}, and `{found_shown}` is `{own}`"
                ));
            }
            _ => {}
        }
    }
    None
}

/// A file a CMake package imports.
struct Imported {
    /// Its path relative to the directory of libraries, `/` between its
    /// parts as the package writes them.
    shown: String,
    /// Its path.
    path: PathBuf,
}

/// The libraries CMake package `name` in `directory`'s `prefix` imports;
/// or what is wrong with it, as [`unusable`] says it: it has no
/// configuration file, imports no library, or imports one that is not
/// there.
fn package(directory: &Path, prefix: &str, name: &str) -> Result<Vec<Imported>, String> {
    let relative = format!("{prefix}/lib/cmake/{name}");
    let package = directory.join(prefix).join("lib").join("cmake").join(name);
    // `find_package` reads `NAMEConfig.cmake` or `NAME-config.cmake`.
    let configured = [
        format!("{name}Config.cmake"),
        format!("{name}-config.cmake"),
    ]
    .iter()
    .any(|file| package.join(file).is_file());
    if !configured {
        return Err(format!("holds no CMake package `{name}` in `{relative}`"));
    }
    let imported = imported(&package, directory, prefix);
    if imported.is_empty() {
        return Err(format!(
            "holds a CMake package `{name}` in `{relative}` that imports no library"
        ));
    }
    if name == "hgraph"
        && let Some(missing) = HGRAPH_LIBRARIES.iter().find(|library| {
            !imported.iter().any(|file| {
                file.path
                    .file_name()
                    .is_some_and(|file| file.to_string_lossy().starts_with(*library))
            })
        })
    {
        return Err(format!(
            "holds a CMake package `hgraph` in `{relative}` that imports no `{missing}`"
        ));
    }
    if let Some(Imported { shown, .. }) = imported.iter().find(|file| !file.path.is_file()) {
        return Err(format!(
            "lacks `{shown}`, which its CMake package `{name}` imports"
        ));
    }
    Ok(imported)
}

/// The files the CMake package in `package` imports, its install prefix
/// being `directory`'s `prefix`: each that CMake's generated
/// `*-targets-CONFIG.cmake` lists to check, `"${_IMPORT_PREFIX}/lib/..."`.
fn imported(package: &Path, directory: &Path, prefix: &str) -> Vec<Imported> {
    let mut files: Vec<PathBuf> = fs::read_dir(package)
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "cmake")
        })
        .collect();
    files.sort();
    let mut imported = Vec::new();
    for file in files {
        let text = fs::read_to_string(&file).unwrap_or_default();
        for line in text.lines() {
            // CMake 3.24 and later write the first; earlier, the second.
            let listed = line.contains("list(APPEND _cmake_import_check_files_for_")
                || line.contains("list(APPEND _IMPORT_CHECK_FILES_FOR_");
            if !listed {
                continue;
            }
            for quoted in line.split('"').skip(1).step_by(2) {
                if let Some(relative) = quoted.strip_prefix("${_IMPORT_PREFIX}/") {
                    let path = relative
                        .split('/')
                        .fold(directory.join(prefix), |path, part| path.join(part));
                    imported.push(Imported {
                        shown: format!("{prefix}/{relative}"),
                        path,
                    });
                }
            }
        }
    }
    imported
}

/// What a library's headers say of it: the name it is loaded by and the
/// names it needs loaded beside it, ELF's `DT_SONAME` and `DT_NEEDED`, or
/// the last parts of Mach-O's `LC_ID_DYLIB` and dylib load commands'.
#[derive(Debug, Default, PartialEq, Eq)]
struct Names {
    /// The name it is loaded by.
    own: Option<String>,
    /// The names it needs.
    needed: Vec<String>,
}

/// What became of reading a library.
#[derive(Debug, PartialEq, Eq)]
enum Read {
    /// It is an ELF or Mach-O shared library whose headers parse whole,
    /// none of its sections or segments running past its end.
    Whole(Names),
    /// It is of a form this check knows and does not read, a static archive
    /// or a universal Mach-O: a build may link it all the same.
    Unread,
    /// It cannot be linked: empty, of no form known, or ELF or Mach-O cut
    /// short, with why.
    Broken(String),
}

/// Reads the library at `path`: its headers and tables, and nothing of
/// its code.
fn read(path: &Path) -> Read {
    let opened = fs::File::open(path).and_then(|file| Ok((file.metadata()?.len(), file)));
    let (length, file) = match opened {
        Ok(opened) => opened,
        Err(error) => return Read::Broken(error.to_string()),
    };
    if length == 0 {
        return Read::Broken("it is empty".to_string());
    }
    // A `ReadCache` reads the file where the parser asks, and keeps it.
    let cache = object::ReadCache::new(file);
    match FileKind::parse(&cache) {
        Ok(FileKind::Elf32 | FileKind::Elf64 | FileKind::MachO32 | FileKind::MachO64) => {}
        Ok(_) => return Read::Unread,
        // Too short to say what it is, or no form known: cut short, say.
        Err(error) => return Read::Broken(format!("its form is not known: {error}")),
    }
    let parsed = match object::File::parse(&cache) {
        Ok(parsed) => parsed,
        Err(error) => return Read::Broken(error.to_string()),
    };
    let past_end =
        |(offset, size): (u64, u64)| offset.checked_add(size).is_none_or(|end| end > length);
    let cut_short = parsed
        .sections()
        .filter_map(|section| section.file_range())
        .chain(parsed.segments().map(|segment| segment.file_range()))
        .any(past_end);
    if cut_short {
        return Read::Broken(format!("it ends at byte {length}, before its last section"));
    }
    let names = match &parsed {
        object::File::Elf32(file) => elf_names(file),
        object::File::Elf64(file) => elf_names(file),
        object::File::MachO32(file) => macho_names(file),
        object::File::MachO64(file) => macho_names(file),
        _ => return Read::Unread,
    };
    match names {
        Ok(names) => Read::Whole(names),
        Err(error) => Read::Broken(error.to_string()),
    }
}

/// The names of an ELF library's dynamic section.
fn elf_names<'data, Elf: FileHeader, R: ReadRef<'data>>(
    file: &ElfFile<'data, Elf, R>,
) -> object::Result<Names> {
    let table = file
        .elf_section_table()
        .dynamic_table(file.endian(), file.data())?;
    let mut names = Names::default();
    for entry in &table {
        let text = || -> object::Result<String> {
            Ok(String::from_utf8_lossy(table.string(entry)?).into_owned())
        };
        if entry.tag == object::elf::DT_NEEDED {
            names.needed.push(text()?);
        } else if entry.tag == object::elf::DT_SONAME {
            names.own = Some(text()?);
        }
    }
    Ok(names)
}

/// The names of a Mach-O library's dylib load commands: each install
/// name's last part, as its file is named beside the others.
fn macho_names<'data, Mach: MachHeader, R: ReadRef<'data>>(
    file: &MachOFile<'data, Mach, R>,
) -> object::Result<Names> {
    let endian = file.endian();
    let last = |name: &[u8]| {
        let name = String::from_utf8_lossy(name);
        name.rsplit('/').next().unwrap_or_default().to_string()
    };
    let mut names = Names::default();
    let mut commands = file.macho_load_commands()?;
    while let Some(command) = commands.next()? {
        match command.variant()? {
            LoadCommandVariant::IdDylib(dylib) => {
                names.own = Some(last(command.string(endian, dylib.dylib.name)?));
            }
            LoadCommandVariant::Dylib(dylib) => {
                names
                    .needed
                    .push(last(command.string(endian, dylib.dylib.name)?));
            }
            _ => {}
        }
    }
    Ok(names)
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::{Names, Read, needs_problem, read};

    /// What reading a whole library loaded by `own` gives.
    fn named(own: &str) -> Read {
        Read::Whole(Names {
            own: Some(own.to_string()),
            needed: Vec::new(),
        })
    }

    #[test]
    fn a_library_needing_one_whose_file_is_broken_cannot_start() {
        // `libfmtd.so.12.1.0` whole, and `libfmtd.so.12`, which the
        // loader loads, cut short.
        let found = |name: &str| {
            let read = if name == "libfmtd.so.12" {
                Read::Broken("it is empty".to_string())
            } else {
                named(name)
            };
            Some((format!("hgraph-deps/lib/{name}"), read))
        };
        assert_eq!(
            needs_problem("hgraph-sdk/lib/libhgraph_runtime.so", &runtime(), &found),
            Some(
                "holds `hgraph-sdk/lib/libhgraph_runtime.so`, which needs `libfmtd.so.12`, and \
                 `hgraph-deps/lib/libfmtd.so.12` is not a whole library: it is empty"
                    .to_string()
            )
        );
    }

    /// The names of a library of the runtime, needing two of the
    /// dependencies and the system's C library.
    fn runtime() -> Names {
        Names {
            own: Some("libhgraph_runtime.so".to_string()),
            needed: ["libfmtd.so.12", "libspdlogd.so.1.17", "libc.so.6"]
                .map(str::to_string)
                .to_vec(),
        }
    }

    #[test]
    fn a_library_needing_one_neither_prefix_holds_cannot_start() {
        // The debug SDK beside the release's optimised dependencies.
        let found = |name: &str| {
            (name != "libfmtd.so.12").then(|| (format!("hgraph-deps/lib/{name}"), named(name)))
        };
        assert_eq!(
            needs_problem("hgraph-sdk/lib/libhgraph_runtime.so", &runtime(), &found),
            Some(
                "holds `hgraph-sdk/lib/libhgraph_runtime.so`, which needs `libfmtd.so.12`, and \
                 neither `hgraph-sdk/lib` nor `hgraph-deps/lib` holds it"
                    .to_string()
            )
        );
    }

    #[test]
    fn a_library_needing_one_another_library_stands_in_for_is_found_out() {
        // `libfmtd.so.12` a link to an optimised fmt.
        let found = |name: &str| {
            let own = if name == "libfmtd.so.12" {
                "libfmt.so.12"
            } else {
                name
            };
            Some((format!("hgraph-deps/lib/{name}"), named(own)))
        };
        assert_eq!(
            needs_problem("hgraph-sdk/lib/libhgraph_runtime.so", &runtime(), &found),
            Some(
                "holds `hgraph-sdk/lib/libhgraph_runtime.so`, which needs `libfmtd.so.12`, and \
                 `hgraph-deps/lib/libfmtd.so.12` is `libfmt.so.12`"
                    .to_string()
            )
        );
        // The system's libraries are the loader's to find.
        let whole = |name: &str| {
            (name != "libc.so.6").then(|| (format!("hgraph-deps/lib/{name}"), named(name)))
        };
        assert_eq!(needs_problem("x", &runtime(), &whole), None);
        // A debug fmt's file a link to an optimised one: a program linking
        // it would load the optimised fmt beside the debug one.
        let linked = Names {
            own: Some("libfmt.so.12".to_string()),
            needed: vec!["libc.so.6".to_string()],
        };
        let debug_only = |name: &str| {
            name.starts_with("libfmtd")
                .then(|| (format!("hgraph-deps/lib/{name}"), named(name)))
        };
        assert_eq!(
            needs_problem("hgraph-deps/lib/libfmtd.so.12.1.0", &linked, &debug_only),
            Some(
                "holds `hgraph-deps/lib/libfmtd.so.12.1.0`, which a program loads as \
                 `libfmt.so.12`, and neither `hgraph-sdk/lib` nor `hgraph-deps/lib` holds it"
                    .to_string()
            )
        );
    }

    #[test]
    fn a_form_the_check_does_not_read_is_said_so_and_an_unknown_one_is_broken() {
        let directory = tempfile::tempdir().unwrap();
        // A static archive's magic and its first member's header.
        let archive = directory.path().join("libfmtd.a");
        let mut bytes = b"!<arch>\n".to_vec();
        bytes.extend([b' '; 60]);
        fs::write(&archive, &bytes).unwrap();
        assert_eq!(read(&archive), Read::Unread);
        let empty = directory.path().join("libfmtd.so");
        fs::write(&empty, b"").unwrap();
        assert_eq!(read(&empty), Read::Broken("it is empty".to_string()));
        // A library cut to a few bytes, too few to say what it is.
        let fragment = directory.path().join("libfmtd.so.12");
        fs::write(&fragment, b"\x7fELF\x02").unwrap();
        assert!(matches!(read(&fragment), Read::Broken(_)));
        let unknown = directory.path().join("libfmtd.so.12.1.0");
        fs::write(&unknown, b"no library at all, of no form").unwrap();
        assert!(
            matches!(read(&unknown), Read::Broken(why) if why.starts_with("its form is not known"))
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_library_is_read_whole_with_the_names_it_needs_and_not_when_cut_short() {
        // This test's own executable needs the system's C library.
        let executable = std::env::current_exe().unwrap();
        let Read::Whole(names) = read(&executable) else {
            panic!("{executable:?} is not read whole");
        };
        assert!(
            names
                .needed
                .iter()
                .any(|name| name.starts_with("libc.so") || name.starts_with("libSystem")),
            "{names:?}"
        );
        let directory = tempfile::tempdir().unwrap();
        let short = directory.path().join("short");
        let whole = fs::read(&executable).unwrap();
        fs::write(&short, &whole[..whole.len() / 2]).unwrap();
        assert!(matches!(read(&short), Read::Broken(_)));
    }
}
