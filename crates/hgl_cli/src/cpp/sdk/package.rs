//! Whether a directory of hgraph's libraries built for debugging can be
//! linked whole, before a debug build takes it ([`super::choose`]).
//!
//! A debug build configures from four CMake packages and links what they
//! import: hgraph's in the SDK, and fmt's, spdlog's and simdjson's in the
//! dependency prefix. A package that is missing, or a library one imports
//! that is missing or cut short, would fail the build, or let CMake find the
//! package in another prefix and link a second copy of a library beside the
//! first. So every one of them is looked at first, and the SDK's
//! `hgl-sdk.txt` must say it was built for debugging.

use std::fs;
use std::path::{Path, PathBuf};

use object::{Object, ObjectSection, ObjectSegment};

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

/// What is wrong with `directory` as hgraph's libraries built for
/// debugging, as words that follow "which"; `None` where a debug build can
/// take it. The first wrong thing found is said, of a path relative to
/// `directory`.
pub(super) fn unusable(directory: &Path) -> Option<String> {
    if let Some(part) = super::DEBUG_HOLDS
        .iter()
        .find(|part| !directory.join(part).exists())
    {
        return Some(format!("holds no `{part}`"));
    }
    let Some(stamp) = stamp(&directory.join("hgraph-sdk")) else {
        return Some(
            "holds no `hgraph-sdk/hgl-sdk.txt`, which says how the SDK was built".to_string(),
        );
    };
    match stamp.build_type.as_deref() {
        Some("Debug") => {}
        Some(other) => {
            return Some(format!(
                "is not built for debugging: its `hgraph-sdk/hgl-sdk.txt` says `build-type={other}`"
            ));
        }
        None => {
            return Some(
                "is not built for debugging: its `hgraph-sdk/hgl-sdk.txt` names no build type, \
                 as an optimised build's does"
                    .to_string(),
            );
        }
    }
    PACKAGES
        .iter()
        .find_map(|(prefix, name)| package_problem(directory, prefix, name))
}

/// What is wrong with CMake package `name` in `directory`'s `prefix`, as
/// [`unusable`] says it: where it has no configuration file, imports no
/// library, or imports one that is missing or not whole.
fn package_problem(directory: &Path, prefix: &str, name: &str) -> Option<String> {
    let relative = format!("{prefix}/lib/cmake/{name}");
    let package = directory.join(&relative);
    // `find_package` reads `NAMEConfig.cmake` or `NAME-config.cmake`.
    let configured = [
        format!("{name}Config.cmake"),
        format!("{name}-config.cmake"),
    ]
    .iter()
    .any(|file| package.join(file).is_file());
    if !configured {
        return Some(format!("holds no CMake package `{name}` in `{relative}`"));
    }
    let imported = imported(&package, &directory.join(prefix));
    if imported.is_empty() {
        return Some(format!(
            "holds a CMake package `{name}` in `{relative}` that imports no library"
        ));
    }
    if name == "hgraph"
        && let Some(missing) = HGRAPH_LIBRARIES.iter().find(|library| {
            !imported.iter().any(|path| {
                path.file_name()
                    .is_some_and(|file| file.to_string_lossy().starts_with(*library))
            })
        })
    {
        return Some(format!(
            "holds a CMake package `hgraph` in `{relative}` that imports no `{missing}`"
        ));
    }
    for library in &imported {
        let shown = library
            .strip_prefix(directory)
            .unwrap_or(library)
            .display()
            .to_string();
        if !library.is_file() {
            return Some(format!(
                "lacks `{shown}`, which its CMake package `{name}` imports"
            ));
        }
        if let Err(why) = whole(library) {
            return Some(format!(
                "holds `{shown}`, which is not a whole library: {why}"
            ));
        }
    }
    None
}

/// The files the CMake package in `package` imports, its install prefix
/// being `prefix`: each that CMake's generated `*-targets-CONFIG.cmake`
/// lists to check, `"${_IMPORT_PREFIX}/lib/..."`.
fn imported(package: &Path, prefix: &Path) -> Vec<PathBuf> {
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
                    imported.push(prefix.join(relative));
                }
            }
        }
    }
    imported
}

/// Whether the library at `path` is whole: an object file whose headers
/// parse, and none of whose sections or segments runs past its end. It
/// reads the headers and tables alone.
fn whole(path: &Path) -> Result<(), String> {
    let file = fs::File::open(path).map_err(|error| error.to_string())?;
    let length = file.metadata().map_err(|error| error.to_string())?.len();
    // A `ReadCache` reads the file where the parser asks, and keeps it.
    let cache = object::ReadCache::new(file);
    let parsed = object::File::parse(&cache).map_err(|error| error.to_string())?;
    let past_end =
        |(offset, size): (u64, u64)| offset.checked_add(size).is_none_or(|end| end > length);
    let cut_short = parsed
        .sections()
        .filter_map(|section| section.file_range())
        .chain(parsed.segments().map(|segment| segment.file_range()))
        .any(past_end);
    if cut_short {
        return Err(format!("it ends at byte {length}, before its last section"));
    }
    Ok(())
}
