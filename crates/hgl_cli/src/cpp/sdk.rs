//! The C++ toolchain the cpp target builds with: hgraph's C++ SDK and
//! its dependencies, the compiler, `ninja`, `cmake`, and a fast linker
//! where there is one; and what is made once for each of them and kept:
//! the runtime library, a description of how a generated unit is
//! compiled and an executable linked, and a precompiled header.
//!
//! `HGL_HGRAPH_SDK` and `HGL_HGRAPH_DEPS` name the SDK's install
//! prefix and its dependency prefix. Either can also come from the
//! release's `libexec` alongside the resolved executable. A debug build
//! links hgraph's libraries built for debugging where they are given
//! ([`choose`]): those `HGL_HGRAPH_DEBUG` names, or those the optional
//! `hgl-debug` package installs in the release's `libexec`. The
//! compiler is of the platform's family (`docs/cpp-target.md`): GCC 15
//! or later on Linux, Apple clang 21 or later on macOS; the one `CXX`
//! names, or else the first of the family's on the `PATH`. What the
//! driver does differently for a family is said by [`Family`] alone.
//! `HGL_CPP_FLAGS` adds flags to every compile and link, the runtime
//! library's too: `-O2`, `-g`, or the sanitizers.
//!
//! What is made for a toolchain is kept under the cache, in a directory
//! named by a hash of everything it depends on: which build of hgraph's
//! libraries it links, the SDK's and the
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
use super::stamp::file_digest;

mod package;

/// The variable that names hgraph's C++ SDK.
pub(super) const SDK: &str = "HGL_HGRAPH_SDK";

/// The variable that names the SDK's dependency prefix.
pub(super) const DEPS: &str = "HGL_HGRAPH_DEPS";

/// The variable that names hgraph's libraries built for debugging, which
/// a debug build links: a directory laid out as a release's `libexec`,
/// with the SDK in `hgraph-sdk` and its dependency prefix in
/// `hgraph-deps`.
pub(super) const DEBUG: &str = "HGL_HGRAPH_DEBUG";

/// The directory of a release's `libexec` that the `hgl-debug` package
/// installs hgraph's libraries built for debugging in, laid out as
/// [`DEBUG`] names one.
const BUNDLED_DEBUG: &str = "hgraph-debug";

/// What a directory of hgraph's libraries holds, as [`DEBUG`] names one
/// and [`BUNDLED_DEBUG`] is: the SDK's CMake package and the dependency
/// prefix's two directories.
const DEBUG_HOLDS: [&str; 3] = [
    "hgraph-sdk/lib/cmake/hgraph/hgraphConfig.cmake",
    "hgraph-deps/include",
    "hgraph-deps/lib",
];

/// The hgraph tag this `hgl` writes C++ for (`HGRAPH_TAG`).
const HGRAPH_TAG: &str = env!("HGL_HGRAPH_TAG");

/// The variable whose flags are added to every compile and link.
pub(super) const FLAGS: &str = "HGL_CPP_FLAGS";

/// The file a prepared toolchain's description is kept in. It is
/// written last: a directory that holds it is ready.
const DESCRIPTION: &str = "description.json";

/// The flags that confine every CMake configure of a toolchain to the SDK
/// and the dependency prefix it names (`CMAKE_PREFIX_PATH`): no package is
/// looked for through `PATH`, the environment, the system's prefixes or a
/// package registry, and hgraph's package does not fetch one it misses.
/// One program can then never link fmt, spdlog or simdjson twice, the
/// prefix's and another's.
const CONFINED: [&str; 7] = [
    "-DCMAKE_FIND_USE_PACKAGE_ROOT_PATH=OFF",
    "-DCMAKE_FIND_USE_CMAKE_ENVIRONMENT_PATH=OFF",
    "-DCMAKE_FIND_USE_SYSTEM_ENVIRONMENT_PATH=OFF",
    "-DCMAKE_FIND_USE_CMAKE_SYSTEM_PATH=OFF",
    "-DCMAKE_FIND_USE_PACKAGE_REGISTRY=OFF",
    "-DCMAKE_FIND_USE_SYSTEM_PACKAGE_REGISTRY=OFF",
    "-DHGRAPH_FETCH_MISSING_DEPENDENCIES=OFF",
];

