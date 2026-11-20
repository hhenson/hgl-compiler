//! The C++ toolchain the cpp target builds with: hgraph's C++ SDK and
//! its dependencies, the compiler, `ninja`, `cmake`, and a fast linker
//! where there is one; and what is made once for each of them and kept:
//! the runtime library, a description of how a generated unit is
//! compiled and an executable linked, and a precompiled header.
//!
//! Where the SDK and its dependencies are is said by two environment
//! variables and nothing else: no default could be any machine's but
//! one. `HGL_HGRAPH_SDK` names the SDK's install prefix, the directory
//! that holds `lib/cmake/hgraph`; `HGL_HGRAPH_DEPS` the prefix of the
//! libraries the SDK was built against (Arrow, fmt, spdlog and the
//! rest). The compiler is the one `CXX` names, or the first GCC of
//! version 15 or later on the `PATH` (`docs/cpp-target.md`: "GCC 15 or
//! later on Linux"). `HGL_CPP_FLAGS` adds flags to every compile and
//! link, the runtime library's too: `-O2`, `-g`, or the sanitizers.
//!
//! What is made for a toolchain is kept under the cache, in a directory
//! named by a hash of everything it depends on: the SDK's and the
//! dependencies' paths and the SDK's own files, the compiler and its
//! version, the linker, the flags, and the runtime library's sources.
//! It is made once, under a lock, by CMake:
//!
//! 1. the runtime library's project (`runtime/cpp`, carried by `hgl`)
//!    is configured, built and installed, as `libhglrt`;
//! 2. a small project of this driver's own, a program that links the
//!    runtime library, is configured, and CMake's file API describes how
//!    CMake would compile and link it: the flags, defines and include
//!    directories hgraph's CMake package asks for, and the libraries in
//!    order. That is the description every generated unit is compiled
//!    with, written to `description.json`;
//! 3. the runtime library's header, the fixed include set, is
//!    precompiled with those flags.

use std::fs;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;

use super::build::{lock, quoted, write_if_changed};

/// The variable that names hgraph's C++ SDK.
pub(super) const SDK: &str = "HGL_HGRAPH_SDK";

/// The variable that names the SDK's dependency prefix.
pub(super) const DEPS: &str = "HGL_HGRAPH_DEPS";

/// The variable whose flags are added to every compile and link.
pub(super) const FLAGS: &str = "HGL_CPP_FLAGS";

/// The oldest GCC the target is written for (`docs/cpp-target.md`).
const OLDEST_GCC: u32 = 15;

/// The compilers looked for on the `PATH` when `CXX` names none, the
/// newest first.
const COMPILERS: [&str; 4] = ["g++-17", "g++-16", "g++-15", "g++"];

/// The file a prepared toolchain's description is kept in. It is
/// written last: a directory that holds it is ready.
const DESCRIPTION: &str = "description.json";

/// A version of what is made for a toolchain: raised when what this
/// driver makes changes, so that what an older `hgl` made is not used.
const FORMAT: u32 = 2;

/// Everything the cpp target builds with, as it was found.
#[derive(Debug, Clone)]
pub(super) struct Toolchain {
    /// The SDK's install prefix.
    sdk: PathBuf,
    /// The dependency prefix.
    deps: PathBuf,
    /// The C++ compiler.
    compiler: PathBuf,
    /// The compiler's version, as `-dumpfullversion` gives it.
    version: String,
    /// `ninja`.
    pub(super) ninja: PathBuf,
    /// True where `ld.lld` is on the `PATH`, and links are made with it.
    lld: bool,
    /// What `HGL_CPP_FLAGS` adds.
    flags: Vec<String>,
}

/// How a generated unit is compiled and an executable linked, for one
/// toolchain: what `prepare` works out once and keeps.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Description {
    /// The compiler.
    pub(super) compiler: String,
    /// The flags of a compile, each as the shell reads it: the
    /// standard, the defines and the include directories.
    pub(super) compile: Vec<String>,
    /// The flags of a link, as the shell reads them: the libraries, in
    /// order, and where the executable finds them as it runs.
    pub(super) link: Vec<String>,
    /// The header every unit of a module is compiled with first, whose
    /// precompiled form is beside it.
    pub(super) header: String,
    /// The directories an executable's libraries are loaded from: the
    /// runtime library's, the SDK's and its dependencies'. An executable
    /// is run with them in `LD_LIBRARY_PATH`: the runtime library and
    /// the SDK's own find what they need by their own `RUNPATH`s, which
    /// do not name the dependencies' directory, and a `RUNPATH` stops
    /// the loader reading the executable's `RPATH` for what they load.
    pub(super) libraries: Vec<String>,
}

