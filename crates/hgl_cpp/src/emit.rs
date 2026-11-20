//! Writes one module as C++, a piece at a time.
//!
//! The typed bodies the analyser made (`hgl_analyser::hir`) say what
//! each function does without its syntax. This walks them and writes
//! the C++ that does the same on hgraph. Each declaration and each named
//! test is a [`Piece`]: its declaration for the module's header, its
//! definition for the module's unit, the functions it names, and the
//! first form in it that is not written yet. `assemble` puts the pieces
//! into files, leaving out each that is not written or reaches one that
//! is not.
//!
//! The modules by concern, each saying what it writes: `names` (every
//! C++ name), `types` (types, and the declarations of structs and
//! enums), `values` (value functions, constants, statements and
//! expressions), `nodes` and `graphs` (temporal functions and
//! applications), `tests` (named tests).

use std::collections::BTreeSet;

use hgl_analyser::{DeclarationKind, Program};
use hgl_diagnostics::{Category, Code, Diagnostic, Diagnostics, Location};

use crate::codes;

mod graphs;
mod names;
mod nodes;
mod tests;
mod types;
mod values;

pub(crate) use names::{identifier, module_namespace, string, test_namespace};

/// What writing a module gave.
pub(crate) struct Written {
    /// A piece for each declaration of the module, in the order
    /// declared.
    pub(crate) declarations: Vec<Piece>,
    /// A piece for each named test of the module, in the order
    /// declared.
    pub(crate) tests: Vec<Piece>,
    /// For an application (UNIT-14), the piece its run statement is
    /// written as. `None` for a module.
    pub(crate) application: Option<Piece>,
}

/// What a piece names: a declaration, as the position of its module in
/// the program and its position in that module.
pub(crate) type Used = (usize, usize);

/// One top-level part of a module as it is written: a declaration or a
/// named test.
#[derive(Default)]
pub(crate) struct Piece {
    /// What the module's header declares of it: a function's prototype.
    /// Empty for a piece that declares nothing, and for a test, which
    /// its entry unit declares.
    pub(crate) declaration: String,
    /// Its definition, in the module's unit. Empty for one that defines
    /// nothing: an operator, or a native function, which the runtime
    /// library defines.
    pub(crate) text: String,
    /// The first form in it that the target does not write yet. A piece
    /// with one is left out of the files.
    pub(crate) broken: Option<Diagnostic>,
    /// The declarations of the program it names, whose definitions it
    /// needs.
    pub(crate) uses: BTreeSet<Used>,
    /// For a test, where each of its statements starts: the line of
    /// its text, counted from 0, and the place in the source.
    pub(crate) statements: Vec<(usize, Location)>,
}

/// Writes module `index` of `program`, reporting each form it does not
/// write to `diagnostics`. `provides` says whether the runtime library
/// defines a native, by the name `names` gives it.
pub(crate) fn module(
    program: &Program,
    index: usize,
    provides: &dyn Fn(&str) -> bool,
    diagnostics: &mut Diagnostics,
) -> Written {
    let mut emitter = Emitter {
        program,
        module: index,
        provides,
        diagnostics,
        piece: Piece::default(),
    };
    let declared = &program.modules[index];
    let declarations = (0..declared.declarations.len())
        .map(|declaration| {
            emitter.declaration(declaration);
            emitter.close()
        })
        .collect();
    let tests = (0..declared.tests.len())
        .map(|test| {
            emitter.test(test);
            emitter.close()
        })
        .collect();
    // UNIT-14: "It holds what a module holds, and one run statement".
    let application = declared.application.then(|| {
        emitter.application();
        emitter.close()
    });
    Written {
        declarations,
        tests,
        application,
    }
}

/// Writes the pieces of one module.
struct Emitter<'a> {
    /// The program.
    program: &'a Program,
    /// The position of the module being written in `program.modules`.
    module: usize,
    /// Whether the runtime library defines a native, by its name.
    provides: &'a dyn Fn(&str) -> bool,
    /// Where what is not written yet is reported.
    diagnostics: &'a mut Diagnostics,
    /// The piece being written.
    piece: Piece,
}

impl Emitter<'_> {
    /// Writes declaration `index` of the module into the piece.
    fn declaration(&mut self, index: usize) {
        let declared = &self.program.modules[self.module].declarations[index];
        let at = declared.at;
        match declared.kind {
            DeclarationKind::Function(hgl_analyser::Role::Value) => self.value_function(index),
            DeclarationKind::Function(hgl_analyser::Role::Temporal) => {
                self.temporal_function(index);
            }
            DeclarationKind::Constant => self.constant(at),
            DeclarationKind::Struct | DeclarationKind::Enum | DeclarationKind::NativeType => {
                self.type_declaration(declared.kind, at);
            }
            // An HGL candidate of an operator is a function of its
            // module (GEN-4).
            DeclarationKind::Implementation(..) => {
                self.unsupported(at, "a candidate of an operator with a body");
            }
            // An operator names its candidates and is no code of its
            // own. A native function or candidate is the runtime
            // library's (NATIVE-7), and a contract body describes it:
            // each call is checked where it is written.
            DeclarationKind::Operator
            | DeclarationKind::NativeFunction(_)
            | DeclarationKind::NativeImplementation(_)
            | DeclarationKind::NativeCandidate(..)
            | DeclarationKind::NativeCandidateImplementation(_) => {}
        }
    }

    /// Reports that the target does not write `what` yet, at `at`: the
    /// piece being written is broken, and is left out of the files.
    fn unsupported(&mut self, at: Location, what: &str) {
        let diagnostic = Diagnostic::error(
            Category::Build,
            Code::Local(codes::UNSUPPORTED),
            at,
            format!("the cpp target does not write {what} yet"),
        );
        if self.piece.broken.is_none() {
            self.piece.broken = Some(diagnostic.clone());
        }
        if !self.diagnostics.as_slice().contains(&diagnostic) {
            self.diagnostics.emit(diagnostic);
        }
    }

    /// Ends the piece being written, and gives it.
    fn close(&mut self) -> Piece {
        std::mem::take(&mut self.piece)
    }
}
