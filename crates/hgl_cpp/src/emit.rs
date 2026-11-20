//! Writes one module as C++, a piece at a time.
//!
//! The typed bodies the analyser made (`hgl_analyser::hir`) say what
//! each function does without its syntax. This walks them and writes
//! the C++ that does the same on hgraph. Each declaration and each named
//! test is a [`Piece`]: its declaration for the module's header, its
//! definition for the module's unit, and what it reaches, as
//! `hgl_lower::pieces` has it: the functions it names, and the first
//! form in it that is not written yet. `assemble` puts the pieces into
//! files, leaving out each that is not written or reaches one that is
//! not.
//!
//! The modules by concern, each saying what it writes: `names` (every
//! C++ name), `types` (types, and the declarations of enums),
//! `composites` (structs, sets, dictionaries and deltas as values),
//! `values` (value functions, their instances and constants),
//! `statements`, `expressions` and `calls` (value code, a test's among
//! it), `nodes` and `graphs` (temporal functions and applications),
//! `tests` (named tests).
//!
//! The temporal forms are written by families, which `forms`, the
//! dispatch point, looks each form up in: `scalars` (a scalar's temporal
//! form), `endpoints` (a hook's reads, endpoint operations and
//! capabilities), `nodes` (a node's hooks), `sources` (a source's body),
//! `graphs` (a graph's wiring and lifted calls), `tests` (evals and
//! traces). A construct lane adds its family as a module here and one
//! entry in `forms::FAMILIES`.

use hgl_analyser::{DeclarationKind, Program};
use hgl_diagnostics::{Category, Code, Diagnostic, Diagnostics, Location};
use hgl_lower::instances::Instances;
use hgl_lower::pieces::Reach;

use crate::codes;

mod calls;
mod composites;
mod endpoints;
mod expressions;
mod forms;
mod graphs;
mod names;
mod nodes;
mod scalars;
mod sources;
mod statements;
mod tests;
mod types;
mod values;

pub(crate) use composites::Layout;
pub(crate) use names::{
    identifier, module_namespace, program_names, string, test_namespace, testing,
};

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
    /// A piece for each instance of a generic declaration of the module
    /// (GEN-2): the declaration's position, the instance's suffix, and
    /// the piece, in the order the instances were asked for.
    pub(crate) instances: Vec<(usize, String, Piece)>,
}

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
    /// What it reaches (`hgl_lower::pieces`): the first form in it that
    /// the target does not write yet, which leaves it out of the files,
    /// and the declarations of the program it names, whose definitions
    /// it needs, an instance by its suffix (GEN-2).
    pub(crate) reach: Reach,
    /// For a test, where each of its statements starts: the line of
    /// its text, counted from 0, and the place in the source.
    pub(crate) statements: Vec<(usize, Location)>,
    /// The enums and structs of other modules it names, each as its
    /// module and its declaration: the header that declares the piece
    /// includes the types header of each one's module. A types header
    /// includes no module's header but another's types header, as an
    /// instance may name a type of a module that imports its own (GEN-2).
    pub(crate) types: std::collections::BTreeSet<(usize, usize)>,
    /// True for the declaration of a type: the module's types header
    /// holds each, which the module's header includes before any
    /// function, which may name it wherever it is declared (UNIT-6).
    pub(crate) is_type: bool,
    /// For the declaration of a struct, what the module's types header
    /// holds of it (`composites::Layout`); `None` for any other piece.
    /// An enum's is its `declaration`.
    pub(crate) layout: Option<Box<Layout>>,
}

/// Writes module `index` of `program`, reporting each form it does not
/// write to `diagnostics`. `provides` says whether the runtime library
/// defines a native, by the name `names` gives it; `instances` are the
/// instances of generic declarations the program asks for, each written
/// in its declaration's module.
pub(crate) fn module(
    program: &Program,
    index: usize,
    provides: &dyn Fn(&str) -> bool,
    instances: &Instances,
    diagnostics: &mut Diagnostics,
) -> Written {
    let mut emitter = Emitter {
        program,
        module: index,
        provides,
        instances,
        diagnostics,
        piece: Piece::default(),
        expanding: Vec::new(),
        temporaries: 0,
        instanced: Vec::new(),
        temporal: None,
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
        instances: emitter.instanced,
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
    /// The instances of generic declarations the program asks for.
    instances: &'a Instances,
    /// Where what is not written yet is reported.
    diagnostics: &'a mut Diagnostics,
    /// The piece being written.
    piece: Piece,
    /// The defaults being written, each as the position of its
    /// function's module, the function's position there and the
    /// parameter's: one that comes back while it is written is reported.
    expanding: Vec<(usize, usize, usize)>,
    /// How many names of its own the piece being written has made for
    /// what it holds while a statement runs: each is told apart by its
    /// number (`names::temporary`).
    temporaries: usize,
    /// The pieces written for the instances of the module's generic
    /// declarations so far: see [`Written::instances`].
    instanced: Vec<(usize, String, Piece)>,
    /// What the code being written is part of, where it is temporal: a
    /// node's hook, a source's body, a graph, a lifted call's node or an
    /// eval's entry (`nodes::Context`). `None` in value code.
    temporal: Option<nodes::Context>,
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
            DeclarationKind::Constant => self.constant(index),
            DeclarationKind::Struct | DeclarationKind::Enum | DeclarationKind::NativeType => {
                self.type_declaration(index);
            }
            // An HGL candidate of an operator is a function of its
            // module (GEN-4): a value one is a value function's.
            DeclarationKind::Implementation(hgl_analyser::Role::Value, _) => {
                self.value_function(index);
            }
            DeclarationKind::Implementation(hgl_analyser::Role::Temporal, _) => {
                self.unsupported(at, "a temporal candidate of an operator");
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
        if self.piece.reach.broken.is_none() {
            self.piece.reach.broken = Some(diagnostic.clone());
        }
        if !self.diagnostics.as_slice().contains(&diagnostic) {
            self.diagnostics.emit(diagnostic);
        }
    }

    /// Ends the piece being written, and gives it.
    fn close(&mut self) -> Piece {
        self.temporaries = 0;
        std::mem::take(&mut self.piece)
    }
}