/// The variables of the environment CMake reads for itself in finding
/// packages, choosing its generator, compilers and flags, and placing what
/// it installs, which are taken out of the environment of every CMake
/// `hgl` runs ([`cmake_command`]): what they would change is given on the command
/// line, or is not to change at all, and the toolchain's hash holds none
/// of them. `CMAKE_FIND_ROOT_PATH` and `CMAKE_MODULE_PATH` are no
/// variables of CMake's environment, and are taken out all the same. Kept:
/// `PATH`, which finds no package ([`CONFINED`]); `MACOSX_DEPLOYMENT_TARGET`
/// and `SDKROOT`, which the hash holds ([`Family::environment`]); and
/// `CMAKE_BUILD_PARALLEL_LEVEL`, how many jobs, which changes nothing made.
const CMAKE_READS: [&str; 33] = [
    "CMAKE_TOOLCHAIN_FILE",
    "CMAKE_PREFIX_PATH",
    "CMAKE_FRAMEWORK_PATH",
    "CMAKE_APPBUNDLE_PATH",
    "CMAKE_INCLUDE_PATH",
    "CMAKE_LIBRARY_PATH",
    "CMAKE_PROGRAM_PATH",
    "CMAKE_FIND_ROOT_PATH",
    "CMAKE_MODULE_PATH",
    "CMAKE_GENERATOR",
    "CMAKE_GENERATOR_INSTANCE",
    "CMAKE_GENERATOR_PLATFORM",
    "CMAKE_GENERATOR_TOOLSET",
    "CMAKE_BUILD_TYPE",
    "CMAKE_CONFIGURATION_TYPES",
    "CMAKE_CONFIG_TYPE",
    "CMAKE_CROSSCOMPILING_EMULATOR",
    "CMAKE_C_COMPILER_LAUNCHER",
    "CMAKE_CXX_COMPILER_LAUNCHER",
    "CMAKE_C_LINKER_LAUNCHER",
    "CMAKE_CXX_LINKER_LAUNCHER",
    "CMAKE_CXX_IMPLICIT_LINK_DIRECTORIES_EXCLUDE",
    "CMAKE_OSX_ARCHITECTURES",
    "CMAKE_APPLE_SILICON_PROCESSOR",
    "CMAKE_INSTALL_PREFIX",
    "CMAKE_INSTALL_MODE",
    "CMAKE_POLICY_VERSION_MINIMUM",
    "DESTDIR",
    "CC",
    "CXX",
    "CFLAGS",
    "CXXFLAGS",
    "LDFLAGS",
];

/// The packages a configure of a toolchain finds, whose `<Package>_DIR`,
/// `<Package>_ROOT` and `<PACKAGE>_ROOT` CMake reads from the environment
/// too, and which are taken out with [`CMAKE_READS`].
const FOUND_PACKAGES: [&str; 6] = ["hgraph", "fmt", "spdlog", "simdjson", "Threads", "doctest"];

/// `cmake`, to be run with none of what [`CMAKE_READS`] and
/// [`FOUND_PACKAGES`] name in its environment.
fn cmake_command(program: &Path) -> Command {
    let mut command = Command::new(program);
    for variable in CMAKE_READS {
        command.env_remove(variable);
    }
    for package in FOUND_PACKAGES {
        let upper = package.to_uppercase();
        for variable in [
            format!("{package}_DIR"),
            format!("{package}_ROOT"),
            format!("{upper}_DIR"),
            format!("{upper}_ROOT"),
        ] {
            command.env_remove(variable);
        }
    }
    command
}

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

/// Which build of hgraph's SDK and of its dependencies a toolchain links.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(super) enum Build {
    /// The build a release ships: optimised, with no debug information.
    Optimised,
    /// The build for debugging: no optimisation, debug information.
    Debug,
}

impl Build {
    /// The build's word, as a message and a description's JSON say it.
    pub(super) fn word(self) -> &'static str {
        match self {
            Build::Optimised => "optimised",
            Build::Debug => "debug",
        }
    }
}

/// hgraph's SDK and its dependency prefix, as [`choose`] chose them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Hgraph {
    /// Which build they are.
    pub(super) build: Build,
    /// The SDK's install prefix.
    pub(super) sdk: PathBuf,
    /// The dependency prefix.
    pub(super) deps: PathBuf,
    /// Where they were found and, for a debug build that links the
    /// optimised ones, why: what `hgl` prints of a debug build.
    pub(super) found: String,
}

impl Hgraph {
    /// What `hgl` prints of the libraries a debug build links.
    pub(super) fn said(&self) -> String {
        format!(
            "a debug build, with hgraph's {} libraries {}",
            self.build.word(),
            self.found
        )
    }

    /// The libraries, as a debug build's JSON description gives them.
    pub(super) fn json(&self) -> Value {
        serde_json::json!({
            "libraries": self.build.word(),
            "sdk": self.sdk.display().to_string(),
            "dependencies": self.deps.display().to_string(),
        })
    }
}

/// Everything the cpp target builds with, as it was found.
#[derive(Debug, Clone)]
pub(super) struct Toolchain {
    /// hgraph's SDK and its dependencies.
    hgraph: Hgraph,
    /// The C++ compiler.
    compiler: PathBuf,
    /// Its family, the platform's.
    family: Family,
    /// Its version, as [`Family::identify`] gives it.
    version: String,
    /// `ninja`.
    ninja: Option<PathBuf>,
    /// True where the family links with `ld.lld` and it is on the
    /// `PATH`, and links are made with it.
    lld: bool,
    /// What `HGL_CPP_FLAGS` adds, and for a debug build `-O0 -g`.
    flags: Vec<String>,
    /// True for a debug build: generated units and the runtime library
    /// are built for debugging, whichever build of hgraph's libraries
    /// [`choose`] gave.
    debug: bool,
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
    /// The configured or bundled toolchain of an ordinary build, or why
    /// it could not be used.
    pub(super) fn find() -> Result<Toolchain, String> {
        Toolchain::with(chosen(false)?)
    }

    /// The toolchain of a debug build: hgraph's libraries built for
    /// debugging where they are given ([`choose`]); generated units and
    /// their precompiled header built with matching symbols and no
    /// optimisation; and the runtime library built as CMake's `Debug`.
    /// The libraries, the flags and the debug build distinguish the
    /// toolchain's cache.
    pub(super) fn find_debug() -> Result<Toolchain, String> {
        let mut toolchain = Toolchain::with(chosen(true)?)?;
        toolchain
            .flags
            .extend(["-O0".to_string(), "-g".to_string()]);
        toolchain.debug = true;
        Ok(toolchain)
    }

