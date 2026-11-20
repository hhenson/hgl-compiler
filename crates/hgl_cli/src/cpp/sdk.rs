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
//! libraries the SDK was built against (fmt, spdlog and simdjson). The
//! compiler is of the platform's family (`docs/cpp-target.md`): GCC 15
//! or later on Linux, Apple clang 21 or later on macOS; the one `CXX`
//! names, or else the first of the family's on the `PATH`. What the
//! driver does differently for a family is said by [`Family`] alone.
//! `HGL_CPP_FLAGS` adds flags to every compile and link, the runtime
//! library's too: `-O2`, `-g`, or the sanitizers.
//!
//! What is made for a toolchain is kept under the cache, in a directory
//! named by a hash of everything it depends on: the SDK's and the
//! dependencies' paths and the SDK's own files, the compiler, its family
//! and version and what it reads from the environment, the linker, the
//! flags, and the runtime library's sources.
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

use std::ffi::OsString;
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

/// The file a prepared toolchain's description is kept in. It is
/// written last: a directory that holds it is ready.
const DESCRIPTION: &str = "description.json";

/// A version of what is made for a toolchain: raised when what this
/// driver makes changes, so that what an older `hgl` made is not used.
const FORMAT: u32 = 4;

/// The family of compilers a platform builds the cpp target with
/// (`docs/cpp-target.md`): GCC on Linux, Apple clang on macOS. Each way
/// the driver differs between them is one of its methods, and is said
/// nowhere else.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(super) enum Family {
    /// GCC, with its libstdc++: the compiler on Linux.
    Gcc,
    /// Apple clang, with the system's libc++: the compiler on macOS.
    AppleClang,
}

impl Family {
    /// The family of the platform `hgl` was built for.
    fn of_platform() -> Family {
        if cfg!(target_os = "macos") {
            Family::AppleClang
        } else {
            Family::Gcc
        }
    }

