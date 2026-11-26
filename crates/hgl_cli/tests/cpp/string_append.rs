//! String append plans through the ordinary emit and SDK drivers.

use std::fs;

use super::{argument, hgl_in, scratch, sdk_named, text, write};

const SOURCE: &str = include_str!("../../../hgl_cpp/tests/fixtures/demo.string_append.hgl");

/// TYPE-1 and EXPR-3: the loop appends directly, and safe chains do not
/// build any concatenation. Other places keep their selected operation.
#[test]
fn emits_a_string_append_without_concatenating_the_prefix() {
    let root = scratch("string-append-emit");
    let source = write(&root, "demo.string_append.hgl", SOURCE);
    let out = root.join("out");
    let output = hgl_in(
        &["emit", &source, "--target", "cpp", "--out", &argument(&out)],
        &[],
        &[],
    );
    assert_eq!(output.status.code(), Some(0), "{}", text(&output.stderr));
    let written = fs::read_to_string(out.join("modules/mod-demo-string_append.cpp")).unwrap();

    let (_, loop_body) = written.split_once("// join_pieces(").unwrap();
    let (loop_body, _) = loop_body.split_once("return text;").unwrap();
    assert!(loop_body.contains("text.append(item);"), "{loop_body}");
    assert!(!loop_body.contains("add__str_str"), "{loop_body}");

    let (_, joined) = written.split_once("// joined(").unwrap();
    let (joined, _) = joined.split_once("return text;").unwrap();
    let mut remaining = joined;
    for letter in "abc0123456789".chars() {
        let append = format!("text.append(::hgraph::Str{{\"{letter}\"}});");
        let (_, next) = remaining
            .split_once(&append)
            .unwrap_or_else(|| panic!("{joined}"));
        remaining = next;
    }
    assert!(!joined.contains("add__str_str"), "{joined}");

    for place in ["saved", "held", "record.text", "::hglrt::item(items, 0)"] {
        assert!(
            written.contains(&format!("{place} = ::hglrt::add__str_str(")),
            "{place}"
        );
    }
    assert!(written.contains("text = ::hglrt::add__str_str(text, ::hgraph::Str{\"+\"});"));
}

/// FMT-5: repeated appends in each ordinary Core collection conversion
/// must reuse the output buffer throughout its traversal.
#[test]
fn core_collection_conversions_append_in_place() {
    let root = scratch("collection-append-emit");
    let source = write(
        &root,
        "collections.hgl",
        r#"module collections
const fn render(a: list<i64>, b: list<i64, 2>, c: list<i64?>, d: list<i64?, 2>,
          e: set<i64>, f: dict<str, i64>, g: dict<str, i64?>) -> list<str> =>
    [convert<str>(a), convert<str>(b), convert<str>(c), convert<str>(d),
     convert<str>(e), convert<str>(f), convert<str>(g)]
"#,
    );
    let out = root.join("out");
    let output = hgl_in(
        &["emit", &source, "--target", "cpp", "--out", &argument(&out)],
        &[],
        &[],
    );
    assert_eq!(output.status.code(), Some(0), "{}", text(&output.stderr));
    let written = fs::read_to_string(out.join("modules/mod-core.cpp")).unwrap();

    let mut conversions = 0;
    for candidate in written.split("// convert<").skip(1) {
        let signature = candidate.lines().next().unwrap();
        if !signature.contains("module core part collection_text") {
            continue;
        }
        let (_, traversal) = candidate.split_once("for (").unwrap();
        let (body, _) = traversal
            .split_once("\n    }")
            .unwrap_or_else(|| panic!("{candidate}"));
        assert!(body.contains("text.append(separator);"), "{candidate}");
        assert!(body.contains("text.append(::hgl::"), "{candidate}");
        let expected_appends = if signature.contains("dict<") { 4 } else { 2 };
        assert_eq!(
            body.matches("text.append(").count(),
            expected_appends,
            "{candidate}"
        );
        assert!(!body.contains("add__str_str"), "{candidate}");
        conversions += 1;
    }
    assert_eq!(conversions, 7);
}

/// TYPE-1 and EXPR-3: in-place append preserves the result, including
/// self-reads, saved aliases, RHS writes, and nested functions.
#[test]
#[ignore = "needs hgraph's C++ SDK, named by HGL_HGRAPH_SDK"]
fn string_append_runs_on_the_sdk() {
    if !sdk_named() {
        return;
    }
    let root = scratch("string-append-sdk");
    let source = write(&root, "demo.string_append.hgl", SOURCE);
    let output = hgl_in(&["test", &source, "--target", "cpp"], &[], &[]);
    let stdout = text(&output.stdout);
    assert_eq!(
        output.status.code(),
        Some(0),
        "{stdout}{}",
        text(&output.stderr)
    );
    assert!(
        stdout.ends_with("executable cases: 3 passed, 0 failed, 0 not run\n"),
        "{stdout}"
    );
}
