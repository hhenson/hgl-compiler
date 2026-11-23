//! Run the first Core formatting slice through the ordinary C++ SDK driver.

use std::fmt::Write;
use std::fs;
use std::path::Path;
use std::process::Command;

use super::{argument, hgl_in, scratch, sdk_named, text, write};

const SOURCE: &str = r#"module demo.formatting

const fn display<const Pattern: str>(value: f64) -> str => format(Pattern, value)
const fn display_value<const Pattern: str, T>(value: T) -> str =>
    format("value " + Pattern, value)
const fn hex_prefix() -> str { return "{:" }
const HEX: str = hex_prefix() + "x}"

struct FormatMember { number: i64 }
impl const fn convert(value: FormatMember) -> str => convert<str>(value.number)
impl const fn convert<N>(value: list<FormatMember, N>) -> str => "specific members"

fn counted(value: i64) -> i64 {
    when {
        write(format("tick {}\n", value), flush: true)
        write("handler\n", .err)
        return value + 1
    }
}

test stream_bytes {
    write("ab")
    write("cd\n\u{00}ef", .out, flush: true)
    write("ij", .err)
    write("kl\n", .err, flush: true)
    assert eval counted(value: [7]) == [8]
}

test typed_formats {
    assert display<"{:.2f}">(1.25) == "1.25"
    assert display_value<"{:x}">(42) == "value 2a"
    assert format(HEX, 255) == "ff"
    let least = -9223372036854775807us - 1us
    assert format("{:%[S].9}", least) == "-9223372036854.775808000"
    let boundary = @0001-01-01T19:59:05.224192Z
    assert format("{:%T.9}", boundary) == "19:59:05.224192000"
}

test selected_converters {
    let members: list<FormatMember, 1> = [FormatMember(number: 7)]
    let nested: list<list<FormatMember, 1>> = [members]
    let marked = Marked<list<list<FormatMember, 1>>>(value: nested, ticked: true)
    assert convert<str>(nested) == "[specific members]"
    assert format("{}", nested) == "[specific members]"
    assert format("{*}", marked) == "*[specific members]"
    assert display_value<"{}">(nested) == "value [specific members]"
}
"#;

/// FMT-3, #695: the target needs the analyser's checked pattern text.
#[test]
fn target_declines_unresolved_format_patterns() {
    let root = scratch("ordinary-format-constants");
    let cases = [
        (
            "positional",
            "const fn show(const p: str, v: f64) -> str => format(p, v)",
            "show(\"{:x}\", 1.5)",
        ),
        (
            "default_option",
            "const fn show{const p: str = \"{:x}\"}(v: f64) -> str => format(p, v)",
            "show(1.5)",
        ),
        (
            "given_option",
            "const fn show{const p: str = \"{}\"}(v: i64) -> str => format(p, v)",
            "show{p: \"{:c}\"}(65)",
        ),
        (
            "constant_from_input",
            "const fn pattern(s: str) -> str => s\nconst P: str = pattern(\"{:x}\")",
            "format(P, 1.5)",
        ),
        (
            "constant_from_block",
            "const fn pattern() -> str {\n    let s = \"{:x}\"\n    return s\n}\nconst P: str = pattern()",
            "format(P, 1.5)",
        ),
    ];
    for (name, declaration, call) in cases {
        let source = format!("module {name}\n{declaration}\ntest t {{ assert {call} == \"\" }}\n");
        let file = write(&root, &format!("{name}.hgl"), &source);
        assert_target_declines_pattern(&file, &root.join(name));
    }
}

