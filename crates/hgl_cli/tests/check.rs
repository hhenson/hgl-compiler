//! Runs `hgl check` the way a user would.
//!
//! The programs are written by the tests when they run, into a scratch
//! directory of their own each. The last test reads core's conformance
//! cases from the pinned `spec/` directory.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

/// Runs `hgl` with `arguments` and waits for it to finish.
fn hgl(arguments: &[&str]) -> Output {
    // Cargo builds the binary before the test and puts its path in this
    // environment variable at compile time.
    Command::new(env!("CARGO_BIN_EXE_hgl"))
        .args(arguments)
        .output()
        .expect("the hgl binary runs")
}

/// A stream's bytes as text.
fn text(bytes: &[u8]) -> String {
    String::from_utf8(bytes.to_vec()).expect("hgl writes UTF-8")
}

/// An empty directory for the test named `test`.
fn scratch(test: &str) -> PathBuf {
    // Cargo gives integration tests a scratch directory under `target/`.
    let directory = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("check")
        .join(test);
    // Left over from an earlier run, perhaps; `ok()` ignores "not there".
    fs::remove_dir_all(&directory).ok();
    fs::create_dir_all(&directory).unwrap();
    directory
}

/// Writes `text` to `directory/name`, making the directories on the way,
/// and gives the path as `hgl` will print it. `name` is written with `/`
/// between its parts on every system.
fn write(directory: &Path, name: &str, text: &str) -> String {
    // Joined a part at a time, so that the path has the system's own
    // separator throughout: `hgl` prints paths as the system gives them.
    let mut path = directory.to_path_buf();
    for part in name.split('/') {
        path.push(part);
    }
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, text).unwrap();
    path.display().to_string()
}

/// A path as an argument.
fn argument(path: &Path) -> String {
    path.display().to_string()
}

/// UNIT-1: a program is every `.hgl` file under its source roots, and a
/// file belongs to the module its `module` line names, wherever it is.
#[test]
fn a_program_is_found_under_its_roots_and_listed() {
    let root = scratch("listed");
    let app = write(
        &root,
        "zebra.hgl",
        concat!(
            "module app\n",
            "use lib::{shared, add}\n",
            "use lib as l\n",
            "impl fn add(left: i64, right: i64) -> i64 => left\n",
            "fn main() {}\n",
            "test smoke { assert true }\n",
        ),
    );
    let part = write(
        &root,
        "deep/er/anything.hgl",
        "module app part helpers\ntest {\n    fn helper() {}\n}\n",
    );
    let lib = write(
        &root,
        "deep/library.hgl",
        concat!(
            "module lib\n",
            "export fn shared() {}\n",
            "operator add(left: i64, right: i64) -> i64\n",
        ),
    );
    // Not source: another extension, and a directory named like a file.
    write(&root, "notes.txt", "module wrong\n");
    write(&root, "deep/not_source.hgl.bak", "module wrong\n");
    fs::create_dir_all(root.join("directory.hgl")).unwrap();

    let output = hgl(&["check", &argument(&root), "--modules"]);
    assert_eq!(text(&output.stderr), "");
    assert_eq!(output.status.code(), Some(0));
    let expected = format!(
        concat!(
            "module app\n",
            "  file {app}\n",
            "  file {part} part helpers\n",
            "  use lib::{{shared}}\n",
            "  use lib::{{add}}\n",
            "  use lib as l\n",
            "  temporal operator implementation add(left: i64, right: i64) -> i64 of lib\n",
            "  temporal function main()\n",
            "  test temporal function helper()\n",
            "  test smoke\n",
            "module lib\n",
            "  file {lib}\n",
            "  export temporal function shared()\n",
            "  operator add(left: i64, right: i64) -> i64\n",
            "checked 3 files in 2 modules: no errors\n",
            "modules, declarations, imports, names, written types and the bodies of value functions are checked; temporal bodies are not yet\n",
        ),
        app = app,
        part = part,
        lib = lib,
    );
    assert_eq!(text(&output.stdout), expected);

    // Without `--modules`, the summary alone.
    let quiet = hgl(&["check", &argument(&root)]);
    assert_eq!(quiet.status.code(), Some(0));
    assert_eq!(
        text(&quiet.stdout),
        concat!(
            "checked 3 files in 2 modules: no errors\n",
            "modules, declarations, imports, names, written types and the bodies of value functions are checked; temporal bodies are not yet\n",
        )
    );
}

