//! Run Core formatting and output through the ordinary C++ SDK driver.

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
const fn positional(const pattern: str, value: f64) -> str => format(pattern, value)
operator configured{const pattern: str = "{}"}(value: f64) -> str
impl const fn configured{const pattern: str}(value: f64) -> str => format(pattern, value)
const fn display_typed<T>(const pattern: str, value: T) -> str => format(pattern, value)
const fn relay_display<T>(const pattern: str, value: T) -> str => display_typed(pattern, value)
operator show_variant<T>{const pattern: str}(value: T) -> str
impl const fn show_variant{const pattern: str}(value: f64) -> str => format(pattern, value)
impl const fn show_variant{const pattern: str}(value: i64) -> str => pattern
const fn choose_variant<T>(const pattern: str, value: T) -> str
    requires T in {f64, i64} => show_variant{pattern: pattern}(value)
const fn required_variant<T>(const pattern: str, value: T) -> str
    requires show_variant(T) -> str => show_variant{pattern: pattern}(value)
const fn indexed_variant<...As>(const pattern: str, values: ...As) -> str =>
    show_variant{pattern: pattern}(values[0])
operator packed_format<T>{const pattern: str = "<{}>"}(value: T) -> str
impl const fn packed_format{const pattern: str}(value: f64) -> str => format(pattern, value)
impl const fn packed_format{const pattern: str}(value: i64) -> str => format(pattern, value)
const fn indexed_format<...As>(const pattern: str, values: ...As) -> str =>
    packed_format{pattern: pattern}(values[0])
const fn repeat_variant<T>(const pattern: str, value: T, turns: i64) -> str
    requires T in {f64, i64} {
    if turns == 0 { return choose_variant(pattern, value) }
    return repeat_variant(pattern + "x", value, turns - 1)
}
const fn identity_text(value: str) -> str => value
const UNREAD_PATTERN: str = identity_text("{:x}")
const fn apply_format(f: const fn(f64) -> str) -> str => f(1.5)
const fn captured_pattern(const pattern: str) -> str => apply_format(fn(value) => format(pattern, value))
const fn apply_typed_format<T>(f: const fn(T) -> str, value: T) -> str => f(value)
const fn capture_typed_pattern<T>(const pattern: str, value: T) -> str =>
    apply_typed_format(fn(item) => format(pattern, item), value)

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
    assert format("{:5}", true) == "true "
    assert format("{:>5}", true) == " true"
    assert format("{:0>5}", true) == "0true"
    assert display_typed("{:05}", 42) == "00042"
    assert display<"{:.2f}">(1.25) == "1.25"
    assert display_value<"{:x}">(42) == "value 2a"
    assert format(HEX, 255) == "ff"
    assert positional("{:.2f}", 1.5) == "1.50"
    assert configured(1.5) == "1.5"
    assert configured{pattern: "{:.2f}"}(1.5) == "1.50"
    let least = -9223372036854775807us - 1us
    assert format("{:%[S].9}", least) == "-9223372036854.775808000"
    let boundary = @0001-01-01T19:59:05.224192Z
    assert format("{:%T.9}", boundary) == "19:59:05.224192000"
}

# FMT-4 and FMT-5: character layout uses Unicode code points, and a
# width-only float field pads its default text.
test text_layout {
    assert format("{:>4}", "日") == "   日"
    assert format("{:.2}", "日日日") == "日日"
    assert format("{:界^4.2s}", "日🙂!") == "界日🙂界"
    assert format("{:.0}", "é日") == ""
    assert format("{:>4}", "é") == "   é"
    assert format("{:>6}", 3.0) == "   3.0"
    assert format("{:9}", -0.0) == "     -0.0"
    assert format("{:*^7}", 3.0) == "**3.0**"
    assert format("{:>8}", 1e21) == "    1e21"
    assert format("{:+6}", 3.0) == "  +3.0"
    assert format("{:*> 6}", 3.0) == "** 3.0"
    assert format("{:06}", -3.0) == "-003.0"
    assert format("{:+06}", 3.0) == "+003.0"
    assert format("{:>06}", 3.0) == "   3.0"
    let infinity = 1.0 / 0.0
    assert format("{:06}", infinity) == "   inf"
    assert format("{:0>6}", infinity) == "000inf"
    assert format("{:0^7}", infinity) == "00inf00"
    assert format("{:0<6}", infinity) == "inf000"
    assert format("{:>6.1f}", 3.0) == "   3.0"
}