    /// The family's name, as a message says it.
    fn name(self) -> &'static str {
        match self {
            Family::Gcc => "GCC",
            Family::AppleClang => "Apple clang",
        }
    }

    /// The family's word in a description's JSON.
    fn word(self) -> &'static str {
        match self {
            Family::Gcc => "gcc",
            Family::AppleClang => "apple-clang",
        }
    }

    /// The family a description's JSON names by [`Family::word`].
    fn from_word(word: &str) -> Option<Family> {
        [Family::Gcc, Family::AppleClang]
            .into_iter()
            .find(|family| family.word() == word)
    }

    /// The oldest major version the target is built with: GCC 15, whose
    /// libstdc++ the SDK's libraries need; Apple clang 21, the oldest
    /// the SDK has been built and tested with.
    fn oldest(self) -> u32 {
        match self {
            Family::Gcc => 15,
            Family::AppleClang => 21,
        }
    }

    /// The compilers looked for on the `PATH` when `CXX` names none, the
    /// newest first.
    fn candidates(self) -> &'static [&'static str] {
        match self {
            Family::Gcc => &["g++-17", "g++-16", "g++-15", "g++"],
            Family::AppleClang => &["clang++"],
        }
    }

    /// The family and version that a compiler's predefined macros, as
    /// `-dM -E` prints them, say it is: `15.2.0` for GCC; for Apple clang
    /// its version and Apple's build, `21.0.0 (21000401)`. `None` for a
    /// compiler of neither family: clang defines `__GNUC__` too, and an
    /// upstream clang is not Apple's.
    fn identify(defines: &str) -> Option<(Family, String)> {
        let value = |name: &str| {
            defines.lines().find_map(|line| {
                let rest = line.strip_prefix("#define ")?.strip_prefix(name)?;
                rest.strip_prefix(' ').map(str::trim)
            })
        };
        let version = |major: &str, minor: &str, patch: &str| {
            Some(format!(
                "{}.{}.{}",
                value(major)?,
                value(minor)?,
                value(patch)?
            ))
        };
        if let Some(build) = value("__apple_build_version__") {
            let version = version("__clang_major__", "__clang_minor__", "__clang_patchlevel__")?;
            return Some((Family::AppleClang, format!("{version} ({build})")));
        }
        if value("__clang__").is_none() && value("__GNUC__").is_some() {
            let version = version("__GNUC__", "__GNUC_MINOR__", "__GNUC_PATCHLEVEL__")?;
            return Some((Family::Gcc, version));
        }
        None
    }

    /// The variables of the environment a compiler of the family reads,
    /// which change what it builds, and so the toolchain: Apple clang,
    /// and CMake for it, take the oldest macOS to build for and the SDK
    /// from them.
    fn environment(self) -> &'static [&'static str] {
        match self {
            Family::Gcc => &[],
            Family::AppleClang => &["MACOSX_DEPLOYMENT_TARGET", "SDKROOT"],
        }
    }

    /// True where the family links with `ld.lld` when it is on the
    /// `PATH`: GCC, whose own `ld` is slower. Apple's linker is the one
    /// macOS builds with, and as fast.
    fn links_with_lld(self) -> bool {
        self == Family::Gcc
    }

    /// The flags a link takes beyond those CMake describes.
    fn link_flags(self, lld: bool) -> Vec<String> {
        match self {
            // An `RPATH` of the program's, which the loader reads for
            // what it loads where that has no `RUNPATH` of its own, so
            // that an executable run by hand finds what it can.
            Family::Gcc => {
                let mut flags = vec!["-Wl,--disable-new-dtags".to_string()];
                if lld {
                    flags.push("-fuse-ld=lld".to_string());
                }
                flags
            }
            // None: macOS's loader resolves a library's `@rpath` through
            // the run paths of every image on the way to it, the
            // executable's among them, which CMake's `-rpath` flags give.
            Family::AppleClang => Vec::new(),
        }
    }

    /// The extension of a precompiled header beside the header, which
    /// `-include` of the header reads in its place: GCC's `.gch`,
    /// clang's `.pch`.
    fn precompiled_extension(self) -> &'static str {
        match self {
            Family::Gcc => "h.gch",
            Family::AppleClang => "h.pch",
        }
    }

    /// The variable the platform's loader reads directories of
    /// libraries from, which an executable is run with: `LD_LIBRARY_PATH`
    /// on Linux, where the runtime library and the SDK's own find what
    /// they need by their own `RUNPATH`s, which do not name the
    /// dependencies' directory, and a `RUNPATH` stops the loader reading
    /// the executable's `RPATH` for what they load. None on macOS, where
    /// the run paths find everything, and where `DYLD_LIBRARY_PATH`
    /// naming the dependency prefix would load conda-forge's libc++ in
    /// place of the system's.
    fn library_variable(self) -> Option<&'static str> {
        match self {
            Family::Gcc => Some("LD_LIBRARY_PATH"),
            Family::AppleClang => None,
        }
    }
}

/// Everything the cpp target builds with, as it was found.
#[derive(Debug, Clone)]
pub(super) struct Toolchain {
    /// The SDK's install prefix.
    sdk: PathBuf,
    /// The dependency prefix.
    deps: PathBuf,
    /// The C++ compiler.
    compiler: PathBuf,
    /// Its family, the platform's.
    family: Family,
    /// Its version, as [`Family::identify`] gives it.
    version: String,
    /// `ninja`.
    pub(super) ninja: PathBuf,
    /// True where the family links with `ld.lld` and it is on the
    /// `PATH`, and links are made with it.
    lld: bool,
    /// What `HGL_CPP_FLAGS` adds.
    flags: Vec<String>,
}

