//! Writes one module as Python.
//!
//! The typed bodies the analyser made (`hgl_analyser::hir`) say what
//! each function does without its syntax. This walks them and writes
//! the Python that does the same on hgraph. A form with no Python here
//! yet is reported once, where it is written, and `None` stands in its
//! place.
//!
//! A module is written a piece at a time: each declaration, then each
//! named test. A piece with a form that is not written is left out of
//! the file, and every piece records the functions it names. A test
//! can then be run when neither it nor anything it reaches was left
//! out, whatever else of the program was.

use std::collections::BTreeSet;

use hgl_analyser::hir::{
    Argument, BinaryOperator, Body, CalendarDate, Callee, Case, Cell, ClockTime, EndpointOperation,
    ErrorPart, Expr, ExprKind, Hook, Local, Run, Stmt, Temporal, Tenor, UnaryOperator, Value,
    ValueCategory,
};
use hgl_analyser::{
    Argument as GenericArgument, Constant, ConstantValue, Declaration, DeclarationKind, Detail,
    FieldRef, LocalKind, NamedKind, OperatorRef, Parameter, Phase, Program, Role, Scalar,
    Signature, Type,
};
use hgl_diagnostics::{Category, Code, Diagnostic, Diagnostics, Location};
use hgl_lower::calls::{LeftOut, field_default, known_duration, left_out, left_out_never_valid};
use hgl_lower::canonical::canonical_argument;
use hgl_lower::copies::{self, Copies, by_value, changed_in_place, may_hold, struct_in_place};
use hgl_lower::instances::{self, Instances};
use hgl_lower::nodes::{
    Designated, Handler, Timer, designated, designates, designations, is_input, is_out,
    is_reference, required_inputs, subscribed_inputs,
};
use hgl_lower::pieces::Reach;
use hgl_lower::types::{
    Asked, HELD_DEPTH, asked_children, empty_struct, field_of, fields_of, growing_list,
    holds_growing_list, inherited, is_nullable, payload_of, read_whole_all_valid, whole_value,
    windows_in,
};

use crate::{codes, provides, python_module, test_function};

mod types;
use types::*;

mod names;
use names::*;

mod annotate;
use annotate::*;

mod publish;
use publish::*;

mod node;
use node::*;

mod places;
use places::*;

mod calls;
use calls::*;

mod compose;
use compose::*;

mod expression;
use expression::*;

/// What writing a module gave.
pub(crate) struct Written {
    /// The module's Python text: every piece but those with a form that
    /// is not written yet.
    pub(crate) text: String,
    /// True when it calls a native function of core.
    pub(crate) natives: bool,
    /// True when it calls the eval harness.
    pub(crate) harness: bool,
    /// The other modules it imports, by module path.
    pub(crate) imports: BTreeSet<String>,
    /// A piece for each declaration of the module, in the order
    /// declared.
    pub(crate) declarations: Vec<Piece>,
    /// A piece for each named test of the module, in the order
    /// declared.
    pub(crate) tests: Vec<Piece>,
    /// A piece for each instance of a generic temporal declaration of
    /// the module (GEN-2), with the declaration's position and the
    /// instance's suffix, in the order written.
    pub(crate) instances: Vec<(usize, String, Piece)>,
    /// For an application (UNIT-14), the piece its run statement is
    /// written as: the function [`APPLICATION`]. `None` for a module.
    pub(crate) application: Option<Piece>,
}

/// The Python function an application's run statement is written as.
/// The `_` first makes the name the target's own: no declaration has
/// such a name.
pub(crate) const APPLICATION: &str = "_application";

/// One top-level part of a module as it is written: a declaration or a
/// named test.
#[derive(Default)]
pub(crate) struct Piece {
    /// Its Python text.
    text: String,
    /// What it reaches: the first form in it that is not written, and
    /// what it names.
    pub(crate) reach: Reach,
    /// True for a type, which is written before the functions: a
    /// function's annotations are read where it is defined, and may
    /// name a type the source declares after it.
    first: bool,
    /// For a test, where each of its statements starts: the line of
    /// the module's file, counted from 1, and the place in the source.
    /// A failure is reported by the line Python gives, and this says
    /// which statement of the test that is.
    pub(crate) statements: Vec<(usize, Location)>,
}

/// Writes module `index` of `program`. `instances` is the instances
/// of generic temporal declarations the program asks for.
pub(crate) fn module(
    program: &Program,
    index: usize,
    instances: &Instances,
    diagnostics: &mut Diagnostics,
) -> Written {
    let mut emitter = Emitter {
        program,
        module: index,
        instances,
        diagnostics,
        hgraph: false,
        natives: false,
        harness: false,
        enums: false,
        datetime: false,
        imports: BTreeSet::new(),
        expanding: Vec::new(),
        text: String::new(),
        prelude: String::new(),
        lifts: Vec::new(),
        lifted: 0,
        switched: 0,
        captured: 0,
        bound: Vec::new(),
        in_case: 0,
        looped: 0,
        ported: Vec::new(),
        unrolling: 0,
        results: Vec::new(),
        scheduled: None,
        empty_selected: false,
        designated: Vec::new(),
        candidates: BTreeSet::new(),
        propagates: false,
        given_back: Vec::new(),
        place_call: false,
        held: Vec::new(),
        hoisted: Vec::new(),
        piece: Piece::default(),
        declared: Vec::new(),
        instanced: Vec::new(),
        tested: Vec::new(),
        ran: None,
    };
    // UNIT-14: "It holds what a module holds, and one run statement":
    // what an application declares is written as a module's is, and
    // its run statement after it.
    emitter.declarations();
    emitter.tests();
    emitter.application();
    let text = emitter.finish();
    Written {
        text,
        natives: emitter.natives,
        harness: emitter.harness,
        imports: emitter.imports,
        declarations: emitter.declared,
        tests: emitter.tested,
        instances: emitter.instanced,
        application: emitter.ran,
    }
}

/// What kind of body is being written, which says how a name is read.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    /// A value function: every name is an ordinary Python value.
    Value,
    /// A node's hook: a temporal parameter is an hgraph input, read
    /// through `.value`, and a `state` is a field of the node's state.
    Node,
    /// A node's `start`: a function of its own to hgraph, which is
    /// given the node's configuration and state, and neither its inputs
    /// nor its output.
    Lifecycle,
    /// A node's `stop`: a function of its own to hgraph, which is given
    /// the node's configuration, state, inputs and any injected output.
    Stop,
    /// A source's statements (SOURCE-1): the body of a Python
    /// generator, which is resumed in an evaluation of the source's
    /// node. A `cache` and a `state` are fields of the node's state,
    /// and so is a capability, which the node puts there for the
    /// evaluation it is resumed in.
    Source,
    /// A graph: a temporal parameter is a port, passed on as it is.
    Graph,
    /// A named test: ordinary values, and evals.
    Test,
}

impl Mode {
    /// True where the statements are run as they are written, one
    /// after another: in a value function and in a node's hooks. A
    /// graph and a test are composition, where a loop and a `switch`
    /// say what is wired (CTRL-15).
    fn runs(self) -> bool {
        matches!(
            self,
            Mode::Value | Mode::Node | Mode::Lifecycle | Mode::Stop | Mode::Source
        )
    }
}

/// The function a body belongs to.
struct Function<'a> {
    /// Its locals, which the body's expressions name by position.
    locals: &'a [Local],
    /// Its signature; a test has none.
    signature: Option<&'a Signature>,
    /// How its names are read.
    mode: Mode,
    /// Which of its `let`s and loops take a copy of what they read,
    /// and which are references into its places.
    copies: Copies,
}

