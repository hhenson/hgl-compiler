//! The cpp target's build of one program: the directory it is built
//! in, the `build.ninja` written there, and `ninja` run on it.
//!
//! A program is built in a workspace of its own, kept between runs so
//! that a second run compiles only what changed: the directory
//! `--keep` names, or one under the cache named by a hash of the
//! program's source roots. A run holds the workspace's lock from the
//! moment it writes the program until the last executable it started
//! has ended, so that two runs of one program take turns.
//!
//! What the target wrote is written into the workspace only where it
//! differs from what is there, and so is `build.ninja`: a file
//! unchanged keeps its time, and `ninja` compiles nothing for it. The
//! build has one object for each module's unit and each entry unit,
//! compiled with the precompiled header, one executable for each
//! module with tests, `bin/<module>`, and one for each application,
//! `run/<application-stem>` (UNIT-14), each linked from its entry and the
//! units `hgl_cpp` says it needs.
//!
//! The caller publishes a content stamp after Ninja succeeds. The stamp
//! covers the objects and the executables needed to reuse the build.

use std::fs;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::sync::atomic::{AtomicU8, Ordering};

use hgl_cpp::{Executable, Output, application_stem};

use super::sdk::{Description, last_lines};

/// The file a workspace's lock is taken on.
const LOCK: &str = ".hgl-lock";

const BUILDING: u8 = 0;
const INTERRUPTED: u8 = 1;
const FINISHED: u8 = 2;

/// A stop requested during a build. The process waits for its native
/// child before releasing the workspace lock and leaves no stamp.
pub(super) struct Interrupt(Arc<AtomicU8>);

impl Interrupt {
    pub(super) fn watch() -> Self {
        let state = Arc::new(AtomicU8::new(BUILDING));
        let signal = Arc::clone(&state);
        // An application run already has its own handler. It queues a
        // request for its host to stop while the build completes.
        let _ = ctrlc::set_handler(move || {
            if signal.compare_exchange(BUILDING, INTERRUPTED, Ordering::SeqCst, Ordering::SeqCst)
                == Err(FINISHED)
            {
                std::process::exit(130);
            }
        });
        Self(state)
    }

    pub(super) fn check(&self) -> Result<(), String> {
        if self.0.load(Ordering::SeqCst) == INTERRUPTED {
            Err("the build was interrupted".to_string())
        } else {
            Ok(())
        }
    }

    /// Allows publication only if no stop was requested during the build.
    pub(super) fn finish(&self) -> Result<(), String> {
        self.0
            .compare_exchange(BUILDING, FINISHED, Ordering::SeqCst, Ordering::SeqCst)
            .map(|_| ())
            .map_err(|_| "the build was interrupted".to_string())
    }
}

impl Drop for Interrupt {
    fn drop(&mut self) {
        self.0.store(FINISHED, Ordering::SeqCst);
    }
}

/// The variable that names the cache, where neither it nor the user's
/// cache directory is the place.
pub(super) const CACHE: &str = "HGL_NATIVE_CACHE";

/// The variable that says how many jobs a C++ build runs at once, as
/// CMake's own builds take it.
pub(super) const JOBS: &str = "CMAKE_BUILD_PARALLEL_LEVEL";

/// The directory everything the cpp target makes is kept under: the one
/// `HGL_NATIVE_CACHE` names, or `hgl/native` in the user's cache
/// directory, as an absolute path. The paths a toolchain's description
/// holds are under it, and `ninja` reads them from a workspace's
/// directory: a relative one would be read from there.
pub(super) fn cache() -> Result<PathBuf, String> {
    let directory = match std::env::var_os(CACHE).filter(|named| !named.is_empty()) {
        Some(named) => PathBuf::from(named),
        None => dirs::cache_dir()
            .map(|directory| directory.join("hgl").join("native"))
            .ok_or_else(|| {
                format!(
                    "there is no cache directory for the user: set {CACHE} to a directory to build in"
                )
            })?,
    };
    std::path::absolute(&directory)
        .map_err(|error| format!("{} cannot be made absolute: {error}", directory.display()))
}

/// Takes the lock of `directory`, waiting for a run that holds it; it
/// is let go when what is given is dropped.
pub(super) fn lock(directory: &Path) -> Result<fs::File, String> {
    let path = directory.join(LOCK);
    let file = fs::File::options()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(&path)
        .map_err(|error| format!("{} could not be opened: {error}", path.display()))?;
    file.lock()
        .map_err(|error| format!("{} could not be locked: {error}", path.display()))?;
    Ok(file)
}

/// Writes `text` to `path`, making the directories on the way, where
/// the file does not already hold it; a file that does keeps its time.
/// The text is written beside it and then renamed onto it, so that
/// nothing reads half of it.
pub(super) fn write_if_changed(path: &Path, text: &str) -> Result<(), String> {
    if fs::read(path).ok().as_deref() == Some(text.as_bytes()) {
        return Ok(());
    }
    let parent = path.parent().unwrap_or(Path::new("."));
    fs::create_dir_all(parent)
        .map_err(|error| format!("{} could not be made: {error}", parent.display()))?;
    let failed =
        |error: std::io::Error| format!("{} could not be written: {error}", path.display());
    let mut temporary = tempfile::NamedTempFile::new_in(parent).map_err(failed)?;
    temporary.write_all(text.as_bytes()).map_err(failed)?;
    temporary
        .persist(path)
        .map_err(|error| failed(error.error))?;
    Ok(())
}