impl Toolchain {
    /// The toolchain the environment names, or why there is none: a
    /// variable missing or naming what is not there, or a program
    /// that cannot be found.
    pub(super) fn find() -> Result<Toolchain, String> {
        let sdk = prefix(
            SDK,
            "the install prefix of hgraph's C++ SDK, the directory that holds `lib/cmake/hgraph`",
            &["lib/cmake/hgraph/hgraphConfig.cmake"],
        )?;
        let deps = prefix(
            DEPS,
            "the prefix of the libraries hgraph's C++ SDK was built against, which holds `include` and `lib`",
            &["include", "lib"],
        )?;
        let (compiler, version) = compiler()?;
        let ninja = which::which("ninja").map_err(|_| {
            "`ninja` is not on the PATH: the cpp target builds with it (https://ninja-build.org)"
                .to_string()
        })?;
        let flags = std::env::var(FLAGS)
            .unwrap_or_default()
            .split_whitespace()
            .map(str::to_string)
            .collect();
        Ok(Toolchain {
            sdk,
            deps,
            compiler,
            version,
            ninja,
            lld: which::which("ld.lld").is_ok(),
            flags,
        })
    }

    /// The name of the directory what is made for this toolchain is
    /// kept in: a hash of all it depends on. The hash is the standard
    /// library's, which may change with Rust's release: a cache made by
    /// an `hgl` built with another is then made again.
    fn key(&self) -> String {
        let mut hash = DefaultHasher::new();
        FORMAT.hash(&mut hash);
        for path in [&self.sdk, &self.deps, &self.compiler] {
            path.hash(&mut hash);
        }
        self.version.hash(&mut hash);
        self.lld.hash(&mut hash);
        self.flags.hash(&mut hash);
        // The SDK's own files: one rebuilt in place is another SDK.
        for file in [
            "lib/cmake/hgraph/hgraphConfigVersion.cmake",
            "lib/libhgraph_runtime.so",
            "lib/libhgraph_wiring.so",
        ] {
            if let Ok(metadata) = fs::metadata(self.sdk.join(file)) {
                metadata.len().hash(&mut hash);
                metadata.modified().ok().hash(&mut hash);
            }
        }
        hgl_cpp::runtime::FILES.hash(&mut hash);
        format!("{:016x}", hash.finish())
    }

    /// The description of this toolchain, made under `cache` the first
    /// time it is asked for and read from there after (see the head of
    /// this file). `jobs` is how many jobs a build may run at once, as
    /// `ninja -j` takes it; `None` for the build's own choice.
    pub(super) fn prepare(&self, cache: &Path, jobs: Option<&str>) -> Result<Description, String> {
        if !hgl_cpp::runtime::available() {
            return Err(
                "this hgl was built without the C++ runtime library: `runtime/cpp` was not in \
                 its source tree"
                    .to_string(),
            );
        }
        let directory = cache.join("toolchains").join(self.key());
        fs::create_dir_all(&directory)
            .map_err(|error| format!("{} could not be made: {error}", directory.display()))?;
        // Two runs that both find the toolchain unprepared make it once.
        let _held = lock(&directory)?;
        let described = directory.join(DESCRIPTION);
        if let Ok(text) = fs::read_to_string(&described)
            && let Some(description) = read_description(&text)
        {
            return Ok(description);
        }
        let cmake = which::which("cmake").map_err(|_| {
            "`cmake` is not on the PATH: the cpp target builds its runtime library with it once \
             for each SDK (https://cmake.org)"
                .to_string()
        })?;
        let runtime = self.build_runtime(&cmake, &directory, jobs)?;
        let mut description = self.describe(&cmake, &directory, &runtime)?;
        description.header = self
            .precompile(&directory, &description)?
            .display()
            .to_string();
        let json = serde_json::json!({
            "compiler": description.compiler,
            "compile": description.compile,
            "link": description.link,
            "header": description.header,
            "libraries": description.libraries,
        });
        write_if_changed(&described, &json.to_string())?;
        Ok(description)
    }

    /// The flags every CMake configure of this toolchain is given.
    fn configure_flags(&self) -> Vec<String> {
        vec![
            "-G".to_string(),
            "Ninja".to_string(),
            format!("-DCMAKE_MAKE_PROGRAM={}", self.ninja.display()),
            format!("-DCMAKE_CXX_COMPILER={}", self.compiler.display()),
            // The flags are these and no others: CMake would otherwise
            // take `CXXFLAGS` and `LDFLAGS` from the environment, which
            // the toolchain's hash does not hold.
            format!("-DCMAKE_CXX_FLAGS={}", self.flags.join(" ")),
            "-DCMAKE_EXE_LINKER_FLAGS=".to_string(),
            "-DCMAKE_SHARED_LINKER_FLAGS=".to_string(),
            format!("-D{SDK}={}", self.sdk.display()),
            format!("-D{DEPS}={}", self.deps.display()),
        ]
    }

