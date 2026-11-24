//! Verify the linkage, architecture and symbol files of the packaged binaries.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::LazyLock;

use regex::Regex;

use crate::{Result, output, text};

use super::paths::{debug_bytes, resolved};

pub(super) const LINUX_SYSTEM_LIBRARIES: [&str; 8] = [
    "ld-linux-x86-64.so.2",
    "libc.so.6",
    "libdl.so.2",
    "libgcc_s.so.1",
    "libm.so.6",
    "libpthread.so.0",
    "librt.so.1",
    "libstdc++.so.6",
];

static MINOS: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?m)^\s+minos ([0-9.]+)$").expect("minimum macOS version"));
static MACH_PATH: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?m)^\s+(?:name|path) (.+) \(offset \d+\)$").expect("Mach-O load path")
});
static ELF_MACHINE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?m)^\s*Machine:\s+Advanced Micro Devices X86-64$").expect("ELF machine")
});
static ELF_NEEDED: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\(NEEDED\).*\[(.+)\]").expect("ELF dependency"));
static ELF_PATH: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\((?:RPATH|RUNPATH)\).*\[(.*)\]").expect("ELF load path"));
static GLIBC: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\bName: GLIBC_(\d+(?:\.\d+)+)\b").expect("ELF glibc version"));
static SECTION: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?m)^\s*\[\s*\d+\]\s+(\S+)").expect("ELF section name"));
static BUILD_ID: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"Build ID: ([0-9a-f]+)").expect("ELF build ID"));
static DEBUG_LINK: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?m)^\s*\[\s*0\]\s+(\S+)$").expect("ELF debug link"));
static UUID: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"UUID: ([0-9A-F-]+)").expect("Mach-O UUID"));

fn command(name: &str, args: &[&str], path: &Path) -> Result<String> {
    text(Command::new(name).args(args).arg(path))
}

/// Verify an Apple silicon binary's declared architecture and load paths.
pub(super) fn verify_macos(bundle: &Path, binary: &Path, names: &BTreeSet<String>) -> Result<()> {
    if command("lipo", &["-archs"], binary)?.trim() != "arm64" {
        return Err(format!("{} is not arm64", binary.strip_prefix(bundle)?.display()).into());
    }
    verify_load_paths(bundle, binary, names)
}

/// Check the paths of libraries the Apple target will load.
pub(super) fn verify_load_paths(
    bundle: &Path,
    binary: &Path,
    names: &BTreeSet<String>,
) -> Result<()> {
    let relative = binary.strip_prefix(bundle)?.display();
    let load_paths = command("otool", &["-l"], binary)?;
    for captured in MINOS.captures_iter(&load_paths) {
        let minimum = &captured[1];
        let mut parts = minimum.split('.');
        let major = parts
            .next()
            .ok_or("missing macOS major version")?
            .parse::<u32>()?;
        let minor = parts
            .next()
            .ok_or("missing macOS minor version")?
            .parse::<u32>()?;
        if (major, minor) > (26, 0) {
            return Err(format!("{relative} requires macOS {minimum}").into());
        }
    }
    for captured in MACH_PATH.captures_iter(&load_paths) {
        let path = &captured[1];
        if path.starts_with("/usr/lib/")
            || path.starts_with("/System/Library/")
            || path
                .strip_prefix("@rpath/")
                .is_some_and(|name| names.contains(name))
        {
            continue;
        }
        let (parent, child) = path.split_once('/').unwrap_or((path, ""));
        let base = match parent {
            "@loader_path" => binary.parent().map(Path::to_path_buf),
            "@executable_path" => Some(bundle.join("bin")),
            _ => None,
        };
        if let Some(base) = base
            && resolved(&base.join(child))?.starts_with(bundle)
        {
            continue;
        }
        return Err(format!("external Mach-O load path in {relative}: {path}").into());
    }
    Ok(())
}

/// Verify x86-64 ELF linkage against the bundle and Ubuntu's system runtime.
pub(super) fn verify_elf(bundle: &Path, binary: &Path, names: &BTreeSet<String>) -> Result<()> {
    let relative = binary.strip_prefix(bundle)?.display();
    if !ELF_MACHINE.is_match(&command("readelf", &["--file-header"], binary)?) {
        return Err(format!("{relative} is not x86-64").into());
    }
    let dynamic = command("readelf", &["--dynamic", "--wide"], binary)?;
    for captured in ELF_NEEDED.captures_iter(&dynamic) {
        let library = &captured[1];
        if !names.contains(library) && !LINUX_SYSTEM_LIBRARIES.contains(&library) {
            return Err(format!("external ELF library in {relative}: {library}").into());
        }
    }
    for captured in ELF_PATH.captures_iter(&dynamic) {
        for path in captured[1].split(':') {
            let (origin, child) = path.split_once('/').unwrap_or((path, ""));
            if origin != "$ORIGIN"
                || !resolved(
                    &binary
                        .parent()
                        .ok_or("missing library directory")?
                        .join(child),
                )?
                .starts_with(bundle)
            {
                return Err(format!("external ELF load path in {relative}: {path}").into());
            }
        }
    }
    let versions = command("readelf", &["--version-info", "--wide"], binary)?;
    for captured in GLIBC.captures_iter(&versions) {
        let version = &captured[1];
        let components = version
            .split('.')
            .map(str::parse::<u32>)
            .collect::<std::result::Result<Vec<_>, _>>()?;
        if components.as_slice() > &[2, 43] {
            return Err(format!("{relative} requires glibc {version}").into());
        }
    }
    Ok(())
}