/// `word` as one word of a POSIX shell's command line: as it is where
/// it needs no quoting, and in `'...'` otherwise.
pub(super) fn quoted(word: &str) -> String {
    let plain = !word.is_empty()
        && word
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || "_@%+=:,./-".contains(character));
    if plain {
        word.to_string()
    } else {
        format!("'{}'", word.replace('\'', r"'\''"))
    }
}

/// `text` as a value of a `ninja` variable: `$` is `ninja`'s escape.
fn escaped(text: &str) -> String {
    text.replace('$', "$$")
}

/// `path` as a path of a `ninja` build line, where a space, a `:` and a
/// `$` are escaped.
fn ninja_path(path: &str) -> String {
    path.replace('$', "$$")
        .replace(' ', "$ ")
        .replace(':', "$:")
}

/// The directory a program is built in, and its lock, held while the
/// value lives.
pub(super) struct Workspace {
    /// The directory.
    pub(super) directory: PathBuf,
    /// The lock, let go when it is dropped.
    _lock: fs::File,
}

impl Workspace {
    /// Opens a normal or debug workspace. A debug build with an
    /// implicit path has its own directory under the cache.
    pub(super) fn open(
        keep: Option<&Path>,
        roots: &[PathBuf],
        cache: &Path,
        debug: bool,
    ) -> Result<Workspace, String> {
        let directory = match keep {
            Some(directory) => std::path::absolute(directory).map_err(|error| {
                format!("{} cannot be made absolute: {error}", directory.display())
            })?,
            None => {
                let mut hash = DefaultHasher::new();
                for root in roots {
                    fs::canonicalize(root)
                        .unwrap_or_else(|_| root.clone())
                        .hash(&mut hash);
                }
                cache
                    .join(if debug { "debug-programs" } else { "programs" })
                    .join(format!("{:016x}", hash.finish()))
            }
        };
        fs::create_dir_all(&directory)
            .map_err(|error| format!("{} could not be made: {error}", directory.display()))?;
        let lock = lock(&directory)?;
        Ok(Workspace {
            directory,
            _lock: lock,
        })
    }

    /// The path of a built test executable given its artifact stem.
    pub(super) fn test_executable(&self, stem: &str) -> PathBuf {
        self.directory.join(test_target_stem(stem))
    }

    /// The executable of application `path` (UNIT-14).
    pub(super) fn application(&self, path: &str) -> PathBuf {
        self.directory.join(application_target(path))
    }

    /// Writes what the target wrote, `output`, and the `build.ninja`
    /// that builds it with `description`, each where it changed.
    pub(super) fn write(&self, output: &Output, description: &Description) -> Result<(), String> {
        for file in &output.files {
            let path = file
                .path
                .split('/')
                .fold(self.directory.clone(), |path, part| path.join(part));
            write_if_changed(&path, &file.text)?;
        }
        write_if_changed(
            &self.directory.join("build.ninja"),
            &ninja_file(output, description),
        )
    }

    /// Compiles the program's units, then links its applications and,
    /// where asked for, its test executables. Returns every built path.
    pub(super) fn build_program(
        &self,
        ninja: &Path,
        output: &Output,
        tests: bool,
        jobs: Option<&str>,
    ) -> Result<Vec<String>, String> {
        let mut targets: Vec<_> = output
            .units
            .iter()
            .map(|unit| object(&unit.source))
            .collect();
        targets.extend(
            output
                .applications
                .iter()
                .filter(|application| application.source.is_some())
                .map(|application| application_target(&application.path)),
        );
        if tests {
            targets.extend(output.executables.iter().map(test_target));
        }
        if !targets.is_empty() {
            self.build_targets(ninja, &targets, jobs)?;
        }
        if tests {
            targets.extend(
                output
                    .executables
                    .iter()
                    .map(|executable| object(&executable.source)),
            );
        }
        targets.extend(
            output
                .applications
                .iter()
                .filter_map(|application| application.source.as_deref())
                .map(object),
        );
        Ok(targets)
    }

