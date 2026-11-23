//! Run the first Core formatting slice through the ordinary C++ SDK driver.

use std::fs;
use std::process::Command;

use super::{argument, hgl_in, scratch, sdk_named, text, write};

const SOURCE: &str = r#"module demo.formatting

const fn display<const Pattern: str>(value: f64) -> str => format(Pattern, value)
const fn display_value<const Pattern: str, T>(value: T) -> str =>
    format("value " + Pattern, value)

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
    let least = -9223372036854775807us - 1us
    assert format("{:%[S].9}", least) == "-9223372036854.775808000"
    let boundary = @0001-01-01T19:59:05.224192Z
    assert format("{:%T.9}", boundary) == "19:59:05.224192000"
}
"#;

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
    for test in ["stream_bytes", "typed_formats"] {
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
