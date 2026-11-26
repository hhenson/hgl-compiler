//! Type values through the frontend, C++ emitter and native runtime.

use super::{hgl_in, scratch, sdk_named, text, write};

/// TYPE-18, GEN-13 and FMT-5: cold queries in constants, defaults,
/// generic functions, nodes and graphs retain type identity and text.
#[test]
#[ignore = "needs hgraph's C++ SDK, named by HGL_HGRAPH_SDK"]
fn type_values_run_on_the_sdk() {
    if !sdk_named() {
        return;
    }
    let root = scratch("type-values");
    let file = write(
        &root,
        "demo.type_values.hgl",
        include_str!("../../../hgl_cpp/tests/fixtures/demo.type_values.hgl"),
    );
    let kinds = write(
        &root,
        "demo.type_argument_kinds.hgl",
        include_str!("../../../hgl_cpp/tests/fixtures/demo.type_argument_kinds.hgl"),
    );
    let output = hgl_in(&["test", &file, &kinds, "--target", "cpp"], &[], &[]);
    let stdout = text(&output.stdout);
    assert_eq!(
        output.status.code(),
        Some(0),
        "{stdout}{}",
        text(&output.stderr)
    );
    assert!(
        stdout.ends_with(
            "rejection cases: 1 passed, 0 failed\nexecutable cases: 22 passed, 0 failed, 0 not run\n"
        ),
        "{stdout}"
    );
}