/// A rejected program exits with status 1 and reports each error, from
/// every stage, with the place it points at and the related one.
#[test]
fn a_rejected_program_reports_each_error_and_exits_one() {
    let root = scratch("rejected");
    let first = write(&root, "a.hgl", "module m\nfn scale() {}\n");
    let second = write(
        &root,
        "b.hgl",
        "module m part second\nfn scale() {}\nfn broken() -> {}\n",
    );
    let output = hgl(&["check", &argument(&root)]);
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        text(&output.stdout),
        concat!(
            "checked 2 files in 1 module: 2 errors\n",
            "modules, declarations, imports, names, written types and the bodies of value functions are checked; temporal bodies are not yet\n",
        )
    );
    let stderr = text(&output.stderr);
    // UNIT-5, from the analyser, with both places.
    assert!(
        stderr.contains(
            "error[name/name.duplicate]: module `m` has two temporal functions named `scale`"
        ),
        "{stderr}"
    );
    assert!(stderr.contains(&format!("{second}:2:4")), "{stderr}");
    assert!(stderr.contains(&format!("{first}:2:4")), "{stderr}");
    assert!(stderr.contains("first declared here"), "{stderr}");
    assert!(stderr.contains("rule UNIT-5"), "{stderr}");
    // Both codes are the catalogue's.
    assert!(!stderr.contains("compiler-local code"), "{stderr}");
    // And the parser's, for the same run.
    assert!(
        stderr.contains("error[parse/syntax.expected_token]"),
        "{stderr}"
    );
    assert!(stderr.contains(&format!("{second}:3:16")), "{stderr}");
}

/// UNIT-11: a name nothing in the program binds is left for the prelude
/// and listed, not reported.
#[test]
fn a_name_left_for_the_prelude_is_listed() {
    let root = scratch("prelude");
    let file = write(
        &root,
        "m.hgl",
        "module m\nimpl fn add(left: i64, right: i64) -> i64 => left\n",
    );
    let output = hgl(&["check", &argument(&root)]);
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(text(&output.stderr), "");
    assert_eq!(
        text(&output.stdout),
        format!(
            concat!(
                "checked 1 file in 1 module: no errors\n",
                "modules, declarations, imports, names, written types and the bodies of value functions are checked; temporal bodies are not yet\n",
                "left for the prelude (UNIT-11), which does not bind them yet:\n",
                "  {file}:2:9 add\n",
            ),
            file = file
        )
    );
}

/// A root may be a file, and several roots make one program. A file
/// reached through two roots is read once.
#[test]
fn roots_may_be_files_and_may_overlap() {
    let root = scratch("roots");
    let lib = write(&root, "lib/lib.hgl", "module lib\nexport fn shared() {}\n");
    let app = write(&root, "app/app.hgl", "module app\nuse lib::{shared}\n");

    // The application alone: its import names a module no root holds.
    let alone = hgl(&["check", &app]);
    assert_eq!(alone.status.code(), Some(1));
    assert!(
        text(&alone.stderr).contains("error[module/import.unknown_module]"),
        "{}",
        text(&alone.stderr)
    );

    // With the library's root, and each file named twice over.
    let together = hgl(&[
        "check",
        &app,
        &argument(&root.join("lib")),
        &lib,
        &argument(&root),
    ]);
    assert_eq!(text(&together.stderr), "");
    assert_eq!(together.status.code(), Some(0));
    assert!(
        text(&together.stdout).starts_with("checked 2 files in 2 modules: no errors\n"),
        "{}",
        text(&together.stdout)
    );
}

/// A command that cannot run exits with status 2 and says why.
#[test]
fn a_root_with_no_source_is_an_error() {
    let root = scratch("empty");
    write(&root, "notes.txt", "nothing here\n");

    let empty = hgl(&["check", &argument(&root)]);
    assert_eq!(empty.status.code(), Some(2));
    assert_eq!(text(&empty.stdout), "");
    assert!(
        text(&empty.stderr).contains("no .hgl file under"),
        "{}",
        text(&empty.stderr)
    );

    let missing = hgl(&["check", &argument(&root.join("missing"))]);
    assert_eq!(missing.status.code(), Some(2));
    assert!(
        text(&missing.stderr).starts_with("hgl: cannot read"),
        "{}",
        text(&missing.stderr)
    );

    let no_roots = hgl(&["check"]);
    assert_eq!(no_roots.status.code(), Some(2));
}