# FMT-4 and OPS-6: a sign or zero flag applies to the default f64
# text even without a field width.
test default_float_signs {
    assert format("{:+}", 3.0) == "+3.0"
    assert format("{:-}", 3.0) == "3.0"
    assert format("{: }", 3.0) == " 3.0"
    assert format("{:0}", 3.0) == "3.0"
    assert format("{:+}", -0.0) == "-0.0"
    assert format("{:+}", 1.0e21) == "+1e21"
    let infinity = 1.0 / 0.0
    let nan = infinity - infinity
    assert format("{:+}", nan) == "+nan"
    assert format("{:-}", nan) == "nan"
    assert format("{: }", nan) == " nan"
    assert format("{:+8}", 3.0) == "    +3.0"
    assert format("{:+.2f}", 3.0) == "+3.00"
}

const fn show_nullable<T>(value: T?) -> str => format("{}", value)

# FMT-5 and GEN-7: value formatting selects the payload's conversion.
test nullable_formats {
    let members: list<str> = ["日"]
    assert show_nullable<i64>(42) == "42"
    assert show_nullable<i64>(null) == "null"
    assert show_nullable<str>("日") == "日"
    assert show_nullable<str>(null) == "null"
    assert show_nullable<list<str>>(members) == "[\"日\"]"
    assert show_nullable<list<str>>(null) == "null"
    assert show_nullable<FormatMember>(FormatMember(number: 7)) == "7"
    assert show_nullable<FormatMember>(null) == "null"
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

test deferred_patterns {
    assert display_typed("{:x}", 255) == "ff"
    assert display_typed("{:x}", true) == format("{:x}", true)
    assert relay_display("{:.2f}", 1.5) == "1.50"
    assert choose_variant("{:.1f}", 1.25) == "1.2"
    assert choose_variant("{:.2f}", 1.25) == "1.25"
    assert choose_variant(UNREAD_PATTERN, 1) == "{:x}"
    assert repeat_variant("", 1, 3) == "xxx"
    assert captured_pattern("[{:.2f}]") == "[1.50]"
    assert captured_pattern("<{:.2f}>") == "<1.50>"
    assert capture_typed_pattern("{:x}", 255) == "ff"
    assert capture_typed_pattern("<{}>", 7) == "<7>"
    assert capture_typed_pattern("<{}>", 1.5) == "<1.5>"
}

# FMT-3, FN-19 and GEN-7: the named requirement and indexed pack
# resolve their candidates before deciding which const values are read.
test selected_operator_patterns {
    assert required_variant("{:.1f}", 1.5) == "1.5"
    assert required_variant("<{:.2f}>", 1.5) == "<1.50>"
    assert required_variant(UNREAD_PATTERN, 7) == "{:x}"
    assert indexed_variant(UNREAD_PATTERN, 7) == "{:x}"
    assert indexed_format("{:.1f}", 1.5) == "1.5"
    assert indexed_format("<{:.2f}>", 1.5) == "<1.50>"
    assert indexed_format("{:x}", 15) == "f"
}
"#;

/// FMT-3, #695: the target needs the analyser's checked pattern text.
#[test]
fn target_declines_unresolved_format_patterns() {
    let root = scratch("unresolved-format-constants");
    let cases = [
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

/// FN-1 and FMT-3: equivalent option values share a compiled body.
#[test]
fn format_options_key_instances_by_value() {
    let root = scratch("format-option-instance-keys");
    let file = write(
        &root,
        "keys.hgl",
        r#"module format_option_keys
const P: str = "{}"
const fn show{const pattern: str = "{}"}(value: f64) -> str => format(pattern, value)
test t {
    assert show(1.5) == "1.5"
    assert show{pattern: P}(1.5) == "1.5"
    assert show{pattern: "{}"}(1.5) == "1.5"
    assert show{pattern: "{:.2f}"}(1.5) == "1.50"
}
"#,
    );
    let output = root.join("cpp");
    let emitted = emit_format_program(&file, &output);
    assert!(emitted.status.success(), "{}", text(&emitted.stderr));
    let module = fs::read_to_string(output.join("modules/mod-format_option_keys.cpp")).unwrap();
    assert_eq!(
        module
            .lines()
            .filter(|line| *line == "namespace i_::show")
            .count(),
        2,
        "{module}"
    );
}

/// FMT-2 and GEN-13: reader traversal limits do not make an unbounded set
/// of instances. Keys count the decoded text.
#[test]
fn bounds_const_format_pattern_reads() {
    let root = scratch("const-format-pattern-budget");
    let mut chain = "const C0: str = \"<\"\n".to_owned();
    for level in 1..=70 {
        writeln!(chain, "const C{level}: str = C{}", level - 1).unwrap();
    }
    let relay = |pattern: &str| {
        format!("{chain}const fn relay(const pattern: str) -> str => show({pattern}, 1.5)\n")
    };
    let deep_left = relay("C70 + pattern");
    let deep_right = relay("pattern + C70");
    let short_right = relay("pattern + C60");
    let cases = [
        (
            "long",
            "",
            format!("show(\"{{}}{}\", 1.5)", "a".repeat(69_998)),
            false,
        ),
        (
            "escaped",
            "",
            format!("show(\"{{}}{}\", 1.5)", "\\n".repeat(33_000)),
            true,
        ),
        (
            "deep",
            chain.as_str(),
            "show(C70 + \"{}\", 1.5)".to_owned(),
            false,
        ),
        (
            "shallow",
            chain.as_str(),
            "show(C60 + \"{}\", 1.5)".to_owned(),
            true,
        ),
        (
            "relay_left",
            deep_left.as_str(),
            "relay(\"{}\")".to_owned(),
            false,
        ),
        (
            "relay_right",
            deep_right.as_str(),
            "relay(\"{}\")".to_owned(),
            false,
        ),
        (
            "relay_shallow",
            short_right.as_str(),
            "relay(\"{}\")".to_owned(),
            true,
        ),
    ];
    for (name, declarations, call, supported) in cases {
        let source = format!(
            "module {name}\n{declarations}\n\
             const fn show(const pattern: str, value: f64) -> str => format(pattern, value)\n\
             test t {{ assert {call} != \"\" }}\n"
        );
        let file = write(&root, &format!("{name}.hgl"), &source);
        let checked = hgl_in(&["check", &file], &[], &[]);
        assert!(
            checked.status.success(),
            "{name}: {}",
            text(&checked.stderr)
        );
        let output = root.join(name);
        let emitted = emit_format_program(&file, &output);
        let stderr = text(&emitted.stderr);
        if supported {
            assert!(emitted.status.success(), "{name}: {stderr}");
        } else {
            assert_eq!(emitted.status.code(), Some(1), "{name}: {stderr}");
            assert!(
                stderr.contains("build/target.unsupported"),
                "{name}: {stderr}"
            );
            assert!(!output.exists());
        }
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

/// Concatenated literals do not consume the declaration depth budget.
#[test]
fn reads_long_format_pattern_expressions() {
    let root = scratch("long-format-pattern-expression");
    let prefix = std::iter::repeat_n("\"a\"", 199)
        .collect::<Vec<_>>()
        .join(" + ");
    let bad_pattern = format!("{prefix} + \"{{:x}}\"");
    let cases = [
        ("inline", String::new(), bad_pattern.clone()),
        (
            "constant",
            format!("const PATTERN: str = {bad_pattern}"),
            "PATTERN".to_string(),
        ),
        (
            "pure_return",
            format!(
                "const fn suffix() -> str => \"{{:x}}\"\nconst PATTERN: str = {prefix} + suffix()"
            ),
            "PATTERN".to_string(),
        ),
    ];
    for (name, declarations, pattern) in cases {
        let source = format!(
            "module {name}\n{declarations}\nconst fn show() -> str => format({pattern}, 1.5)\n"
        );
        let file = write(&root, &format!("{name}.hgl"), &source);
        let checked = hgl_in(&["check", &file], &[], &[]);
        let stderr = text(&checked.stderr);
        assert_eq!(checked.status.code(), Some(1), "{name}: {stderr}");
        assert!(stderr.contains("format.specifier"), "{name}: {stderr}");
    }
    let good = format!("module good\nconst fn show() -> str => format({prefix} + \"{{}}\", 1.5)\n");
    let file = write(&root, "good.hgl", &good);
    let emitted = emit_format_program(&file, &root.join("good"));
    assert!(emitted.status.success(), "{}", text(&emitted.stderr));
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
    for test in [
        "stream_bytes",
        "typed_formats",
        "text_layout",
        "default_float_signs",
        "nullable_formats",
        "selected_converters",
        "deferred_patterns",
    ] {
        assert!(
            stdout.contains(&format!("passed   executable  demo.formatting::{test}")),
            "{stdout}"
        );
    }
    let results = root.join("results");
    let output = run_test_case(&kept, &results, "demo.formatting", "stream_bytes");
    assert!(output.status.success(), "{}", text(&output.stderr));
    assert_eq!(output.stdout, b"abcd\n\0eftick 7\n");
    assert_eq!(output.stderr, b"ijkl\nhandler\n");
    let recorded = fs::read_to_string(results).unwrap();
    assert_eq!(
        recorded.lines().collect::<Vec<_>>(),
        ["start 0", "passed 0"]
    );
}

/// Run one case directly so its process streams and result file can
/// be checked independently.
fn run_test_case(kept: &Path, results: &Path, module: &str, case: &str) -> std::process::Output {
    let name = module.replace('.', "-");
    let executable = kept
        .join("bin")
        .join(format!("test-{name}{}", std::env::consts::EXE_SUFFIX));
    let mut run = Command::new(executable);
    run.arg(results).arg(format!("{module}::{case}"));
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
    run.output().unwrap()
}

/// DBG-1, DBG-3 and DBG-9: the HGL candidate forwards both packs and
/// writes the lines where calls execute, outside the host's records.
#[test]
#[ignore = "needs hgraph's C++ SDK, named by HGL_HGRAPH_SDK"]
fn value_debug_print_runs_on_the_sdk() {
    if !sdk_named() {
        return;
    }
    let root = scratch("value-debug-print");
    let file = write(
        &root,
        "diagnostics.hgl",
        include_str!("../../../../spec/hgl_core/tests/core_conformance/diagnostics.hgl"),
    );
    let kept = root.join("build");
    let report = hgl_in(
        &["test", &file, "--target", "cpp", "--keep", &argument(&kept)],
        &[],
        &[],
    );
    let stdout = text(&report.stdout);
    assert!(report.status.success(), "{stdout}{}", text(&report.stderr));
    assert_eq!(
        stdout.lines().last(),
        Some("executable cases: 3 passed, 0 failed, 0 not run"),
        "{stdout}"
    );

    let results = root.join("results");
    for (case, expected) in [
        (
            "debug_print_writes_value_lines",
            "ready\nquote 100.00 / 102.00\nscore: 07\n",
        ),
        (
            "debug_print_writes_in_a_handler",
            "tick 1 is 10\ncount 1\ntick 2 is 40\ncount 2\n",
        ),
        (
            "debug_print_uses_explicit_marks_and_wall_time",
            "*7\n7\nwall ",
        ),
    ] {
        let output = run_test_case(&kept, &results, "core_conformance", case);
        assert!(output.status.success(), "{case}: {}", text(&output.stderr));
        assert!(output.stderr.is_empty(), "{case}: {}", text(&output.stderr));
        let printed = text(&output.stdout);
        if case == "debug_print_uses_explicit_marks_and_wall_time" {
            let year = printed
                .strip_prefix(expected)
                .and_then(|text| text.strip_suffix('\n'))
                .unwrap_or_else(|| panic!("{case}: {printed:?}"));
            assert!(
                year.len() == 4 && year.bytes().all(|byte| byte.is_ascii_digit()),
                "{case}: {printed:?}"
            );
        } else {
            assert_eq!(printed, expected, "{case}");
        }
        let recorded = fs::read_to_string(&results).unwrap();
        assert_eq!(
            recorded.lines().collect::<Vec<_>>(),
            ["start 0", "passed 0"],
            "{case}"
        );
    }
}

/// FN-14, FN-17 and FMT-3: specialized callbacks carry the const inputs
/// that determined their text, including from a node's handler.
#[test]
#[ignore = "needs hgraph's C++ SDK, named by HGL_HGRAPH_SDK"]
fn distinguishes_anonymous_formatters_by_enclosing_const_inputs() {
    if !sdk_named() {
        return;
    }
    let root = scratch("anonymous-format-const-inputs");
    let file = write(
        &root,
        "formatting.hgl",
        r#"module anonymous_const_keys
const fn apply_float(f: const fn(f64) -> str, v: f64) -> str => f(v)
const fn apply_typed<T>(f: const fn(T) -> str, v: T) -> str => f(v)
fn node_format(x: f64, const pattern: str) -> str {
    when { return apply_float(fn(v) => format(pattern, v), x) }
}
const fn value_format(const pattern: str) -> str =>
    format(pattern, 1.0) + "|" + apply_float(fn(v) => format(pattern, v), 1.5)
const fn generic_format<T>(const pattern: str, x: T) -> str =>
    format(pattern, 1.0) + "|" + apply_typed(fn(v) => format(pattern, v), x)
test node_first { assert eval node_format(x: [1.5], pattern: "<{}>") == ["<1.5>"] }
test node_second { assert eval node_format(x: [1.5], pattern: "[{}]") == ["[1.5]"] }
test value_first { assert value_format("<{:.1f}>") == "<1.0>|<1.5>" }
test value_second { assert value_format("[{:.1f}]") == "[1.0]|[1.5]" }
test generic_first { assert generic_format("<{:.1f}>", 1.5) == "<1.0>|<1.5>" }
test generic_second { assert generic_format("[{:.1f}]", 1.5) == "[1.0]|[1.5]" }
"#,
    );
    let report = hgl_in(&["test", &file, "--target", "cpp"], &[], &[]);
    let stdout = text(&report.stdout);
    assert!(report.status.success(), "{stdout}{}", text(&report.stderr));
    assert_eq!(
        stdout
            .matches("passed   executable  anonymous_const_keys::")
            .count(),
        6,
        "{stdout}"
    );
}

/// FN-1, FN-19 and GEN-13: a forwarding body's const input keys its
/// instances only where the instance reads it at compile time: one
/// pattern with one argument shape is one body, and a label read only
/// at run time keys none.
#[test]
fn forwarded_const_inputs_key_instances_by_demand() {
    let root = scratch("forwarded-const-input-keys");
    let file = write(
        &root,
        "keys.hgl",
        r#"module forwarded_keys
const fn count<...As>(const label: str, args: ...As) -> str => label
const fn relay<...As>(const label: str, args: ...As) -> str => count(label, ...args)
const fn bang(s: str) -> str => s + "!"
const fn called<...As>(const label: str, f: const fn(str) -> str, args: ...As) -> str =>
    f(label) + count(label, ...args)
const fn show<...As>(const p: str, args: ...As) -> str => format(p, ...args)
test t {
    assert relay("x", 1) == "x"
    assert relay("y", 2) == "y"
    assert called("x", bang, 1) == "x!x"
    assert called("y", bang, 1) == "y!y"
    assert show("{:x}", 255) == "ff"
    assert show("{:x}", 16) == "10"
    assert show("<{}>", 1) == "<1>"
}
"#,
    );
    let output = root.join("cpp");
    let emitted = emit_format_program(&file, &output);
    assert!(emitted.status.success(), "{}", text(&emitted.stderr));
    let module = fs::read_to_string(output.join("modules/mod-forwarded_keys.cpp")).unwrap();
    let instances = |name: &str| {
        module
            .lines()
            .filter(|line| *line == format!("namespace i_::{name}"))
            .count()
    };
    // FN-17: what a function parameter is given is read when it runs.
    assert_eq!(
        [
            instances("count"),
            instances("relay"),
            instances("called"),
            instances("show")
        ],
        [1, 1, 1, 2],
        "{module}"
    );
}

/// FN-1, FN-17 and FN-19: a label read only at run time, given to a
/// callback beside forwarded packs, is written whatever its length; a
/// pack entry given to a generic relay of a forwarding body runs.
#[test]
#[ignore = "needs hgraph's C++ SDK, named by HGL_HGRAPH_SDK"]
fn runs_run_time_labels_and_relayed_entries_beside_packs() {
    if !sdk_named() {
        return;
    }
    let root = scratch("forwarded-run-time-labels");
    let long = "a".repeat(70_000);
    let file = write(
        &root,
        "labels.hgl",
        &format!(
            r#"module forwarded_labels
const LONG: str = "{long}"
const fn bang(s: str) -> str => s + "!"
const fn count<...As>(const label: str, args: ...As) -> str => label
const fn relay<...As>(const label: str, f: const fn(str) -> str, args: ...As) -> str =>
    f(label) + count(label, ...args)
const fn logline<...As, ...Ks>(const fmt_str: str, args: ...As, kwargs: ...{{Ks}}) -> str
    => format(fmt_str, ...args, ...kwargs) + "\n"
const fn log_one<T>(const fmt_str: str, v: T) -> str => logline(fmt_str, v)
const fn first<...As>(const fmt_str: str, args: ...As) -> str => log_one(fmt_str, args[0])
test long_label {{ assert relay(LONG, bang, 1) == LONG + "!" + LONG }}
test relayed_entry {{ assert first("<{{:.1f}}>", 2.5, "x") == "<2.5>\n" }}
"#
        ),
    );
    let report = hgl_in(&["test", &file, "--target", "cpp"], &[], &[]);
    let stdout = text(&report.stdout);
    assert!(report.status.success(), "{stdout}{}", text(&report.stderr));
    assert_eq!(
        stdout
            .matches("passed   executable  forwarded_labels::")
            .count(),
        2,
        "{stdout}"
    );
}

/// FN-19 and FMT-3: a const pattern given beside forwarded packs, once or
/// twice, is written with the text the direct `format` call writes.
#[test]
#[ignore = "needs hgraph's C++ SDK, named by HGL_HGRAPH_SDK"]
fn runs_const_patterns_forwarded_beside_packs() {
    if !sdk_named() {
        return;
    }
    let root = scratch("forwarded-const-patterns");
    let file = write(
        &root,
        "formatting.hgl",
        r#"module forwarded_patterns
const fn both<...As, ...Ks>(const p: str, args: ...As, kwargs: ...{Ks}) -> str
    => format(p, ...args, ...kwargs)
const fn positional<...As>(const p: str, args: ...As) -> str => format(p, ...args)
const fn named<...Ks>(const p: str, kwargs: ...{Ks}) -> str => format(p, ...kwargs)
const fn outer<...As, ...Ks>(const p: str, args: ...As, kwargs: ...{Ks}) -> str
    => both(p, ...args, ...kwargs)
const fn typed<T>(const q: str, v: T) -> str => outer(q, v)
const fn helper<T>(const p: str, v: T) -> str => format(p, v)
const fn entry<...As>(const p: str, args: ...As) -> str => helper(p, args[0])
test both_packs { assert both("{} {name}", 1, name: "a") == "1 a" }
test positional_pack {
    assert positional("{:x}|{:>4}|{:.2f}", 255, "ab", 1.5)
        == format("{:x}|{:>4}|{:.2f}", 255, "ab", 1.5)
}
test named_pack { assert named("{b}-{a:>3}", a: 1, b: "x") == format("{b}-{a:>3}", a: 1, b: "x") }
test empty_packs { assert both("plain {{x}}") == "plain {x}" }
test twice {
    assert outer("{} {name}", 1, name: "a") == "1 a"
    assert outer("[{:.2f}]", 1.5) == "[1.50]"
}
test patterns_apart {
    assert positional("{:x}", 255) == "ff"
    assert positional("{:.1f}", 1.5) == "1.5"
    assert positional("{:x}", true) == format("{:x}", true)
    assert positional("<{}>", "s") == "<s>"
}
test through_generic { assert typed("{:x}", 255) == "ff" }
test pack_entry { assert entry("<{:.1f}>", 2.5) == "<2.5>" }
"#,
    );
    let report = hgl_in(&["test", &file, "--target", "cpp"], &[], &[]);
    let stdout = text(&report.stdout);
    assert!(report.status.success(), "{stdout}{}", text(&report.stderr));
    assert_eq!(
        stdout
            .matches("passed   executable  forwarded_patterns::")
            .count(),
        8,
        "{stdout}"
    );
}

/// FMT-9: a lifted native call uses its own node's cycle clock.
#[test]
#[ignore = "needs hgraph's C++ SDK, named by HGL_HGRAPH_SDK"]
fn engine_time_formatting_follows_the_lifted_nodes_ticks() {
    if !sdk_named() {
        return;
    }
    let root = scratch("engine-time-formatting");
    let file = write(
        &root,
        "demo.engine_time.hgl",
        include_str!("../../../hgl_cpp/tests/fixtures/demo.engine_time.hgl"),
    );
    let output = hgl_in(&["test", &file, "--target", "cpp"], &[], &[]);
    let stdout = text(&output.stdout);
    assert!(output.status.success(), "{stdout}{}", text(&output.stderr));
    assert!(
        stdout.contains("executable cases: 3 passed, 0 failed, 0 not run"),
        "{stdout}"
    );
    assert!(
        stdout.contains("rejection cases: 5 passed, 0 failed"),
        "{stdout}"
    );
}

/// FMT-9 admits the graph call; FMT-6's graph-start trigger is left to
/// the HGL graph candidate (#434), so emission must decline explicitly.
#[test]
fn engine_time_without_a_temporal_input_declines_without_a_cycle_context() {
    let root = scratch("engine-time-without-cycle");
    for (name, call) in [
        ("no_values", r#"format("{engine_time}")"#),
        ("scalar_value", r#"format("{engine_time} {}", 7)"#),
    ] {
        let source = format!(
            "module {name}\nfn describe() -> str => {call}\ntest clock {{ assert eval describe() == [\"\"] }}\n"
        );
        let file = write(&root, &format!("{name}.hgl"), &source);
        let checked = hgl_in(&["check", &file], &[], &[]);
        assert!(checked.status.success(), "{}", text(&checked.stderr));
        let target = root.join(name);
        let emitted = emit_format_program(&file, &target);
        let stderr = text(&emitted.stderr);
        assert_eq!(emitted.status.code(), Some(1), "{stderr}");
        assert!(stderr.contains("build/target.unsupported"), "{stderr}");
        assert!(stderr.contains("{engine_time}"), "{stderr}");
        assert!(stderr.contains("cycle context"), "{stderr}");
        assert!(!target.exists());
    }
}