impl<'a> Function<'a> {
    /// The function `body` belongs to, read in `mode`: its locals, its
    /// signature (`None` for a test or a module's store, which have
    /// none) and the copies its body takes.
    fn of(
        program: &Program,
        body: &'a Body,
        signature: Option<&'a Signature>,
        mode: Mode,
    ) -> Function<'a> {
        Function {
            locals: &body.locals,
            signature,
            mode,
            copies: Copies::of(program, body),
        }
    }

    /// A function with no local in scope and no signature: what reads
    /// a module's constant, a default, or an application's run
    /// statement.
    fn cold(mode: Mode) -> Function<'a> {
        Function {
            locals: &[],
            signature: None,
            mode,
            copies: Copies::default(),
        }
    }

    /// The same body read in another `mode`: a node's `start` and
    /// `stop` read the locals the node's hooks read. The copies are
    /// the body's, so they are not worked out again.
    fn in_mode(&self, mode: Mode) -> Function<'a> {
        Function {
            locals: self.locals,
            signature: self.signature,
            mode,
            copies: self.copies.clone(),
        }
    }

    /// True when `local` is a temporal input of the node being written:
    /// a parameter that is not `const`.
    fn is_input(&self, local: usize) -> bool {
        is_input(self.locals, self.signature, local)
    }

    /// True when `local` is a temporal input of the node being written
    /// that is marked `ref` (FN-16): an endpoint whose value is not
    /// read, hgraph's `REF` input.
    fn is_reference(&self, local: usize) -> bool {
        is_reference(self.locals, self.signature, local)
    }

    /// True when `local` is an endpoint the node writes as hgraph's `REF`:
    /// a `ref` parameter, or `out` of a `ref` result (FN-16).
    fn designates(&self, local: usize) -> bool {
        self.is_reference(local)
            || (self.is_out(local) && self.signature.is_some_and(|signature| signature.reference))
    }

    /// True while a `when` of a node with a `ref` result is written:
    /// what the result designates is then kept by the provider's
    /// `Designation`, `_designation` (FN-16, TIME-10).
    fn designating(&self) -> bool {
        self.mode == Mode::Node && self.signature.is_some_and(|signature| signature.reference)
    }

    /// True when `local` is the node's `inject out` (TIME-7).
    fn is_out(&self, local: usize) -> bool {
        is_out(self.locals, local)
    }

    /// The type of what the port `value` reads in a hook holds: an
    /// input's or `out`'s declared type, and not the type the analyser
    /// reads it as, an atomic's V (TYPE-9) or the type under a `?` that
    /// marks omission (TYPE-14). hgraph keeps an atomic's value whole in
    /// its port, and its read is that value, not one made anew. Any
    /// other read is of its own type.
    fn port_type<'f>(&'f self, value: &'f Expr) -> &'f Type {
        match value.kind {
            ExprKind::Local(local) if self.is_input(local) || self.is_out(local) => {
                &self.locals[local].ty
            }
            _ => &value.ty,
        }
    }

    /// True when what the function being written publishes of `value`
    /// may be a null that is Python's `None`: its result's type is
    /// written `T?`, and `value` is a value of it. A value that is the
    /// nullable's delta is hgraph's own already, `Null` or the value
    /// (TYPE-16).
    ///
    /// It asks whether `value` is a nullable's `delta<T?>`, and is not
    /// `whole_value`, which asks whether `value` is the result's delta:
    /// the two part at an `atomic<T?>` result, whose delta is the `T?`
    /// a whole value has (`Type::delta`). `whole_value` calls such a
    /// `T?` value a delta; here it is a value whose null is published.
    fn publishes_null(&self, value: &Expr) -> bool {
        self.signature
            .and_then(|signature| signature.result.as_ref())
            .is_some_and(publishes_whole_null)
            && !matches!(&value.ty, Type::Delta(of) if is_nullable(of))
    }
}

/// The writer of one module.
struct Emitter<'a> {
    /// The program the module is of.
    program: &'a Program,
    /// The module's position in it.
    module: usize,
    /// The instances of generic temporal declarations the program asks
    /// for, each of which is written in its declaration's module.
    instances: &'a Instances,
    /// Where what cannot be written is reported.
    diagnostics: &'a mut Diagnostics,
    /// True once the text names something of hgraph.
    hgraph: bool,
    /// True once a native function of core is called.
    natives: bool,
    /// True once the eval harness is called.
    harness: bool,
    /// True once the module declares an enum, which is a Python `Enum`.
    enums: bool,
    /// True once the text names a type of Python's `datetime`, which is
    /// what a `duration` is: a `timedelta`.
    datetime: bool,
    /// The other generated modules this one names a function of, by
    /// module path.
    imports: BTreeSet<String>,
    /// The defaults being written in place of an argument or a field
    /// left out, each as its module, its declaration, and its parameter
    /// or its field among those the struct writes: a default is not
    /// written inside itself.
    expanding: Vec<(usize, usize, usize)>,
    /// The Python of the piece being written.
    text: String,
    /// The lifted nodes the piece being written uses, which are
    /// written before it.
    prelude: String,
    /// Those lifted nodes: the text of each before its name and after
    /// it, and its name. One lifted the same way twice in a piece is
    /// written once.
    lifts: Vec<((String, String), String)>,
    /// How many lifted nodes the module has been given names for.
    lifted: usize,
    /// How many `if`s and `switch`es over a connection have been
    /// written: each names its branches' graphs with its number.
    switched: usize,
    /// How many captures of a graph given a wiring-time value have been
    /// written: each names its graph with its number (ERR-7).
    captured: usize,
    /// The locals of the graph being written that have a value so far:
    /// its parameters, a `let` or `var` declared with one, a `var` once
    /// assigned. A branch's graph is given only these (CTRL-14).
    bound: Vec<usize>,
    /// How many branch graphs of a `switch_` enclose what is being
    /// written: a `return` of an ordinary value there is a constant.
    in_case: usize,
    /// How many loops of a graph that make one child for each member
    /// have been written: each names its child's graph with its number.
    looped: usize,
    /// The locals of the graph being written that the language has as
    /// ordinary values and hgraph gives as connections: the key or the
    /// member of a loop that makes one child for each member, which
    /// hgraph's `map_` gives its child as a port (`Emitter::each`).
    ported: Vec<usize>,
    /// How many loops unrolled where the graph is built enclose what is
    /// being written. A `return` under a branch selected at runtime
    /// there would need the passes after it as its other path.
    unrolling: usize,
    /// What each branch graph of a `switch_` that encloses what is
    /// being written gives, innermost last: what a `return` there gives.
    results: Vec<Type>,
    /// While the `when`s of a node that injects a scheduler or an alarm
    /// are written, what `scheduled()` is there (TIME-9): the name what
    /// woke the node is held under where the node is entered. `None`
    /// elsewhere, `start` and `stop` among them.
    scheduled: Option<String>,
    /// True while the body of a handler whose header selects
    /// `modified([])` is written: there it is true (TIME-3, "what the
    /// header selected is taken as nothing modified").
    empty_selected: bool,
    /// While the `when`s of a node with a `ref` result are written, each
    /// input it designates and the Python name of the `REF` input the
    /// provider's `Designation` reads it through (FN-16,
    /// `Emitter::designated_inputs`).
    designated: Vec<(usize, String)>,
    /// The Python names of the candidates the module has written.
    candidates: BTreeSet<String>,
    /// True once the value function being written has a `?`: a
    /// failure then leaves the function through an exception of the
    /// provider's, which the function catches and returns (ERR-5).
    propagates: bool,
    /// The `var` parameters of the value function being written, as
    /// Python names them (FN-15): what each holds when the function
    /// returns is given back to its caller, after the result.
    given_back: Vec<String>,
    /// True while the call that is the whole of a statement is
    /// written: one that assigns a place is written only there.
    place_call: bool,
    /// What is evaluated before a call that assigns places, each by
    /// where its expression is written, with the name it is held
    /// under: an index or a key of a place, which is evaluated once so
    /// that the place read and the place assigned are one (FN-15), and
    /// then each argument, so that they are evaluated in the order
    /// written (EXPR-2). While an argument is evaluated, what its
    /// indices are of is here too.
    held: Vec<(Location, String)>,
    /// Lines to write before the statement being written: an index of
    /// a place the provider goes in to, evaluated once and held
    /// (`steps`).
    hoisted: Vec<String>,
    /// What is known of the piece being written, but for its text.
    piece: Piece,
    /// The pieces written for the module's declarations.
    declared: Vec<Piece>,
    /// The pieces written for the instances of the module's generic
    /// temporal declarations: each with its declaration's position and
    /// its suffix.
    instanced: Vec<(usize, String, Piece)>,
    /// The pieces written for the module's named tests.
    tested: Vec<Piece>,
    /// The piece written for an application's run statement.
    ran: Option<Piece>,
}

