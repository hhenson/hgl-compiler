//! Every piece of HGL in the pinned specification lexes cleanly.
//!
//! The corpus is each fenced `hgl` block in the Markdown under `spec/` and
//! each `.hgl` file there. For every item the test checks that
//!
//! * the tokens cover the text exactly, in order, with no gap or overlap;
//! * no token is `Unknown`;
//! * lexing and literal validation report nothing.
//!
//! A second test rewrites every item with Windows (CRLF) and classic Mac
//! (CR) line breaks and checks that nothing but the line-break text changes.
//!
//! This is a smoke test of the lexer against real HGL, not conformance: it
//! says nothing about whether the token *kinds* are the right ones. It does
//! catch a symbol or literal form the specification uses that the lexer does
//! not know.

use std::fs;
use std::path::{Path, PathBuf};

use hgl_diagnostics::{Diagnostics, render};
use hgl_lexer::{Token, TokenKind, lex, literal};
use hgl_source::{LineCol, SourceMap};

/// One piece of HGL source and where it came from.
struct Sample {
    /// For example `spec/hgl_spec/spec/language/source-text.md:12`.
    origin: String,
    text: String,
}

/// The `spec/` directory at the workspace root.
fn spec_root() -> PathBuf {
    // CARGO_MANIFEST_DIR is this crate's directory, set by Cargo when it
    // compiles the test. The workspace root is two levels up.
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../spec")
}

/// Collects every file under `directory`, depth first, in name order so the
/// test visits files in the same order on every machine.
fn files_under(directory: &Path, found: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(directory) else {
        return;
    };
    let mut paths: Vec<PathBuf> = entries.flatten().map(|entry| entry.path()).collect();
    paths.sort();
    for path in paths {
        if path.is_dir() {
            files_under(&path, found);
        } else {
            found.push(path);
        }
    }
}

/// The fenced `hgl` blocks of a Markdown document, each with the 1-based
/// line number of its first line of code.
fn hgl_blocks(markdown: &str) -> Vec<(usize, String)> {
    let mut blocks = Vec::new();
    // `Some((first_line, text_so_far))` while inside a block.
    let mut open: Option<(usize, String)> = None;
    for (index, line) in markdown.lines().enumerate() {
        let fence = line.trim_start().starts_with("```");
        match (&mut open, fence) {
            (None, true) if line.trim() == "```hgl" => open = Some((index + 2, String::new())),
            (Some(_), true) => blocks.extend(open.take()),
            (Some((_, text)), false) => {
                text.push_str(line);
                text.push('\n');
            }
            _ => {}
        }
    }
    blocks
}

/// Every sample in the pinned specification.
fn corpus() -> Vec<Sample> {
    let root = spec_root();
    let mut files = Vec::new();
    files_under(&root, &mut files);

    let mut samples = Vec::new();
    for path in files {
        let origin = path
            .strip_prefix(&root)
            .unwrap_or(&path)
            .display()
            .to_string();
        let extension = path.extension().and_then(|extension| extension.to_str());
        match extension {
            Some("hgl") => {
                let text = fs::read_to_string(&path).expect("readable HGL file");
                samples.push(Sample { origin, text });
            }
            Some("md") => {
                let markdown = fs::read_to_string(&path).expect("readable Markdown file");
                for (line, text) in hgl_blocks(&markdown) {
                    samples.push(Sample {
                        origin: format!("{origin}:{line}"),
                        text,
                    });
                }
            }
            _ => {}
        }
    }
    samples
}

