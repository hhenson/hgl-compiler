//! Audit release and debug trees against the platform's packaging contract.

use std::collections::BTreeSet;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use regex::Regex;

use super::NATIVE_LIBRARIES;
use super::licenses::is_notice;
use crate::{Result, Target};

mod binary;
mod paths;

const SDK: &str = "libexec/hgraph-sdk";
const DEPS: &str = "libexec/hgraph-deps";
const DEBUG: &str = "libexec/hgraph-debug";
const PREFIX_LAYOUT: [&str; 2] = ["include", "lib"];

static MACOS_DEP_LIBRARY: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^lib(?:fmt|spdlog|simdjson)(?:\.[0-9]+)*\.dylib$")
        .expect("macOS dependency library name")
});
static LINUX_DEP_LIBRARY: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^lib(?:fmt|spdlog|simdjson)\.so(?:\.[0-9]+)*$")
        .expect("Linux dependency library name")
});
static LINUX_SPLIT_LIBRARY: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\.so(?:\.[0-9]+)*$").expect("Linux shared library name"));

fn entries(directory: &Path) -> io::Result<Vec<PathBuf>> {
    let mut paths: Vec<_> = fs::read_dir(directory)?
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<io::Result<_>>()?;
    paths.sort_unstable();
    Ok(paths)
}

fn entries_if_present(directory: &Path) -> io::Result<Vec<PathBuf>> {
    match entries(directory) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(Vec::new()),
        result => result,
    }
}

fn name(path: &Path) -> String {
    path.file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned()
}

fn libraries(sdk: &Path, deps: &Path, macos: bool) -> Result<(Vec<PathBuf>, BTreeSet<String>)> {
    let libraries: Vec<_> = entries_if_present(&sdk.join("lib"))?
        .into_iter()
        .chain(entries_if_present(&deps.join("lib"))?)
        .filter(|library| {
            let name = name(library);
            if macos {
                name.ends_with(".dylib")
            } else {
                name.contains(".so")
            }
        })
        .collect();
    let names = libraries.iter().map(|library| name(library)).collect();
    Ok((libraries, names))
}

fn require_paths(root: &Path, paths: &[PathBuf]) -> Result<()> {
    for path in paths {
        if !path.exists() {
            return Err(format!("missing {}", path.strip_prefix(root)?.display()).into());
        }
    }
    Ok(())
}

fn sdk_paths(sdk: &Path, deps: &Path) -> Vec<PathBuf> {
    vec![
        sdk.join("hgl-sdk.txt"),
        sdk.join("lib/cmake/hgraph/hgraphConfig.cmake"),
        deps.join("include/fmt"),
        deps.join("include/spdlog"),
        deps.join("include/simdjson.h"),
    ]
}

fn notices(licenses: &Path, description: &str) -> Result<()> {
    for library in NATIVE_LIBRARIES {
        let files = entries_if_present(&licenses.join(library))?;
        if !files
            .iter()
            .any(|path| path.is_file() && is_notice(path.file_name().unwrap_or_default()))
        {
            return Err(format!("missing license notice for {description}{library}").into());
        }
    }
    Ok(())
}

fn require_libraries(names: &BTreeSet<String>, required: &[&str], description: &str) -> Result<()> {
    for prefix in required {
        if !names.iter().any(|name| name.starts_with(prefix)) {
            return Err(format!("missing {description}{prefix} shared library").into());
        }
    }
    Ok(())
}

fn canonical_libraries(libraries: &[PathBuf]) -> io::Result<BTreeSet<PathBuf>> {
    libraries.iter().map(|path| path.canonicalize()).collect()
}

/// Check the release's layout, architecture, relocatability and system libraries.
pub(crate) fn verify(bundle: &Path, forbidden: &[PathBuf], target: Target) -> Result<()> {
    let bundle = bundle.canonicalize()?;
    let hgl = bundle.join("bin/hgl");
    let sdk = bundle.join(SDK);
    let deps = bundle.join(DEPS);
    let licenses = bundle.join("licenses");
    let mut required = vec![hgl.clone()];
    required.extend(sdk_paths(&sdk, &deps));
    required.extend(
        [
            "hgl-compiler.txt",
            "katex.txt",
            "mermaid.txt",
            "mermaid-third-party.txt",
        ]
        .into_iter()
        .map(|file| licenses.join(file)),
    );
    require_paths(&bundle, &required)?;
    notices(&licenses, "")?;

    let macos = matches!(target, Target::Macos);
    let (libraries, names) = libraries(&sdk, &deps, macos)?;
    require_libraries(
        &names,
        &[
            "libhgraph_runtime",
            "libhgraph_wiring",
            "libfmt",
            "libspdlog",
            "libsimdjson",
        ],
        "",
    )?;
    if entries(&deps)?
        .iter()
        .map(|path| name(path))
        .ne(PREFIX_LAYOUT)
    {
        return Err("unexpected dependency prefix contents".into());
    }
    let library_name = if macos {
        &MACOS_DEP_LIBRARY
    } else {
        &LINUX_DEP_LIBRARY
    };
    for library in entries(&deps.join("lib"))? {
        let name = name(&library);
        if name != "cmake" && !library_name.is_match(&name) {
            return Err(format!("unexpected dependency library: {name}").into());
        }
    }
    if !macos
        && binary::LINUX_SYSTEM_LIBRARIES
            .iter()
            .any(|name| names.contains(*name))
    {
        return Err("system C or C++ runtime bundled on Linux".into());
    }

    paths::verify_paths(&bundle, forbidden)?;
    let libraries = canonical_libraries(&libraries)?;
    for binary in std::iter::once(&hgl).chain(&libraries) {
        if macos {
            binary::verify_macos(&bundle, binary, &names)?;
        } else {
            binary::verify_elf(&bundle, binary, &names)?;
        }
    }
    for library in &libraries {
        binary::verify_split(&bundle, library, macos)?;
    }
    println!("Verified {}", name(&bundle));
    Ok(())
}