impl<'a> Emitter<'a> {
    /// Reports a form this target does not write yet. The piece being
    /// written is then left out of the file.
    fn unsupported(&mut self, at: Location, what: &str) {
        let diagnostic = Diagnostic::error(
            Category::Build,
            Code::Local(codes::UNSUPPORTED),
            at,
            format!("the hgraph target does not write {what} yet"),
        );
        if self.piece.reach.broken.is_none() {
            self.piece.reach.broken = Some(diagnostic.clone());
        }
        // A declaration written by instance is written more than once,
        // and what one instance lacks another may lack too: it is
        // reported once.
        if !self.diagnostics.as_slice().contains(&diagnostic) {
            self.diagnostics.emit(diagnostic);
        }
    }

    /// Ends the piece being written, and gives it.
    fn close(&mut self) -> Piece {
        let mut piece = std::mem::take(&mut self.piece);
        // The lifted nodes it uses come first, and its statements are
        // then that many lines further down.
        let prelude = std::mem::take(&mut self.prelude);
        let lines = prelude.matches('\n').count();
        for (line, _) in &mut piece.statements {
            *line += lines;
        }
        piece.text = prelude + &std::mem::take(&mut self.text);
        self.lifts.clear();
        piece
    }

    /// Adds one line at `depth` levels of indentation.
    fn line(&mut self, depth: usize, text: &str) {
        for _ in 0..depth {
            self.text.push_str("    ");
        }
        self.text.push_str(text);
        self.text.push('\n');
    }

    /// The header and the import lines, then each piece that was
    /// written whole.
    fn finish(&mut self) -> String {
        let unit = &self.program.modules[self.module];
        let (kind, path) = (
            if unit.application {
                "application"
            } else {
                "module"
            },
            &unit.path,
        );
        let mut out = format!(
            "# Generated by the hgl compiler for hgraph, from {kind} `{path}`. Do not edit.\n"
        );
        // Each import has a name of the target's own, which starts with
        // `_` as no HGL name's Python form does: a program's `graph`,
        // `evaluate` or `float` is then no clash with hgraph's, the
        // harness's or Python's own.
        out.push_str("import builtins as _py\n");
        if self.datetime {
            out.push_str("import datetime as _dt\n");
        }
        if self.enums {
            out.push_str("import enum as _enum\n");
        }
        if self.hgraph {
            out.push_str("import hgraph as _hg\n");
        }
        if self.natives {
            out.push_str("import _hgl._core_native as _core\n");
        }
        if self.harness {
            out.push_str("import _hgl._harness as _harness\n");
        }
        // The types first, then the functions, then the tests, each in
        // the order declared.
        let (mut types, functions): (Vec<&mut Piece>, Vec<&mut Piece>) =
            self.declared.iter_mut().partition(|piece| piece.first);
        // The other modules the types name are imported before the
        // types, and the rest after them. Modules may import one
        // another in Python where they do not in the source: an
        // instance is written in its declaration's module with the
        // types of the module that calls it (GEN-2), and a call may
        // select a candidate of a module that imports this one (GEN-5).
        // A function's annotations are read where it is defined, so a
        // module must have its types before it loads a module whose
        // functions name them. A type's own definition names nothing
        // of another module today, so every such import is after the
        // types.
        let program = self.program;
        let early: BTreeSet<&str> = types
            .iter()
            .filter(|piece| piece.reach.broken.is_none())
            .flat_map(|piece| piece.reach.uses.iter())
            .filter(|(module, _, _)| *module != self.module)
            .map(|(module, _, _)| program.modules[*module].path.as_str())
            .collect();
        let import = |module: &str| {
            let python = python_module(module);
            format!("import {python} as {}\n", alias(module))
        };
        for module in &self.imports {
            if early.contains(module.as_str()) {
                out.push_str(&import(module));
            }
        }
        // A piece's statements were marked by the line of the piece
        // they start on, counted from 0, and are given the line of the
        // file, counted from 1: `lines` is the lines written so far.
        let mut lines = out.matches('\n').count();
        let write = |piece: &mut Piece, out: &mut String, lines: &mut usize| {
            if piece.reach.broken.is_some() {
                // It is in no file, so its statements are on no line.
                piece.statements.clear();
                return;
            }
            for (line, _) in &mut piece.statements {
                *line += *lines + 1;
            }
            *lines += piece.text.matches('\n').count();
            out.push_str(&piece.text);
        };
        for piece in types.iter_mut() {
            write(piece, &mut out, &mut lines);
        }
        let mut late = String::new();
        for module in &self.imports {
            if !early.contains(module.as_str()) {
                late.push_str(&import(module));
            }
        }
        if !late.is_empty() {
            // Two blank lines after the types, as before any piece.
            if types.iter().any(|piece| piece.reach.broken.is_none()) {
                late.insert_str(0, "\n\n");
            }
            lines += late.matches('\n').count();
            out.push_str(&late);
        }
        let instances = self.instanced.iter_mut().map(|(_, _, piece)| piece);
        for piece in functions
            .into_iter()
            .chain(instances)
            .chain(self.tested.iter_mut())
            .chain(self.ran.iter_mut())
        {
            write(piece, &mut out, &mut lines);
        }
        out
    }

    // ---- Declarations ----

    /// Every declaration of the module, in the order declared.
    fn declarations(&mut self) {
        let program = self.program;
        let module = &program.modules[self.module];
        for index in 0..module.declarations.len() {
            self.declaration(index);
            let piece = self.close();
            self.declared.push(piece);
        }
    }

