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
//! it), `nodes` and `graphs` (temporal functions), `tests` (named
//! tests), `apps` (applications).
//!
//! The temporal forms are written by families, which `forms`, the
//! dispatch point, looks each form up in: `scalars` (a scalar's temporal
//! form), `endpoints` (a hook's reads, endpoint operations and
//! capabilities), `nodes` (a node's hooks), `sources` (a source's body),
//! `graphs` (a graph's wiring), `lifting` (lifted calls, `capture`, the
//! temporal forms of `error` and `result`, a `?` in a `when`), `tests`
//! (evals and traces), `windows` (a window's temporal form), `references`
//! (a `ref` parameter's and result's designations), `apps` (the run's
//! store where a graph or an application sets it), `scheduling` (a
//! scheduler, an alarm, and a source's declarations and hooks),
//! `activity` (an input's activity, and an enum's temporal form),
//! `control` (`if`, `switch` and `for` over connections in a graph),
//! `nullables` (a nullable scalar's temporal form), `structs` (a struct's,
//! a tuple's and an atomic struct's), `keyed` (a set's and a
//! dictionary's), `lists` (a list's);
//! `lifecycle` writes a run's bounds for `tests` and `apps`. A construct
//! lane adds its family as a module here and one entry in
//! `forms::FAMILIES`.

use hgl_analyser::{DeclarationKind, Program};
use hgl_diagnostics::{Category, Code, Diagnostic, Diagnostics, Location};
use hgl_lower::instances::Instances;
use hgl_lower::pieces::Reach;

use crate::codes;

mod activity;
mod anonymous;
mod apps;
mod calls;
mod composites;
mod configuration;
mod control;
mod endpoints;
mod expressions;
mod formatting;
mod forms;
mod graphs;
mod keyed;
mod lifecycle;
mod lifting;
mod lists;
mod names;
mod nodes;
mod nullables;
pub(crate) mod readers;
mod references;
mod scalars;
mod scheduling;
mod sources;
mod statements;
mod structs;
mod tests;
mod types;
mod values;
mod windows;