    /// Writes out the runtime library's sources under `directory`, and
    /// configures, builds and installs them; gives the prefix it is
    /// installed under.
    fn build_runtime(
        &self,
        cmake: &Path,
        directory: &Path,
        jobs: Option<&str>,
    ) -> Result<PathBuf, String> {
        let source = directory.join("runtime-source");
        let build = directory.join("runtime-build");
        let installed = directory.join("runtime");
        for made in [&source, &build, &installed] {
            if made.exists() {
                fs::remove_dir_all(made)
                    .map_err(|error| format!("{} could not be removed: {error}", made.display()))?;
            }
        }
        for (path, bytes) in hgl_cpp::runtime::FILES {
            let file = path
                .split('/')
                .fold(source.clone(), |file, part| file.join(part));
            if let Some(parent) = file.parent() {
                fs::create_dir_all(parent)
                    .map_err(|error| format!("{} could not be made: {error}", parent.display()))?;
            }
            fs::write(&file, bytes)
                .map_err(|error| format!("{} could not be written: {error}", file.display()))?;
        }
        let mut configure = Command::new(cmake);
        configure
            .arg("-S")
            .arg(&source)
            .arg("-B")
            .arg(&build)
            .args(self.configure_flags())
            .arg("-DCMAKE_BUILD_TYPE=Release")
            .arg(format!("-DCMAKE_INSTALL_PREFIX={}", installed.display()))
            // The library is installed in `lib` on every system, where the
            // description's program finds it.
            .arg("-DCMAKE_INSTALL_LIBDIR=lib")
            .arg("-DBUILD_TESTING=OFF");
        run("the runtime library's configuration", &mut configure)?;
        let mut built = Command::new(cmake);
        built.arg("--build").arg(&build);
        if let Some(jobs) = jobs {
            built.arg("--parallel").arg(jobs);
        }
        run("the runtime library's build", &mut built)?;
        run(
            "the runtime library's installation",
            Command::new(cmake).arg("--install").arg(&build),
        )?;
        Ok(installed)
    }

    /// Configures this driver's own small CMake project, a program
    /// linked with the runtime library installed under `runtime`, and
    /// reads from CMake's file API how CMake compiles and links it.
    fn describe(
        &self,
        cmake: &Path,
        directory: &Path,
        runtime: &Path,
    ) -> Result<Description, String> {
        let source = directory.join("probe-source");
        let build = directory.join("probe-build");
        if build.exists() {
            fs::remove_dir_all(&build)
                .map_err(|error| format!("{} could not be removed: {error}", build.display()))?;
        }
        write_if_changed(&source.join("CMakeLists.txt"), PROBE)?;
        write_if_changed(&source.join("probe.cpp"), "int main() { return 0; }\n")?;
        // A file under `.cmake/api/v1/query` asks CMake to describe the
        // build; an empty one named `codemodel-v2` asks for the targets.
        write_if_changed(&build.join(".cmake/api/v1/query/codemodel-v2"), "")?;
        let mut configure = Command::new(cmake);
        configure
            .arg("-S")
            .arg(&source)
            .arg("-B")
            .arg(&build)
            .args(self.configure_flags())
            .arg(format!("-DHGL_RUNTIME={}", runtime.display()));
        run("the description of a build against the SDK", &mut configure)?;
        let target = probe_target(&build.join(".cmake/api/v1/reply"))?;
        let mut description = description_of(&target, &self.compiler)?;
        // An `RPATH` of the program's, which the loader reads for what
        // it loads where that has no `RUNPATH` of its own, so that an
        // executable run by hand finds what it can.
        description.link.push("-Wl,--disable-new-dtags".to_string());
        description.libraries = [
            runtime.join("lib"),
            self.sdk.join("lib"),
            self.deps.join("lib"),
        ]
        .iter()
        .map(|directory| directory.display().to_string())
        .collect();
        if self.lld {
            description.link.push("-fuse-ld=lld".to_string());
        }
        Ok(description)
    }