/// How a generated unit is compiled and an executable linked, for one
/// toolchain: what `prepare` works out once and keeps.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Description {
    /// The compiler's family.
    pub(super) family: Family,
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
    /// is run with them in the family's [`Family::library_variable`],
    /// where it has one ([`library_path`]).
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
        let family = Family::of_platform();
        let (compiler, version) = compiler(family)?;
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
            family,
            version,
            ninja,
            lld: family.links_with_lld() && which::which("ld.lld").is_ok(),
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
        self.family.hash(&mut hash);
        self.version.hash(&mut hash);
        for variable in self.family.environment() {
            std::env::var_os(variable).hash(&mut hash);
        }
        self.lld.hash(&mut hash);
        self.flags.hash(&mut hash);
        // The SDK's own files: one rebuilt in place is another SDK. A
        // shared library is `.so` on Linux, `.dylib` on macOS.
        let suffix = std::env::consts::DLL_SUFFIX;
        for file in [
            "lib/cmake/hgraph/hgraphConfigVersion.cmake".to_string(),
            format!("lib/libhgraph_runtime{suffix}"),
            format!("lib/libhgraph_wiring{suffix}"),
        ] {
            if let Ok(metadata) = fs::metadata(self.sdk.join(&file)) {
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
            "family": description.family.word(),
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
        let mut description = description_of(&target, self.family, &self.compiler)?;
        description.link.extend(self.family.link_flags(self.lld));
        description.libraries = [
            runtime.join("lib"),
            self.sdk.join("lib"),
            self.deps.join("lib"),
        ]
        .iter()
        .map(|directory| directory.display().to_string())
        .collect();
        Ok(description)
    }

    /// Precompiles the runtime library's header with `description`'s
    /// flags, and gives the header units are compiled with: the compiler
    /// takes the precompiled form beside it in its place, `hgl_pch.h.gch`
    /// for GCC and `hgl_pch.h.pch` for clang.
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
            quoted(
                &header
                    .with_extension(self.family.precompiled_extension())
                    .display()
                    .to_string()
            )
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
  IMPORTED_LOCATION "${HGL_RUNTIME}/lib/${CMAKE_SHARED_LIBRARY_PREFIX}hglrt${CMAKE_SHARED_LIBRARY_SUFFIX}"
  INTERFACE_INCLUDE_DIRECTORIES "${HGL_RUNTIME}/include"
  INTERFACE_LINK_LIBRARIES "hgraph::runtime;hgraph::wiring")
add_executable(hgl_probe probe.cpp)
target_compile_features(hgl_probe PRIVATE cxx_std_23)
# Strict C++23, not GNU's dialect: generated code is written for any
# platform's compiler and headers.
set_target_properties(hgl_probe PROPERTIES CXX_EXTENSIONS OFF)
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

/// The compiler `CXX` names, or the first of `family`'s candidates on
/// the `PATH`, with its version; or why there is none of the family at
/// its oldest version or later.
fn compiler(family: Family) -> Result<(PathBuf, String), String> {
    let named = std::env::var_os("CXX").filter(|named| !named.is_empty());
    let candidates: Vec<PathBuf> = match &named {
        Some(named) => vec![which::which(named).map_err(|_| {
            format!(
                "CXX names {}, which is not a program",
                PathBuf::from(named).display()
            )
        })?],
        None => family
            .candidates()
            .iter()
            .filter_map(|name| which::which(name).ok())
            .collect(),
    };
    let mut seen = Vec::new();
    for candidate in candidates {
        // The macros it defines say which compiler it is, whatever its
        // name: `g++` is Apple clang on macOS.
        let found = Command::new(&candidate)
            .args(["-dM", "-E", "-x", "c++", "/dev/null"])
            .output()
            .ok()
            .filter(|output| output.status.success())
            .and_then(|output| Family::identify(&String::from_utf8_lossy(&output.stdout)));
        let major = |version: &str| {
            version
                .split('.')
                .next()
                .and_then(|major| major.parse::<u32>().ok())
        };
        match found {
            // The path as found, and not where a link leads: a wrapper
            // such as `ccache` linked as `g++` knows the compiler by it.
            Some((of, version))
                if of == family
                    && major(&version).is_some_and(|major| major >= family.oldest()) =>
            {
                return Ok((candidate, version));
            }
            Some((of, version)) => {
                seen.push(format!("{} ({} {version})", candidate.display(), of.name()))
            }
            None => seen.push(format!(
                "{} (neither GCC nor Apple clang)",
                candidate.display()
            )),
        }
    }
    let found = if seen.is_empty() {
        "none was found".to_string()
    } else {
        format!("found {}", seen.join(", "))
    };
    Err(format!(
        "the cpp target needs {} {} or later here, named by CXX or on the PATH as one of {}: \
         {found}",
        family.name(),
        family.oldest(),
        family.candidates().join(", ")
    ))
}