    /// The declaration `index` of the module.
    fn declaration(&mut self, index: usize) {
        let program = self.program;
        let module = &program.modules[self.module];
        let declaration = &module.declarations[index];
        // A declaration of a test context is the tests' own, and is
        // written with them once they are.
        if declaration.test_only {
            self.unsupported(declaration.at, "a declaration of a test context");
            return;
        }
        let signature = match module.details.get(index) {
            Some(Detail::Callable(signature)) => Some(signature),
            _ => None,
        };
        let body = module.bodies.get(index).and_then(Option::as_ref);
        // GEN-2: a temporal function or candidate with generic
        // parameters is written once for each set of generic arguments
        // the program calls it with, and not at all where it calls it
        // with none. So is a value one whose body needs to know what
        // answers its requirements (GEN-16).
        if self.instances.instanced(program, self.module, index) {
            let candidate = declaration.kind.candidate().is_some();
            if candidate
                && !self.new_candidate(self.definition_name(self.module, index), declaration.at)
            {
                return;
            }
            // Each instance is a piece of its own: one with a form
            // that is not written is left out, and the others are not.
            let instances = self.instances;
            for instance in instances.declared(self.module, index) {
                let python = self.instance_name(self.module, index, &instance.suffix);
                if declaration.kind.role() == Some(Role::Value) {
                    self.value_function(&python, &instance.signature, &instance.body);
                } else {
                    self.node_or_graph(index, &python, &instance.signature, &instance.body);
                }
                let piece = self.close();
                self.instanced.push((index, instance.suffix.clone(), piece));
            }
            return;
        }
        match (declaration.kind, signature, body) {
            (DeclarationKind::Function(Role::Value), Some(signature), Some(body)) => {
                let python = self.definition_name(self.module, index);
                self.value_function(&python, signature, body);
            }
            // GEN-4: a value candidate of an operator with a body of
            // its own is a function as any other, under a name of the
            // target's: an operator may have many.
            (DeclarationKind::Implementation(Role::Value, _), Some(signature), Some(body)) => {
                let python = self.definition_name(self.module, index);
                if self.new_candidate(python.clone(), declaration.at) {
                    self.value_function(&python, signature, body);
                }
            }
            (DeclarationKind::Function(Role::Temporal), Some(signature), Some(body)) => {
                let python = self.definition_name(self.module, index);
                self.node_or_graph(index, &python, signature, body);
            }
            // GEN-4: "An `impl fn` is a temporal candidate of the
            // operator it names". It is a node or a graph as a temporal
            // function is, under a name of the target's.
            (DeclarationKind::Implementation(Role::Temporal, _), Some(signature), Some(body)) => {
                let python = self.definition_name(self.module, index);
                if self.new_candidate(python.clone(), declaration.at) {
                    self.node_or_graph(index, &python, signature, body);
                }
            }
            // An operator is a name for its candidates, which are the
            // functions: it has nothing of its own to write.
            (DeclarationKind::Operator, _, _) => {}
            (DeclarationKind::Enum, _, _) => self.enumeration(index),
            (DeclarationKind::Struct, _, _) => self.structure(index),
            (DeclarationKind::Constant, _, _) => self.constant(index),
            _ => {
                let what = match declaration.kind {
                    DeclarationKind::NativeType => "a native type",
                    kind if kind.candidate().is_some() => "an operator's candidate",
                    _ => "a native function",
                };
                self.unsupported(declaration.at, what);
            }
        }
    }

    /// True where `python` names no candidate of the module written
    /// before, and records it. Two candidates that differ only in what
    /// they require would be one Python function: the second, declared
    /// at `at`, is reported.
    fn new_candidate(&mut self, python: String, at: Location) -> bool {
        if self.candidates.insert(python) {
            return true;
        }
        self.unsupported(
            at,
            "a candidate with the parameter types of another of its module",
        );
        false
    }

    /// `const name: T = expression`, the declaration `index` of the
    /// module (UNIT-13): a function of no parameter that returns the
    /// expression, which the provider runs once. "The constant has that
    /// one value wherever it is read", and a read is a call of it.
    ///
    /// A function, and not a name bound where the module is loaded, so
    /// that the expression may name what is written after it (UNIT-6):
    /// it is run when the constant is first read, by when all of the
    /// module is defined.
    fn constant(&mut self, index: usize) {
        let program = self.program;
        let module = &program.modules[self.module];
        let declaration = &module.declarations[index];
        let Some(value) = module.constants.get(index).and_then(Option::as_ref) else {
            self.unsupported(declaration.at, "this constant");
            return;
        };
        self.natives = true;
        self.line(0, "");
        self.line(0, "");
        self.line(0, "@_core.constant");
        self.line(
            0,
            &format!("def {}():", self.definition_name(self.module, index)),
        );
        // It reads no parameter and no local (UNIT-13).
        let function = Function::cold(Mode::Value);
        self.propagates = false;
        let written = self.expression(&function, value);
        if std::mem::take(&mut self.propagates) {
            self.unsupported(value.at, "a `?` in a constant");
        }
        self.line(1, &format!("return {written}"));
    }

    /// `enum Side { buy, sell = 5 }`, the declaration `index` of the
    /// module: a Python `Enum`, each member with its number (TYPE-7:
    /// "Unnumbered numbering starts at zero and increments the previous
    /// member; explicit numbers reset the next value").
    ///
    /// It is made by a call and not as a class, so that a member may
    /// have any name: a class would take none that is a Python keyword.
    ///
    /// hgraph knows a Python enum by its class's name: two of one name
    /// are one type to it, and one named as an enum of hgraph's own,
    /// `CmpResult`, is taken for that. So the class is named with its
    /// module's path, `demo.orders::Side`, which no other enum is and
    /// no Python class can be.
    fn enumeration(&mut self, index: usize) {
        let program = self.program;
        let module = &program.modules[self.module];
        let declaration = &module.declarations[index];
        let members = match module.details.get(index) {
            Some(Detail::Enum(read)) => read.members.as_slice(),
            _ => &[],
        };
        let mut written = Vec::new();
        for member in members {
            match member.value {
                Some(number) => {
                    written.push(format!(
                        "({}, {number})",
                        string(&member_name(&member.name))
                    ));
                }
                None => self.unsupported(member.at, "a member whose number is not known"),
            }
        }
        self.enums = true;
        self.piece.first = true;
        self.line(0, "");
        self.line(0, "");
        self.line(
            0,
            &format!(
                "{} = _enum.Enum({}, [{}])",
                name(&declaration.name),
                string(&format!("{}::{}", module.path, declaration.name)),
                written.join(", ")
            ),
        );
    }

    /// `struct Point { x: f64, y: f64 }`, the declaration `index` of
    /// the module: a class the provider makes, with every field the
    /// struct has, its parents' first (TYPE-3: "Structs are nominal,
    /// with public ordered fields and no methods").
    ///
    /// A field's default is no part of the class: a construction that
    /// leaves the field out is written with the default in its place
    /// (see `construct`), as a call is for a parameter. An abstract
    /// struct is a class too, of which no value is made.
    fn structure(&mut self, index: usize) {
        let program = self.program;
        let module = &program.modules[self.module];
        let declaration = &module.declarations[index];
        let fields: Vec<String> = match module.details.get(index) {
            Some(Detail::Struct(read)) => {
                read.members.iter().map(|field| name(&field.name)).collect()
            }
            _ => Vec::new(),
        };
        // A struct that is a value is never changed where it is: the
        // provider gives it a new value for a change of a field, though
        // a parent's type stands for it (`by_value`).
        let by_value = if by_value(program, self.module, index) {
            ", by_value=True"
        } else {
            ""
        };
        self.natives = true;
        self.piece.first = true;
        self.line(0, "");
        self.line(0, "");
        self.line(
            0,
            &format!(
                "{} = _core.struct({}, {}{by_value})",
                name(&declaration.name),
                string(&declaration.name),
                tuple(&fields)
            ),
        );
    }