/// Check debug builds and match the released optimised binary's symbol files.
pub(crate) fn verify_debug(
    tree: &Path,
    release: &Path,
    forbidden: &[PathBuf],
    target: Target,
) -> Result<()> {
    let tree = tree.canonicalize()?;
    let release = release.canonicalize()?;
    let debug = tree.join(DEBUG);
    let sdk = debug.join("hgraph-sdk");
    let deps = debug.join("hgraph-deps");
    let mut required = sdk_paths(&sdk, &deps);
    required.extend([
        sdk.join("share/licenses/hgraph/LICENSE"),
        debug.join("hgraph-source/src/hgraph/runtime/node.cpp"),
        debug.join("hgraph-source/include/hgraph"),
    ]);
    require_paths(&tree, &required)?;
    let stamp = fs::read_to_string(sdk.join("hgl-sdk.txt"))?;
    if !["build-type=Debug", "dependencies=../hgraph-deps"]
        .into_iter()
        .all(|required| stamp.lines().any(|line| line == required))
    {
        return Err("the debug SDK is not stamped as a Debug build against ../hgraph-deps".into());
    }
    notices(&debug.join("licenses"), "the debug ")?;
    if entries(&deps)?
        .iter()
        .map(|path| name(path))
        .ne(PREFIX_LAYOUT)
    {
        return Err("unexpected debug dependency prefix contents".into());
    }

    let macos = matches!(target, Target::Macos);
    let (libraries, names) = libraries(&sdk, &deps, macos)?;
    require_libraries(
        &names,
        &[
            "libhgraph_runtime",
            "libhgraph_wiring",
            "libfmtd",
            "libspdlogd",
            "libsimdjson",
        ],
        "the debug ",
    )?;
    let mut failures = Vec::new();
    for library in canonical_libraries(&libraries)? {
        let relative = library.strip_prefix(&tree)?;
        let linked = if macos {
            binary::verify_load_paths(&tree, &library, &names)
        } else {
            binary::verify_elf(&tree, &library, &names)
        };
        if let Err(error) = linked {
            failures.push(error.to_string());
        }
        if macos {
            let directory = library.parent().ok_or("missing library directory")?;
            if let Some(dsym) = binary::dsym_of(&library, directory)? {
                failures.extend(binary::dsym_problems(&dsym, &tree)?);
            } else {
                failures.push(format!("{} has no dSYM of its UUID", relative.display()));
            }
        } else {
            failures.extend(binary::elf_debug_problems(&library, relative)?);
        }
    }
    for directory in ["libexec/hgraph-sdk/lib", "libexec/hgraph-deps/lib"] {
        for library in entries(&release.join(directory))? {
            let file = name(&library);
            if library.is_symlink()
                || !library.is_file()
                || !(if macos {
                    file.ends_with(".dylib")
                } else {
                    LINUX_SPLIT_LIBRARY.is_match(&file)
                })
            {
                continue;
            }
            let relative = library.strip_prefix(&release)?.display();
            if macos {
                if let Some(dsym) = binary::dsym_of(&library, &tree.join(directory))? {
                    failures.extend(binary::dsym_problems(&dsym, &tree)?);
                } else {
                    failures.push(format!("no dSYM of {relative}'s UUID"));
                }
                continue;
            }
            let Some(link) = binary::debug_link(&library)? else {
                failures.push(format!("no symbol file for {relative}"));
                continue;
            };
            let symbols = tree.join(directory).join(".debug").join(link);
            if !symbols.is_file() {
                failures.push(format!("no symbol file for {relative}"));
                continue;
            }
            let symbol_path = symbols.strip_prefix(&tree)?;
            if binary::build_id(&symbols)? != binary::build_id(&library)? {
                failures.push(format!(
                    "{} is not {relative}'s build",
                    symbol_path.display()
                ));
            }
            failures.extend(binary::elf_debug_problems(&symbols, symbol_path)?);
        }
    }
    if let Err(error) = paths::verify_paths(&tree, forbidden) {
        failures.push(error.to_string());
    }
    if !failures.is_empty() {
        return Err(failures.join("\n").into());
    }
    println!("Verified the debug tree of {}", name(&tree));
    Ok(())
}