    /// Precompiles the runtime library's header with `description`'s
    /// flags, and gives the header units are compiled with: GCC takes
    /// the precompiled form beside it, `hgl_pch.h.gch`, in its place.
    fn precompile(&self, directory: &Path, description: &Description) -> Result<PathBuf, String> {
        let header = directory.join("pch").join("hgl_pch.h");
        write_if_changed(
            &header,
            &format!("#include <{}>\n", hgl_cpp::runtime::HEADER),
        )?;
        // The flags are as the shell reads them, as `ninja` gives them
        // to the shell for every unit: the header is compiled by the
        // shell too, so that it reads them the same.
        let command = format!(
            "{} {} -x c++-header {} -o {}",
            description.compiler,
            description.compile.join(" "),
            quoted(&header.display().to_string()),
            quoted(&header.with_extension("h.gch").display().to_string())
        );
        run(
            "the precompiled header",
            Command::new("/bin/sh").arg("-c").arg(command),
        )?;
        Ok(header)
    }
}

/// The project whose build CMake describes: a program that links the
/// runtime library, as an executable of generated tests does. The
/// library is an imported target over its installed files; it carries
/// hgraph's `runtime` and `wiring` libraries with their own
/// dependencies, and nothing of hgraph's standard library.
const PROBE: &str = r#"# Written by hgl: a program linked as the cpp target's are, which
# CMake's file API describes. It is configured and never built.
cmake_minimum_required(VERSION 3.28)
project(hgl_describe LANGUAGES CXX)
list(APPEND CMAKE_PREFIX_PATH "${HGL_HGRAPH_SDK}" "${HGL_HGRAPH_DEPS}")
find_package(hgraph CONFIG REQUIRED)
add_library(hglrt SHARED IMPORTED)
set_target_properties(hglrt PROPERTIES
  IMPORTED_LOCATION "${HGL_RUNTIME}/lib/libhglrt.so"
  INTERFACE_INCLUDE_DIRECTORIES "${HGL_RUNTIME}/include"
  INTERFACE_LINK_LIBRARIES "hgraph::runtime;hgraph::wiring")
add_executable(hgl_probe probe.cpp)
target_compile_features(hgl_probe PRIVATE cxx_std_23)
target_link_libraries(hgl_probe PRIVATE hglrt)
"#;

/// The directory a prefix variable `variable` names, which holds each
/// of `holds`; or why it cannot be used, saying what it is to name.
fn prefix(variable: &str, what: &str, holds: &[&str]) -> Result<PathBuf, String> {
    let Some(named) = std::env::var_os(variable).filter(|named| !named.is_empty()) else {
        return Err(format!("{variable} is not set: set it to {what}"));
    };
    let path = PathBuf::from(&named);
    for part in holds {
        if !path.join(part).exists() {
            return Err(format!(
                "{variable} names {}, which holds no `{part}`: set it to {what}",
                path.display()
            ));
        }
    }
    fs::canonicalize(&path).map_err(|error| format!("{variable} names {}: {error}", path.display()))
}

/// The compiler `CXX` names, or the first of [`COMPILERS`] on the
/// `PATH`, with its version; or why there is none of version
/// [`OLDEST_GCC`] or later.
fn compiler() -> Result<(PathBuf, String), String> {
    let named = std::env::var_os("CXX").filter(|named| !named.is_empty());
    let candidates: Vec<PathBuf> = match &named {
        Some(named) => vec![which::which(named).map_err(|_| {
            format!(
                "CXX names {}, which is not a program",
                PathBuf::from(named).display()
            )
        })?],
        None => COMPILERS
            .iter()
            .filter_map(|name| which::which(name).ok())
            .collect(),
    };
    let mut seen = Vec::new();
    for candidate in candidates {
        let version = Command::new(&candidate)
            .arg("-dumpfullversion")
            .output()
            .ok()
            .filter(|output| output.status.success())
            .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_string())
            .unwrap_or_default();
        let major = version
            .split('.')
            .next()
            .and_then(|major| major.parse::<u32>().ok());
        // The path as found, and not where a link leads: a wrapper
        // such as `ccache` linked as `g++` knows the compiler by it.
        if major.is_some_and(|major| major >= OLDEST_GCC) {
            return Ok((candidate, version));
        }
        seen.push(format!(
            "{} ({})",
            candidate.display(),
            if version.is_empty() {
                "no version"
            } else {
                &version
            }
        ));
    }
    let found = if seen.is_empty() {
        "none was found".to_string()
    } else {
        format!("found {}", seen.join(", "))
    };
    Err(format!(
        "the cpp target needs GCC {OLDEST_GCC} or later, named by CXX or on the PATH as one of \
         {}: {found}",
        COMPILERS.join(", ")
    ))
}