    /// `const fn name(a, b) -> T`: a plain Python function, `python`.
    /// `signature` and `body` are a declaration's own, or an
    /// instance's.
    fn value_function(&mut self, python: &str, signature: &Signature, body: &Body) {
        let mut parameters = Vec::new();
        for parameter in &signature.parameters {
            let python = name(&parameter.name);
            // FN-8: a pack captures the arguments the fixed parameters
            // leave, which a call records with no parameter of their
            // own: they are not written yet.
            if parameter.pack.is_some() {
                self.unsupported(parameter.at, "a pack parameter");
            }
            // A parameter that may be left out, one with a default or
            // one written `T?` (UNIT-8), is still always given: a call
            // that leaves it out is written with the default, or
            // `None`, in its place (see `arguments`). Python then has
            // no default to order or to evaluate.
            parameters.push(python);
        }
        self.line(0, "");
        self.line(0, "");
        self.line(0, &format!("def {python}({}):", parameters.join(", ")));
        let function = Function::of(self.program, body, Some(signature), Mode::Value);
        // FN-15: "what it holds when the function returns is what the
        // place is given". Python has no place to give a value to, so
        // the function returns what each `var` parameter holds after
        // its result, `(result, count)`, and the call assigns the
        // places.
        self.given_back = signature
            .parameters
            .iter()
            .filter(|parameter| parameter.var)
            .map(|parameter| name(&parameter.name))
            .collect();
        let start = self.text.len();
        self.propagates = false;
        self.block(&function, &body.statements, 1);
        if !self.given_back.is_empty()
            && !matches!(body.statements.last(), Some(Stmt::Return { .. }))
        {
            // Where the body may end with no `return`.
            let given = self.returned("None");
            self.line(1, &format!("return {given}"));
        }
        if std::mem::take(&mut self.propagates) {
            // ERR-5: "In a value function returning `result<U,E>`,
            // failure returns the same E as Err." A `?` is inside an
            // expression, where Python has no `return`: the provider
            // raises, and the function returns what was raised.
            self.tried(start, "_core.Propagated as _left");
            // What `?` passes on may hold what the function was given.
            // A result is the caller's to keep, so it is a copy, as
            // any result that is not the function's own is
            // (`given_out`).
            let failure = if signature
                .result
                .as_ref()
                .is_some_and(|result| changed_in_place(self.program, result, 0))
            {
                self.natives = true;
                "_core.copied(_left.failure)"
            } else {
                "_left.failure"
            };
            let given = self.returned(failure);
            self.line(2, &format!("return {given}"));
        }
        if !self.given_back.is_empty() {
            // FN-15: "A function that fails leaves the place holding
            // what its parameter held when it failed: no change it had
            // made is undone." What a `var` parameter of a type changed
            // where it is holds is the caller's own already, but a
            // parameter of any other type, and one given a value of
            // its own, is a name of this function's. So what fails is
            // caught, and given back in the result's place with what
            // each `var` parameter holds then: the call assigns each
            // place, and then raises it again
            // (`_core.raise_if_failed`).
            self.natives = true;
            self.tried(start, "_py.Exception as _failed");
            let given = self.returned("_core.Failed(_failed)");
            self.line(2, &format!("return {given}"));
        }
        self.given_back.clear();
    }

    /// Puts the lines of a function's body written since `start` under a
    /// `try:`, and writes the `except` that catches `caught` after them,
    /// for the lines of its handler to follow.
    fn tried(&mut self, start: usize, caught: &str) {
        let body: String = self.text[start..]
            .lines()
            .map(|line| format!("    {line}\n"))
            .collect();
        self.text.truncate(start);
        self.line(1, "try:");
        self.text.push_str(&body);
        self.line(1, &format!("except {caught}:"));
    }

    /// What a `return` of `value` gives: the value, or in a function
    /// with `var` parameters the value and then what each of them
    /// holds (FN-15).
    fn returned(&self, value: &str) -> String {
        if self.given_back.is_empty() {
            value.to_string()
        } else {
            format!("({value}, {})", self.given_back.join(", "))
        }
    }

    /// Every named test of the module: a function `test_name`, which a
    /// Python test runner finds by that name.
    fn tests(&mut self) {
        let program = self.program;
        let module = &program.modules[self.module];
        for (index, test) in module.tests.iter().enumerate() {
            match module.test_bodies.get(index).and_then(Option::as_ref) {
                Some(body) => {
                    self.line(0, "");
                    self.line(0, "");
                    self.line(0, &format!("def {}():", test_function(&test.name)));
                    let function = Function::of(self.program, body, None, Mode::Test);
                    self.block(&function, &body.statements, 1);
                }
                // The analyser gave the test no body to write.
                None => self.unsupported(test.at, "this test"),
            }
            let piece = self.close();
            self.tested.push(piece);
        }
    }

    /// An application's run statement (UNIT-14), as the function
    /// [`APPLICATION`], which runs the entry and returns the run's
    /// output.
    ///
    /// RUN-1: "its run statement is `run(call, config)` or `eval(call,
    /// config)`, where the call is of an entry ... giving the entry's
    /// const arguments, and the configuration is a value of core's
    /// `RunConfig`, or `RunConfig()` where none is written. The entry's
    /// result, if any, is the run's output". RUN-3: "The word of a run
    /// statement is its mode."
    ///
    /// The harness's `run` is given the entry, core's `record` for the
    /// entry's result, which the run wires to it (core's CRUN-8), or
    /// `None` where it has none, whether the run is in real time, and
    /// then the entry's arguments by name and the configuration as
    /// `_config`, as an eval's are ([`Emitter::eval`]).
    fn application(&mut self) {
        let program = self.program;
        let module = &program.modules[self.module];
        let Some(headed) = module.application_at() else {
            return;
        };
        // UNIT-14: "Between its declarations and its run statement it
        // may set the run's store: `inject global_state`, and then
        // calls of `put` on it. Each is evaluated once, in the order
        // written, before the run's graph is built". They are a
        // function of the store, which the harness calls with the
        // run's own before it builds the graph (core's CRUN-7).
        let seeded = match &module.store_body {
            Some(body) => {
                let setting = Function::of(self.program, body, None, Mode::Lifecycle);
                // The `inject` is the function's parameter.
                let puts: Vec<Stmt> = body
                    .statements
                    .iter()
                    .filter(|statement| !matches!(statement, Stmt::Inject { .. }))
                    .cloned()
                    .collect();
                self.hgraph = true;
                self.line(0, "");
                self.line(0, "");
                self.line(0, &format!("def {APPLICATION}_store(_store):"));
                self.block(&setting, &puts, 1);
                true
            }
            None => false,
        };
        self.line(0, "");
        self.line(0, "");
        self.line(0, &format!("def {APPLICATION}():"));
        match &module.run_body {
            Some(run) => {
                // The statement stands in no function: it has no local
                // to name, and is read as a test's statement is.
                let function = Function::cold(Mode::Test);
                let line = self.text.matches('\n').count();
                self.piece.statements.push((line, run.at));
                let mut written = self.run_statement(&function, run);
                if seeded {
                    // Given last, by a name no parameter has.
                    let call = written.strip_suffix(')').unwrap_or(&written).to_string();
                    written = format!("{call}, _store={APPLICATION}_store)");
                }
                self.line(1, &format!("return {written}"));
            }
            // The analyser gave the statement no reading.
            None => self.unsupported(headed, "this application's run statement"),
        }
        let piece = self.close();
        self.ran = Some(piece);
    }

    /// The run statement `run` as a call of the harness's `run`.
    fn run_statement(&mut self, function: &Function, run: &Run) -> String {
        let program = self.program;
        let ExprKind::Call {
            callee:
                Callee::Function {
                    module,
                    declaration,
                    generics,
                    ..
                },
            arguments,
            ..
        } = &run.call.kind
        else {
            self.unsupported(run.call.at, "this run statement");
            return "None".to_string();
        };
        let Some(Detail::Callable(signature)) = program.modules[*module].details.get(*declaration)
        else {
            self.unsupported(run.call.at, "this run statement");
            return "None".to_string();
        };
        self.harness = true;
        // CRUN-8: "The run wires `record` to the entry's result under
        // the key `"run.output"`". Core has `record` as HGL for any
        // type, so it is written for the type the entry publishes
        // (GEN-2).
        let record = match instances::run_result(program, run) {
            Some(result) => match instances::core_function(program, "record") {
                Some(found) => {
                    self.called_name(found, &[GenericArgument::Type(result)], run.call.at)
                }
                None => {
                    self.unsupported(run.call.at, "a run, with no `record` in core");
                    "None".to_string()
                }
            },
            None => "None".to_string(),
        };
        let target = self.called_name((*module, *declaration), generics, run.call.at);
        let arguments = self.arguments(
            function,
            (*module, *declaration),
            generics,
            signature,
            arguments,
        );
        let real_time = if run.real_time { "True" } else { "False" };
        let mut given = vec![target, record, real_time.to_string()];
        if !arguments.is_empty() {
            given.push(arguments);
        }
        if let Some(config) = &run.config {
            given.push(format!("_config={}", self.expression(function, config)));
        }
        format!("_harness.run({})", given.join(", "))
    }

