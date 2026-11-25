//! Clean a locked program workspace, including outputs left by older compilers.
//! Generated translation units identify their corresponding objects and executables.
//! The build directory can also contain user files or the other build profile.

use std::collections::HashSet;
use std::fs;
use std::io::{self, Read};
use std::path::{Path, PathBuf};

use super::build::{MANIFEST_HEADER, Workspace};

/// The caller holds the workspace lock until the replacement build completes.
pub(super) fn workspace(workspace: &Workspace, stamped: bool) -> Result<(), String> {
    clean(&workspace.directory, stamped)
        .map_err(|error| format!("cannot clean {}: {error}", workspace.directory.display()))
}

fn has_header(path: &Path, header: &str) -> io::Result<bool> {
    let mut bytes = Vec::new();
    fs::File::open(path)?
        .take((header.len() + 1) as u64)
        .read_to_end(&mut bytes)?;
    Ok(bytes.strip_suffix(b"\n") == Some(header.as_bytes()))
}

fn clean(root: &Path, stamped: bool) -> io::Result<()> {
    // Check every output directory before inspecting children. Never follow a
    // directory link into a source tree, a shared native cache or another build.
    for dir in [
        "",
        "modules",
        "tests",
        "applications",
        "obj",
        "obj/modules",
        "obj/tests",
        "obj/applications",
        "run",
        "bin",
        "results",
        "debug-results",
        "debug-sources",
        "debug-sources/native",
    ] {
        let path = if dir.is_empty() {
            root.to_owned()
        } else {
            root.join(dir)
        };
        match fs::symlink_metadata(&path) {
            Ok(metadata) if !metadata.is_dir() => {
                return Err(io::Error::other(format!(
                    "{} is not a build directory",
                    path.display()
                )));
            }
            Err(e) if e.kind() != io::ErrorKind::NotFound => return Err(e),
            _ => {}
        }
    }
    let manifest = root.join("build.ninja");
    let owned = match fs::symlink_metadata(&manifest) {
        Ok(info) if info.is_file() && has_header(&manifest, MANIFEST_HEADER)? => true,
        Ok(_) => return Err(io::Error::other("build.ninja is not an HGL build manifest")),
        Err(e) if e.kind() == io::ErrorKind::NotFound => stamped,
        Err(e) => return Err(e),
    };
    let mut products: Vec<PathBuf> = [
        ".hgl-built.json",
        ".hgl-previous-build.json",
        "compile_commands.json",
        ".ninja_log",
        ".ninja_deps",
    ]
    .into_iter()
    .map(|p| root.join(p))
    .collect();
    let mut sources = Vec::new();
    let mut test_stems = HashSet::new();
    let independent = products.len();
    for dir in [
        "modules",
        "tests",
        "applications",
        "debug-sources",
        "debug-sources/native",
        "results",
    ] {
        let entries = match fs::read_dir(root.join(dir)) {
            Ok(entries) => entries,
            Err(e) if e.kind() == io::ErrorKind::NotFound => continue,
            Err(e) => return Err(e),
        };
        for entry in entries {
            let entry = entry?;
            let name = entry.file_name();
            let Some(name) = name.to_str() else { continue };
            let path = entry.path();
            if dir == "results" {
                // The test host records each invocation as <test stem>.<position>.txt.
                if let Some((stem, position)) =
                    name.strip_suffix(".txt").and_then(|s| s.rsplit_once('.'))
                    && test_stems.contains(stem)
                    && position.parse::<usize>().is_ok()
                {
                    products.push(path);
                }
                continue;
            }
            if dir.starts_with("debug-sources") {
                if owned && name.ends_with(".hgl.source") {
                    sources.push(path);
                }
                continue;
            }
            let generated_name = match dir {
                "applications" => name.starts_with("app-") && name.ends_with(".cpp"),
                "tests" => name.starts_with("test-") && name.ends_with(".cpp"),
                _ => {
                    (name.starts_with("mod-") && (name.ends_with(".h") || name.ends_with(".cpp")))
                        || (name.starts_with("test-mod-") && name.ends_with(".cpp"))
                        || (name.starts_with("type-") && name.ends_with(".h"))
                }
            };
            if !generated_name
                || !entry.file_type()?.is_file()
                || !has_header(&path, hgl_cpp::GENERATED_HEADER)?
            {
                continue;
            }
            sources.push(path);
            let Some(stem) = name.strip_suffix(".cpp") else {
                continue;
            };
            let object = root.join("obj").join(dir);
            products.push(object.join(format!("{stem}.o")));
            products.push(object.join(format!("{stem}.o.d")));
            match dir {
                "tests" => {
                    test_stems.insert(stem.to_owned());
                    products.push(root.join(super::build::test_target_stem(stem)));
                    products.push(root.join("debug-results").join(format!("{stem}.txt")));
                }
                "applications" => {
                    products.push(
                        root.join("run")
                            .join(format!("{stem}{}", std::env::consts::EXE_SUFFIX)),
                    );
                    products.push(root.join("results").join(format!("{stem}.run.txt")));
                }
                _ => {}
            }
        }
    }
    // A failed first build may leave its directory empty. A foreign workspace
    // with colliding files must never be adopted by a clean.
    products.extend(sources);
    products.push(manifest);
    for (index, path) in products.iter().enumerate() {
        match fs::symlink_metadata(path) {
            Ok(_) if !owned && index < independent => {
                return Err(io::Error::other(format!(
                    "{} has no HGL build manifest",
                    path.display()
                )));
            }
            Ok(info) if info.is_dir() => {
                return Err(io::Error::other(format!(
                    "expected a file at {}",
                    path.display()
                )));
            }
            Err(e) if e.kind() != io::ErrorKind::NotFound => return Err(e),
            _ => {}
        }
    }
    for path in products {
        match fs::remove_file(&path) {
            Err(e) if e.kind() != io::ErrorKind::NotFound => {
                return Err(io::Error::new(e.kind(), format!("{}: {e}", path.display())));
            }
            _ => {}
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clean_refuses_a_foreign_compilation_database() {
        let workspace = tempfile::tempdir().unwrap();
        let file = workspace.path().join("compile_commands.json");
        fs::write(&file, "keep").unwrap();
        assert!(clean(workspace.path(), false).is_err());
        assert_eq!(fs::read_to_string(file).unwrap(), "keep");
    }

    #[cfg(unix)]
    #[test]
    fn a_linked_object_directory_is_rejected_before_deletion() {
        let workspace = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let db = workspace.path().join("compile_commands.json");
        fs::write(
            workspace.path().join("build.ninja"),
            format!("{MANIFEST_HEADER}\n"),
        )
        .unwrap();
        fs::write(&db, "db").unwrap();
        fs::create_dir(workspace.path().join("obj")).unwrap();
        std::os::unix::fs::symlink(outside.path(), workspace.path().join("obj/modules")).unwrap();
        let sentinel = outside.path().join("mod-demo.o");
        fs::write(&sentinel, "keep").unwrap();
        assert!(clean(workspace.path(), false).is_err());
        assert!(db.is_file());
        assert_eq!(fs::read_to_string(sentinel).unwrap(), "keep");
    }
}
