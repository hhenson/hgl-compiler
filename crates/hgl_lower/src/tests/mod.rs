//! Real analysed programs for tests of the facts lowering exposes to targets.

use hgl_analyser::hir::{Body, Expr, Stmt};
use hgl_analyser::{Detail, Module, ParsedFile, Program, Signature, analyse};
use hgl_diagnostics::{Diagnostics, render};
use hgl_lexer::{lex, literal};
use hgl_parser::{Entry, Limits, parse};
use hgl_source::SourceMap;

mod copies;
mod instances;
mod types;

/// Analyse the fixtures with the pinned prelude, as the compiler does.
fn program(files: &[(&str, &str)]) -> Program {
    let mut sources = SourceMap::new();
    let mut diagnostics = Diagnostics::new();
    let mut parsed = Vec::new();
    for &(name, text) in files.iter().chain(&hgl_analyser::CORE_FILES) {
        let file = sources.add(name, text).expect("a distinct fixture path");
        let tokens = lex(&sources, file, &mut diagnostics);
        literal::validate_literals(&sources, file, &tokens, &mut diagnostics);
        let tree = parse(
            &sources,
            file,
            &tokens,
            Entry::SourceFile,
            Limits::default(),
            &mut diagnostics,
        );
        parsed.push(ParsedFile::new(file, tree.syntax()));
    }
    let program = analyse(&parsed, &mut diagnostics);
    assert!(
        !diagnostics.has_errors(),
        "{}",
        render::to_string(&sources, &diagnostics)
    );
    program
}

/// The declaration index in a fixture with one declaration of this name.
fn declaration(module: &Module, name: &str) -> usize {
    module
        .declarations
        .iter()
        .position(|declaration| declaration.name == name)
        .unwrap_or_else(|| panic!("fixture declaration {name}"))
}

fn body<'p>(program: &'p Program, path: &str, name: &str) -> &'p Body {
    let module = program.module(path).expect("fixture module");
    module.bodies[declaration(module, name)]
        .as_ref()
        .expect("fixture function has a body")
}

fn signature<'p>(program: &'p Program, path: &str, name: &str) -> &'p Signature {
    let module = program.module(path).expect("fixture module");
    let Detail::Callable(signature) = &module.details[declaration(module, name)] else {
        panic!("{name} is a function");
    };
    signature
}

fn returned(body: &Body) -> &Expr {
    let Some(Stmt::Return {
        value: Some(value), ..
    }) = body.statements.last()
    else {
        panic!("fixture body returns a value");
    };
    value
}