    // ---- Statements ----

    /// The statements of a body, or `pass` where it writes none.
    fn block(&mut self, function: &Function, statements: &[Stmt], depth: usize) {
        if self.statements(function, statements, depth) == 0 {
            self.line(depth, "pass");
        }
    }

    /// Writes `statements`, and says how many lines of them it wrote.
    fn statements(&mut self, function: &Function, statements: &[Stmt], depth: usize) -> usize {
        let before = self.text.len();
        for (position, statement) in statements.iter().enumerate() {
            // CTRL-13 to CTRL-15: in a graph an `if` and a `switch`
            // are composition. One whose branch returns takes the rest
            // of the block for its other branch.
            if function.mode == Mode::Graph {
                match self.branched(function, statement, &statements[position + 1..], depth) {
                    Branched::No => {}
                    Branched::Written => continue,
                    Branched::Ended => break,
                }
            }
            if function.mode.runs() && self.valued(function, statement, depth) {
                continue;
            }
            self.statement(function, statement, depth);
        }
        self.text[before..].lines().count()
    }

    /// True where nothing else holds what `value` gives
    /// ([`copies::fresh`]), and where it is a connection of a graph,
    /// which is never changed where it is. An ordinary value of a graph
    /// is no connection: a native function and a `var` parameter
    /// change it where it is, as in any function, and the copy rule is
    /// the same for it.
    fn fresh(&self, function: &Function, value: &Expr) -> bool {
        self.connection(function, value)
            || self.made_anew(function, value)
            || copies::fresh(self.program, value)
    }

    /// True where reading `value` in a hook makes a value nothing else
    /// holds: a temporal input, `out`, or a field of one, of a struct
    /// whose bundle's ports are read each into a new value
    /// (`port_value`). A struct with a field that may hold what is
    /// changed where it is is not one: hgraph may hold that part.
    fn made_anew(&self, function: &Function, value: &Expr) -> bool {
        matches!(function.mode, Mode::Node | Mode::Stop)
            && self.input_port(function, value).is_some()
            && built_anew(self.program, function.port_type(value), 0)
    }

    /// True where `value` is a connection of a graph: hgraph's wiring,
    /// which nothing changes where it is, and which is never copied.
    fn connection(&self, function: &Function, value: &Expr) -> bool {
        function.mode == Mode::Graph && value.is_connection()
    }

    /// `written`, the text of `value`, as what keeps it takes it: a
    /// copy, where something else holds the value too (`fresh`). What
    /// keeps a value is a place it is assigned to or into, a native
    /// function that puts it in the place it is given, and the runtime
    /// that is given it to publish.
    fn copied(&mut self, function: &Function, value: &Expr, written: String) -> String {
        if self.fresh(function, value) {
            written
        } else {
            self.natives = true;
            format!("_core.copied({written})")
        }
    }

    /// What a `return` gives of `value`, written `written`.
    ///
    /// A value function's result is the caller's to keep, so it is the
    /// function's own, or a copy: what is read from a `var` the body
    /// declares is gone when the function returns and is given as it
    /// is, and what is read from a parameter is the caller's already.
    /// A hook's `return` publishes: the runtime keeps the value, and a
    /// `state` or a `cache` it is read from goes on changing. A graph's
    /// ordinary result is wired as a constant, which hgraph keeps: it
    /// is the graph's own, as a value function's is, or a copy. In a
    /// branch's graph it is read from the graph around it, which is
    /// not the branch's own.
    fn given_out(&mut self, function: &Function, value: &Expr, written: String) -> String {
        let locals = function.locals;
        let own = match function.mode {
            Mode::Test => true,
            Mode::Graph if self.in_case > 0 => false,
            Mode::Value | Mode::Graph => {
                let parameters = function
                    .signature
                    .map_or(0, |signature| signature.parameters.len());
                function.copies.own(locals, parameters, value)
            }
            Mode::Node | Mode::Lifecycle | Mode::Stop => !function
                .copies
                .places(self.program, locals, value)
                .iter()
                .any(|&place| matches!(locals[place].kind, LocalKind::State | LocalKind::Cache)),
            Mode::Source => false,
        };
        if own {
            written
        } else {
            self.copied(function, value, written)
        }
    }

    /// What is written into the local `local` for `value`, which is
    /// written `written`.
    ///
    /// A `var`, a `state` and a `cache` keep what they are given: it
    /// is copied, where something else holds it. A `let` is a
    /// reference to what it reads, and takes a copy only where the
    /// place it reads from is changed while the name is in scope
    /// ([`Copies`]).
    fn kept(&mut self, function: &Function, local: usize, value: &Expr, written: String) -> String {
        if copies::is_place(function.locals, local) {
            self.copied(function, value, written)
        } else if function.copies.copied(local)
            && !self.connection(function, value)
            && changed_in_place(self.program, &value.ty, 0)
        {
            self.natives = true;
            format!("_core.copied({written})")
        } else {
            written
        }
    }