    /// hgraph's libraries this toolchain links.
    pub(super) fn hgraph(&self) -> &Hgraph {
        &self.hgraph
    }

    /// The toolchain that links `hgraph`, with the compiler and the
    /// tools found as the environment says.
    fn with(hgraph: Hgraph) -> Result<Toolchain, String> {
        let family = Family::of_platform();
        let (compiler, version) = compiler(family)?;
        let ninja = which::which("ninja").ok();
        let flags = std::env::var(FLAGS)
            .unwrap_or_default()
            .split_whitespace()
            .map(str::to_string)
            .collect();
        Ok(Toolchain {
            hgraph,
            compiler,
            family,
            version,
            ninja,
            lld: family.links_with_lld() && which::which("ld.lld").is_ok(),
            flags,
            debug: false,
        })
    }

    /// Ninja is needed on a build miss, after reuse has been considered.
    pub(super) fn ninja(&self) -> Result<&Path, String> {
        self.ninja.as_deref().ok_or_else(|| {
            "`ninja` is not on the PATH: the cpp target builds with it (https://ninja-build.org)"
                .to_string()
        })
    }

    /// The name of the directory what is made for this toolchain is
    /// kept in: a hash of all it depends on. The hash is the standard
    /// library's, which may change with Rust's release: a cache made by
    /// an `hgl` built with another is then made again.
    pub(super) fn key(&self) -> String {
        let mut hash = DefaultHasher::new();
        FORMAT.hash(&mut hash);
        self.hgraph.build.hash(&mut hash);
        for path in [&self.hgraph.sdk, &self.hgraph.deps, &self.compiler] {
            path.hash(&mut hash);
        }
        self.family.hash(&mut hash);
        self.version.hash(&mut hash);
        for variable in self.family.environment() {
            std::env::var_os(variable).hash(&mut hash);
        }
        self.lld.hash(&mut hash);
        self.flags.hash(&mut hash);
        self.debug.hash(&mut hash);
        // The SDK's own files: one rebuilt in place is another SDK. A
        // shared library is `.so` on Linux, `.dylib` on macOS.
        let suffix = std::env::consts::DLL_SUFFIX;
        for file in [
            "lib/cmake/hgraph/hgraphConfigVersion.cmake".to_string(),
            format!("lib/libhgraph_runtime{suffix}"),
            format!("lib/libhgraph_wiring{suffix}"),
        ] {
            let digest = file_digest(&self.hgraph.sdk.join(file)).ok();
            digest.as_ref().map(blake3::Hash::as_bytes).hash(&mut hash);
        }
        hgl_cpp::runtime::FILES.hash(&mut hash);
        format!("{:016x}", hash.finish())
    }