/// Runs `command` to its end; `what` names it in the reason it failed,
/// with the last lines it printed.
fn run(what: &str, command: &mut Command) -> Result<(), String> {
    let output = command
        .output()
        .map_err(|error| format!("{what} could not be started: {error}"))?;
    if output.status.success() {
        return Ok(());
    }
    let mut printed = String::from_utf8_lossy(&output.stdout).into_owned();
    printed.push_str(&String::from_utf8_lossy(&output.stderr));
    Err(format!(
        "{what} failed ({}):\n{}",
        output.status,
        last_lines(&printed, 20)
    ))
}

/// The last `count` lines of `text` that are not empty.
pub(super) fn last_lines(text: &str, count: usize) -> String {
    let lines: Vec<&str> = text
        .lines()
        .filter(|line| !line.trim().is_empty())
        .collect();
    lines[lines.len().saturating_sub(count)..].join("\n")
}

/// The file-API description of the probe's target, from the reply
/// directory `reply`: the newest index, its code model, and the target
/// `hgl_probe` in it.
fn probe_target(reply: &Path) -> Result<Value, String> {
    let unreadable = |what: &str| format!("CMake's description of the build has no {what}");
    let read = |path: &Path| -> Result<Value, String> {
        let text = fs::read_to_string(path)
            .map_err(|error| format!("{} could not be read: {error}", path.display()))?;
        serde_json::from_str(&text).map_err(|error| format!("{}: {error}", path.display()))
    };
    // Index files are named by when they were written: the last in
    // order is the newest.
    let mut indexes: Vec<PathBuf> = fs::read_dir(reply)
        .map_err(|_| unreadable("reply"))?
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .is_some_and(|name| name.to_string_lossy().starts_with("index-"))
        })
        .collect();
    indexes.sort();
    let index = read(indexes.last().ok_or_else(|| unreadable("index"))?)?;
    let model = index["reply"]["codemodel-v2"]["jsonFile"]
        .as_str()
        .ok_or_else(|| unreadable("code model"))?;
    let model = read(&reply.join(model))?;
    let targets = model["configurations"][0]["targets"]
        .as_array()
        .ok_or_else(|| unreadable("targets"))?;
    let file = targets
        .iter()
        .find(|target| target["name"] == "hgl_probe")
        .and_then(|target| target["jsonFile"].as_str())
        .ok_or_else(|| unreadable("program"))?;
    read(&reply.join(file))
}

/// The description a file-API target gives: its one group of compile
/// flags, defines and include directories, and its link's fragments.
fn description_of(target: &Value, compiler: &Path) -> Result<Description, String> {
    let missing = || "CMake's description of the build has no compile flags".to_string();
    let group = &target["compileGroups"][0];
    let mut compile: Vec<String> = Vec::new();
    for fragment in group["compileCommandFragments"]
        .as_array()
        .ok_or_else(missing)?
    {
        compile.extend(fragment["fragment"].as_str().map(str::to_string));
    }
    for define in group["defines"].as_array().into_iter().flatten() {
        if let Some(define) = define["define"].as_str() {
            compile.push(quoted(&format!("-D{define}")));
        }
    }
    for include in group["includes"].as_array().into_iter().flatten() {
        if let Some(path) = include["path"].as_str() {
            let flag = if include["isSystem"] == true {
                "-isystem"
            } else {
                "-I"
            };
            compile.push(format!("{flag} {}", quoted(path)));
        }
    }
    let link = target["link"]["commandFragments"]
        .as_array()
        .ok_or_else(|| "CMake's description of the build has no link".to_string())?
        .iter()
        .filter_map(|fragment| fragment["fragment"].as_str())
        .filter(|fragment| !fragment.is_empty())
        .map(str::to_string)
        .collect();
    Ok(Description {
        compiler: quoted(&compiler.display().to_string()),
        compile,
        link,
        header: String::new(),
        libraries: Vec::new(),
    })
}

/// A description as `prepare` wrote it, or `None` for text that is not
/// one.
fn read_description(text: &str) -> Option<Description> {
    let json: Value = serde_json::from_str(text).ok()?;
    let strings = |value: &Value| -> Option<Vec<String>> {
        value
            .as_array()?
            .iter()
            .map(|item| item.as_str().map(str::to_string))
            .collect()
    };
    Some(Description {
        compiler: json["compiler"].as_str()?.to_string(),
        compile: strings(&json["compile"])?,
        link: strings(&json["link"])?,
        header: json["header"].as_str()?.to_string(),
        libraries: strings(&json["libraries"])?,
    })
}

#[cfg(test)]
mod tests {
    use super::read_description;

    #[test]
    fn a_description_that_is_not_whole_is_none() {
        assert_eq!(read_description("{}"), None);
        assert_eq!(read_description("not json"), None);
    }
}
