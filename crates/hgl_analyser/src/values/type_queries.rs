//! Static operand types for Type constants read before body typing (TYPE-18).
//! The ordinary expression reader checks the operand; its HIR and effects
//! are discarded, so no second expression semantics is maintained here.

use std::cell::RefCell;

use hgl_diagnostics::{Diagnostics, Location};
use hgl_parser::{HasSpan, SyntaxNode};
use hgl_source::FileId;

use super::{Tables, Typer, tables};
use crate::{Declaration, Detail, Program, Signature, Type};

/// Read a query in its declaration's lexical environment. A body reader
/// supplies its already checked local types through the constant cache;
/// this path is for constants and defaults checked earlier in the pipeline.
pub(crate) fn query_type(program: &Program, file: FileId, query: &SyntaxNode) -> Option<Type> {
    let module = program
        .modules
        .iter()
        .find(|module| module.files.iter().any(|source| source.file == file))?;
    let found = module
        .declarations
        .iter()
        .enumerate()
        .find(|(_, declaration)| {
            let span = declaration.syntax.span();
            declaration.at.file == file
                && span.start <= query.span().start
                && query.span().end <= span.end
        });
    let stand_in = Declaration::stand_in(
        "type query",
        Location::new(file, query.span()),
        module.files.iter().position(|source| source.file == file)?,
        query,
        false,
    );
    let declaration = found.map_or(&stand_in, |(_, declaration)| declaration);
    let empty = Signature {
        generics: Vec::new(),
        parameters: Vec::new(),
        result: None,
        reference: false,
    };
    let signature = match found.and_then(|(position, _)| module.details.get(position)) {
        Some(Detail::Callable(signature)) => signature,
        _ => &empty,
    };
    let (bindings, written, calls) = tables(module);
    let selecting = RefCell::new(Vec::new());
    let mut diagnostics = Diagnostics::new();
    let mut typer = Typer::new(
        program,
        module,
        declaration,
        signature,
        Tables {
            bindings: &bindings,
            written: &written,
            calls: &calls,
            selecting: &selecting,
        },
        &mut diagnostics,
    );
    let operand = query.children().next()?;
    let ty = typer.expression(&operand, None).ty;
    (ty != Type::Unknown).then_some(ty)
}