pub(crate) use apps::APPLICATION;
pub(crate) use composites::Layout;
pub(crate) use names::{
    module_namespace, program_names, string, test_identifier, test_namespace, testing,
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
    /// Whether the piece uses the Core value-formatting header.
    pub(crate) formatting: bool,
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
    positions: crate::Positions<'_>,
    instances: &Instances,
    readers: &readers::Readers,
    diagnostics: &mut Diagnostics,
) -> Written {
    let mut emitter = Emitter {
        program,
        module: index,
        provides,
        position: positions.source,
        debugger: positions.debugger,
        lines: Vec::new(),
        instances,
        readers,
        diagnostics,
        piece: Piece::default(),
        expanding: Vec::new(),
        temporaries: 0,
        anonymous_number: 0,
        instanced: Vec::new(),
        temporal: None,
        ahead: Vec::new(),
        control: control::State::default(),
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
    /// The source position to show, where the driver gives one: the
    /// generated code then sets the HGL file and line of each statement
    /// with `#line` ([`Emitter::line_directive`]).
    position: Option<crate::SourcePosition<'a>>,
    /// Whether to add HGL positions for debugger stepping.
    debugger: bool,
    /// Where each line starts in each file a directive was written for: a
    /// place in the file, by which it is told apart, and the offsets.
    lines: Vec<(Location, Vec<usize>)>,
    /// The instances of generic declarations the program asks for.
    instances: &'a Instances,
    /// Schema variants for contextual reads in a node.
    readers: &'a readers::Readers,
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
    /// The module-wide sequence for private anonymous helpers (FN-17).
    anonymous_number: usize,
    /// The pieces written for the instances of the module's generic
    /// declarations so far: see [`Written::instances`].
    instanced: Vec<(usize, String, Piece)>,
    /// What the code being written is part of, where it is temporal: a
    /// node's hook, a source's body, a graph, a lifted call's node or an
    /// eval's entry (`nodes::Context`). `None` in value code.
    temporal: Option<nodes::Context>,
    /// What the piece being written puts ahead of its definition, at the
    /// scope of its namespace: the node an eval writes for a value
    /// function it lifts (`lifting`), which a test, a function, cannot
    /// hold with its name.
    ahead: Vec<String>,
    /// What the control family keeps while it writes a graph: what has a
    /// value where a child graph is written (`control`).
    control: control::State,
}

impl Emitter<'_> {
    /// Writes declaration `index` of the module into the piece.
    fn declaration(&mut self, index: usize) {
        let declared = &self.program.modules[self.module].declarations[index];
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
            // GEN-4: a temporal candidate is a node or a graph of its
            // module, as a temporal function is.
            DeclarationKind::Implementation(hgl_analyser::Role::Temporal, _) => {
                self.temporal_function(index);
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

    /// The `#line` directive that gives what follows it the HGL file and
    /// line of `at`, on a line of its own: a failure the C++ raises there
    /// names that place, as the runtime library's `std::source_location`
    /// gives it (`docs/architecture.md`, "The C++ runtime library").
    /// `assemble` gives what follows each piece its own file and line
    /// again. `None` where the driver gave no file names, or `at` is in no
    /// file of the program.
    fn line_directive(&mut self, at: Location) -> Option<String> {
        let position = self.position?;
        let line = self.hgl_line(at)?;
        Some(match position(at, line) {
            Some((name, line)) => format!("#line {line} {}\n", string(&name)),
            None => format!("{RESTORE}\n"),
        })
    }

    /// Maps a debug-only position and keeps ordinary C++ unchanged.
    fn debug_position(&mut self, at: Location) -> Option<String> {
        if self.debugger {
            self.line_directive(at)
        } else {
            None
        }
    }

    /// Gives these generated lines their HGL source in a debugger build.
    fn debug_stamp(&mut self, at: Location, text: String) -> String {
        match self.debug_position(at) {
            Some(directive) => stamped(&directive, &text),
            None => text,
        }
    }

    /// Gives the generated closing line the position at the end of its
    /// HGL body. Syntax spans end immediately after the closing token.
    fn debug_end(&mut self, mut at: Location, end: u32, text: String) -> String {
        at.span.end = end;
        at.span.start = end.saturating_sub(1);
        self.debug_stamp(at, text)
    }

    /// The line, counted from 1, that `at` is on in its file, read from the
    /// text of a declaration, a test or a statement of an application in
    /// that file, one of which holds every place a statement is written at.
    fn hgl_line(&mut self, at: Location) -> Option<usize> {
        let starts = match self.lines.iter().position(|(seen, _)| seen.file == at.file) {
            Some(known) => &self.lines[known].1,
            None => {
                let text = file_text(self.program, at)?;
                let starts: Vec<usize> = std::iter::once(0)
                    .chain(text.match_indices('\n').map(|(offset, _)| offset + 1))
                    .collect();
                self.lines.push((at, starts));
                &self.lines[self.lines.len() - 1].1
            }
        };
        let offset = at.span.start as usize;
        Some(starts.partition_point(|&start| start <= offset))
    }

    /// Ends the piece being written, and gives it.
    fn close(&mut self) -> Piece {
        self.temporaries = 0;
        if !self.ahead.is_empty() {
            let helpers = std::mem::take(&mut self.ahead).join("\n") + "\n";
            let mut prefix = helpers.clone();
            restore_after(&mut prefix, &helpers);
            let moved = prefix.matches('\n').count();
            for (line, _) in &mut self.piece.statements {
                *line += moved;
            }
            self.piece.text = prefix + &self.piece.text;
        }
        std::mem::take(&mut self.piece)
    }
}

/// The line written after code that sets the HGL file and line of its
/// statements with `#line` ([`Emitter::line_directive`]), which `assemble`
/// makes the directive that numbers what follows as the file's own again,
/// once the file's lines are all written: after each piece of a unit, and
/// after what a test writes ahead of itself (`tests`).
pub(crate) const RESTORE: &str = "// The lines of this file again, from here (#line).";

/// `text`, the code written for one HGL statement, with `directive`, the
/// statement's `#line` ([`Emitter::line_directive`]), before each of its
/// lines that does not follow a directive already. C++ numbers the line
/// after `#line N` as N and each line after it one more, so every line the
/// statement writes is given the statement's line: what it needs run
/// before it (`Scope::before`), the statement itself, and the lines of a
/// block around the statements inside it. Each statement inside it has
/// given its own lines its own line first, and keeps it. Restore the C++
/// location before the next compiler-generated line.
pub(crate) fn stamped(directive: &str, text: &str) -> String {
    let mut out = String::with_capacity(text.len() * 2);
    let mut after_directive = false;
    for line in text.lines() {
        let trimmed = line.trim_start();
        let is_directive = trimmed.starts_with("#line ") || trimmed == RESTORE;
        if !is_directive && !after_directive {
            out.push_str(directive);
        }
        out.push_str(line);
        out.push('\n');
        after_directive = is_directive;
    }
    if !out.is_empty() {
        out.push_str(RESTORE);
        out.push('\n');
    }
    out
}

/// Writes [`RESTORE`] on a line of its own at the end of `text`, where
/// `written`, the code just put there, sets an HGL line: a directive on a
/// line of its own, at any indent, as a window result's handlers are
/// written inside the evaluation that holds what they publish (`windows`).
pub(crate) fn restore_after(text: &mut String, written: &str) {
    if !written
        .lines()
        .any(|line| line.trim_start().starts_with("#line "))
    {
        return;
    }
    if !text.ends_with('\n') {
        text.push('\n');
    }
    text.push_str(RESTORE);
    text.push('\n');
}

/// The text of the file `at` is in: the root of the syntax tree of a
/// declaration, a test, a store statement or a run statement written in
/// it. `None` where none of the program's is.
fn file_text(program: &Program, at: Location) -> Option<String> {
    let node = program.modules.iter().find_map(|module| {
        let declarations = module
            .declarations
            .iter()
            .map(|declaration| (declaration.at, &declaration.syntax));
        let tests = module.tests.iter().map(|test| (test.at, &test.syntax));
        let store = module
            .store
            .iter()
            .map(|statement| (statement.at, &statement.syntax));
        let run = module
            .run
            .iter()
            .map(|statement| (statement.at, &statement.syntax));
        declarations
            .chain(tests)
            .chain(store)
            .chain(run)
            .find(|(place, _)| place.file == at.file)
            .map(|(_, syntax)| syntax.clone())
    })?;
    let root = node.ancestors().last().unwrap_or(node);
    Some(root.text().to_string())
}