#[test]
fn the_specification_corpus_lexes_cleanly() {
    let samples = corpus();
    // An empty corpus means the submodules are not checked out. Passing
    // then would report success for a check that never ran.
    assert!(
        !samples.is_empty(),
        "no HGL found under spec/; run `git submodule update --init`"
    );

    // Shown by `cargo test -- --nocapture`.
    println!(
        "lexing {} samples from the pinned specification",
        samples.len()
    );

    let mut failures = Vec::new();
    for sample in &samples {
        let mut sources = SourceMap::new();
        let file = sources
            .add(sample.origin.clone(), sample.text.clone())
            .unwrap();
        let mut diagnostics = Diagnostics::new();
        let tokens = lex(&sources, file, &mut diagnostics);
        literal::validate_literals(&sources, file, &tokens, &mut diagnostics);

        // Lossless: each token starts where the last one ended, and the last
        // one ends at the end of the text.
        let mut position = 0;
        for token in &tokens {
            assert_eq!(
                token.span.start, position,
                "gap or overlap in {}",
                sample.origin
            );
            assert!(!token.span.is_empty(), "empty token in {}", sample.origin);
            position = token.span.end;
        }
        assert_eq!(
            position as usize,
            sample.text.len(),
            "text left over in {}",
            sample.origin
        );

        let unknown = tokens.iter().any(|token| token.kind == TokenKind::Unknown);
        if unknown || !diagnostics.is_empty() {
            failures.push(render::to_string(&sources, &diagnostics));
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {} samples did not lex cleanly:\n{}",
        failures.len(),
        samples.len(),
        failures.join("\n")
    );
}

/// What the line-break test compares for one token: its kind, where it
/// starts, and its text with every line break written as LF.
type Observation = (TokenKind, LineCol, String);

/// Lexes `text` and returns one [`Observation`] per token, with the number
/// of diagnostics reported.
fn observe(origin: &str, text: &str) -> (Vec<Observation>, usize) {
    let mut sources = SourceMap::new();
    let file = sources.add(origin, text).unwrap();
    let mut diagnostics = Diagnostics::new();
    let tokens: Vec<Token> = lex(&sources, file, &mut diagnostics);
    literal::validate_literals(&sources, file, &tokens, &mut diagnostics);
    let source = sources.file(file);
    let observations = tokens
        .iter()
        .map(|token| {
            // CRLF first, so its carriage return is not rewritten twice.
            let text = source
                .slice(token.span)
                .replace("\r\n", "\n")
                .replace('\r', "\n");
            (token.kind, source.line_col(token.span.start), text)
        })
        .collect();
    (observations, diagnostics.len())
}

#[test]
fn the_corpus_lexes_alike_under_every_line_break_form() {
    // LEX-15.
    // LF, CRLF and CR are each one line break and mean the same.
    let samples = corpus();
    assert!(
        !samples.is_empty(),
        "no HGL found under spec/; run `git submodule update --init`"
    );
    for sample in &samples {
        // Start from LF whatever the checkout wrote: Git on Windows may
        // have given the `.hgl` files CRLF.
        let with_lf = sample.text.replace("\r\n", "\n").replace('\r', "\n");
        let (expected, expected_diagnostics) = observe(&sample.origin, &with_lf);
        for (name, line_break) in [("CRLF", "\r\n"), ("CR", "\r")] {
            let rewritten = with_lf.replace('\n', line_break);
            let (observed, diagnostics) = observe(&sample.origin, &rewritten);
            assert_eq!(
                diagnostics, expected_diagnostics,
                "{} with {name} line breaks",
                sample.origin
            );
            // Same kinds, same line and column, same text once line breaks
            // are written alike. `{:#?}` is not used: the lists are long.
            assert_eq!(
                observed.len(),
                expected.len(),
                "{} with {name}",
                sample.origin
            );
            for (observed, expected) in observed.iter().zip(&expected) {
                assert_eq!(
                    observed, expected,
                    "{} with {name} line breaks",
                    sample.origin
                );
            }
        }
    }
}

#[test]
fn block_extraction_finds_only_hgl_fences() {
    let markdown = "\
intro
```hgl
let a = 1
```
```ebnf
a = b ;
```
- item
  ```hgl
  let b = 2
  ```
";
    let blocks = hgl_blocks(markdown);
    assert_eq!(
        blocks,
        [
            (3, "let a = 1\n".to_string()),
            (10, "  let b = 2\n".to_string())
        ]
    );
}