/// Each declaration doubles its predecessor's text without doubling the
/// analyser's work. Programs beyond the reader's budget still check cleanly.
#[test]
fn bounds_doubling_format_patterns() {
    let root = scratch("doubling-format-patterns");
    for (name, depth, use_functions) in [
        ("readable", 14, true),
        ("large_function", 20, true),
        ("larger_function", 24, true),
        ("large_constant", 24, false),
    ] {
        let mut source = format!("module {name}\n");
        let mut expression = "\"x\"".to_string();
        for level in 0..=depth {
            let declaration = format!("p{level}");
            if use_functions {
                writeln!(source, "const fn {declaration}() -> str => {expression}").unwrap();
                expression = format!("{declaration}()");
            } else {
                writeln!(source, "const {declaration}: str = {expression}").unwrap();
                expression = declaration;
            }
            if level != depth {
                expression = format!("{expression} + {expression}");
            }
        }
        writeln!(source, "test t {{ assert format({expression}) != \"\" }}").unwrap();
        let file = write(&root, &format!("{name}.hgl"), &source);
        let output = root.join(name);
        if name == "readable" {
            let emitted = emit_format_program(&file, &output);
            assert!(emitted.status.success(), "{}", text(&emitted.stderr));
        } else {
            assert_target_declines_pattern(&file, &output);
        }
    }
}

fn emit_format_program(file: &str, output: &Path) -> std::process::Output {
    hgl_in(
        &["emit", file, "--target", "cpp", "--out", &argument(output)],
        &[],
        &[],
    )
}

fn assert_target_declines_pattern(file: &str, output: &Path) {
    let checked = hgl_in(&["check", file], &[], &[]);
    assert!(checked.status.success(), "{}", text(&checked.stderr));
    let emitted = emit_format_program(file, output);
    let stderr = text(&emitted.stderr);
    assert_eq!(emitted.status.code(), Some(1), "{stderr}");
    assert!(
        stderr.contains("does not write a format pattern whose text is not known here yet"),
        "{stderr}"
    );
    assert!(!output.exists());
}

/// OUT-1, OUT-2, FMT-4: keep the process streams independent of the test
/// host's publications, and format clock fractions for a minimum duration
/// and an early instant without converting the whole value to nanoseconds.
#[test]
#[ignore = "needs hgraph's C++ SDK, named by HGL_HGRAPH_SDK"]
fn core_format_and_write_run_on_the_sdk() {
    if !sdk_named() {
        return;
    }
    let root = scratch("native-formatting");
    let file = write(&root, "formatting.hgl", SOURCE);
    let kept = root.join("build");
    let report = hgl_in(
        &["test", &file, "--target", "cpp", "--keep", &argument(&kept)],
        &[],
        &[],
    );
    let stdout = text(&report.stdout);
    assert!(report.status.success(), "{stdout}{}", text(&report.stderr));
    for test in ["stream_bytes", "typed_formats", "selected_converters"] {
        assert!(
            stdout.contains(&format!("passed   executable  demo.formatting::{test}")),
            "{stdout}"
        );
    }
    let executable = kept.join("bin").join(format!(
        "test-demo-formatting{}",
        std::env::consts::EXE_SUFFIX
    ));
    let results = root.join("results");
    let mut run = Command::new(executable);
    run.arg(&results).arg("demo.formatting::stream_bytes");
    if cfg!(windows) {
        let sdk = std::env::var_os("HGL_HGRAPH_SDK").unwrap();
        let deps = std::env::var_os("HGL_HGRAPH_DEPS").unwrap();
        let path = std::env::var_os("PATH").unwrap_or_default();
        let libraries = [
            std::path::Path::new(&sdk).join("bin"),
            std::path::Path::new(&deps).join("bin"),
        ];
        run.env(
            "PATH",
            std::env::join_paths(libraries.into_iter().chain(std::env::split_paths(&path)))
                .unwrap(),
        );
    }
    let output = run.output().unwrap();
    assert!(output.status.success(), "{}", text(&output.stderr));
    assert_eq!(output.stdout, b"abcd\n\0eftick 7\n");
    assert_eq!(output.stderr, b"ijkl\nhandler\n");
    let recorded = fs::read_to_string(results).unwrap();
    assert_eq!(
        recorded.lines().collect::<Vec<_>>(),
        ["start 0", "passed 0"]
    );
}
