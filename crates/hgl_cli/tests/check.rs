//! Runs `hgl check` the way a user would.
//!
//! The programs are written by the tests when they run, into a scratch
//! directory of their own each. The last test reads core's conformance
//! cases from the pinned `spec/` directory.

mod common;

use std::fs;
use std::path::Path;

use common::{argument, hgl, scratch, text, write};

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
            "export operator add(left: i64, right: i64) -> i64\n",
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
            "  export operator add(left: i64, right: i64) -> i64\n",
            "checked 3 files in 2 modules: no errors\n",
            "modules, declarations, imports, names, written types and the bodies of value functions, nodes, sources, graphs and tests are checked; no test is run\n",
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
            "modules, declarations, imports, names, written types and the bodies of value functions, nodes, sources, graphs and tests are checked; no test is run\n",
        )
    );
}

/// UNIT-14: an application is a unit of its own. It is listed under
/// its own word, and counted apart from the modules.
#[test]
fn an_application_is_listed_and_counted_as_one() {
    let root = scratch("application");
    write(
        &root,
        "lib.hgl",
        "module lib\nexport fn beat() -> i64 => 1\n",
    );
    let live = write(
        &root,
        "live.hgl",
        "application lib.live\nuse lib::{beat}\nrun beat()\n",
    );
    let output = hgl(&["check", &argument(&root), "--modules"]);
    assert_eq!(text(&output.stderr), "");
    assert_eq!(output.status.code(), Some(0));
    let listed = text(&output.stdout);
    assert!(
        listed.contains(&format!(
            "application lib.live\n  file {live}\n  use lib::{{beat}}\n"
        )),
        "{listed}"
    );
    assert!(
        listed.contains("checked 2 files in 1 module and 1 application: no errors\n"),
        "{listed}"
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
            "modules, declarations, imports, names, written types and the bodies of value functions, nodes, sources, graphs and tests are checked; no test is run\n",
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

/// ERR-1: "A warning does not reject valid source." A program with a
/// warning and no error exits with status 0; the warning is printed by
/// its code, which has no category, and counted after the errors.
#[test]
fn a_warning_is_reported_and_does_not_reject() {
    let root = scratch("warned");
    let file = write(
        &root,
        "a.hgl",
        "module m\nfn dormant(a: f64) -> f64 {\n    when modified([]) { return a }\n}\n",
    );
    let output = hgl(&["check", &argument(&root)]);
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(
        text(&output.stdout),
        concat!(
            "checked 1 file in 1 module: no errors, 1 warning\n",
            "modules, declarations, imports, names, written types and the bodies of value functions, nodes, sources, graphs and tests are checked; no test is run\n",
        )
    );
    let stderr = text(&output.stderr);
    // TIME-3, at the selector.
    assert!(
        stderr.contains("warning[node.no_activation]: `modified([])` subscribes to no input"),
        "{stderr}"
    );
    assert!(stderr.contains(&format!("{file}:3:10")), "{stderr}");
    assert!(stderr.contains("rule TIME-3"), "{stderr}");
    assert!(!stderr.contains("error["), "{stderr}");
}

/// UNIT-10: "A name that none of these binds rejects (`name.unknown`)":
/// a parameter's name spelled wrongly is no name. GEN-4: an `impl` of
/// an operator nothing declares is `impl.unknown_operator`. The program
/// is rejected: status 1.
#[test]
fn a_name_nothing_binds_is_rejected() {
    let root = scratch("unknown");
    write(
        &root,
        "m.hgl",
        concat!(
            "module m\n",
            "const fn doubled(value: i64) -> i64 => vaule\n",
            "impl fn add(left: i64, right: i64) -> i64 => left\n",
        ),
    );
    let output = hgl(&["check", &argument(&root)]);
    assert_eq!(output.status.code(), Some(1));
    let said = text(&output.stderr);
    assert!(
        said.contains("error[name/name.unknown]: nothing binds `vaule` here"),
        "{said}"
    );
    assert!(
        said.contains(
            "error[operator/impl.unknown_operator]: nothing declares an operator `add`, so \
             nothing can implement it"
        ),
        "{said}"
    );
    assert_eq!(
        text(&output.stdout),
        concat!(
            "checked 1 file in 1 module: 2 errors\n",
            "modules, declarations, imports, names, written types and the bodies of value functions, nodes, sources, graphs and tests are checked; no test is run\n",
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

/// The declarations of core's rejection cases the compiler does not yet
/// reject: the `[rejections]` of `crates/hgl_cpp/ci/unsupported_cases.toml`,
/// each listed under a reason that names the rule not yet built.
fn unrejected() -> Vec<String> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../hgl_cpp/ci/unsupported_cases.toml");
    let text = fs::read_to_string(path).expect("the list of unsupported cases");
    text.lines()
        .skip_while(|line| line.trim() != "[rejections]")
        .skip(1)
        .take_while(|line| !line.trim_start().starts_with('['))
        .filter_map(|line| {
            let name = line.trim().strip_prefix('"')?.strip_suffix("\",")?;
            Some(name.rsplit_once("::")?.1.to_string())
        })
        .collect()
}

/// UNIT-1, CONF-3: core's conformance cases are the parts of one module.
/// Its source rejections, in `rejections/`, are a module of their own,
/// `conformance.rejections`, which `hgl check` rejects and `hgl test`
/// passes, each case rejected as its annotation says (TST-12, TST-14),
/// but for those listed as not yet rejected (`unrejected`), each of which
/// fails as no other does.
#[test]
fn the_conformance_cases_are_one_module() {
    let core = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../spec/hgl_core/tests/conformance");
    assert!(
        core.is_dir(),
        "no conformance cases; run `git submodule update --init`"
    );
    // The cases less the rejections, as they are.
    let cases = scratch("conformance");
    for entry in fs::read_dir(&core).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_some_and(|extension| extension == "hgl") {
            let name = path.file_name().unwrap().to_str().unwrap().to_string();
            write(&cases, &name, &fs::read_to_string(&path).unwrap());
        }
    }
    let rejections = core.join("rejections");
    let tested = hgl(&["test", &argument(&rejections), "--rejections-only"]);
    assert_eq!(text(&tested.stderr), "");
    let stdout = text(&tested.stdout);
    let failed: Vec<String> = stdout
        .lines()
        .filter(|line| line.starts_with("FAILED"))
        .filter_map(|line| line.rsplit_once(' ').map(|(_, name)| name.to_string()))
        .collect();
    let listed = unrejected();
    assert_eq!(failed, listed, "{stdout}");
    let code = if listed.is_empty() { 0 } else { 1 };
    assert_eq!(tested.status.code(), Some(code), "{stdout}");
    assert!(
        stdout.contains(&format!(
            "rejection cases: {} passed, {} failed\n",
            61 - listed.len(),
            listed.len()
        )),
        "{stdout}"
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

    // A program that brings its own `module core` is checked with that
    // one, which is then counted and listed with the rest.
    let core = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../spec/hgl_core/hgl");
    let own = hgl(&["check", "--modules", &root, &argument(&core)]);
    let stdout = text(&own.stdout);
    assert!(own.status.success(), "{}", text(&own.stderr));
    assert!(
        stdout.contains("checked 14 files in 2 modules: no errors\n"),
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

/// ERR-1, UNIT-5: keep primary and related ranges across files and count
/// an astral character as two UTF-16 units, independent of BOM and CRLF.
#[test]
fn json_preserves_diagnostic_identity_and_editor_ranges() {
    use serde_json::{Value, json};
    let root = scratch("json-related");
    let first = write(&root, "first.hgl", "module m\r\nfn scale() {}\r\n");
    let second = write(
        &root,
        "second.hgl",
        "\u{feff}module m part second\r\n/*😀*/ fn scale() {}\r\n",
    );
    let output = hgl(&["check", "--json", &argument(&root)]);
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stderr.is_empty());
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["version"], 1);
    let items = report["diagnostics"].as_array().unwrap();
    assert_eq!(items.len(), 1, "{report}");
    let d = &items[0];
    assert_eq!(d["severity"], "error");
    assert_eq!(d["category"], "name");
    assert_eq!(d["code"], "name.duplicate");
    assert_eq!(d["catalogued"], true);
    assert_eq!(d["rule"], "UNIT-5");
    assert_eq!(
        d["primary"],
        json!({"file": second, "start":{"line":1,"character":10}, "end":{"line":1,"character":15}})
    );
    assert_eq!(
        d["related"][0],
        json!({"message":"first declared here, as a temporal function", "location":{"file":first,"start":{"line":1,"character":3},"end":{"line":1,"character":8}}})
    );
}

/// ERR-1: clean and warning-only sources both succeed. I/O failures are distinct.
#[test]
fn json_clean_warning_and_invocation_failure() {
    use serde_json::{Value, json};
    let root = scratch("json-success");
    let file = write(&root, "a.hgl", "module m\n");
    let check = || hgl(&["check", "--json", &file]);
    let clean = check();
    assert!(clean.status.success() && clean.stderr.is_empty());
    assert_eq!(
        serde_json::from_slice::<Value>(&clean.stdout).unwrap(),
        json!({"version":1,"diagnostics":[]})
    );
    write(
        &root,
        "a.hgl",
        "module m\nfn dormant(a: f64) -> f64 {\n    when modified([]) { return a }\n}\n",
    );
    let warned = check();
    assert!(warned.status.success() && warned.stderr.is_empty());
    let report: Value = serde_json::from_slice(&warned.stdout).unwrap();
    assert_eq!(report["diagnostics"][0]["severity"], "warning");
    assert_eq!(report["diagnostics"][0]["code"], "node.no_activation");
    for failed in [
        hgl(&["check", "--modules", "--json", &file]),
        hgl(&["check", "--json", &argument(&root.join("missing"))]),
    ] {
        assert_eq!(failed.status.code(), Some(2));
        assert!(failed.stdout.is_empty() && !failed.stderr.is_empty());
    }
}

/// Runs `hgl check --json` on `roots`, and gives its exit status and
/// the report it wrote. It writes nothing else: standard error is empty.
fn json_check(roots: &[&str]) -> (Option<i32>, serde_json::Value) {
    let output = hgl(&[&["check", "--json"], roots].concat());
    assert_eq!(text(&output.stderr), "");
    let report = serde_json::from_slice(&output.stdout).expect("the report is JSON");
    (output.status.code(), report)
}

/// A place as the report writes it: the file, and the line and the
/// character of each end of the range.
fn place(file: &str, start: (u32, u32), end: (u32, u32)) -> serde_json::Value {
    serde_json::json!({
        "file": file,
        "start": {"line": start.0, "character": start.1},
        "end": {"line": end.0, "character": end.1},
    })
}

/// ERR-1: "Distinguish source rejection, execution failure, ...". A
/// syntax error rejects the source: it is in the report, and the check
/// exits with status 1. Both of the file's errors are there: the `{`
/// where a parameter is missing, and what is missing at the end of the
/// file, which is after its last line break: line 2, character 0.
#[test]
fn json_reports_a_syntax_error_and_exits_one() {
    let root = scratch("json-syntax");
    let file = write(&root, "a.hgl", "module m\nfn scale( {}\n");
    let (status, report) = json_check(&[&file]);
    assert_eq!(status, Some(1));
    let items = report["diagnostics"].as_array().unwrap();
    assert_eq!(items.len(), 2, "{report}");
    for item in items {
        assert_eq!(item["severity"], "error", "{report}");
        assert_eq!(item["category"], "parse", "{report}");
        assert_eq!(item["code"], "syntax.expected_token", "{report}");
    }
    assert_eq!(items[0]["primary"], place(&file, (1, 10), (1, 11)));
    assert_eq!(items[1]["primary"], place(&file, (2, 0), (2, 0)));
}

/// LEX-16: "One U+FEFF at the start of a source is ignored." A column
/// of the first line counts from after it: `1bad` is characters 7 to 11
/// of line 0, as it is in the file without the mark.
#[test]
fn json_columns_of_the_first_line_count_after_a_byte_order_mark() {
    let root = scratch("json-bom");
    let file = write(&root, "a.hgl", "\u{feff}module 1bad\n");
    let (status, report) = json_check(&[&file]);
    assert_eq!(status, Some(1));
    let items = report["diagnostics"].as_array().unwrap();
    assert!(!items.is_empty(), "{report}");
    for item in items {
        assert_eq!(item["primary"], place(&file, (0, 7), (0, 11)), "{report}");
    }
}

/// LEX-15: "A line break is a line feed, a carriage return immediately
/// followed by a line feed, or a carriage return alone." In a file whose
/// lines end in a carriage return alone, the second `scale` (UNIT-5) is
/// on line 2, and the first, the related place, on line 1.
#[test]
fn json_counts_lines_that_end_in_a_lone_carriage_return() {
    let root = scratch("json-cr");
    let file = write(&root, "a.hgl", "module m\rfn scale() {}\rfn scale() {}\r");
    let (status, report) = json_check(&[&file]);
    assert_eq!(status, Some(1));
    let items = report["diagnostics"].as_array().unwrap();
    assert_eq!(items.len(), 1, "{report}");
    assert_eq!(items[0]["code"], "name.duplicate");
    assert_eq!(items[0]["rule"], "UNIT-5");
    assert_eq!(items[0]["primary"], place(&file, (2, 3), (2, 8)));
    assert_eq!(
        items[0]["related"][0]["location"],
        place(&file, (1, 3), (1, 8))
    );
}

/// What is missing at the end of a file with no line break after its
/// last line is at the end of that line: line 1, character 9.
#[test]
fn json_places_an_error_at_the_end_of_a_file_with_no_final_newline() {
    let root = scratch("json-eof");
    let file = write(&root, "a.hgl", "module m\nfn scale(");
    let (status, report) = json_check(&[&file]);
    assert_eq!(status, Some(1));
    let items = report["diagnostics"].as_array().unwrap();
    assert_eq!(items.len(), 1, "{report}");
    assert_eq!(items[0]["code"], "syntax.expected_token");
    assert_eq!(items[0]["primary"], place(&file, (1, 9), (1, 9)));
}

/// LEX-16: "Source that is not well-formed UTF-8 is a source error."
/// The file is reported once, as `syntax.invalid_encoding`, with a range
/// of no width at the first byte that is not UTF-8: line 1, character 0.
#[test]
fn json_reports_a_file_that_is_not_utf8_once() {
    let root = scratch("json-encoding");
    let path = root.join("a.hgl");
    fs::write(&path, b"module m\n\xff\n").unwrap();
    let file = argument(&path);
    let (status, report) = json_check(&[&file]);
    assert_eq!(status, Some(1));
    let items = report["diagnostics"].as_array().unwrap();
    assert_eq!(items.len(), 1, "{report}");
    assert_eq!(items[0]["category"], "parse");
    assert_eq!(items[0]["code"], "syntax.invalid_encoding");
    assert_eq!(items[0]["rule"], "LEX-16");
    assert_eq!(items[0]["primary"], place(&file, (1, 0), (1, 0)));
}

/// UNIT-12: "outside core's mandatory module neither a declaration nor
/// an import alias takes a prelude name". A program that declares a
/// function core's module has is rejected, and core's declaration is
/// the related place. That is in the core the compiler supplies, which
/// was read from no path: its file is the compiler's display name for it.
#[test]
fn json_names_a_file_of_the_supplied_core_by_its_display_name() {
    let root = scratch("json-core");
    let file = write(
        &root,
        "a.hgl",
        "module m\nexport const fn log_debug(logger: Logger, message: str) {\n}\n",
    );
    let (status, report) = json_check(&[&file]);
    assert_eq!(status, Some(1));
    let items = report["diagnostics"].as_array().unwrap();
    assert_eq!(items.len(), 1, "{report}");
    assert_eq!(items[0]["code"], "name.duplicate");
    assert_eq!(items[0]["rule"], "UNIT-12");
    assert_eq!(items[0]["primary"]["file"], file.as_str());
    assert_eq!(
        items[0]["related"][0]["location"]["file"],
        "<core>/core/core.hgl"
    );
}

/// UNIT-5: "Duplicate ordinary functions of the same name/role ... in
/// one scope are rejected." Two source roots given together are one
/// program, with one report: the second `scale` is in one root's file
/// and the first, the related place, in the other's, each named by its
/// path under the root it was found in.
#[test]
fn json_reports_two_source_roots_together() {
    let root = scratch("json-roots");
    let first = write(&root, "one/a.hgl", "module m\nfn scale() {}\n");
    let second = write(&root, "two/b.hgl", "module m part second\nfn scale() {}\n");
    let (status, report) =
        json_check(&[&argument(&root.join("one")), &argument(&root.join("two"))]);
    assert_eq!(status, Some(1));
    let items = report["diagnostics"].as_array().unwrap();
    assert_eq!(items.len(), 1, "{report}");
    assert_eq!(items[0]["code"], "name.duplicate");
    assert_eq!(items[0]["primary"], place(&second, (1, 3), (1, 8)));
    assert_eq!(
        items[0]["related"][0]["location"],
        place(&first, (1, 3), (1, 8))
    );
}
