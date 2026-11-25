//! Boolean membership through typed inputs, results and dictionary key sets.

use super::{hgl_in, scratch, sdk_named, text, write};

/// TYPE-9, TIME-1/7 and SET-1: Boolean set members retain their value
/// through typed and erased SDK views, including additions and removals.
#[test]
#[ignore = "needs hgraph's C++ SDK, named by HGL_HGRAPH_SDK"]
fn boolean_key_collections_run_on_the_sdk() {
    if !sdk_named() {
        return;
    }
    let root = scratch("boolean-key-collections");
    let file = write(
        &root,
        "demo.bool_keys.hgl",
        include_str!("../../../hgl_cpp/tests/fixtures/demo.bool_keys.hgl"),
    );
    let output = hgl_in(&["test", &file, "--target", "cpp"], &[], &[]);
    let stdout = text(&output.stdout);
    assert_eq!(
        output.status.code(),
        Some(0),
        "{stdout}{}",
        text(&output.stderr)
    );
    assert!(stdout.contains("rejection cases: 0 passed, 0 failed\n"));
    assert!(
        stdout.ends_with("executable cases: 3 passed, 0 failed, 0 not run\n"),
        "{stdout}"
    );
}
