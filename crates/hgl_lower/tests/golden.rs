//! Stable target-neutral graphs for END-2, CEVAL-2, TST-6, CTRL-7 and CTRL-15.
use hgl_analyser::{ParsedFile, analyse};
use hgl_diagnostics::Diagnostics;
use hgl_parser::{Entry, Limits, parse};
use hgl_source::SourceMap;

fn check(name: &str, source: &str, test: &str, golden: &str) {
    let mut sources = SourceMap::new();
    let mut diagnostics = Diagnostics::new();
    let mut files = vec![];
    for (name, text) in [(name, source)].iter().chain(&hgl_analyser::CORE_FILES) {
        // Golden spans use byte offsets in LF source, including the core submodule fixtures.
        let file = sources.add(*name, text.replace("\r\n", "\n")).unwrap();
        let tokens = hgl_lexer::lex(&sources, file, &mut diagnostics);
        let tree = parse(
            &sources,
            file,
            &tokens,
            Entry::SourceFile,
            Limits::default(),
            &mut diagnostics,
        );
        files.push(ParsedFile {
            file,
            root: tree.syntax(),
            omit: vec![],
        });
    }
    let p = analyse(&files, &mut diagnostics);
    assert!(!diagnostics.has_errors(), "{:?}", diagnostics.as_slice());
    let (m, i) = p
        .modules
        .iter()
        .find_map(|m| m.tests.iter().position(|t| t.name == test).map(|i| (m, i)))
        .unwrap();
    let cases = hgl_lower::test(&p, m, i).unwrap_or_else(|e| {
        panic!(
            "{} at {:?}",
            e.1,
            sources.file(e.0.file).line_col(e.0.span.start)
        )
    });
    let text = hgl_lower::dump(&cases);
    if std::env::var_os("UPDATE_HGL_GOLDENS").is_some() {
        std::fs::write(golden, &text).unwrap();
    }
    assert_eq!(std::fs::read_to_string(golden).unwrap(), text);
}

#[test]
fn scalar() {
    check(
        "scalar_eval.hgl",
        include_str!("../../../spec/hgl_core/tests/conformance/scalar_eval.hgl"),
        "replay_preserves_equal_ticks",
        "tests/scalar.golden",
    );
}

#[test]
fn keyed() {
    check(
        "demo.graph_for.hgl",
        include_str!("../../hgl_hgraph/tests/fixtures/demo.graph_for.hgl"),
        "a_dictionary_has_a_child_for_each_key",
        "tests/keyed.golden",
    );
}