/// What an executable built with `description` is run with so that it
/// finds its libraries: the family's [`Family::library_variable`], and
/// as its value the description's directories before those it already
/// names. `None` where the family has no such variable.
pub(super) fn library_path(description: &Description) -> Option<(&'static str, OsString)> {
    let variable = description.family.library_variable()?;
    let inherited = std::env::var_os(variable).unwrap_or_default();
    let paths = description
        .libraries
        .iter()
        .map(PathBuf::from)
        .chain(std::env::split_paths(&inherited));
    // A directory with a `:` in it cannot be named in the list; it is
    // then left to the executable's run path.
    Some((variable, std::env::join_paths(paths).unwrap_or(inherited)))
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
fn description_of(target: &Value, family: Family, compiler: &Path) -> Result<Description, String> {
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
        family,
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
        family: Family::from_word(json["family"].as_str()?)?,
        compiler: json["compiler"].as_str()?.to_string(),
        compile: strings(&json["compile"])?,
        link: strings(&json["link"])?,
        header: json["header"].as_str()?.to_string(),
        libraries: strings(&json["libraries"])?,
    })
}

#[cfg(test)]
mod tests {
    use super::{Family, read_description};

    /// Some of what GCC 15.2 defines, as `-dM -E` prints it.
    const GCC: &str = "#define __cplusplus 201703L\n#define __GNUC__ 15\n\
                       #define __GNUC_MINOR__ 2\n#define __GNUC_PATCHLEVEL__ 0\n";

    /// Some of what Apple clang 21 defines: `__GNUC__` too.
    const APPLE_CLANG: &str = "#define __GNUC__ 4\n#define __GNUC_MINOR__ 2\n\
                               #define __GNUC_PATCHLEVEL__ 1\n#define __apple_build_version__ 21000401\n\
                               #define __clang__ 1\n#define __clang_major__ 21\n\
                               #define __clang_minor__ 0\n#define __clang_patchlevel__ 0\n";

    /// Some of what an upstream clang defines: no Apple build.
    const CLANG: &str = "#define __GNUC__ 4\n#define __GNUC_MINOR__ 2\n\
                         #define __GNUC_PATCHLEVEL__ 1\n#define __clang__ 1\n\
                         #define __clang_major__ 20\n#define __clang_minor__ 1\n\
                         #define __clang_patchlevel__ 8\n";

    #[test]
    fn a_description_that_is_not_whole_is_none() {
        assert_eq!(read_description("{}"), None);
        assert_eq!(read_description("not json"), None);
    }

    #[test]
    fn a_compiler_is_known_by_the_macros_it_defines() {
        assert_eq!(
            Family::identify(GCC),
            Some((Family::Gcc, "15.2.0".to_string()))
        );
        assert_eq!(
            Family::identify(APPLE_CLANG),
            Some((Family::AppleClang, "21.0.0 (21000401)".to_string()))
        );
        assert_eq!(Family::identify(CLANG), None);
        assert_eq!(Family::identify(""), None);
    }

    #[test]
    fn a_family_is_written_and_read_by_its_word() {
        for family in [Family::Gcc, Family::AppleClang] {
            assert_eq!(Family::from_word(family.word()), Some(family));
        }
        assert_eq!(Family::from_word("msvc"), None);
    }
}