    fn statement(&mut self, function: &Function, statement: &Stmt, depth: usize) {
        // Where a statement of a test starts, by the line of its piece:
        // a failure is traced back to the statement by it.
        if function.mode == Mode::Test {
            let line = self.text.matches('\n').count();
            self.piece
                .statements
                .push((line, at_of(statement, function.locals)));
        }
        match statement {
            Stmt::Declare { local, value } => {
                if let Some(value) = value {
                    // ERR-7: a capture of a node, of a lifted call or of
                    // a graph given a wiring-time value is of a graph
                    // of the target's own, written before the statement.
                    let captured = match &value.kind {
                        ExprKind::Capture(call) if function.mode == Mode::Graph => {
                            self.captured_call(function, call, depth)
                        }
                        _ => None,
                    };
                    let written = match captured {
                        Some(captured) => captured,
                        None => match self.place_call(function, value, depth) {
                            Some(result) => result,
                            None => self.expression(function, value),
                        },
                    };
                    let kept = self.kept(function, *local, value, written);
                    let ty = &function.locals[*local].ty;
                    let value = self.narrowed(function, ty, value, kept);
                    let target = self.place(function, *local);
                    self.line(depth, &format!("{target} = {value}"));
                    self.bind(function, *local);
                } else if matches!(function.locals[*local].ty, Type::Nullable(_)) {
                    // CTRL-11: a `var` whose type is written `T?` "is
                    // null instead, and is assigned from its
                    // declaration".
                    let target = self.place(function, *local);
                    self.line(depth, &format!("{target} = None"));
                    self.bind(function, *local);
                }
                // Any other `var` declared with no value has none to
                // write: the analyser has held every read of it to an
                // assignment.
            }
            // A compound assignment's value is already the symbol with
            // the target as its left operand.
            // FN-16, proposed: "`out = items[idx]` designates as `return
            // items[idx]` does": the result designates the endpoint
            // (`_core.Designation`).
            Stmt::Assign {
                local,
                value: assigned,
                ..
            } if function.is_out(*local) && designates(assigned) => {
                if let Some(designating) = self.designation(function, assigned) {
                    self.line(depth, &designating.call(false));
                }
            }
            Stmt::Assign {
                local,
                value: assigned,
                ..
            } => {
                let value = match self.place_call(function, assigned, depth) {
                    Some(result) => result,
                    None => self.expression(function, assigned),
                };
                // TIME-7: an assignment to `out` publishes, a null
                // among what it may publish.
                let declared = &function.locals[*local];
                let value = if declared.kind == LocalKind::Inject && declared.name == "out" {
                    // TIME-7: the runtime keeps what is published, and
                    // the hook may go on to change what it read it
                    // from: a copy, a nullable one among them.
                    self.kept_publication(function, assigned, value)
                } else {
                    let kept = self.kept(function, *local, assigned, value);
                    self.narrowed(function, &declared.ty, assigned, kept)
                };
                let target = self.place(function, *local);
                self.line(depth, &format!("{target} = {value}"));
                self.bind(function, *local);
            }
            // EXPR-3: "An assignment may name a place inside a value, at
            // any depth". A compound assignment's value is already the
            // symbol with the place as its left operand.
            Stmt::AssignPlace {
                place,
                operator,
                value,
                ..
            } => {
                // TIME-7: a part of `out` is assigned through its port
                // (`assigned_to_out`).
                let root = place.place_root();
                let to_out = matches!(root.kind, ExprKind::Local(local) if function.is_out(local));
                // EXPR-3: "A compound assignment reads the place
                // first". The place is written once, so each index of
                // it is evaluated once (EXPR-2), and is the index of
                // the read and of the write: `items[next()] += 1`
                // reads and writes one element. An index that is more
                // than a name is so held, with what it is an index of
                // where reading that may fail, each in the order
                // written, as a call that assigns a place holds them.
                let mut held_here = Vec::new();
                let indices = indices_of(place);
                if operator.is_some() && indices.iter().any(|(_, index)| !self.simple_index(index))
                {
                    let (mut held_indices, mut reads) = (0, 0);
                    for (of, index) in indices {
                        if let Some(of) = of {
                            reads += 1;
                            let held = format!("_in_{reads}");
                            // Read only to be read through: nothing of
                            // it is kept.
                            let written = self.expression(function, of);
                            self.line(depth, &format!("{held} = {written}"));
                            self.held.push((of.at, held));
                            held_here.push(of.at);
                        }
                        held_indices += 1;
                        let held = format!("_at_{held_indices}");
                        // Held as a copy, as a call's index is.
                        let written = self.expression(function, index);
                        let written = self.copied(function, index, written);
                        self.line(depth, &format!("{held} = {written}"));
                        self.held.push((index.at, held));
                        held_here.push(index.at);
                    }
                }
                // The place keeps what it is given: a copy, where
                // something else holds it.
                let written = self.expression(function, value);
                let copied = self.copied(function, value, written);
                // EXPR-15: in a graph, a place of a `var` that holds a
                // connection gives the `var` a new connection
                // (`connected_place`).
                let assigned = if to_out {
                    self.assigned_to_out(function, place, value, copied)
                } else if function.mode == Mode::Graph && root.is_connection() {
                    self.connected_place(function, place, value, copied)
                } else {
                    self.assigned(function, place, copied)
                };
                if let Some((target, value)) = assigned {
                    self.hoist(depth);
                    self.line(depth, &format!("{target} = {value}"));
                }
                self.held.retain(|(at, _)| !held_here.contains(at));
            }
            // FN-7: "in a temporal function that declares one it ends
            // the evaluation and publishes nothing itself: a write
            // already made through `out` stands". A `ref` result's is
            // what the node returns (`_core.Designation`).
            Stmt::Return { value: None, .. } if function.designating() => {
                self.line(depth, "return _designation.published()")
            }
            // FN-7: a `return` with a value publishes it and ends the
            // evaluation; with none, and where a body ends, nothing is
            // published. hgraph has a node that returns `None` not tick.
            Stmt::Return { value: None, .. } if self.given_back.is_empty() => {
                self.line(depth, "return")
            }
            // FN-15: a function with `var` parameters gives back what
            // each holds, after its result.
            Stmt::Return { value: None, .. } => {
                let given = self.returned("None");
                self.line(depth, &format!("return {given}"));
            }
            // FN-16: "a `return` to a `ref` result designates" the
            // endpoint it names: it samples a whole `ref` parameter, and
            // designates an ordinary input or a child path, whose
            // re-binding its readers follow (`_core.Designation`).
            Stmt::Return {
                value: Some(returned),
                ..
            } if designates(returned) => match self.designation(function, returned) {
                Some(designating) => {
                    self.line(depth, &format!("return {}", designating.call(true)));
                }
                None => self.line(depth, "return _designation.published()"),
            },
            Stmt::Return {
                value: Some(returned),
                ..
            } => {
                let value = match self.place_call(function, returned, depth) {
                    Some(result) => result,
                    None => self.expression(function, returned),
                };
                let value = self.given_out(function, returned, value);
                let value = self.bundled(function, returned, value);
                let value = match function
                    .signature
                    .and_then(|signature| signature.result.as_ref())
                {
                    Some(result) => self.narrowed(function, result, returned, value),
                    None => value,
                };
                // FN-3: in a branch's graph "an ordinary value in a
                // branch is then wired as a constant".
                let nullable = function.mode == Mode::Graph
                    && self
                        .current_result(function)
                        .is_some_and(|result| matches!(result, Type::Nullable(_)));
                let value = if nullable {
                    let connection = returned.is_connection();
                    self.normalized(function, value, connection, returned.at)
                } else if function.mode == Mode::Graph
                    && self.in_case > 0
                    && !returned.is_connection()
                {
                    let ty = function
                        .signature
                        .and_then(|signature| signature.result.as_ref())
                        .unwrap_or(&returned.ty)
                        .clone();
                    self.constant_connection(&ty, returned.at, value)
                } else {
                    value
                };
                // A node that publishes a nullable publishes its null.
                let value = if function.mode == Mode::Node && function.publishes_null(returned) {
                    self.published(value)
                } else {
                    value
                };
                let given = self.returned(&value);
                self.line(depth, &format!("return {given}"));
            }
            Stmt::Assert { condition, .. } => {
                // Python's own `assert` is removed when the interpreter
                // is run with `-O` or `PYTHONOPTIMIZE`, and a test
                // would then pass whatever it asserts. An `if` is
                // always run.
                let condition = self.expression(function, condition);
                self.line(depth, &format!("if not ({condition}):"));
                self.line(depth + 1, "raise _py.AssertionError()");
            }
            // TST-10: "Run its block once. A matching escaping error
            // passes ...; then continue after the assertion. Normal
            // completion, a different code ... fails the test." The
            // harness says whether what escaped has the code, and
            // TST-11: a failed assertion in the block does not.
            Stmt::AssertRaises {
                code: Some(code),
                body,
                at,
            } if function.mode == Mode::Test => {
                // A failure this target gives with no code, or not at
                // all, is not asserted: EXPR-3 and EXPR-7's index outside
                // a list, `value.index_out_of_range`, and core's OPS-1
                // text repeated a count below zero, `numeric.out_of_range`
                // (`uncoded_failure`).
                if let Some(form) = uncoded_failure(self.program, code, body) {
                    self.unsupported(*at, form);
                    return;
                }
                self.harness = true;
                self.line(depth, "try:");
                self.block(function, body, depth + 1);
                // What follows the block is the assertion's own again:
                // a block that raised nothing fails at the assertion,
                // and not at its last statement.
                let line = self.text.matches('\n').count();
                self.piece.statements.push((line, *at));
                self.line(depth, "except _py.Exception as _raised:");
                self.line(
                    depth + 1,
                    &format!("_harness.raised(_raised, {})", string(code)),
                );
                self.line(depth, "else:");
                self.line(depth + 1, &format!("_harness.not_raised({})", string(code)));
            }
            Stmt::Expr(Expr {
                kind:
                    ExprKind::If {
                        condition,
                        then,
                        otherwise,
                    },
                ..
            }) => self.if_statement(function, condition, then, otherwise.as_deref(), depth),
            // SOURCE-1: the one block that holds every `yield` may be
            // `{ ... }`: its statements, one after another, and then
            // the expression it ends with, where it ends with one. A
            // source's block gives no value, so that is a statement
            // too.
            Stmt::Expr(Expr {
                kind: ExprKind::Block { statements, value },
                ..
            }) if function.mode == Mode::Source => {
                self.statements(function, statements, depth);
                if let Some(value) = value {
                    self.statement(function, &Stmt::Expr((**value).clone()), depth);
                }
            }
            Stmt::Expr(value) => {
                // FN-15: a call that assigns places is written with
                // its assignments, and has no value to write.
                if self.place_call(function, value, depth).is_none() {
                    let value = self.expression(function, value);
                    self.line(depth, &value);
                }
            }
            // SOURCE-2: "Yield evaluates its time once, then payload
            // once, before target validation." Python evaluates a
            // tuple's entries in order, and the provider validates the
            // target where the generator is resumed.
            Stmt::Yield {
                time,
                value: yielded,
                ..
            } if function.mode == Mode::Source => {
                let time = self.expression(function, time);
                let value = self.expression(function, yielded);
                // The runtime keeps what is published, and the source
                // goes on after its `yield`.
                let value = self.kept_publication(function, yielded, value);
                self.line(depth, &format!("yield ({time}, {value})"));
            }
            // CTRL-15: in a graph "The condition of a `while` is an
            // ordinary `bool`, tested while the graph is built": Python's
            // own loop, which composes its body on each pass.
            Stmt::While {
                condition: Some(condition),
                body,
                ..
            } if function.mode == Mode::Graph => {
                let condition = self.expression(function, condition);
                self.line(depth, &format!("while {condition}:"));
                self.scoped(|emitter| {
                    emitter.unrolling += 1;
                    emitter.block(function, body, depth + 1);
                    emitter.unrolling -= 1;
                });
            }
            // CTRL-15: a `for` of a graph over ordinary data "composes its
            // body once for each element", and over a connection of
            // `list<T, N>` "composes its body `N` times": Python's own
            // loop, over the value or over hgraph's `TSL`, whose ports
            // are its positions. Over a connection of `list<T>`,
            // `set<T>`, or `dict<K, V>` "it makes one child for each
            // member" (`Emitter::each`).
            Stmt::For {
                names,
                sequence,
                body,
                at,
            } if function.mode == Mode::Graph => {
                let fixed = matches!(
                    &sequence.ty,
                    Type::List { size: Some(size), .. }
                        if !matches!(size.value, ConstantValue::Unbounded)
                );
                if sequence.is_connection() && !fixed {
                    self.each(function, names, sequence, body, *at, depth);
                } else {
                    self.scoped(|emitter| {
                        for &name in names.iter() {
                            emitter.bind(function, name);
                        }
                        emitter.unrolling += 1;
                        emitter.for_loop(function, (names, sequence), body, *at, depth);
                        emitter.unrolling -= 1;
                    });
                }
            }
            // CTRL-12: "`while condition { ... }` repeats its body while
            // the condition holds". The form with no condition is a
            // source's (SOURCE-4: "omitted condition is unbounded").
            Stmt::While {
                condition: Some(condition),
                body,
                ..
            } if function.mode.runs() => {
                let condition = self.expression(function, condition);
                self.line(depth, &format!("while {condition}:"));
                // A body may not run: what it binds is not bound after.
                self.scoped(|emitter| emitter.block(function, body, depth + 1));
            }
            Stmt::While {
                condition: None,
                body,
                ..
            } if function.mode == Mode::Source => {
                self.line(depth, "while True:");
                self.block(function, body, depth + 1);
            }
            Stmt::For {
                names,
                sequence,
                body,
                at,
            } if function.mode.runs() => {
                self.scoped(|emitter| {
                    for &name in names.iter() {
                        emitter.bind(function, name);
                    }
                    emitter.for_loop(function, (names, sequence), body, *at, depth);
                });
            }
            Stmt::Switch {
                selector,
                cases,
                default,
                ..
            } if function.mode.runs() => {
                // Only the case selected runs: what a case binds is not
                // bound after.
                self.scoped(|emitter| {
                    emitter.switch(function, selector, cases, default.as_deref(), depth)
                });
            }
            // FN-9: "A graph injects `global_state` and no other, and
            // uses it where the graph is built: each call on it is
            // evaluated once then, in the order written, before any
            // node of that graph starts". hgraph builds a graph with
            // the run's store current, so the store is the current one
            // where the graph's function runs, and a call on it there
            // is run then.
            Stmt::Inject { local, .. }
                if function.mode == Mode::Graph
                    && function.locals[*local].name == "global_state" =>
            {
                self.hgraph = true;
                self.line(depth, "_store = _hg.GlobalState.instance()");
            }
            other => self.unsupported(at_of(other, function.locals), "this statement"),
        }
    }

