//! Helpers shared by the test files of this directory.
//!
//! Cargo compiles each file directly under `tests/` as a program of its
//! own. A file in a subdirectory is not one; the test files pull this one
//! in with `mod common;`.

// Each test file uses some of these helpers and not others; without this,
// every unused one would be a warning in that file's build.
#![allow(dead_code)]

use std::fs;
use std::path::{Path, PathBuf};

use hgl_diagnostics::{Category, Diagnostics, render};
use hgl_lexer::{Token, TokenKind, lex, literal};
use hgl_parser::{Entry, SyntaxKind, SyntaxNode, outline, parse};
use hgl_source::SourceMap;

// ---- Parsing ----

/// One diagnostic, reduced to what tests compare.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// The code, for example `syntax.expected_token`.
    pub code: String,
    /// True when the code is in the specification's catalogue.
    pub catalogued: bool,
    /// The rule cited, for example `LEX-6`.
    pub rule: Option<&'static str>,
    /// The message.
    pub message: String,
    /// Where it points, as `line:column`.
    pub at: String,
}

/// What lexing and parsing make of one text.
pub struct Outcome {
    /// The tree's root.
    pub tree: SyntaxNode,
    /// The diagnostics of both stages, in source order.
    pub findings: Vec<Finding>,
    /// The diagnostics as `hgl` prints them; empty when there are none.
    pub report: String,
}

impl Outcome {
    /// The tree's shape on one line. See [`hgl_parser::outline`].
    pub fn outline(&self) -> String {
        outline(&self.tree)
    }

    /// True when the tree holds an `Error` node.
    pub fn has_error_node(&self) -> bool {
        self.tree
            .descendants()
            .any(|node| node.kind() == SyntaxKind::Error)
    }

    /// The rule each diagnostic cites, `-` for none, in source order.
    pub fn rules(&self) -> Vec<&'static str> {
        self.findings
            .iter()
            .map(|finding| finding.rule.unwrap_or("-"))
            .collect()
    }
}

/// Lexes `text` and parses it as `entry`.
///
/// Checks what must hold whatever the text is: the tree's text is the
/// source text, the root is of the kind asked for, and every diagnostic is
/// a `parse` one that points inside the text.
pub fn run(entry: Entry, text: &str) -> Outcome {
    let mut sources = SourceMap::new();
    let file = sources.add("test.hgl", text).unwrap();
    let mut diagnostics = Diagnostics::new();
    let tokens = lex(&sources, file, &mut diagnostics);
    literal::validate_literals(&sources, file, &tokens, &mut diagnostics);
    let tree = parse(&sources, file, &tokens, entry, &mut diagnostics).syntax();
    diagnostics.sort_by_location();

    assert_eq!(tree.text().to_string(), text, "the tree is lossless");
    let root = match entry {
        Entry::SourceFile => SyntaxKind::SourceFile,
        Entry::Declarations | Entry::Statements => SyntaxKind::Fragment,
    };
    assert_eq!(tree.kind(), root);

    let source = sources.file(file);
    let findings = diagnostics
        .as_slice()
        .iter()
        .map(|diagnostic| {
            assert_eq!(diagnostic.category, Category::Parse);
            let span = diagnostic.primary.span;
            assert!(span.start <= span.end && span.end as usize <= text.len());
            Finding {
                code: diagnostic.code.as_str().to_string(),
                catalogued: diagnostic.code.is_catalogued(),
                rule: diagnostic.rule,
                message: diagnostic.message.clone(),
                at: source.line_col(span.start).to_string(),
            }
        })
        .collect();
    let report = if diagnostics.is_empty() {
        String::new()
    } else {
        render::to_string(&sources, &diagnostics)
    };
    Outcome {
        tree,
        findings,
        report,
    }
}

/// What a text is, from its first tokens: a file when it starts with a
/// `module` line, declarations when it starts as a declaration does, and
/// statements otherwise.
pub fn entry_for(text: &str) -> Entry {
    let mut sources = SourceMap::new();
    let file = sources.add("test.hgl", text).unwrap();
    let tokens: Vec<Token> = lex(&sources, file, &mut Diagnostics::new());
    let mut significant = tokens
        .iter()
        // The lexer does not count a line break as trivia: it may matter
        // (LEX-5). It does not matter to what the first two tokens are.
        .filter(|token| !token.kind.is_trivia() && token.kind != TokenKind::Newline)
        .map(|token| (token.kind, &text[token.span.range()]));
    let first = significant.next();
    let second = significant.next();
    match (first, second) {
        (Some((TokenKind::KwModule, _)), _) => Entry::SourceFile,
        (
            Some((
                TokenKind::KwUse
                | TokenKind::KwExport
                | TokenKind::KwImpl
                | TokenKind::KwFn
                | TokenKind::KwStruct
                | TokenKind::KwAbstract
                | TokenKind::KwEnum
                | TokenKind::KwOperator
                | TokenKind::KwInstantiate
                | TokenKind::KwTest
                | TokenKind::At,
                _,
            )),
            _,
        ) => Entry::Declarations,
        // `const` and `native` are names; they start a declaration when
        // `fn`, `const` or `type` follows.
        (Some((TokenKind::Ident, "const")), Some((TokenKind::KwFn, _))) => Entry::Declarations,
        (Some((TokenKind::Ident, "native")), Some((TokenKind::KwFn | TokenKind::Ident, _))) => {
            Entry::Declarations
        }
        _ => Entry::Statements,
    }
}

// ---- The specification's HGL ----

/// One piece of HGL source and where it came from.
pub struct Sample {
    /// For example `hgl_spec/spec/language/source-text.md:12`.
    pub origin: String,
    /// The source, with LF line breaks.
    pub text: String,
}

/// The `spec/` directory at the workspace root.
fn spec_root() -> PathBuf {
    // CARGO_MANIFEST_DIR is this crate's directory, set by Cargo when it
    // compiles the test. The workspace root is two levels up.
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../spec")
}

/// Collects every file under `directory`, depth first, in name order so the
/// tests visit files in the same order on every machine.
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
pub fn hgl_blocks(markdown: &str) -> Vec<(usize, String)> {
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

/// Every sample in the pinned specification: each fenced `hgl` block in
/// the Markdown under `spec/` and each `.hgl` file there.
pub fn corpus() -> Vec<Sample> {
    let root = spec_root();
    let mut files = Vec::new();
    files_under(&root, &mut files);

    let mut samples = Vec::new();
    for path in files {
        // `/` on every system, so that origins read alike everywhere.
        let origin = path
            .strip_prefix(&root)
            .unwrap_or(&path)
            .display()
            .to_string()
            .replace('\\', "/");
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
    // Start from LF whatever the checkout wrote: Git on Windows may have
    // given the files CRLF.
    for sample in &mut samples {
        sample.text = sample.text.replace("\r\n", "\n").replace('\r', "\n");
    }
    // An empty corpus means the submodules are not checked out. Passing
    // then would report success for a check that never ran.
    assert!(
        !samples.is_empty(),
        "no HGL found under spec/; run `git submodule update --init`"
    );
    samples
}