    /// Runs Ninja for every requested artifact. A failed build is never stamped.
    fn build_targets(
        &self,
        ninja: &Path,
        targets: &[String],
        jobs: Option<&str>,
    ) -> Result<(), String> {
        let mut command = Command::new(ninja);
        command.arg("-C").arg(&self.directory).arg("-k").arg("0");
        if let Some(jobs) = jobs {
            command.arg("-j").arg(jobs);
        }
        let finished = command
            .args(targets)
            .output()
            .map_err(|error| format!("`{}` could not be started: {error}", ninja.display()))?;
        if finished.status.success() {
            return Ok(());
        }
        let printed = String::from_utf8_lossy(&finished.stdout).into_owned()
            + &String::from_utf8_lossy(&finished.stderr);
        // What failed first and what the compiler said of it, without
        // the command `ninja` prints on the line after.
        let failure = match printed.find("FAILED:") {
            Some(first) => {
                let mut lines = printed[first..].lines();
                let head = lines.next().unwrap_or_default().trim_end();
                let said: Vec<&str> = lines.skip(1).take(20).collect();
                format!("{head}\n{}", said.join("\n"))
            }
            None => last_lines(&printed, 20),
        };
        Err(format!("the build of its executable failed:\n{failure}"))
    }
}

/// The `build.ninja` of `output`, built with `description`.
fn ninja_file(output: &Output, description: &Description) -> String {
    let mut text = String::from(
        "# Written by hgl: the build of a program written by the cpp target.\n\
         # It is written again whenever it changes.\n\
         ninja_required_version = 1.10\n\n",
    );
    text.push_str(&format!("cxx = {}\n", escaped(&description.compiler)));
    text.push_str(&format!(
        "cflags = {}\n",
        escaped(&description.compile.join(" "))
    ));
    text.push_str(&format!(
        "pch = {}\n",
        escaped(&quoted(&description.header))
    ));
    text.push_str(&format!(
        "ldflags = {}\n\n",
        escaped(&description.link.join(" "))
    ));
    text.push_str(
        "# A module's unit, with the precompiled header of the\n\
         # fixed include set.\n\
         rule unit\n  command = $cxx $cflags -Winvalid-pch -include $pch -MD -MF $out.d -c $in -o $out\n  \
         depfile = $out.d\n  deps = gcc\n  description = compile $in\n\n\
         # An entry unit, which includes the test host's light header alone, or\n\
         # an application's host's, and is compiled with no precompiled header.\n\
         rule entry\n  command = $cxx $cflags -MD -MF $out.d -c $in -o $out\n  \
         depfile = $out.d\n  deps = gcc\n  description = compile $in\n\n\
         rule link\n  command = $cxx -o $out $in $ldflags\n  description = link $out\n\n",
    );
    for unit in &output.units {
        text.push_str(&format!(
            "build {}: unit {}\n",
            ninja_path(&object(&unit.source)),
            ninja_path(&unit.source)
        ));
    }
    for executable in &output.executables {
        text.push_str(&format!(
            "build {}: entry {}\n",
            ninja_path(&object(&executable.source)),
            ninja_path(&executable.source)
        ));
        let mut objects = vec![ninja_path(&object(&executable.source))];
        objects.extend(
            executable
                .units
                .iter()
                .map(|&unit| ninja_path(&object(&output.units[unit].source))),
        );
        text.push_str(&format!(
            "build {}: link {}\n",
            ninja_path(&test_target(executable)),
            objects.join(" ")
        ));
    }
    // UNIT-14: an application's executable, linked as a module's tests'
    // is, from its entry unit and the units it needs.
    for application in &output.applications {
        let Some(source) = &application.source else {
            continue;
        };
        text.push_str(&format!(
            "build {}: entry {}\n",
            ninja_path(&object(source)),
            ninja_path(source)
        ));
        let mut objects = vec![ninja_path(&object(source))];
        objects.extend(
            application
                .units
                .iter()
                .map(|&unit| ninja_path(&object(&output.units[unit].source))),
        );
        text.push_str(&format!(
            "build {}: link {}\n",
            ninja_path(&application_target(&application.path)),
            objects.join(" ")
        ));
    }
    text
}

/// The test executable's path, with the host's executable suffix where required.
fn test_target(executable: &Executable) -> String {
    test_target_stem(&executable.stem)
}

pub(super) fn test_target_stem(stem: &str) -> String {
    format!("bin/{stem}{}", std::env::consts::EXE_SUFFIX)
}

/// The path, in the build, of the executable of application `path`:
/// `run/app-demo-app`, with the host's executable suffix where required.
pub(super) fn application_target(path: &str) -> String {
    format!(
        "run/{}{}",
        application_stem(path),
        std::env::consts::EXE_SUFFIX
    )
}

/// The object a unit at `source` is compiled to, such as
/// `obj/modules/mod-demo-calc.o` for `modules/mod-demo-calc.cpp`.
fn object(source: &str) -> String {
    format!("obj/{}.o", source.trim_end_matches(".cpp"))
}

#[cfg(test)]
mod tests {
    use super::{ninja_path, quoted};

    #[test]
    fn a_word_is_quoted_only_where_it_needs_to_be() {
        assert_eq!(quoted("-DFMT_SHARED"), "-DFMT_SHARED");
        assert_eq!(quoted("/a b"), "'/a b'");
        assert_eq!(quoted("it's"), r"'it'\''s'");
    }

    #[test]
    fn a_path_is_escaped_for_ninja() {
        assert_eq!(ninja_path("bin/a b:c$"), "bin/a$ b$:c$$");
    }
}