/// The compiler's limits apply to every file of the program.
#[test]
fn the_limits_options_apply() {
    let root = scratch("limits");
    write(
        &root,
        "m.hgl",
        "module m\nconst fn deep() -> i64 => ((((((1))))))\n",
    );
    let within = hgl(&["check", &argument(&root)]);
    assert_eq!(within.status.code(), Some(0));
    let past = hgl(&["check", &argument(&root), "--nesting-limit", "4"]);
    assert_eq!(past.status.code(), Some(1));
    assert!(
        text(&past.stderr).contains("error[parse/syntax.nesting_limit]"),
        "{}",
        text(&past.stderr)
    );
}

/// UNIT-1, CONF-3: core's conformance cases are the parts of one module.
#[test]
fn the_conformance_cases_are_one_module() {
    let cases = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../spec/hgl_core/tests/conformance");
    assert!(
        cases.is_dir(),
        "no conformance cases; run `git submodule update --init`"
    );
    let output = hgl(&["check", &argument(&cases), "--modules"]);
    assert_eq!(text(&output.stderr), "");
    assert_eq!(output.status.code(), Some(0));
    let stdout = text(&output.stdout);
    assert!(stdout.starts_with("module conformance\n"), "{stdout}");
    assert_eq!(stdout.matches("\nmodule ").count(), 0, "{stdout}");
    assert!(stdout.contains(" part scalar_eval\n"), "{stdout}");
    assert!(
        stdout.contains("\n  test runs_are_independent\n"),
        "{stdout}"
    );
    assert!(stdout.contains(" in 1 module: no errors\n"), "{stdout}");
}

/// UNIT-11: "every module binds its public declarations without a
/// `use`". Core is the part every program has, so a program is always
/// checked with core's module: the compiler supplies the one it is
/// built with to a program whose source roots hold none. It is not
/// counted or listed: the counts and the listing are of what the roots
/// hold.
#[test]
fn core_is_supplied_when_the_roots_hold_none() {
    let directory = scratch("core_supplied");
    write(
        &directory,
        "use.hgl",
        concat!(
            "module examples.core_use\n",
            "const fn uses(a: f64, b: f64, n: i64) -> result<f64, error> {\n",
            "    let real = convert<f64>(n)\n",
            "    return div_(a, real, zero: .error)\n",
            "}\n",
        ),
    );
    let root = argument(&directory);

    let supplied = hgl(&["check", "--modules", &root]);
    let stdout = text(&supplied.stdout);
    assert!(supplied.status.success(), "{}", text(&supplied.stderr));
    assert!(
        stdout.contains("checked 1 file in 1 module: no errors\n"),
        "{stdout}"
    );
    assert!(stdout.contains("module examples.core_use\n"), "{stdout}");
    assert!(!stdout.contains("module core\n"), "{stdout}");
    assert!(!stdout.contains("left for the prelude"), "{stdout}");

    // A program that brings its own `module core` is checked with that
    // one, which is then counted and listed with the rest.
    let core = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../spec/hgl_core/hgl");
    let own = hgl(&["check", "--modules", &root, &argument(&core)]);
    let stdout = text(&own.stdout);
    assert!(own.status.success(), "{}", text(&own.stderr));
    assert!(
        stdout.contains("checked 3 files in 2 modules: no errors\n"),
        "{stdout}"
    );
    assert!(stdout.contains("module core\n"), "{stdout}");
}

/// The limits given to `hgl check` are for the program's own source.
/// Core's module, which the compiler supplies, is read under the default
/// ones: a limit too small for core's files reports nothing of them,
/// and a program the limit cuts down to no module is still counted.
#[test]
fn a_limit_is_for_the_programs_source_and_not_for_the_core_supplied() {
    let directory = scratch("core_limits");
    write(
        &directory,
        "use.hgl",
        concat!(
            "module examples.core_limits\n",
            "const fn uses(a: f64, b: f64) -> f64 => div_(a, b)\n",
        ),
    );
    let root = argument(&directory);

    // Too small for any file: the program's is not built at all.
    let shallow = hgl(&["check", "--tree-depth-limit", "1", &root]);
    let stdout = text(&shallow.stdout);
    let stderr = text(&shallow.stderr);
    assert_eq!(shallow.status.code(), Some(1), "{stderr}");
    assert!(
        stdout.contains("checked 1 file in 0 modules: 1 error\n"),
        "{stdout}"
    );
    assert!(stderr.contains("syntax.nesting_limit"), "{stderr}");
    assert!(!stderr.contains("<core>"), "{stderr}");

    // Too small for the nesting core's own files have.
    let flat = hgl(&["check", "--nesting-limit", "1", &root]);
    let stderr = text(&flat.stderr);
    assert_eq!(flat.status.code(), Some(1), "{stderr}");
    assert!(!stderr.contains("<core>"), "{stderr}");
}