    /// The description of this toolchain, made under `cache` the first
    /// time it is asked for and read from there after (see the head of
    /// this file). `jobs` is how many jobs a build may run at once, as
    /// `ninja -j` takes it; `None` for the build's own choice.
    pub(super) fn prepare(
        &self,
        cache: &Path,
        key: &str,
        jobs: Option<&str>,
    ) -> Result<Description, String> {
        if !hgl_cpp::runtime::available() {
            return Err(
                "this hgl was built without the C++ runtime library: `runtime/cpp` was not in \
                 its source tree"
                    .to_string(),
            );
        }
        let directory = cache.join("toolchains").join(key);
        fs::create_dir_all(&directory)
            .map_err(|error| format!("{} could not be made: {error}", directory.display()))?;
        // Two runs that both find the toolchain unprepared make it once.
        let _held = lock(&directory)?;
        let described = directory.join(DESCRIPTION);
        if let Some(description) = self.prepared(cache, key) {
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

    /// Reads a toolchain that has already been prepared. Its description
    /// is published by an atomic rename after preparation succeeds.
    pub(super) fn prepared(&self, cache: &Path, key: &str) -> Option<Description> {
        let text = fs::read_to_string(cache.join("toolchains").join(key).join(DESCRIPTION)).ok()?;
        let description = read_description(&text)?;
        let pch =
            Path::new(&description.header).with_extension(self.family.precompiled_extension());
        pch.is_file().then_some(description)
    }

    /// The flags every CMake configure of this toolchain is given.
    fn configure_flags(&self) -> Result<Vec<String>, String> {
        let mut flags = vec![
            "-G".to_string(),
            "Ninja".to_string(),
            format!("-DCMAKE_MAKE_PROGRAM={}", self.ninja()?.display()),
            format!("-DCMAKE_CXX_COMPILER={}", self.compiler.display()),
            // The flags are these and no others: CMake would otherwise
            // take `CXXFLAGS` and `LDFLAGS` from the environment, which
            // the toolchain's hash does not hold.
            format!("-DCMAKE_CXX_FLAGS={}", self.flags.join(" ")),
            "-DCMAKE_EXE_LINKER_FLAGS=".to_string(),
            "-DCMAKE_SHARED_LINKER_FLAGS=".to_string(),
            format!("-D{SDK}={}", self.hgraph.sdk.display()),
            format!("-D{DEPS}={}", self.hgraph.deps.display()),
        ];
        flags.extend(CONFINED.iter().map(|flag| flag.to_string()));
        Ok(flags)
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
        let mut configure = cmake_command(cmake);
        configure
            .arg("-S")
            .arg(&source)
            .arg("-B")
            .arg(&build)
            .args(self.configure_flags()?)
            // A debug build's runtime library is built for debugging: a
            // Release build's own `-O3 -DNDEBUG` would follow `-O0 -g`.
            .arg(format!(
                "-DCMAKE_BUILD_TYPE={}",
                if self.debug { "Debug" } else { "Release" }
            ))
            .arg(format!("-DCMAKE_INSTALL_PREFIX={}", installed.display()))
            // The library is installed in `lib` on every system, where the
            // description's program finds it.
            .arg("-DCMAKE_INSTALL_LIBDIR=lib")
            .arg("-DBUILD_TESTING=OFF");
        run("the runtime library's configuration", &mut configure)?;
        let mut built = cmake_command(cmake);
        built.arg("--build").arg(&build);
        if let Some(jobs) = jobs {
            built.arg("--parallel").arg(jobs);
        }
        run("the runtime library's build", &mut built)?;
        run(
            "the runtime library's installation",
            cmake_command(cmake).arg("--install").arg(&build),
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
        let mut configure = cmake_command(cmake);
        configure
            .arg("-S")
            .arg(&source)
            .arg("-B")
            .arg(&build)
            .args(self.configure_flags()?)
            .arg(format!("-DHGL_RUNTIME={}", runtime.display()));
        run("the description of a build against the SDK", &mut configure)?;
        let target = probe_target(&build.join(".cmake/api/v1/reply"))?;
        let mut description = description_of(&target, self.family, &self.compiler)?;
        description.link.extend(self.family.link_flags(self.lld));
        description.libraries = [
            runtime.join("lib"),
            self.hgraph.sdk.join("lib"),
            self.hgraph.deps.join("lib"),
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

/// What [`SDK`] names, as a message says it.
const WHAT_SDK: &str =
    "the install prefix of hgraph's C++ SDK, the directory that holds `lib/cmake/hgraph`";

/// What [`DEPS`] names, as a message says it.
const WHAT_DEPS: &str = "the prefix of the libraries hgraph's C++ SDK was built against, which holds `include` and `lib`";

/// What [`DEBUG`] names, as a message says it.
const WHAT_DEBUG: &str = "the directory of hgraph's libraries built for debugging, which holds the SDK in `hgraph-sdk` and its dependencies in `hgraph-deps`";

/// The variables of the environment, read by name: what [`choose`]
/// reads them through, so that a test can give its own.
type Variables<'a> = &'a dyn Fn(&str) -> Option<OsString>;

/// The directory a release unpacks to, which the resolved executable is
/// in as `bin/hgl`; `None` where the executable cannot be resolved.
fn bundle() -> Option<PathBuf> {
    let executable = fs::canonicalize(std::env::current_exe().ok()?).ok()?;
    Some(executable.parent()?.parent()?.to_path_buf())
}

/// [`choose`] with this process's environment and the release this
/// executable is in.
fn chosen(debug: bool) -> Result<Hgraph, String> {
    choose(debug, &|name| std::env::var_os(name), bundle().as_deref())
}

/// hgraph's SDK and dependency prefix that a build links, with the
/// environment's `variables` and the release unpacked in `bundle`.
///
/// An ordinary build links the optimised libraries, always: those
/// `HGL_HGRAPH_SDK` and `HGL_HGRAPH_DEPS` name, or else the release's
/// own. A debug build links the libraries built for debugging where they
/// are given, and the optimised ones otherwise:
///
/// 1. those `HGL_HGRAPH_DEBUG` names, which must be whole and built for
///    debugging (`package::usable`), or it is an error;
/// 2. else, where neither `HGL_HGRAPH_SDK` nor `HGL_HGRAPH_DEPS` names
///    libraries of the user's own, those the `hgl-debug` package
///    installs in the release's `libexec/hgraph-debug`, where they are
///    whole, built for debugging and of the hgraph tag this `hgl` writes
///    for;
/// 3. else the optimised ones, saying why.
fn choose(debug: bool, variables: Variables, bundle: Option<&Path>) -> Result<Hgraph, String> {
    let libexec = bundle.map(|bundle| bundle.join("libexec"));
    let optimised = |why: String| -> Result<Hgraph, String> {
        let sdk = prefix(
            SDK,
            variables(SDK),
            libexec.as_deref(),
            "hgraph-sdk",
            WHAT_SDK,
            &["lib/cmake/hgraph/hgraphConfig.cmake"],
        )?;
        let deps = prefix(
            DEPS,
            variables(DEPS),
            libexec.as_deref(),
            "hgraph-deps",
            WHAT_DEPS,
            &["include", "lib"],
        )?;
        Ok(Hgraph {
            build: Build::Optimised,
            found: format!("in {}{why}", sdk.display()),
            sdk,
            deps,
        })
    };
    if !debug {
        return optimised(String::new());
    }
    if let Some(named) = variables(DEBUG) {
        if named.is_empty() {
            return Err(format!(
                "{DEBUG} is empty: set it to {WHAT_DEBUG}, or unset it"
            ));
        }
        let directory = PathBuf::from(named);
        let notes = package::usable(&directory).map_err(|problem| {
            format!(
                "{DEBUG} names {}, which {problem}: set it to {WHAT_DEBUG}",
                directory.display()
            )
        })?;
        return debug_libraries(&directory, &format!("named by {DEBUG}"), &notes);
    }
    if let Some(own) = [SDK, DEPS]
        .into_iter()
        .find(|name| variables(name).is_some())
    {
        return optimised(format!(
            ": {own} names them, and {DEBUG} names no debug libraries"
        ));
    }
    let beside = libexec.as_ref().map(|libexec| libexec.join(BUNDLED_DEBUG));
    let Some(beside) = beside.filter(|beside| beside.exists()) else {
        return optimised(format!(
            ": the `hgl-debug` package is not installed beside hgl, and {DEBUG} is not set"
        ));
    };
    let notes = match package::usable(&beside) {
        Ok(notes) => notes,
        Err(problem) => {
            return optimised(format!(
                ": the `hgl-debug` package beside hgl, in {}, {problem}",
                beside.display()
            ));
        }
    };
    let tag = package::stamp(&beside.join("hgraph-sdk")).and_then(|stamp| stamp.tag);
    if tag.as_deref() != Some(HGRAPH_TAG) {
        return optimised(format!(
            ": those installed beside hgl, in {}, are hgraph {}'s, and this hgl writes for hgraph \
             {HGRAPH_TAG}",
            beside.display(),
            tag.as_deref().unwrap_or("no tag")
        ));
    }
    debug_libraries(&beside, "installed beside hgl", &notes)
}

/// The libraries built for debugging in `directory`, which
/// `package::usable` finds nothing wrong with, found as `how` says, with
/// what the check could not read of them, `notes`.
fn debug_libraries(directory: &Path, how: &str, notes: &[String]) -> Result<Hgraph, String> {
    let resolved = |path: PathBuf| {
        fs::canonicalize(&path).map_err(|error| format!("{}: {error}", path.display()))
    };
    let directory = resolved(directory.to_path_buf())?;
    Ok(Hgraph {
        build: Build::Debug,
        sdk: resolved(directory.join("hgraph-sdk"))?,
        deps: resolved(directory.join("hgraph-deps"))?,
        found: format!(
            "in {}, {how}{}",
            directory.display(),
            notes
                .iter()
                .map(|note| format!("; {note}"))
                .collect::<String>()
        ),
    })
}

/// The prefix `named`, the value of `variable`, names; or, where it is
/// not set, the one `bundled` names in the release's `libexec`. Each
/// file or directory of `holds` must be in it.
fn prefix(
    variable: &str,
    named: Option<OsString>,
    libexec: Option<&Path>,
    bundled: &str,
    what: &str,
    holds: &[&str],
) -> Result<PathBuf, String> {
    let valid = |path: &Path| holds.iter().all(|part| path.join(part).exists());
    let Some(named) = named else {
        return libexec
            .map(|libexec| libexec.join(bundled))
            .filter(|path| valid(path))
            .ok_or_else(|| {
                format!(
                    "{variable} is not set and the executable has no bundled `../libexec/{bundled}`: set it to {what}"
                )
            });
    };
    if named.is_empty() {
        return Err(format!(
            "{variable} is empty: set it to {what} or unset it to use the bundle"
        ));
    }
    let path = PathBuf::from(named);
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
    let inherited = std::env::var_os(variable).filter(|value| !value.is_empty());
    let paths = description.libraries.iter().map(PathBuf::from).chain(
        inherited
            .as_deref()
            .into_iter()
            .flat_map(std::env::split_paths),
    );
    // A directory with a `:` in it cannot be named in the list; it is
    // then left to the executable's run path.
    std::env::join_paths(paths)
        .ok()
        .or(inherited)
        .filter(|value| !value.is_empty())
        .map(|value| (variable, value))
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
    use std::ffi::OsString;
    use std::fs;
    use std::path::{Path, PathBuf};

    use super::{
        Build, CONFINED, Family, HGRAPH_TAG, Hgraph, Toolchain, choose, cmake_command,
        read_description,
    };

    /// The suffix of a shared library on this platform.
    const SUFFIX: &str = std::env::consts::DLL_SUFFIX;

    /// Lays out in `directory` an SDK in `hgraph-sdk`, stamped as an
    /// optimised build of `tag`, and a dependency prefix in `hgraph-deps`,
    /// as a release's `libexec` holds them: what the choice looks for of
    /// optimised libraries.
    fn lay_out(directory: &Path, tag: &str) {
        let package = directory.join("hgraph-sdk/lib/cmake/hgraph");
        fs::create_dir_all(&package).unwrap();
        fs::write(package.join("hgraphConfig.cmake"), "").unwrap();
        fs::write(
            directory.join("hgraph-sdk/hgl-sdk.txt"),
            format!("tag={tag}\n"),
        )
        .unwrap();
        for part in ["include", "lib"] {
            fs::create_dir_all(directory.join("hgraph-deps").join(part)).unwrap();
        }
    }

    /// A library at `path`: on a Unix, a link to this test's own
    /// executable, an object file whose headers parse whole.
    fn library(path: &Path) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(std::env::current_exe().unwrap(), path).unwrap();
        #[cfg(not(unix))]
        fs::write(path, "").unwrap();
    }

    /// Lays out in `directory` hgraph's libraries built for debugging, of
    /// `tag`, as the `hgl-debug` package installs them: the SDK, stamped
    /// as a Debug build, and the dependency prefix, each CMake package with
    /// the libraries it imports.
    fn lay_out_debug(directory: &Path, tag: &str) {
        lay_out(directory, tag);
        fs::write(
            directory.join("hgraph-sdk/hgl-sdk.txt"),
            format!("tag={tag}\nbuild-type=Debug\n"),
        )
        .unwrap();
        let packages: [(&str, &str, &str, &[&str]); 4] = [
            (
                "hgraph-sdk",
                "hgraph",
                "Config",
                &["libhgraph_runtime", "libhgraph_wiring"],
            ),
            ("hgraph-deps", "fmt", "-config", &["libfmtd"]),
            ("hgraph-deps", "spdlog", "Config", &["libspdlogd"]),
            ("hgraph-deps", "simdjson", "-config", &["libsimdjson"]),
        ];
        for (prefix, name, config, libraries) in packages {
            let package = directory.join(prefix).join("lib/cmake").join(name);
            fs::create_dir_all(&package).unwrap();
            fs::write(package.join(format!("{name}{config}.cmake")), "").unwrap();
            let mut targets = String::new();
            for file in libraries {
                library(&directory.join(prefix).join(format!("lib/{file}{SUFFIX}")));
                targets.push_str(&format!(
                    "list(APPEND _cmake_import_check_files_for_{name}::{name} \
                     \"${{_IMPORT_PREFIX}}/lib/{file}{SUFFIX}\" )\n"
                ));
            }
            fs::write(package.join(format!("{name}-targets-debug.cmake")), targets).unwrap();
        }
    }

    /// A release unpacked in a new directory, with its optimised
    /// libraries; and, where `debug` gives a tag, the `hgl-debug`
    /// package's libraries of that tag installed beside them.
    fn release(debug: Option<&str>) -> tempfile::TempDir {
        let bundle = tempfile::tempdir().unwrap();
        let libexec = bundle.path().join("libexec");
        lay_out(&libexec, HGRAPH_TAG);
        if let Some(tag) = debug {
            lay_out_debug(&libexec.join("hgraph-debug"), tag);
        }
        bundle
    }

    /// `path` resolved, as the choice gives its paths.
    fn real(path: PathBuf) -> PathBuf {
        fs::canonicalize(path).unwrap()
    }

    /// The choice with only `set` in the environment, and the release
    /// unpacked in `bundle`, resolved as [`super::bundle`] gives it.
    fn chosen_with(
        debug: bool,
        set: &[(&str, &Path)],
        bundle: Option<&Path>,
    ) -> Result<Hgraph, String> {
        let variables = |name: &str| {
            set.iter()
                .find(|(named, _)| *named == name)
                .map(|(_, value)| OsString::from(value))
        };
        let bundle = bundle.map(|bundle| real(bundle.to_path_buf()));
        choose(debug, &variables, bundle.as_deref())
    }

    /// The choice of a debug build of a release with the `hgl-debug`
    /// package beside it, after `damage` is done to the package: the
    /// optimised libraries, and why.
    fn passed_over(damage: impl Fn(&Path)) -> String {
        let bundle = release(Some(HGRAPH_TAG));
        let beside = bundle.path().join("libexec/hgraph-debug");
        damage(&beside);
        let hgraph = chosen_with(true, &[], Some(bundle.path())).unwrap();
        assert_eq!(hgraph.build, Build::Optimised, "{}", hgraph.said());
        assert_eq!(hgraph.sdk, real(bundle.path().join("libexec/hgraph-sdk")));
        let said = hgraph.said();
        let prefix = format!(
            "a debug build, with hgraph's optimised libraries in {}: the `hgl-debug` package beside \
             hgl, in {}, ",
            hgraph.sdk.display(),
            real(beside).display()
        );
        said.strip_prefix(&prefix)
            .unwrap_or_else(|| panic!("{said}"))
            .to_string()
    }

    #[test]
    fn an_ordinary_build_links_the_optimised_libraries_whatever_else_is_given() {
        let bundle = release(Some(HGRAPH_TAG));
        let named = tempfile::tempdir().unwrap();
        lay_out_debug(named.path(), HGRAPH_TAG);
        let hgraph = chosen_with(
            false,
            &[("HGL_HGRAPH_DEBUG", named.path())],
            Some(bundle.path()),
        )
        .unwrap();
        assert_eq!(hgraph.build, Build::Optimised);
        assert_eq!(hgraph.sdk, real(bundle.path().join("libexec/hgraph-sdk")));
        assert_eq!(hgraph.deps, real(bundle.path().join("libexec/hgraph-deps")));
    }

    #[cfg(unix)]
    #[test]
    fn a_debug_build_links_the_libraries_hgl_hgraph_debug_names() {
        let bundle = release(Some(HGRAPH_TAG));
        let named = tempfile::tempdir().unwrap();
        // Of another tag: the variable names what the user means.
        lay_out_debug(named.path(), "0.0.1");
        let own = release(None);
        let own_sdk = own.path().join("libexec/hgraph-sdk");
        let hgraph = chosen_with(
            true,
            &[
                ("HGL_HGRAPH_DEBUG", named.path()),
                ("HGL_HGRAPH_SDK", &own_sdk),
            ],
            Some(bundle.path()),
        )
        .unwrap();
        assert_eq!(hgraph.build, Build::Debug);
        assert_eq!(hgraph.sdk, real(named.path().join("hgraph-sdk")));
        assert_eq!(hgraph.deps, real(named.path().join("hgraph-deps")));
        assert_eq!(
            hgraph.said(),
            format!(
                "a debug build, with hgraph's debug libraries in {}, named by HGL_HGRAPH_DEBUG",
                real(named.path().to_path_buf()).display()
            )
        );
    }

    #[test]
    fn hgl_hgraph_debug_names_whole_libraries_built_for_debugging() {
        let bundle = release(Some(HGRAPH_TAG));
        let named = |damage: &dyn Fn(&Path)| {
            let directory = tempfile::tempdir().unwrap();
            lay_out_debug(directory.path(), HGRAPH_TAG);
            damage(directory.path());
            let reason = chosen_with(
                true,
                &[("HGL_HGRAPH_DEBUG", directory.path())],
                Some(bundle.path()),
            )
            .unwrap_err();
            let prefix = format!(
                "HGL_HGRAPH_DEBUG names {}, which ",
                directory.path().display()
            );
            let suffix = ": set it to the directory of hgraph's libraries built for debugging, \
                          which holds the SDK in `hgraph-sdk` and its dependencies in `hgraph-deps`";
            reason
                .strip_prefix(&prefix)
                .and_then(|rest| rest.strip_suffix(suffix))
                .unwrap_or_else(|| panic!("{reason}"))
                .to_string()
        };
        let empty = chosen_with(
            true,
            &[("HGL_HGRAPH_DEBUG", Path::new(""))],
            Some(bundle.path()),
        );
        assert!(
            empty
                .unwrap_err()
                .starts_with("HGL_HGRAPH_DEBUG is empty: ")
        );
        assert_eq!(
            named(&|directory| fs::remove_dir(directory.join("hgraph-deps/include")).unwrap()),
            "holds no `hgraph-deps/include`"
        );
        // The optimised libraries, named as debug ones.
        assert_eq!(
            named(&|directory| {
                fs::write(directory.join("hgraph-sdk/hgl-sdk.txt"), "tag=0.9.23\n").unwrap()
            }),
            "is not built for debugging: its `hgraph-sdk/hgl-sdk.txt` names no build type, as an \
             optimised build's does"
        );
        assert_eq!(
            named(&|directory| {
                fs::remove_file(directory.join(format!("hgraph-deps/lib/libspdlogd{SUFFIX}")))
                    .unwrap()
            }),
            format!(
                "lacks `hgraph-deps/lib/libspdlogd{SUFFIX}`, which its CMake package `spdlog` imports"
            )
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_debug_build_links_the_package_installed_beside_hgl() {
        let bundle = release(Some(HGRAPH_TAG));
        let hgraph = chosen_with(true, &[], Some(bundle.path())).unwrap();
        let beside = real(bundle.path().join("libexec/hgraph-debug"));
        assert_eq!(hgraph.build, Build::Debug);
        assert_eq!(hgraph.sdk, beside.join("hgraph-sdk"));
        assert_eq!(hgraph.deps, beside.join("hgraph-deps"));
        assert_eq!(
            hgraph.said(),
            format!(
                "a debug build, with hgraph's debug libraries in {}, installed beside hgl",
                beside.display()
            )
        );
    }

    #[test]
    fn a_debug_build_with_no_package_links_the_optimised_libraries_and_says_why() {
        let bundle = release(None);
        let hgraph = chosen_with(true, &[], Some(bundle.path())).unwrap();
        assert_eq!(hgraph.build, Build::Optimised);
        assert_eq!(
            hgraph.said(),
            format!(
                "a debug build, with hgraph's optimised libraries in {}: the `hgl-debug` package \
                 is not installed beside hgl, and HGL_HGRAPH_DEBUG is not set",
                real(bundle.path().join("libexec/hgraph-sdk")).display()
            )
        );
    }

    #[test]
    fn a_package_beside_hgl_laid_out_wrongly_says_what_is_wrong() {
        assert_eq!(
            passed_over(|beside| fs::remove_dir(beside.join("hgraph-deps/include")).unwrap()),
            "holds no `hgraph-deps/include`"
        );
        assert_eq!(
            passed_over(|beside| fs::remove_dir_all(beside.join("hgraph-sdk")).unwrap()),
            "holds no `hgraph-sdk/lib/cmake/hgraph/hgraphConfig.cmake`"
        );
    }

    #[test]
    fn a_package_beside_hgl_not_built_for_debugging_is_passed_over() {
        assert_eq!(
            passed_over(|beside| fs::remove_file(beside.join("hgraph-sdk/hgl-sdk.txt")).unwrap()),
            "holds no `hgraph-sdk/hgl-sdk.txt`, which says how the SDK was built"
        );
        assert_eq!(
            passed_over(|beside| {
                fs::write(
                    beside.join("hgraph-sdk/hgl-sdk.txt"),
                    format!("tag={HGRAPH_TAG}\nbuild-type=Release\n"),
                )
                .unwrap()
            }),
            "is not built for debugging: its `hgraph-sdk/hgl-sdk.txt` says `build-type=Release`"
        );
    }

    #[test]
    fn a_package_beside_hgl_missing_a_library_is_passed_over() {
        assert_eq!(
            passed_over(|beside| {
                fs::remove_file(beside.join(format!("hgraph-sdk/lib/libhgraph_runtime{SUFFIX}")))
                    .unwrap()
            }),
            format!(
                "lacks `hgraph-sdk/lib/libhgraph_runtime{SUFFIX}`, which its CMake package \
                 `hgraph` imports"
            )
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_package_beside_hgl_with_a_library_cut_short_is_passed_over() {
        let said = passed_over(|beside| {
            let library = beside.join(format!("hgraph-deps/lib/libfmtd{SUFFIX}"));
            let whole = fs::read(&library).unwrap();
            fs::remove_file(&library).unwrap();
            fs::write(&library, &whole[..whole.len() / 2]).unwrap();
        });
        assert!(
            said.starts_with(&format!(
                "holds `hgraph-deps/lib/libfmtd{SUFFIX}`, which is not a whole library: "
            )),
            "{said}"
        );
    }

    #[test]
    fn a_package_beside_hgl_missing_a_cmake_package_is_passed_over() {
        // A debug build would otherwise find spdlog's package in another
        // prefix, one on the `PATH` say, and link two spdlogs.
        assert_eq!(
            passed_over(|beside| {
                fs::remove_dir_all(beside.join("hgraph-deps/lib/cmake/spdlog")).unwrap()
            }),
            "holds no CMake package `spdlog` in `hgraph-deps/lib/cmake/spdlog`"
        );
        assert_eq!(
            passed_over(|beside| {
                fs::write(
                    beside.join("hgraph-deps/lib/cmake/fmt/fmt-targets-debug.cmake"),
                    "",
                )
                .unwrap()
            }),
            "holds a CMake package `fmt` in `hgraph-deps/lib/cmake/fmt` that imports no library"
        );
    }

    #[test]
    fn a_debug_build_against_the_users_own_sdk_does_not_take_the_package() {
        let bundle = release(Some(HGRAPH_TAG));
        let own = release(None);
        let sdk = own.path().join("libexec/hgraph-sdk");
        let deps = own.path().join("libexec/hgraph-deps");
        for set in [
            vec![
                ("HGL_HGRAPH_SDK", sdk.as_path()),
                ("HGL_HGRAPH_DEPS", deps.as_path()),
            ],
            vec![("HGL_HGRAPH_DEPS", deps.as_path())],
        ] {
            let hgraph = chosen_with(true, &set, Some(bundle.path())).unwrap();
            assert_eq!(hgraph.build, Build::Optimised);
            assert_eq!(hgraph.deps, real(deps.clone()));
            assert!(
                hgraph.said().ends_with(&format!(
                    ": {} names them, and HGL_HGRAPH_DEBUG names no debug libraries",
                    set[0].0
                )),
                "{}",
                hgraph.said()
            );
        }
    }

    #[cfg(unix)]
    #[test]
    fn a_package_of_another_hgraph_is_not_taken() {
        let bundle = release(Some("0.0.1"));
        let hgraph = chosen_with(true, &[], Some(bundle.path())).unwrap();
        assert_eq!(hgraph.build, Build::Optimised);
        assert!(
            hgraph.said().ends_with(&format!(
                "are hgraph 0.0.1's, and this hgl writes for hgraph {HGRAPH_TAG}"
            )),
            "{}",
            hgraph.said()
        );
    }

    /// A toolchain of `build`'s libraries in `libexec`, for a debug build
    /// where `debug` says.
    fn toolchain(libexec: &Path, build: Build, debug: bool) -> Toolchain {
        Toolchain {
            hgraph: Hgraph {
                build,
                sdk: libexec.join("hgraph-sdk"),
                deps: libexec.join("hgraph-deps"),
                found: String::new(),
            },
            compiler: PathBuf::from("g++"),
            family: Family::Gcc,
            version: "15.2.0".to_string(),
            ninja: Some(PathBuf::from("ninja")),
            lld: false,
            flags: Vec::new(),
            debug,
        }
    }

    #[test]
    fn the_toolchains_hash_covers_its_libraries_and_whether_it_builds_for_debugging() {
        let bundle = release(None);
        let libexec = bundle.path().join("libexec");
        let ordinary = toolchain(&libexec, Build::Optimised, false).key();
        assert_eq!(ordinary, toolchain(&libexec, Build::Optimised, false).key());
        assert_ne!(ordinary, toolchain(&libexec, Build::Debug, false).key());
        // A debug build that links the optimised libraries builds its
        // runtime library for debugging all the same.
        assert_ne!(ordinary, toolchain(&libexec, Build::Optimised, true).key());
    }

    #[test]
    fn every_configure_finds_packages_in_the_two_prefixes_alone() {
        let libexec = Path::new("libexec");
        for (build, debug) in [
            (Build::Debug, true),
            (Build::Optimised, true),
            (Build::Optimised, false),
        ] {
            let flags = toolchain(libexec, build, debug).configure_flags().unwrap();
            assert!(
                CONFINED
                    .iter()
                    .all(|flag| flags.iter().any(|given| given == flag)),
                "{flags:?}"
            );
        }
    }

    #[test]
    fn cmake_runs_with_none_of_its_own_variables_of_the_environment() {
        let command = cmake_command(Path::new("cmake"));
        let removed: Vec<String> = command
            .get_envs()
            .filter(|(_, value)| value.is_none())
            .map(|(name, _)| name.to_string_lossy().into_owned())
            .collect();
        // Windows knows a variable's name whatever its case, and keeps
        // one spelling of `spdlog_ROOT` and `SPDLOG_ROOT`.
        let same = |name: &str, variable: &str| {
            if cfg!(windows) {
                name.eq_ignore_ascii_case(variable)
            } else {
                name == variable
            }
        };
        for variable in [
            "CMAKE_TOOLCHAIN_FILE",
            "CMAKE_PREFIX_PATH",
            "CMAKE_GENERATOR",
            "CMAKE_BUILD_TYPE",
            "DESTDIR",
            "fmt_DIR",
            "spdlog_ROOT",
            "SPDLOG_ROOT",
            "hgraph_DIR",
        ] {
            assert!(
                removed.iter().any(|name| same(name, variable)),
                "{variable}: {removed:?}"
            );
        }
        assert!(
            !removed
                .iter()
                .any(|name| same(name, "PATH") || same(name, "SDKROOT"))
        );
    }

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