    // ---- Expressions ----

    // ---- Lifting ----
}

/// Where a statement is written. A declaration is where its local is.
fn at_of(statement: &Stmt, locals: &[Local]) -> Location {
    match statement {
        Stmt::Assign { at, .. }
        | Stmt::AssignPlace { at, .. }
        | Stmt::Inject { at, .. }
        | Stmt::Hook { at, .. }
        | Stmt::Return { at, .. }
        | Stmt::Assert { at, .. }
        | Stmt::AssertRaises { at, .. }
        | Stmt::Yield { at, .. }
        | Stmt::For { at, .. }
        | Stmt::While { at, .. }
        | Stmt::Switch { at, .. }
        | Stmt::Unread { at } => *at,
        Stmt::Expr(value) => value.at,
        Stmt::Declare { local, .. } => locals[*local].at,
    }
}

/// The name of the hidden `REF` input a node designates `input` through
/// (`Designated`), and the name the provider is told it by where there
/// is one for each position. hgraph makes a reference only from a `REF`
/// input, so the node is given the input a second time as a hidden `REF`
/// input, `_ref_name`, or one for each position, `_ref0_name` (`node`),
/// which is active whatever the node's own activity is: a `ref`
/// parameter that may be passive among them. Every name the target makes
/// up starts with `_`, and a program's own never does.
fn designated_name(input: &Local) -> String {
    format!("_ref_{}", name(&input.name))
}

/// What an `assert raises(code)` of `body` asserts that this target does
/// not give, where it asserts one: a failure coded
/// `value.index_out_of_range` (EXPR-3, EXPR-7), which the provider's
/// index outside a list has no code for, and core's OPS-1 text repeated a
/// count below zero, `numeric.out_of_range`, which the provider does not
/// write (`mul__str_i64`): a call of core's `mul_` of a `str` and an
/// `i64`, in either order.
fn uncoded_failure(program: &Program, code: &str, body: &[Stmt]) -> Option<&'static str> {
    if code == "value.index_out_of_range" {
        return Some("a failure coded `value.index_out_of_range` (EXPR-3, EXPR-7)");
    }
    if code != "numeric.out_of_range" {
        return None;
    }
    let text = |ty: &Type| *ty == Type::Scalar(Scalar::Str);
    let count = |ty: &Type| *ty == Type::Scalar(Scalar::I64);
    let mut repeated = false;
    hgl_analyser::hir::walk_statements(body, &mut |value| {
        if core_operator(program, value) == Some("mul_")
            && let ExprKind::Call { arguments, .. } = &value.kind
            && let [first, second, ..] = arguments.as_slice()
            && ((text(&first.value.ty) && count(&second.value.ty))
                || (count(&first.value.ty) && text(&second.value.ty)))
        {
            repeated = true;
        }
    });
    repeated.then_some("text repeated a count below zero, `numeric.out_of_range` (OPS-1)")
}

/// The name of the operator of core that `value` calls a candidate of,
/// `mul_` of `"ab" * 2` or of `mul_("ab", 2)`; `None` for any other
/// value.
pub(super) fn core_operator<'p>(program: &'p Program, value: &Expr) -> Option<&'p str> {
    let ExprKind::Call {
        callee: Callee::Candidate { operator, .. },
        ..
    } = &value.kind
    else {
        return None;
    };
    (Some(operator.0) == program.core()).then(|| {
        program.modules[operator.0].declarations[operator.1]
            .name
            .as_str()
    })
}