/// Check that the optimised library retains symbols but keeps no debug data.
pub(super) fn verify_split(bundle: &Path, library: &Path, macos: bool) -> Result<()> {
    let relative = library.strip_prefix(bundle)?.display();
    if macos {
        if !command("otool", &["-l"], library)?.contains("LC_UUID") {
            return Err(format!("{relative} has no UUID").into());
        }
        if command("nm", &["-ap"], library)?.contains(" OSO ") {
            return Err(format!("{relative} keeps a debug map").into());
        }
        return Ok(());
    }

    let sections = command("readelf", &["--section-headers", "--wide"], library)?;
    let names: BTreeSet<_> = SECTION
        .captures_iter(&sections)
        .filter_map(|name| name.get(1))
        .map(|name| name.as_str())
        .collect();
    if names.iter().any(|name| name.starts_with(".debug_")) {
        return Err(format!("{relative} keeps its debug information").into());
    }
    for required in [".symtab", ".note.gnu.build-id", ".gnu_debuglink"] {
        if !names.contains(required) {
            return Err(format!("{relative} has no {required}").into());
        }
    }
    let name = library
        .file_name()
        .ok_or("missing library name")?
        .to_string_lossy();
    if debug_link(library)?.as_deref() != Some(format!("{name}.debug").as_str()) {
        return Err(format!("{relative} links no symbol file of its own name").into());
    }
    Ok(())
}

pub(super) fn build_id(binary: &Path) -> Result<String> {
    let notes = command("readelf", &["--notes"], binary)?;
    let found = BUILD_ID
        .captures(&notes)
        .ok_or_else(|| format!("{} has no build ID", binary.display()))?;
    Ok(found[1].to_string())
}

pub(super) fn debug_link(binary: &Path) -> Result<Option<String>> {
    // The section ends in a binary CRC, not a string.
    let dump = output(
        Command::new("readelf")
            .arg("--string-dump=.gnu_debuglink")
            .arg(binary),
    )?;
    Ok(DEBUG_LINK
        .captures(&String::from_utf8_lossy(&dump.stdout))
        .map(|found| found[1].to_string()))
}

/// Report missing debug information or the wrong source mapping.
pub(super) fn elf_debug_problems(file: &Path, relative: &Path) -> Result<Vec<String>> {
    let label = relative.display();
    let sections = command("readelf", &["--section-headers", "--wide"], file)?;
    if !sections.contains(".debug_info") {
        return Ok(vec![format!("{label} has no debug information")]);
    }
    if !contains(&debug_bytes(file)?, b"/hgl-release/") {
        return Ok(vec![format!("{label} names no source under /hgl-release/")]);
    }
    Ok(Vec::new())
}

fn uuids(binary: &Path) -> Result<BTreeSet<String>> {
    let found = command("dwarfdump", &["--uuid"], binary)?;
    Ok(UUID
        .captures_iter(&found)
        .map(|uuid| uuid[1].to_string())
        .collect())
}

/// Find a dSYM carrying the library's UUID.
pub(super) fn dsym_of(library: &Path, directory: &Path) -> Result<Option<PathBuf>> {
    let mut bundles = super::entries_if_present(directory)?;
    bundles.retain(|path| {
        path.extension()
            .is_some_and(|extension| extension == "dSYM")
    });
    let uuids_of_library = uuids(library)?;
    for bundle in bundles {
        if !uuids_of_library.is_disjoint(&uuids(&bundle)?) {
            return Ok(Some(bundle));
        }
    }
    Ok(None)
}

/// Report missing DWARF data, the wrong source mapping or a relocation map.
pub(super) fn dsym_problems(bundle: &Path, root: &Path) -> Result<Vec<String>> {
    let relative = bundle.strip_prefix(root)?.display();
    let dwarf = super::entries_if_present(&bundle.join("Contents/Resources/DWARF"))?;
    let mut problems = Vec::new();
    if dwarf.is_empty() {
        problems.push(format!("{relative} holds no DWARF"));
    }
    for file in dwarf {
        let bytes = fs::read(&file)?;
        let relative = file.strip_prefix(root)?.display();
        if !contains(&bytes, b"__debug_info") {
            problems.push(format!("{relative} has no debug information"));
        } else if !contains(&bytes, b"/hgl-release/") {
            problems.push(format!("{relative} names no source under /hgl-release/"));
        }
    }
    if bundle.join("Contents/Resources/Relocations").exists() {
        problems.push(format!("{relative} keeps dsymutil's relocation map"));
    }
    Ok(problems)
}

fn contains(bytes: &[u8], needle: &[u8]) -> bool {
    bytes.windows(needle.len()).any(|window| window == needle)
}
