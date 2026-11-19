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
    FieldRef, LocalKind, Member, NamedKind, OperatorRef, Parameter, Phase, Program, Role, Scalar,
    Signature, Type,
};
use hgl_diagnostics::{Category, Code, Diagnostic, Diagnostics, Location};

use crate::copies::{
    self, Copies, built_anew, by_value, changed_in_place, may_hold, struct_in_place,
};
use crate::instances::Instances;
use crate::{codes, provides, python_module, test_function};

mod types;
pub(crate) use types::inherited;
use types::*;

mod names;
pub(crate) use names::canonical_argument;
use names::*;

mod annotate;
use annotate::*;

mod publish;
use publish::*;

mod node;
use node::*;

mod places;
pub(crate) use places::place_root;
use places::*;

mod calls;
use calls::*;

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

/// What a piece names: a declaration, as the position of its module in
/// the program and its position in that module, and for a declaration
/// that is written by instance, the suffix of the instance named.
pub(crate) type Used = (usize, usize, Option<String>);

/// One top-level part of a module as it is written: a declaration or a
/// named test.
#[derive(Default)]
pub(crate) struct Piece {
    /// Its Python text.
    text: String,
    /// The first form in it that the target does not write yet. A piece
    /// with one is left out of the file.
    pub(crate) broken: Option<Diagnostic>,
    /// The functions and the types it names.
    pub(crate) uses: BTreeSet<Used>,
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
        let declared = &self.locals[local];
        declared.kind == LocalKind::Parameter
            && self.signature.is_some_and(|signature| {
                signature
                    .parameters
                    .iter()
                    .any(|parameter| parameter.name == declared.name && !parameter.constant)
            })
    }

    /// True when `local` is the node's `inject out` (TIME-7).
    fn is_out(&self, local: usize) -> bool {
        let declared = &self.locals[local];
        declared.kind == LocalKind::Inject && declared.name == "out"
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

/// A number with a fraction as a Python literal.
fn float(value: f64) -> String {
    if value.is_nan() {
        "_py.float(\"nan\")".to_string()
    } else if value.is_infinite() {
        let sign = if value < 0.0 { "-" } else { "" };
        format!("_py.float(\"{sign}inf\")")
    } else {
        // `{:?}` writes the shortest text that reads back as the same
        // number, and always with a `.` or an exponent.
        format!("{value:?}")
    }
}

/// A date as Python's `date` and `datetime` take one: year, month, day.
fn date_fields(date: CalendarDate) -> String {
    format!("{}, {}, {}", date.year, date.month, date.day)
}

/// A time of day as Python's `time` and `datetime` take one: hour,
/// minute, second, microsecond.
fn time_fields(time: ClockTime) -> String {
    format!(
        "{}, {}, {}, {}",
        time.hour, time.minute, time.second, time.microsecond
    )
}

/// Whether the instant of `date` and `time` written `minutes` east of
/// UTC is in the years 1 to 9999 in UTC, which a Python `datetime` holds.
/// An offset is less than a day (LEX-19), so only a time on the first
/// day written east of UTC, or on the last day written west of it, can
/// leave them.
fn in_calendar(date: CalendarDate, time: ClockTime, minutes: i16) -> bool {
    let first = (date.year, date.month, date.day) == (1, 1, 1);
    let last = (date.year, date.month, date.day) == (9999, 12, 31);
    // The minutes of the day the time is at, its seconds aside: an
    // offset is whole minutes, so they do not change which day it is.
    let into_day = i32::from(time.hour) * 60 + i32::from(time.minute);
    let minutes = i32::from(minutes);
    !((first && into_day < minutes) || (last && into_day - minutes >= 24 * 60))
}

/// What `Emitter::branched` made of a statement of a graph.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Branched {
    /// It is no `if` and no `switch` of composition: it is written as
    /// any statement.
    No,
    /// It was written, and the statements after it follow.
    Written,
    /// It was written with the statements after it, as a branch.
    Ended,
}

/// One branch of an `if` or a `switch` over a connection: the child
/// graph hgraph's `switch_` runs while its key is selected.
struct Branch {
    /// The key it is selected by, as Python: `True`, a label, or
    /// hgraph's `DEFAULT`.
    key: String,
    /// Its statements.
    statements: Vec<Stmt>,
    /// What it gives.
    tail: Tail,
}

/// What a branch's graph gives after its statements.
enum Tail {
    /// The branch's value.
    Value(Box<Expr>),
    /// The `var` the branches assign, as the branch leaves it.
    Local(usize),
    /// Nothing: a connection that never ticks (CTRL-13).
    Nothing,
    /// Its statements end with a `return` of their own.
    Returns,
    /// Nothing, and the graph has no output: CTRL-3, "An outputless if
    /// has no synthetic result".
    Outputless,
    /// The `default` of a `switch` that writes none: CTRL-6, "An
    /// unmatched selector uses default or fails". The graph fails when
    /// it is selected, as a node's `switch` does (`_core.unmatched`).
    Unmatched,
}

/// The statements of a branch of an `if` that stands as a statement:
/// none with no `else`, a block's, with the block's last expression (a
/// nested `if`, say) as a statement of its own, and for an `else if`
/// the nested `if` as its one statement.
fn alternative_statements(otherwise: Option<&Expr>) -> Vec<Stmt> {
    match otherwise {
        None => Vec::new(),
        Some(Expr {
            kind: ExprKind::Block { statements, value },
            ..
        }) => {
            let mut all = statements.clone();
            all.extend(value.iter().map(|value| Stmt::Expr((**value).clone())));
            all
        }
        Some(alternative) => vec![Stmt::Expr(alternative.clone())],
    }
}

/// True where some path through `statements` has a `return`, at any
/// depth.
fn may_return(statements: &[Stmt]) -> bool {
    let mut found = false;
    hgl_analyser::hir::walk_statement_nodes(statements, &mut |node| {
        found |= matches!(
            node,
            hgl_analyser::hir::Node::Statement(Stmt::Return { .. })
        );
    });
    found
}

/// True where an `if` or a `switch` among `statements` has a `return`
/// in it: it needs what follows it to lower its other paths.
fn nested_return(statements: &[Stmt]) -> bool {
    statements.iter().any(|statement| {
        matches!(
            statement,
            Stmt::Switch { .. }
                | Stmt::Expr(Expr {
                    kind: ExprKind::If { .. },
                    ..
                })
        ) && may_return(std::slice::from_ref(statement))
    })
}

/// `statements`, and after them `rest` where they can reach their end.
fn continued(statements: &[Stmt], rest: &[Stmt]) -> Vec<Stmt> {
    let mut all = statements.to_vec();
    if !always_returns(statements) {
        all.extend_from_slice(rest);
    }
    all
}

/// The locals `statements` assign, by name or by a place inside them,
/// that they do not declare themselves: the `var`s of what encloses
/// them that they change.
fn assigned_outside(statements: &[Stmt]) -> Vec<usize> {
    use hgl_analyser::hir::{Node, walk_statement_nodes};
    let mut assigned = Vec::new();
    let mut declared = Vec::new();
    walk_statement_nodes(statements, &mut |node| match node {
        Node::Statement(Stmt::Declare { local, .. }) => declared.push(*local),
        Node::Statement(Stmt::For { names, .. }) => declared.extend(names),
        Node::Statement(Stmt::Assign { local, .. }) => assigned.push(*local),
        Node::Statement(Stmt::AssignPlace { place, .. }) => {
            if let ExprKind::Local(local) = place_root(place).kind {
                assigned.push(local);
            }
        }
        _ => {}
    });
    assigned.retain(|local| !declared.contains(local));
    assigned.sort_unstable();
    assigned.dedup();
    assigned
}

/// Adds what `node`, a part of a child graph's statements, names to
/// `read`, and what it declares to `declared`: a local read or
/// assigned, and a `let`, a `var` or a loop's names.
fn named_in(node: hgl_analyser::hir::Node, read: &mut Vec<usize>, declared: &mut Vec<usize>) {
    use hgl_analyser::hir::Node;
    match node {
        Node::Statement(Stmt::Declare { local, .. }) => declared.push(*local),
        Node::Statement(Stmt::For { names, .. }) => declared.extend(names),
        Node::Statement(Stmt::Assign { local, .. }) => read.push(*local),
        Node::Expression(Expr {
            kind: ExprKind::Local(local),
            ..
        }) => read.push(*local),
        _ => {}
    }
}

/// True where every path through `statements` ends in a `return`: the
/// last is one, or an `if` with an `else` or a `switch` with a
/// `default` each branch of which always returns.
fn always_returns(statements: &[Stmt]) -> bool {
    match statements.last() {
        Some(Stmt::Return { .. }) => true,
        Some(Stmt::Expr(Expr {
            kind:
                ExprKind::If {
                    then,
                    otherwise: Some(otherwise),
                    ..
                },
            ..
        })) => {
            always_returns(&alternative_statements(Some(then)))
                && always_returns(&alternative_statements(Some(otherwise)))
        }
        Some(Stmt::Switch {
            cases,
            default: Some(default),
            ..
        }) => cases.iter().all(|case| always_returns(&case.body)) && always_returns(default),
        _ => false,
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
        if self.piece.broken.is_none() {
            self.piece.broken = Some(diagnostic.clone());
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
            .filter(|piece| piece.broken.is_none())
            .flat_map(|piece| piece.uses.iter())
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
            if piece.broken.is_some() {
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
            if types.iter().any(|piece| piece.broken.is_none()) {
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
        let record = match crate::instances::run_result(program, run) {
            Some(result) => match crate::instances::core_function(program, "record") {
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

    /// CTRL-10: "A block whose last statement is an expression has that
    /// expression's value and type". A `let`, a `var`, an assignment to
    /// a name or a `return` whose value is a block with statements in
    /// it, or an `if` with such a block for a branch, in a value
    /// function and a node's hooks, written where `statement` is one:
    /// says whether it was. Python has no expression with statements in
    /// it, so the statements are written in their order and the value
    /// is given after them, in each branch of Python's `if`. "With no
    /// `else` it is as if its `else` were null", and "A branch that
    /// returns from the function gives the `if` no value" (CTRL-5).
    pub(super) fn valued(&mut self, function: &Function, statement: &Stmt, depth: usize) -> bool {
        // A value that has statements in it, at any depth of blocks and
        // `if`s; any other is written as an expression.
        fn has_statements(value: &Expr) -> bool {
            match &value.kind {
                ExprKind::Block { statements, value } => {
                    !statements.is_empty() || value.as_deref().is_some_and(has_statements)
                }
                ExprKind::If {
                    then, otherwise, ..
                } => has_statements(then) || otherwise.as_deref().is_some_and(has_statements),
                _ => false,
            }
        }
        let value = match statement {
            Stmt::Declare {
                value: Some(value), ..
            }
            | Stmt::Assign {
                operator: None,
                value,
                ..
            }
            | Stmt::Return {
                value: Some(value), ..
            } if has_statements(value) => value,
            _ => return false,
        };
        // The statement again, with `value` in place of its own.
        let given = |value: Expr| -> Stmt {
            let mut again = statement.clone();
            match &mut again {
                Stmt::Declare { value: place, .. } | Stmt::Return { value: place, .. } => {
                    *place = Some(value)
                }
                Stmt::Assign { value: place, .. } => *place = value,
                _ => {}
            }
            again
        };
        match &value.kind {
            ExprKind::Block {
                statements,
                value: Some(inner),
            } => {
                let mut all = statements.clone();
                all.push(given((**inner).clone()));
                self.statements(function, &all, depth);
            }
            ExprKind::If {
                condition,
                then,
                otherwise,
            } => {
                let condition = self.expression(function, condition);
                self.line(depth, &format!("if {condition}:"));
                // A branch's statements, then the statement given its
                // value: a block's last expression, or the branch
                // itself; null where it has none.
                let null = Expr {
                    kind: ExprKind::Literal(Value::Null),
                    ty: value.ty.clone(),
                    at: value.at,
                };
                for (position, branch) in [Some(&**then), otherwise.as_deref()]
                    .into_iter()
                    .enumerate()
                {
                    if position == 1 {
                        self.line(depth, "else:");
                    }
                    let mut all = Vec::new();
                    match branch.map(|branch| &branch.kind) {
                        Some(ExprKind::Block { statements, value }) => {
                            all.extend(statements.iter().cloned());
                            if !always_returns(statements) {
                                let value = value.as_deref().cloned().unwrap_or(null.clone());
                                all.push(given(value));
                            }
                        }
                        Some(_) => all.extend(branch.cloned().map(given)),
                        None => all.push(given(null.clone())),
                    }
                    self.block(function, &all, depth + 1);
                }
            }
            // A block with no value gives none: the analyser holds such
            // a value to no place.
            _ => self.unsupported(value.at, "this block as a value"),
        }
        true
    }

    /// An `if` or a `switch` of a graph (CTRL-13 to CTRL-15), written
    /// where `statement` is one: says whether it was, and whether the
    /// statements after it, `rest`, were written with it.
    ///
    /// Over an ordinary value it is decided where the graph is built,
    /// and is Python's own `if`: "a `switch` over an ordinary value
    /// composes the one case selected". Over a connection "each case is
    /// a child graph selected at runtime": hgraph's `switch_`, given a
    /// graph for each branch (`Emitter::switched`).
    ///
    /// CTRL-14: "A `return value` in a branch of a temporal `if` makes
    /// the graph's result that value while the branch is selected and
    /// the continuation's otherwise": the rest of the block is the
    /// other branch's graph. "After the `if` the `var` is one
    /// connection: the branch's binding while that branch is selected,
    /// and the binding it had before for a branch that does not assign
    /// it": each branch's graph gives the `var`, and what `switch_`
    /// gives is assigned to it.
    pub(super) fn branched(
        &mut self,
        function: &Function,
        statement: &Stmt,
        rest: &[Stmt],
        depth: usize,
    ) -> Branched {
        // The statements and the value of a branch: a block's, or an
        // `else if`, which is the value of its `else`.
        fn parts(branch: &Expr) -> (&[Stmt], Option<&Expr>) {
            match &branch.kind {
                ExprKind::Block { statements, value } => (statements, value.as_deref()),
                _ => (&[], Some(branch)),
            }
        }
        match statement {
            // An `if` that stands as a statement, over a connection.
            Stmt::Expr(Expr {
                kind:
                    ExprKind::If {
                        condition,
                        then,
                        otherwise,
                    },
                at,
                ..
            }) if self.connects(function, condition) => {
                // Each branch's statements; an `else if` is the one
                // statement of the `else`.
                let then_owned = alternative_statements(Some(then));
                let then_statements = then_owned.as_slice();
                let other_owned = alternative_statements(otherwise.as_deref());
                let other_statements = other_owned.as_slice();
                if may_return(then_statements) || may_return(other_statements) {
                    if self.unrolled_return(*at) {
                        return Branched::Ended;
                    }
                    // A path that does not return goes on into the rest
                    // of the block: each branch's graph has it after
                    // its own statements.
                    let on = |statements: &[Stmt]| continued(statements, rest);
                    let Some(result) = self.current_result(function) else {
                        self.unsupported(*at, "a `return` in an `if` of a graph with no result");
                        return Branched::Ended;
                    };
                    let branches = [
                        Branch {
                            key: "True".to_string(),
                            statements: on(then_statements),
                            tail: Tail::Returns,
                        },
                        Branch {
                            key: "False".to_string(),
                            statements: on(other_statements),
                            tail: Tail::Returns,
                        },
                    ];
                    let call =
                        self.switched(function, condition, &branches, Some(&result), *at, depth);
                    self.line(depth, &format!("return {call}"));
                    return Branched::Ended;
                }
                let branches = [("True", then_statements), ("False", other_statements)];
                self.assigned_in_branches(function, condition, (&branches, false), *at, depth);
                Branched::Written
            }
            // An `if` over an ordinary value that stands as a statement,
            // with a returning `if` or `switch` in a branch: what follows
            // it is written in each branch, so that the nested one is
            // lowered with its other path.
            Stmt::Expr(Expr {
                kind:
                    ExprKind::If {
                        condition,
                        then,
                        otherwise,
                    },
                ..
            }) if !rest.is_empty()
                && (nested_return(&alternative_statements(Some(then)))
                    || nested_return(&alternative_statements(otherwise.as_deref()))) =>
            {
                let condition = self.expression(function, condition);
                self.line(depth, &format!("if {condition}:"));
                let before = self.bound.clone();
                let then = continued(&alternative_statements(Some(then)), rest);
                self.block(function, &then, depth + 1);
                self.bound = before;
                self.line(depth, "else:");
                let other = continued(&alternative_statements(otherwise.as_deref()), rest);
                self.block(function, &other, depth + 1);
                Branched::Ended
            }
            // An `if` whose value the graph returns, or a `let` or a
            // `var` is given.
            Stmt::Return {
                value:
                    Some(
                        whole @ Expr {
                            kind:
                                ExprKind::If {
                                    condition,
                                    then,
                                    otherwise,
                                },
                            at,
                            ..
                        },
                    ),
                ..
            }
            | Stmt::Declare {
                value:
                    Some(
                        whole @ Expr {
                            kind:
                                ExprKind::If {
                                    condition,
                                    then,
                                    otherwise,
                                },
                            at,
                            ..
                        },
                    ),
                ..
            } => {
                // What the value is given to: the graph's result, or a
                // name.
                let declared = match statement {
                    Stmt::Declare { local, .. } => Some(*local),
                    _ => None,
                };
                let target = declared.map(|local| self.place(function, local));
                let given = |value: String| match &target {
                    Some(target) => format!("{target} = {value}"),
                    None => format!("return {value}"),
                };
                let (then_statements, then_value) = parts(then);
                let other = otherwise.as_deref().map(parts);
                let branch_statements = [
                    Some(then_statements),
                    other.map(|(statements, _)| statements),
                ];
                if self.connects(function, condition) {
                    // A `return` in a branch whose value a `let` or a
                    // `var` is given, in its statements or in its value,
                    // leaves the graph, not the branch's graph (CTRL-5:
                    // "Return targets the enclosing HGL function").
                    let returning = |value: Option<&Expr>| {
                        value.is_some_and(|value| may_return(&[Stmt::Expr(value.clone())]))
                    };
                    if let Some(local) = declared
                        && (branch_statements
                            .iter()
                            .flatten()
                            .any(|statements| may_return(statements))
                            || returning(then_value)
                            || other.is_some_and(|(_, value)| returning(value)))
                    {
                        return self.returned_value(
                            function,
                            (condition, local, *at),
                            [Some((then_statements, then_value)), other],
                            rest,
                            depth,
                        );
                    }
                    // CTRL-14: "After the `if` the `var` is one
                    // connection: the branch's binding while that branch
                    // is selected". `switch_` gives one connection, the
                    // `if`'s value: a `var` of what encloses the `if`
                    // that a branch assigns is not written here. Where
                    // the graph returns the value nothing reads the
                    // `var` after the branch, which reads its own.
                    if declared.is_some()
                        && !assigned_outside(std::slice::from_ref(&Stmt::Expr(whole.clone())))
                            .is_empty()
                    {
                        self.unsupported(
                            *at,
                            "an `if` over a connection that gives a `let` or a `var` its value and assigns a `var` declared before it",
                        );
                        return Branched::Written;
                    }
                    let tail = |value: Option<&Expr>| match value {
                        Some(value) => Tail::Value(Box::new(value.clone())),
                        None => Tail::Nothing,
                    };
                    let branches = [
                        Branch {
                            key: "True".to_string(),
                            statements: then_statements.to_vec(),
                            tail: tail(then_value),
                        },
                        // CTRL-13: "With no `else`, under a temporal
                        // condition it is a connection of `T` that
                        // never ticks while the condition is false".
                        Branch {
                            key: "False".to_string(),
                            statements: other
                                .map_or(Vec::new(), |(statements, _)| statements.to_vec()),
                            tail: other.map_or(Tail::Nothing, |(_, value)| tail(value)),
                        },
                    ];
                    // What a `return` gives is the graph's result, or the
                    // branch's it stands in: each branch's graph gives
                    // that, null where it is nullable (TYPE-14).
                    let result = match declared {
                        Some(_) => Some(whole.ty.clone()),
                        None => self.current_result(function),
                    }
                    .unwrap_or_else(|| whole.ty.clone());
                    let call =
                        self.switched(function, condition, &branches, Some(&result), *at, depth);
                    self.line(depth, &given(call));
                    if let Some(local) = declared {
                        self.bind(function, local);
                    }
                    return Branched::Written;
                }
                // A wiring-time condition: the one branch is composed.
                // "a branch that is a connection makes it a connection
                // of `T` that never ticks where the branch is not
                // composed", and an ordinary value a `T?`.
                let connection = then_value.is_some_and(|value| self.connects(function, value))
                    || other.is_some_and(|(_, value)| {
                        value.is_some_and(|value| self.connects(function, value))
                    });
                let absent = if connection {
                    match self.annotation(&whole.ty, true, *at) {
                        Some(annotation) => format!("_hg.nothing({annotation})"),
                        None => "None".to_string(),
                    }
                } else {
                    "None".to_string()
                };
                // Each branch is its statements and then what it gives,
                // as a statement of its own: a `return` of its value, or
                // the name's declaration, so that an `if` over a
                // connection among them, or as the value, is lowered with
                // what follows it. Where one of them returns, what
                // follows the `let` is written in each branch too.
                // A branch's value that is itself an `if` with a `return`
                // in it needs what follows too.
                let returning_value = |value: Option<&Expr>| {
                    value.is_some_and(|value| may_return(&[Stmt::Expr(value.clone())]))
                };
                let ends = declared.is_some()
                    && (branch_statements
                        .iter()
                        .flatten()
                        .any(|statements| nested_return(statements))
                        || returning_value(then_value)
                        || other.is_some_and(|(_, value)| returning_value(value)));
                let gives = |value: Option<&Expr>| -> Option<Stmt> {
                    let value = value?.clone();
                    let at = value.at;
                    Some(match declared {
                        Some(local) => Stmt::Declare {
                            local,
                            value: Some(value),
                        },
                        None => Stmt::Return {
                            value: Some(value),
                            at,
                        },
                    })
                };
                let condition = self.expression(function, condition);
                self.line(depth, &format!("if {condition}:"));
                let before = self.bound.clone();
                let mut binds = Vec::new();
                let mut reaches = Vec::new();
                let alternatives = [
                    Some((then_statements, then_value)),
                    other.or(Some((&[][..], None))),
                ];
                for (position, (statements, value)) in
                    alternatives.into_iter().flatten().enumerate()
                {
                    if position == 1 {
                        self.line(depth, "else:");
                    }
                    self.bound = before.clone();
                    let mut all = statements.to_vec();
                    if let Some(given) = gives(value) {
                        all.push(given);
                        if ends {
                            all = continued(&all, rest);
                        }
                        self.statements(function, &all, depth + 1);
                    } else if let (true, Some(target), Some(local)) =
                        (ends && nested_return(statements), &target, declared)
                    {
                        // A branch with no value whose statements return
                        // under a nested `if` or `switch`: the name is
                        // given the absent value first, and what follows
                        // comes after the statements, so that the nested
                        // one has it as its other path. Nothing in the
                        // branch reads the name it is the value of.
                        if !always_returns(statements) {
                            self.line(depth + 1, &format!("{target} = {absent}"));
                            self.bind(function, local);
                        }
                        all = continued(statements, rest);
                        self.statements(function, &all, depth + 1);
                    } else {
                        // A branch with no value: its statements, the
                        // absent value, and then what follows.
                        self.statements(function, statements, depth + 1);
                        if !always_returns(statements) {
                            match &target {
                                Some(target) => {
                                    self.line(depth + 1, &format!("{target} = {absent}"))
                                }
                                // An absent ordinary value returned is a
                                // null, published as the `return` of one
                                // is (TYPE-14).
                                None if !connection => {
                                    let null = Stmt::Return {
                                        value: Some(Expr {
                                            kind: ExprKind::Literal(Value::Null),
                                            ty: whole.ty.clone(),
                                            at: *at,
                                        }),
                                        at: *at,
                                    };
                                    self.statement(function, &null, depth + 1);
                                }
                                None => self.line(depth + 1, &format!("return {absent}")),
                            }
                            if let Some(local) = declared {
                                self.bind(function, local);
                            }
                            if ends {
                                self.statements(function, rest, depth + 1);
                                all = continued(statements, rest);
                            }
                        }
                    }
                    reaches.push(!always_returns(&all));
                    binds.push(std::mem::take(&mut self.bound));
                }
                if ends {
                    return Branched::Ended;
                }
                // Python's `if`: what both branches bind is bound after,
                // less a branch that returns, which does not reach the
                // next statement.
                let mut joined: Option<Vec<usize>> = None;
                for (bound, reaches) in binds.into_iter().zip(reaches) {
                    if reaches {
                        joined = Some(match joined {
                            None => bound,
                            Some(mut joined) => {
                                joined.retain(|local| bound.contains(local));
                                joined
                            }
                        });
                    }
                }
                self.bound = joined.unwrap_or(before);
                if let Some(local) = declared {
                    self.bind(function, local);
                }
                Branched::Written
            }
            // A `switch` over a connection.
            Stmt::Switch {
                selector,
                cases,
                default,
                at,
            } if self.connects(function, selector) => {
                if self.labels_repeat(cases) {
                    return Branched::Ended;
                }
                let mut branches: Vec<(String, &[Stmt])> = Vec::new();
                for case in cases {
                    let label = self.expression(function, &case.label);
                    branches.push((label, &case.body));
                }
                if let Some(default) = default {
                    // CTRL-6: "An unmatched selector uses default".
                    branches.push(("_hg.DEFAULT".to_string(), default));
                }
                // CTRL-6: "An unmatched selector ... fails": with no
                // `default`, `switch_`'s own is a graph that fails as
                // a node's `switch` does (`Tail::Unmatched`).
                let unmatched = default.is_none();
                if branches
                    .iter()
                    .any(|(_, statements)| may_return(statements))
                {
                    if self.unrolled_return(*at) {
                        return Branched::Ended;
                    }
                    let Some(result) = self.current_result(function) else {
                        self.unsupported(*at, "a `return` in a `switch` of a graph with no result");
                        return Branched::Ended;
                    };
                    // A case that can reach its end goes on into the rest
                    // of the block, as an `if`'s branch does. With no
                    // `default`, an unmatched selector fails (CTRL-6).
                    let mut branches: Vec<Branch> = branches
                        .into_iter()
                        .map(|(key, statements)| Branch {
                            key,
                            statements: continued(statements, rest),
                            tail: Tail::Returns,
                        })
                        .collect();
                    if unmatched {
                        branches.push(Branch {
                            key: "_hg.DEFAULT".to_string(),
                            statements: Vec::new(),
                            tail: Tail::Unmatched,
                        });
                    }
                    let call =
                        self.switched(function, selector, &branches, Some(&result), *at, depth);
                    self.line(depth, &format!("return {call}"));
                    return Branched::Ended;
                }
                let branches: Vec<(&str, &[Stmt])> = branches
                    .iter()
                    .map(|(key, statements)| (key.as_str(), *statements))
                    .collect();
                self.assigned_in_branches(function, selector, (&branches, unmatched), *at, depth);
                Branched::Written
            }
            // CTRL-15: "a `switch` over an ordinary value composes the
            // one case selected": the selector is read where the graph
            // is built, and each case is an `if` on it (`Emitter::switch`).
            // A case with a returning `if` or `switch` in it has what
            // follows the `switch` after its own statements, as a
            // wiring-time `if`'s branch does.
            Stmt::Switch {
                selector,
                cases,
                default,
                ..
            } => {
                if self.labels_repeat(cases) {
                    return Branched::Ended;
                }
                let returns = cases.iter().any(|case| nested_return(&case.body))
                    || default.as_deref().is_some_and(nested_return);
                let ends = returns && !rest.is_empty();
                let on = |statements: &[Stmt]| {
                    if ends {
                        continued(statements, rest)
                    } else {
                        statements.to_vec()
                    }
                };
                let cases: Vec<Case> = cases
                    .iter()
                    .map(|case| Case {
                        label: case.label.clone(),
                        value: case.value.clone(),
                        body: on(&case.body),
                    })
                    .collect();
                let default = default.as_deref().map(on);
                // What is bound after: what each case that reaches its
                // end binds. One that matches no case fails (CTRL-6).
                let before = self.bound.clone();
                let mut joined: Option<Vec<usize>> = None;
                let selected = format!("_selected_{depth}");
                let written = self.expression(function, selector);
                self.line(depth, &format!("{selected} = {written}"));
                let bodies = cases
                    .iter()
                    .map(|case| Some(&case.body[..]))
                    .chain([default.as_deref()]);
                for (index, body) in bodies.enumerate() {
                    self.bound = before.clone();
                    if index < cases.len() {
                        let label = self.expression(function, &cases[index].label);
                        let word = if index == 0 { "if" } else { "elif" };
                        self.line(depth, &format!("{word} {selected} == {label}:"));
                    } else if !cases.is_empty() {
                        self.line(depth, "else:");
                    }
                    let inner = if cases.is_empty() { depth } else { depth + 1 };
                    match body {
                        Some(body) => {
                            self.block(function, body, inner);
                            if !always_returns(body) {
                                let bound = std::mem::take(&mut self.bound);
                                joined = Some(match joined {
                                    None => bound,
                                    Some(mut joined) => {
                                        joined.retain(|local| bound.contains(local));
                                        joined
                                    }
                                });
                            }
                        }
                        None => {
                            self.natives = true;
                            self.line(inner, &format!("_core.unmatched({selected})"));
                        }
                    }
                }
                self.bound = joined.unwrap_or(before);
                if ends {
                    Branched::Ended
                } else {
                    Branched::Written
                }
            }
            _ => Branched::No,
        }
    }

    /// What a `return` written now gives: the result of the innermost
    /// branch graph of a `switch_` that encloses it, or the graph's.
    pub(super) fn current_result(&self, function: &Function) -> Option<Type> {
        self.results.last().cloned().or_else(|| {
            function
                .signature
                .and_then(|signature| signature.result.clone())
        })
    }

    /// `written`, what a `return` of a graph gives, as a connection of
    /// the `T?` the graph, or the branch graph it stands in, gives.
    /// TYPE-14: "a T is a value where a T? is expected". hgraph's
    /// `switch_` takes branches of one type, and a connection of `T` is
    /// hgraph's `TS[T]`, not `TS[T | None]`: the provider's `nullable`
    /// gives the one for the other, and a connection that is one
    /// already as it is. FN-3: an ordinary value is "wired as a
    /// constant, one publication when the graph starts", and a null is
    /// published as hgraph's `Null` (TYPE-15). A `T?` of any other `T`
    /// is reported.
    pub(super) fn normalized(
        &mut self,
        function: &Function,
        written: String,
        connection: bool,
        at: Location,
    ) -> String {
        let scalar = match self.current_result(function) {
            Some(Type::Nullable(inner)) if whole_scalar(&inner) => {
                self.annotation(&inner, false, at)
            }
            _ => None,
        };
        let Some(scalar) = scalar else {
            self.unsupported(at, "a nullable result of a graph of this type");
            return written;
        };
        self.natives = true;
        self.hgraph = true;
        // A graph's own result may be either: what a graph gives is
        // taken as it is, and a branch graph that is given it makes it
        // one.
        if connection && self.results.is_empty() {
            written
        } else if connection {
            format!("_core.nullable({written}, {scalar})")
        } else {
            format!("_hg.const(_core.published({written}), _hg.TS[{scalar} | None])")
        }
    }

    /// The case among `cases` whose label has the value of an earlier
    /// one's. CTRL-6: "Duplicate values reject". The analyser rejects
    /// those it can tell apart, and gives each case the value it
    /// resolved its label to (`Case::value`), which is taken here as it
    /// is: the two stages do not decide it twice. A label it could not
    /// resolve, a generic value parameter, is in an instance the literal
    /// or the member the call gives it (GEN-13), which is read as it is.
    fn repeated_label(cases: &[Case]) -> Option<Location> {
        use hgl_analyser::hir::LabelValue;
        let mut seen: Vec<LabelValue> = Vec::new();
        for case in cases {
            let value = case.value.clone().or_else(|| match &case.label.kind {
                ExprKind::Literal(Value::Int(value)) => Some(LabelValue::Int(*value)),
                ExprKind::Literal(Value::Bool(value)) => Some(LabelValue::Bool(*value)),
                ExprKind::Literal(Value::Str(value)) => Some(LabelValue::Str(value.clone())),
                ExprKind::EnumMember { member, .. } => Some(LabelValue::Member(*member)),
                _ => None,
            });
            let Some(value) = value else {
                continue;
            };
            if seen.contains(&value) {
                return Some(case.label.at);
            }
            seen.push(value);
        }
        None
    }

    /// Reports a `switch` two of whose labels have one value (see
    /// [`Emitter::repeated_label`]), and says whether it did: hgraph's
    /// `switch_` would keep the last of them, and Python's `if` the
    /// first.
    fn labels_repeat(&mut self, cases: &[Case]) -> bool {
        match Self::repeated_label(cases) {
            Some(at) => {
                self.unsupported(
                    at,
                    "a `switch` two of whose labels have one value in an instance",
                );
                true
            }
            None => false,
        }
    }

    /// Reports a `return` under a branch selected at runtime where it
    /// stands in a loop unrolled where the graph is built, and says
    /// whether it did. CTRL-14: such a `return` "makes the graph's
    /// result that value while the branch is selected and the
    /// continuation's otherwise", and the continuation of a pass is the
    /// passes after it and what follows the loop, which this target does
    /// not write as a branch's graph.
    fn unrolled_return(&mut self, at: Location) -> bool {
        if self.unrolling > 0 {
            self.unsupported(
                at,
                "a `return` under a branch selected at runtime in a loop unrolled where the graph is built",
            );
        }
        self.unrolling > 0
    }

    /// `let name = if condition { ... } else { ... }`, or a `var`, over
    /// a connection, where a branch returns from the graph in its
    /// statements or in its value. `given` is the condition, the name
    /// and where the `if` is written; `branches` each branch's
    /// statements and value, the second `None` where there is no
    /// `else`.
    ///
    /// CTRL-14: "A `return value` in a branch of a temporal `if` makes
    /// the graph's result that value while the branch is selected and
    /// the continuation's otherwise". The name is the branch's: each
    /// branch's graph has its statements, then the name given the
    /// branch's value, and then what follows, `rest`, which returns.
    /// The graph returns what `switch_` gives. A branch with no value
    /// that can reach its end gives the name a connection that never
    /// ticks (CTRL-13), which is not written here.
    fn returned_value(
        &mut self,
        function: &Function,
        (condition, local, at): (&Expr, usize, Location),
        branches: [Option<(&[Stmt], Option<&Expr>)>; 2],
        rest: &[Stmt],
        depth: usize,
    ) -> Branched {
        if self.unrolled_return(at) {
            return Branched::Ended;
        }
        let Some(result) = self.current_result(function) else {
            self.unsupported(at, "a `return` in an `if` of a graph with no result");
            return Branched::Ended;
        };
        let mut written = Vec::new();
        for (key, branch) in ["True", "False"].into_iter().zip(branches) {
            let (statements, value) = branch.unwrap_or((&[], None));
            let statements = match value {
                _ if always_returns(statements) => statements.to_vec(),
                Some(value) => {
                    let mut all = statements.to_vec();
                    all.push(Stmt::Declare {
                        local,
                        value: Some(value.clone()),
                    });
                    continued(&all, rest)
                }
                None => {
                    self.unsupported(
                        at,
                        "a `return` in an `if` over a connection that gives a `let` or a `var` its value, with a branch that has no value",
                    );
                    return Branched::Ended;
                }
            };
            written.push(Branch {
                key: key.to_string(),
                statements,
                tail: Tail::Returns,
            });
        }
        let call = self.switched(function, condition, &written, Some(&result), at, depth);
        self.line(depth, &format!("return {call}"));
        Branched::Ended
    }

    /// An `if` or a `switch` over the connection `key` whose branches
    /// assign a `var` (CTRL-14): each branch's graph gives the `var`
    /// as the branch leaves it, and what hgraph's `switch_` gives is
    /// assigned to it. One `var` is written; branches that assign
    /// several are reported.
    fn assigned_in_branches(
        &mut self,
        function: &Function,
        key: &Expr,
        (branches, unmatched): (&[(&str, &[Stmt])], bool),
        at: Location,
        depth: usize,
    ) {
        // A `switch` with no `default` fails where no case matches.
        let failing = unmatched.then(|| Branch {
            key: "_hg.DEFAULT".to_string(),
            statements: Vec::new(),
            tail: Tail::Unmatched,
        });
        // The `var`s declared outside the branches that one assigns, by
        // name or by a place inside them.
        let mut assigned: Vec<usize> = Vec::new();
        for (_, statements) in branches {
            for local in assigned_outside(statements) {
                if !assigned.contains(&local) {
                    assigned.push(local);
                }
            }
        }
        // CTRL-3: "An outputless if has no synthetic result; an omitted
        // false branch performs no conditional work": branches that
        // assign nothing are graphs with no output, and `switch_` gives
        // nothing.
        if assigned.is_empty() {
            let branches: Vec<Branch> = branches
                .iter()
                .map(|(key, statements)| Branch {
                    key: (*key).to_string(),
                    statements: statements.to_vec(),
                    tail: Tail::Outputless,
                })
                .chain(failing)
                .collect();
            let call = self.switched(function, key, &branches, None, at, depth);
            self.line(depth, &call);
            return;
        }
        let [var] = assigned.as_slice() else {
            self.unsupported(
                at,
                "an `if` or a `switch` over a connection whose branches assign more than one `var`",
            );
            return;
        };
        let result = function.locals[*var].ty.clone();
        let branches: Vec<Branch> = branches
            .iter()
            .map(|(key, statements)| Branch {
                key: (*key).to_string(),
                statements: statements.to_vec(),
                tail: Tail::Local(*var),
            })
            .chain(failing)
            .collect();
        let call = self.switched(function, key, &branches, Some(&result), at, depth);
        let target = self.place(function, *var);
        self.line(depth, &format!("{target} = {call}"));
        self.bind(function, *var);
    }

    /// hgraph's `switch_` over the connection `key`, with a graph for
    /// each branch, written before the statement at `depth`. Gives the
    /// call. `result` is what each branch's graph gives, `None` for
    /// branches with no output (CTRL-3).
    ///
    /// A child graph is given what it reads: every branch's graph takes
    /// the connections any branch reads that are bound outside it, by
    /// their names, and `switch_` is given them. An ordinary value is
    /// read where the graph is built, as Python reads a name of the
    /// function it is written in. hgraph starts a branch's graph where
    /// its key is selected and stops it where another is, and keeps it
    /// while the key is published again as it was (CTRL-2).
    fn switched(
        &mut self,
        function: &Function,
        key: &Expr,
        branches: &[Branch],
        result: Option<&Type>,
        at: Location,
        depth: usize,
    ) -> String {
        use hgl_analyser::hir::walk_statement_nodes;
        self.hgraph = true;
        let number = self.switched;
        self.switched += 1;
        // What each branch's graph takes: every connection any branch
        // names (`taken`).
        let mut read: Vec<usize> = Vec::new();
        let mut declared: Vec<usize> = Vec::new();
        for branch in branches {
            let mut visit = |node| named_in(node, &mut read, &mut declared);
            walk_statement_nodes(&branch.statements, &mut visit);
            match &branch.tail {
                Tail::Value(value) => value.walk_nodes(&mut visit),
                // A branch that does not assign the `var` gives the
                // binding it had before.
                Tail::Local(var) => read.push(*var),
                Tail::Nothing | Tail::Returns | Tail::Outputless | Tail::Unmatched => {}
            }
        }
        let read = self.taken(function, read, &declared);
        self.windows_given(function, &read, None, at);
        let annotation = result.map(|result| {
            self.annotation(result, true, at)
                .unwrap_or_else(|| "_py.object".to_string())
        });
        let returns = annotation
            .as_ref()
            .map_or(String::new(), |annotation| format!(" -> {annotation}"));
        let parameters = self.parameters_of(function, &read, at);
        let mut cases = Vec::new();
        let outside = std::mem::replace(&mut self.bound, read.clone());
        for (position, branch) in branches.iter().enumerate() {
            // A child graph has its parameters bound, and what its own
            // statements bind.
            self.bound = read.clone();
            let graph = format!("_case_{number}_{position}");
            self.line(depth, "@_hg.graph");
            if let Tail::Unmatched = branch.tail {
                // `switch_` gives the selector to a branch whose first
                // parameter is `key`.
                let failing = self.unmatched_node(key, annotation.as_deref(), at);
                let mut taken = vec![format!(
                    "key: {}",
                    self.annotation(&key.ty, true, at)
                        .unwrap_or_else(|| "_py.object".to_string())
                )];
                taken.extend(parameters.iter().cloned());
                self.line(
                    depth,
                    &format!("def {graph}({}){returns}:", taken.join(", ")),
                );
                let call = format!("{failing}(key=key)");
                self.line(
                    depth + 1,
                    &if annotation.is_some() {
                        format!("return {call}")
                    } else {
                        call
                    },
                );
                cases.push(format!("{}: {graph}", branch.key));
                continue;
            }
            self.line(
                depth,
                &format!("def {graph}({}){returns}:", parameters.join(", ")),
            );
            self.renamed(function, &read, depth + 1);
            // What the branch gives is the last of its statements, so
            // that an `if` or a `switch` among them is lowered with it.
            let mut all = branch.statements.clone();
            match &branch.tail {
                Tail::Value(value) => all.push(Stmt::Return {
                    value: Some((**value).clone()),
                    at: value.at,
                }),
                Tail::Local(var) => all.push(Stmt::Return {
                    value: Some(Expr {
                        kind: ExprKind::Local(*var),
                        ty: function.locals[*var].ty.clone(),
                        at,
                    }),
                    at,
                }),
                Tail::Nothing | Tail::Returns | Tail::Outputless | Tail::Unmatched => {}
            }
            self.in_case += 1;
            self.results.extend(result.cloned());
            let lines = self.statements(function, &all, depth + 1);
            if result.is_some() {
                self.results.pop();
            }
            self.in_case -= 1;
            if let (Tail::Nothing, Some(annotation)) = (&branch.tail, &annotation)
                && !always_returns(&all)
            {
                self.line(depth + 1, &format!("return _hg.nothing({annotation})"));
            } else if lines == 0 {
                // CTRL-3: "an omitted false branch performs no
                // conditional work": a graph with nothing in it, as
                // `switch_` needs a graph for each key it is given.
                self.line(depth + 1, "pass");
            }
            cases.push(format!("{}: {graph}", branch.key));
        }
        let key = self.expression(function, key);
        self.bound = outside;
        let mut given = vec![key, format!("{{{}}}", cases.join(", "))];
        given.extend(read.iter().map(|&local| {
            let python = name(&function.locals[local].name);
            format!("{}={python}", taken_name(&python))
        }));
        format!("_hg.switch_({})", given.join(", "))
    }

    /// The node a `switch_`'s failing `default` wires (`Tail::Unmatched`):
    /// it fails with the provider's `unmatched` when it is evaluated, as
    /// the branch is selected, giving what `annotation` names, or
    /// nothing.
    fn unmatched_node(&mut self, key: &Expr, annotation: Option<&str>, at: Location) -> String {
        self.natives = true;
        let selector = self
            .annotation(&key.ty, true, at)
            .unwrap_or_else(|| "_py.object".to_string());
        let (decorator, returns) = match annotation {
            Some(annotation) => ("compute_node", format!(" -> {annotation}")),
            None => ("sink_node", String::new()),
        };
        self.lifted_node(
            "unmatched",
            format!("@_hg.{decorator}\ndef "),
            format!("(key: {selector}){returns}:\n    _core.unmatched(key.value)\n"),
        )
    }

    /// What a child graph takes of the graph it is written in, from
    /// what its statements name, `read`, and declare, `declared`
    /// ([`named_in`]): every connection named, read or assigned, at any
    /// depth, that has a value where the child is written
    /// (`Emitter::bound`), less the names the child declares itself.
    /// One the child does not need is harmless; one with no value yet is
    /// never given. A key that hgraph gives as a port is a connection
    /// here (`Emitter::ported`).
    fn taken(&self, function: &Function, mut read: Vec<usize>, declared: &[usize]) -> Vec<usize> {
        read.retain(|local| {
            !declared.contains(local)
                && self.bound.contains(local)
                && (function.locals[*local].category == ValueCategory::Connection
                    || self.ported.contains(local))
        });
        read.sort_unstable();
        read.dedup();
        read
    }

    /// Reports, at `at`, a window given to a child graph: one that a
    /// connection `taken` holds, itself or within (`windows_in`), or
    /// that the loop's sequence `over` holds. hgraph's `switch_` and
    /// `map_` fail as the graph runs where a `TSW` is given to the
    /// graph they make, alone or as the child of a bundle or a
    /// dictionary, even as a reference to one.
    fn windows_given(
        &mut self,
        function: &Function,
        taken: &[usize],
        over: Option<&Type>,
        at: Location,
    ) {
        let program = self.program;
        let given = taken
            .iter()
            .map(|&local| &function.locals[local].ty)
            .chain(over)
            .any(|ty| !windows_in(program, ty).is_empty());
        if given {
            self.unsupported(at, "a window given to the graph of a branch or of a loop");
        }
    }

    /// The parameters of a child graph that takes the connections
    /// `taken`, each by its name, with its annotation.
    fn parameters_of(&mut self, function: &Function, taken: &[usize], at: Location) -> Vec<String> {
        taken
            .iter()
            .map(|&local| {
                let declared = &function.locals[local];
                let of = self
                    .annotation(&declared.ty, true, at)
                    .unwrap_or_else(|| "_py.object".to_string());
                format!("{}: {of}", taken_name(&name(&declared.name)))
            })
            .collect()
    }

    /// The first lines of a child graph that takes the connections
    /// `taken`: each one taken under a name of the target's own
    /// ([`taken_name`]) is given its own name.
    fn renamed(&mut self, function: &Function, taken: &[usize], depth: usize) {
        for &local in taken {
            let python = name(&function.locals[local].name);
            let given = taken_name(&python);
            if given != python {
                self.line(depth, &format!("{python} = {given}"));
            }
        }
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
        function.mode == Mode::Graph && self.connects(function, value)
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
                let root = place_root(place);
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
                let assigned = if to_out {
                    self.assigned_to_out(function, place, value, copied)
                } else {
                    self.assigned(function, place, copied)
                };
                if let Some((target, value)) = assigned {
                    self.hoist(depth);
                    self.line(depth, &format!("{target} = {value}"));
                }
                self.held.retain(|(at, _)| !held_here.contains(at));
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
                    let connection = self.connects(function, returned);
                    self.normalized(function, value, connection, returned.at)
                } else if function.mode == Mode::Graph
                    && self.in_case > 0
                    && !self.connects(function, returned)
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
                let before = self.bound.clone();
                self.unrolling += 1;
                self.block(function, body, depth + 1);
                self.unrolling -= 1;
                self.bound = before;
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
                if self.connects(function, sequence) && !fixed {
                    self.each(function, names, sequence, body, *at, depth);
                } else {
                    let before = self.bound.clone();
                    for &name in names.iter() {
                        self.bind(function, name);
                    }
                    self.unrolling += 1;
                    self.for_loop(function, (names, sequence), body, *at, depth);
                    self.unrolling -= 1;
                    self.bound = before;
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
                let before = self.bound.clone();
                self.block(function, body, depth + 1);
                self.bound = before;
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
                let before = self.bound.clone();
                for &name in names.iter() {
                    self.bind(function, name);
                }
                self.for_loop(function, (names, sequence), body, *at, depth);
                self.bound = before;
            }
            Stmt::Switch {
                selector,
                cases,
                default,
                ..
            } if function.mode.runs() => {
                // Only the case selected runs: what a case binds is not
                // bound after.
                let before = self.bound.clone();
                self.switch(function, selector, cases, default.as_deref(), depth);
                self.bound = before;
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

    /// CTRL-12: "`for item in items` runs its body for each element of
    /// a list, in index order, or of a set, and `for key, value in
    /// entries` for each key and value of a dictionary." CTRL-8: "During
    /// node evaluation, traversal uses current values ... Lists preserve
    /// index order": in a hook a temporal input is read as its current
    /// value (`port_value`), a list's a tuple in index order.
    ///
    /// Python's `for` reads what it loops over once, where the loop
    /// begins: a `var` the body assigns is not what is looped over.
    pub(super) fn for_loop(
        &mut self,
        function: &Function,
        (names, sequence): (&[usize], &Expr),
        body: &[Stmt],
        at: Location,
        depth: usize,
    ) {
        let over = self.expression(function, sequence);
        // A loop runs over the value its sequence had where the loop
        // began. Its names are references to the entries, and the loop
        // takes a copy only where its own body changes the place the
        // sequence is read from ([`Copies`]).
        let over = if names
            .first()
            .is_some_and(|&first| function.copies.copied(first))
            && !self.connection(function, sequence)
        {
            self.natives = true;
            format!("_core.copied({over})")
        } else {
            over
        };
        let bound: Vec<String> = names
            .iter()
            .map(|&local| self.place(function, local))
            .collect();
        match (&sequence.ty, bound.as_slice()) {
            (Type::List { .. } | Type::Set(_), [item]) => {
                self.line(depth, &format!("for {item} in {over}:"));
            }
            (Type::Dict { .. }, [key, value]) => {
                self.line(depth, &format!("for {key}, {value} in {over}.items():"));
            }
            _ => {
                self.unsupported(at, "this loop");
                return;
            }
        }
        self.block(function, body, depth + 1);
    }

    /// A `for` of a graph over a connection of `list<T>`, `set<T>` or
    /// `dict<K, V>`. CTRL-15: "it makes one child for each member
    /// (CTRL-7): `item` is a connection of `T` for a list and an
    /// ordinary `T`, the member itself, for a set; `key` is an ordinary
    /// `K` and `value` a connection of `V` for a dictionary." CTRL-7:
    /// "Update keeps the child; removal stops it; reinsertion creates
    /// fresh state. Captured temporal connections are bound whole and
    /// add no membership."
    ///
    /// The body is a graph of its own, `_each_N`, and hgraph's `map_`
    /// runs one of it for each key of a `TSD` it is given, keeps it
    /// while the key stays and stops it where the key is removed; a key
    /// added again has a child that starts afresh. A dictionary's `TSD`
    /// is given as it is, and each child has its value's port. A set's
    /// `TSS` is the keys, `__keys__`. A list, which ticks whole here
    /// (`list_annotation`), is made hgraph's `TSD` of its positions by
    /// hgraph's `convert`: a position that changes ticks, one past the
    /// list's end is removed. hgraph gives a child its key as a port,
    /// `__key_arg__`, and so a key or a member is read only as the whole
    /// argument of a temporal parameter (`key_read_as_value`), where a
    /// port is what is taken. The connections the body reads from
    /// outside it are given whole, `_hg.pass_through`.
    pub(super) fn each(
        &mut self,
        function: &Function,
        names: &[usize],
        sequence: &Expr,
        body: &[Stmt],
        at: Location,
        depth: usize,
    ) {
        use hgl_analyser::hir::walk_statement_nodes;
        // The key hgraph gives each child, and the member's own port.
        let (key, member) = match (&sequence.ty, names) {
            (Type::Set(_), [member]) => (Some(*member), None),
            (Type::Dict { .. }, [key, value]) => (Some(*key), Some(*value)),
            (Type::List { .. }, [item]) => (None, Some(*item)),
            _ => {
                self.unsupported(at, "this loop");
                return;
            }
        };
        if let Some(key) = key
            && let Some(read) = self.key_read_as_value(key, body)
        {
            self.unsupported(
                read,
                "a loop's key or member read other than as the argument of a temporal parameter",
            );
            return;
        }
        self.hgraph = true;
        let number = self.looped;
        self.looped += 1;
        // What the child takes of the graph it is written in, less the
        // loop's own names.
        let mut read: Vec<usize> = Vec::new();
        let mut declared: Vec<usize> = names.to_vec();
        walk_statement_nodes(body, &mut |node| named_in(node, &mut read, &mut declared));
        let key = key.filter(|key| read.contains(key));
        let taken = self.taken(function, read, &declared);
        self.windows_given(function, &taken, Some(&sequence.ty), at);
        // The child's parameters: the key, where the body reads it, the
        // member's port, and what it takes.
        let mut parameters = Vec::new();
        let mut own = Vec::new();
        if let Some(key) = key {
            // hgraph's key is a `TS` of a scalar.
            let ty = &function.locals[key].ty;
            if !whole_scalar(ty) {
                self.unsupported(at, &format!("a loop's key or member of type `{ty}`"));
            }
            let of = self
                .annotation(ty, true, at)
                .unwrap_or_else(|| "_py.object".to_string());
            parameters.push(format!("{}: {of}", name(&function.locals[key].name)));
            own.push(key);
        }
        let mut multiplexed = None;
        if let Some(member) = member {
            let child = match &sequence.ty {
                Type::Dict { value, .. } => self.child_annotation(&sequence.ty, value, at),
                // An element of a list that ticks whole publishes whole.
                Type::List { element, .. } if !is_nullable(element) => {
                    self.annotation(element, true, at)
                }
                _ => {
                    self.unsupported(at, &format!("a loop over `{}`", sequence.ty));
                    None
                }
            };
            let child = child.unwrap_or_else(|| "_py.object".to_string());
            let python = taken_name(&name(&function.locals[member].name));
            parameters.push(format!("{python}: {child}"));
            own.push(member);
            multiplexed = Some((python, child));
        }
        parameters.extend(self.parameters_of(function, &taken, at));
        let graph = format!("_each_{number}");
        self.line(depth, "@_hg.graph");
        self.line(depth, &format!("def {graph}({}):", parameters.join(", ")));
        self.renamed(function, &own[key.iter().len()..], depth + 1);
        self.renamed(function, &taken, depth + 1);
        own.extend(&taken);
        // The child has its parameters bound, and its key is a port.
        let outside = std::mem::replace(&mut self.bound, own);
        let unrolling = std::mem::take(&mut self.unrolling);
        let ported = self.ported.len();
        self.ported.extend(key);
        self.block(function, body, depth + 1);
        self.ported.truncate(ported);
        self.unrolling = unrolling;
        self.bound = outside;
        let over = self.expression(function, sequence);
        let mut given = vec![graph];
        match (&sequence.ty, multiplexed) {
            (Type::Set(_), _) => given.push(format!("__keys__={over}")),
            (Type::List { .. }, Some((member, child))) => given.push(format!(
                "{member}=_hg.convert[_hg.TSD[_py.int, {child}]]({over})"
            )),
            (_, Some((member, _))) => given.push(format!("{member}={over}")),
            (_, None) => {}
        }
        let key_arg = key.map_or(String::new(), |key| name(&function.locals[key].name));
        given.push(format!("__key_arg__={}", string(&key_arg)));
        given.extend(taken.iter().map(|&local| {
            let python = name(&function.locals[local].name);
            format!("{}=_hg.pass_through({python})", taken_name(&python))
        }));
        self.line(depth, &format!("_hg.map_({})", given.join(", ")));
    }

    /// Where `body`, the body of a loop that makes one child for each
    /// member, reads `key`, the loop's key or member, other than as the
    /// whole argument of a temporal parameter of a temporal function:
    /// `None` where it reads it only so. hgraph's `map_` gives a child
    /// its key as a port, and a port is what such a parameter takes; an
    /// ordinary value given one is a constant of it (FN-3), which ticks
    /// where the child starts, as the key's port does. Anywhere else the
    /// key would be a value known where the child is built, which
    /// hgraph does not give.
    fn key_read_as_value(&self, key: usize, body: &[Stmt]) -> Option<Location> {
        use hgl_analyser::hir::{Node, walk_statement_nodes};
        let program = self.program;
        let mut reads = Vec::new();
        let mut given = Vec::new();
        walk_statement_nodes(body, &mut |node| match node {
            Node::Expression(Expr {
                kind: ExprKind::Local(local),
                at,
                ..
            }) if *local == key => reads.push(*at),
            Node::Expression(Expr {
                kind:
                    ExprKind::Call {
                        callee:
                            Callee::Function {
                                module,
                                declaration,
                                ..
                            },
                        arguments,
                        ..
                    },
                ..
            }) => {
                let declared = &program.modules[*module];
                let temporal =
                    declared.declarations[*declaration].kind.role() == Some(Role::Temporal);
                let Some(Detail::Callable(signature)) = declared.details.get(*declaration) else {
                    return;
                };
                for argument in arguments {
                    let taken = argument
                        .parameter
                        .and_then(|index| signature.parameters.get(index))
                        .is_some_and(|parameter| {
                            !parameter.constant && !parameter.var && parameter.pack.is_none()
                        });
                    if temporal
                        && taken
                        && matches!(argument.value.kind, ExprKind::Local(local) if local == key)
                    {
                        given.push(argument.value.at);
                    }
                }
            }
            _ => {}
        });
        reads.into_iter().find(|at| !given.contains(at))
    }

    /// CTRL-6: "Switch labels are typed source constants, compared by
    /// resolved value ... There is no fallthrough. An unmatched
    /// selector uses default or fails." CTRL-12: "A `switch` there
    /// selects on a `bool`, `i64`, `str`, or enum value."
    ///
    /// The selector is evaluated once and held under a name of the
    /// target's own, `_selected_N`, N the depth the `switch` is written
    /// at: one nested in a case has another. Each case is an `if` on
    /// it, and what no case matches falls to the `default`, or fails in
    /// the provider.
    pub(super) fn switch(
        &mut self,
        function: &Function,
        selector: &Expr,
        cases: &[Case],
        default: Option<&[Stmt]>,
        depth: usize,
    ) {
        if self.labels_repeat(cases) {
            return;
        }
        let selected = format!("_selected_{depth}");
        let selector = self.expression(function, selector);
        self.line(depth, &format!("{selected} = {selector}"));
        for (index, case) in cases.iter().enumerate() {
            let label = self.expression(function, &case.label);
            let word = if index == 0 { "if" } else { "elif" };
            self.line(depth, &format!("{word} {selected} == {label}:"));
            self.block(function, &case.body, depth + 1);
        }
        // With no case there is no `if` for an `else` to follow.
        let inner = if cases.is_empty() {
            depth
        } else {
            self.line(depth, "else:");
            depth + 1
        };
        match default {
            Some(default) => self.block(function, default, inner),
            None => {
                self.natives = true;
                self.line(inner, &format!("_core.unmatched({selected})"));
            }
        }
    }

    /// `if condition { ... } else { ... }` standing as a statement.
    pub(super) fn if_statement(
        &mut self,
        function: &Function,
        condition: &Expr,
        then: &Expr,
        otherwise: Option<&Expr>,
        depth: usize,
    ) {
        let condition = self.expression(function, condition);
        self.line(depth, &format!("if {condition}:"));
        let before = self.bound.clone();
        self.branch(function, then, depth + 1);
        let after_then = std::mem::replace(&mut self.bound, before);
        if let Some(otherwise) = otherwise {
            self.line(depth, "else:");
            self.branch(function, otherwise, depth + 1);
        }
        // What is bound after: what both branches bind, less a branch
        // that returns, which does not reach the next statement.
        let ends = |branch: &Expr| always_returns(&alternative_statements(Some(branch)));
        if ends(then) {
            // The `else`'s bindings stand.
        } else if otherwise.is_some_and(ends) {
            self.bound = after_then;
        } else {
            self.bound.retain(|local| after_then.contains(local));
        }
    }

    /// Marks `local` as having a value in the graph being written
    /// (`Emitter::bound`).
    pub(super) fn bind(&mut self, function: &Function, local: usize) {
        if function.mode == Mode::Graph && !self.bound.contains(&local) {
            self.bound.push(local);
        }
    }

    /// A branch of an `if` that stands as a statement: the statements
    /// of its block, and its last expression as one more.
    fn branch(&mut self, function: &Function, branch: &Expr, depth: usize) {
        let before = self.text.len();
        // `else if` is the next `if`, as the one statement of the
        // `else`. Each goes through `statements`, so that in a graph an
        // `if` over a connection in the branch is composition
        // (CTRL-13).
        let statements = alternative_statements(Some(branch));
        self.statements(function, &statements, depth);
        if self.text.len() == before {
            self.line(depth, "pass");
        }
    }

    // ---- Expressions ----

    /// An expression as Python. One with no Python here yet is reported
    /// and written `None`.
    fn expression(&mut self, function: &Function, value: &Expr) -> String {
        // In a graph a value may be a connection, and Python's `and`,
        // `not` and `if` would act on the port: only a name, a literal,
        // a member of an enum, a constant, a call and a widening are
        // written there.
        if function.mode == Mode::Graph
            && !matches!(
                value.kind,
                ExprKind::Literal(_)
                    | ExprKind::Local(_)
                    | ExprKind::Call { .. }
                    | ExprKind::Widen(_)
                    | ExprKind::EnumMember { .. }
                    | ExprKind::Constant { .. }
                    | ExprKind::Capture(_)
                    | ExprKind::Captured { .. }
                    | ExprKind::TagTest { .. }
            )
            && !self.connected_field(function, value)
            && !self.connected_element(function, value)
            && !(matches!(value.kind, ExprKind::Tuple(_)) && self.connects(function, value))
        {
            self.unsupported(value.at, "this expression in a graph");
            return "None".to_string();
        }
        // What a call that assigns places has evaluated already is held
        // under a name of its own, and is not evaluated again (FN-15).
        if let Some((_, held)) = self.held.iter().find(|(at, _)| *at == value.at) {
            return held.clone();
        }
        match &value.kind {
            ExprKind::Literal(literal) => self.literal(literal, value.at),
            // UNIT-13: a constant is read by a call of the function
            // that gives its value, of its own module or another's.
            ExprKind::Constant {
                module,
                declaration,
            } => format!("{}()", self.function_name(*module, *declaration)),
            ExprKind::Local(local) => {
                // In a hook a temporal input is read through its
                // payload: in a `when`, and in `stop` (TIME-5). So is
                // `out` read whole: a struct's value is made from its
                // bundle's fields (TYPE-9), as an input's is. The port
                // holds a value of the declared type (`port_type`).
                if matches!(function.mode, Mode::Node | Mode::Stop)
                    && let Some(port) = self.input_port(function, value)
                {
                    return self.port_value(function.port_type(value), &port, value.at);
                }
                let read = self.place(function, *local);
                if function.mode == Mode::Lifecycle && function.is_input(*local) {
                    // The analyser rejects it (`input.lifecycle`).
                    self.unsupported(value.at, "a temporal input read in `start`");
                    read
                } else {
                    // What a place holds is read as it is: a copy is
                    // made where a reference to it is kept, and not
                    // where it is read (see `kept`).
                    read
                }
            }
            ExprKind::Endpoint {
                operation,
                inputs: Some(inputs),
            } if inputs.len() == 1 => {
                let endpoint = &function.locals[inputs[0]];
                // At stop, validity guards payload reads of inputs and out,
                // and both expose their last publication time (TIME-5, TIME-7).
                let given = match function.mode {
                    Mode::Lifecycle => false,
                    Mode::Stop => {
                        matches!(
                            operation,
                            EndpointOperation::Valid
                                | EndpointOperation::AllValid
                                | EndpointOperation::LastModified
                        ) || (endpoint.kind != LocalKind::Inject
                            && matches!(
                                operation,
                                EndpointOperation::IsActive
                                    | EndpointOperation::Activate
                                    | EndpointOperation::Passivate
                            ))
                    }
                    _ => true,
                };
                if !given {
                    self.unsupported(value.at, "this endpoint read in `start` or `stop`");
                }
                let input = endpoint_name(endpoint);
                // The endpoint of what holds a window, itself or within
                // (`windows_in`): an input's readiness is asked of its
                // parameter (`window_inputs`), and `out`'s here.
                // hgraph's `TSW` keeps what it holds when it is
                // invalidated, where END-4 has "Invalidation clears
                // validity, modification, payload, and delta".
                if !windows_in(self.program, &endpoint.ty).is_empty() {
                    if endpoint.kind == LocalKind::Inject {
                        self.readable_window(&endpoint.ty, value.at);
                    }
                    if matches!(operation, EndpointOperation::Invalidate) {
                        self.unsupported(value.at, "the invalidation of a window");
                    }
                }
                match operation {
                    // TIME-9, END-7: "`valid` of a collection is true where any
                    // element, field or entry is valid", as hgraph's
                    // `valid` of a `TSL`, a `TSB` and a `TSD` is.
                    EndpointOperation::Valid => format!("{input}.valid"),
                    // TIME-9: "`all_valid` is true where every one is, by
                    // its own `valid`, one level deep": hgraph's
                    // `all_valid`. An unsized list ticks whole here
                    // (`list_annotation`): it is valid with each element.
                    EndpointOperation::AllValid if growing_list(&endpoint.ty) => {
                        format!("{input}.valid")
                    }
                    EndpointOperation::AllValid => format!("{input}.all_valid"),
                    // END-4: "Invalidation clears validity,
                    // modification, payload, and delta." hgraph's own
                    // `modified` of an input in the cycle it is
                    // invalidated is true to a node that injects
                    // something, so it is held to the input being
                    // valid.
                    EndpointOperation::Modified => {
                        format!("({input}.modified and {input}.valid)")
                    }
                    // END-4: "Invalidation clears validity,
                    // modification, payload, and delta."
                    EndpointOperation::Invalidate => format!("{input}.invalidate()"),
                    // TIME-9: input activity controls delivery of notifications.
                    EndpointOperation::Activate => format!("{input}.make_active()"),
                    EndpointOperation::Passivate => format!("{input}.make_passive()"),
                    EndpointOperation::IsActive => format!("{input}.active"),
                    // END-3: what the input published this cycle, read as
                    // the program reads a delta of its type
                    // (`language_delta`).
                    EndpointOperation::DeltaValue => {
                        let ty = endpoint.ty.clone();
                        self.language_delta(&ty, format!("{input}.delta_value"), value.at)
                    }
                    // END-6: an invalid endpoint has no timestamp, even
                    // if it held a value before being invalidated.
                    EndpointOperation::LastModified => {
                        format!("({input}.last_modified_time if {input}.valid else None)")
                    }
                    other => {
                        self.unsupported(value.at, &format!("`{}`", other.name()));
                        "None".to_string()
                    }
                }
            }
            // END-8: "A child of a collection is an endpoint whether or
            // not it is valid: `valid`, `modified`, `delta_value` and
            // `last_modified` take it, and `all_valid` where it is a
            // collection". It is hgraph's own child of the input.
            ExprKind::ChildEndpoint { operation, child } => {
                self.child_operation(function, *operation, child, value.at)
            }
            // TIME-1: "Elsewhere, `parts[i]`, it is checked when it
            // runs: a child that is not valid fails it
            // (`value.unset_read`), or gives an `err` where its place
            // takes a `result` (ERR-14)."
            ExprKind::CheckedChild { read, result } => {
                self.checked_child(function, read, *result, value.at)
            }
            // TIME-9: `scheduled()` as a bool, where it sets no
            // subscription (TIME-3: "Selectors under negation or
            // disjunction leave the default in place"): what woke the
            // node, as `node` holds it where the node is entered. In
            // `start` and `stop` no wake is due, and the rule says
            // nothing of it there.
            ExprKind::Endpoint {
                operation: EndpointOperation::Scheduled,
                ..
            } => match (function.mode, &self.scheduled) {
                (Mode::Node, Some(woke)) => woke.clone(),
                _ => {
                    self.unsupported(value.at, "`scheduled()` outside a `when`");
                    "None".to_string()
                }
            },
            // The language widens a literal and no connection (EXPR-5),
            // so what is widened here is known where the graph is
            // built. Python's `float` would act on a port.
            ExprKind::Widen(inner)
                if function.mode == Mode::Graph && self.connects(function, inner) =>
            {
                self.unsupported(value.at, "a widening of a connection");
                "None".to_string()
            }
            ExprKind::Widen(inner) => {
                format!("_py.float({})", self.expression(function, inner))
            }
            // Core's OPS-6: `convert<list<B>>(values)`.
            ExprKind::ParentList(_) => {
                self.unsupported(
                    value.at,
                    "a list of structs converted to a list of their parent",
                );
                "None".to_string()
            }
            ExprKind::Unary {
                operator: UnaryOperator::Not,
                operand,
            } => format!("(not {})", self.expression(function, operand)),
            ExprKind::Binary {
                operator: operator @ (BinaryOperator::And | BinaryOperator::Or),
                left,
                right,
            } => {
                let word = if *operator == BinaryOperator::And {
                    "and"
                } else {
                    "or"
                };
                let left = self.expression(function, left);
                let right = self.expression(function, right);
                format!("({left} {word} {right})")
            }
            // A test against `null` is the language's own, and is
            // recorded as it is written (ERR-12).
            ExprKind::Binary {
                operator: operator @ (BinaryOperator::Equal | BinaryOperator::NotEqual),
                left,
                right,
            } if is_null(left) || is_null(right) => {
                let tested = if is_null(left) { right } else { left };
                let tested = self.expression(function, tested);
                if *operator == BinaryOperator::Equal {
                    format!("({tested} is None)")
                } else {
                    format!("({tested} is not None)")
                }
            }
            ExprKind::Call {
                callee, arguments, ..
            } => self.call(function, callee, arguments, value),
            // A block that is only its value, and an `if` between two
            // such.
            ExprKind::Block { statements, value } if statements.is_empty() => match value {
                Some(value) => self.expression(function, value),
                None => "None".to_string(),
            },
            ExprKind::If {
                condition,
                then,
                otherwise: Some(otherwise),
            } => {
                let condition = self.expression(function, condition);
                let then = self.expression(function, then);
                let otherwise = self.expression(function, otherwise);
                format!("({then} if {condition} else {otherwise})")
            }
            // CTRL-10: "With no `else` it is as if its `else` were null".
            ExprKind::If {
                condition,
                then,
                otherwise: None,
            } => {
                let condition = self.expression(function, condition);
                let then = self.expression(function, then);
                format!("({then} if {condition} else None)")
            }
            ExprKind::Tuple(entries)
                if function.mode == Mode::Graph && self.connects(function, value) =>
            {
                self.connected_tuple(function, value, entries)
            }
            ExprKind::Tuple(entries) => {
                let entries: Vec<String> = entries
                    .iter()
                    .map(|entry| self.expression(function, entry))
                    .collect();
                format!("({},)", entries.join(", "))
            }
            ExprKind::Sequence(entries) => {
                let entries: Vec<String> = entries
                    .iter()
                    .map(|entry| self.expression(function, entry))
                    .collect();
                // EXPR-9: "The use site and expected type constrain
                // element types, keys, ordering, uniqueness": a
                // sequence that is a set is one in Python too, and
                // "Empty brackets have no entries": `[]` where a
                // dictionary is expected is an empty one.
                let made = match &value.ty {
                    Type::Nullable(inner) => inner,
                    ty => ty,
                };
                let is_set = matches!(made, Type::Set(_));
                if matches!(made, Type::Dict { .. }) {
                    "{}".to_string()
                } else if is_set {
                    format!(
                        "_py.frozenset(({}{}))",
                        entries.join(", "),
                        if entries.len() == 1 { "," } else { "" }
                    )
                } else {
                    format!("[{}]", entries.join(", "))
                }
            }
            // EXPR-9: `[key: value, ...]`, a dictionary. Python evaluates
            // each key and then its value, in the order written.
            ExprKind::Pairs(entries) => {
                let entries: Vec<String> = entries
                    .iter()
                    .map(|(key, entry)| {
                        let key = self.expression(function, key);
                        let entry = self.expression(function, entry);
                        format!("{key}: {entry}")
                    })
                    .collect();
                format!("{{{}}}", entries.join(", "))
            }
            ExprKind::Delta { arguments } => self.delta(function, value, arguments),
            // TST-17, RUN-3: `run(...)` in an integration test is the
            // eval's run in real time.
            ExprKind::Eval {
                call,
                config,
                real_time,
            } => self.eval(function, call, &value.ty, config.as_deref(), *real_time),
            // TST-16: "A trace given to a `const fn` is an ordinary
            // value there: ... a `list<tuple<datetime, delta<T>>>`, one
            // tuple for each publication". The harness makes it of the
            // dense trace an eval gives.
            ExprKind::Publications(trace) => {
                let trace = self.expression(function, trace);
                format!("_harness.publications({trace})")
            }
            ExprKind::Trace(cells) => self.trace(function, cells, &value.ty),
            ExprKind::TraceCompare {
                left,
                right,
                negated,
            } => {
                // TST-4: "Harness comparison is intrinsic: compare
                // presence, exact present data, and shape/length". The
                // harness compares, and not Python's `==`: hgraph's set
                // delta takes a removed element for an added one.
                let left = self.expression(function, left);
                let right = self.expression(function, right);
                let not = if *negated { "not " } else { "" };
                format!("{not}_harness.same({left}, {right})")
            }
            // TYPE-7: a member of an enum, however it was written,
            // read from its `Enum` by its name.
            ExprKind::EnumMember {
                module,
                declaration,
                member,
            } => {
                let written = match self.program.modules[*module].details.get(*declaration) {
                    Some(Detail::Enum(read)) => read.members.get(*member),
                    _ => None,
                };
                let Some(written) = written else {
                    self.unsupported(value.at, "this member");
                    return "None".to_string();
                };
                let member = string(&member_name(&written.name));
                format!("{}[{member}]", self.type_name(*module, *declaration))
            }
            // TYPE-7: "`Side(1)` is the member whose number is 1, and
            // `Side("buy")` the member of that exact name ... a value
            // that is none fails where it is evaluated."
            ExprKind::EnumFrom {
                module,
                declaration,
                value,
            } => {
                self.natives = true;
                let of = self.type_name(*module, *declaration);
                let value = self.expression(function, value);
                format!("_core.member({of}, {value})")
            }
            ExprKind::Construct {
                module,
                declaration,
                arguments,
            } => self.construct(function, (*module, *declaration), arguments, value),
            // TIME-1: a child of a temporal input read in a hook, where it
            // is admitted, is read through its own endpoint, hgraph's
            // child of the input (`child_port`), as one read when it runs
            // is: never through its siblings, which may not be valid.
            ExprKind::Field { .. } | ExprKind::Index { .. }
                if matches!(function.mode, Mode::Node | Mode::Stop)
                    && self.reads_child(function, value) =>
            {
                match self.child_port(function, value) {
                    Some(port) => self.part_value(&value.ty, &port, value.at),
                    None => {
                        self.unsupported(value.at, "this read of a child");
                        "None".to_string()
                    }
                }
            }
            // TYPE-3: a struct's fields are public, and read by name.
            ExprKind::Field { base, member } => {
                // CAP-1: a property of a capability "is read as an
                // ordinary value". It is the provider's to read, as a
                // native function is: `_core.Clock_evaluation_time`.
                if let Type::Named {
                    name: native,
                    module,
                    declaration,
                    kind: NamedKind::Native,
                    ..
                } = &base.ty
                {
                    let program = self.program;
                    let property = match program.modules[*module].details.get(*declaration) {
                        Some(Detail::Native(read)) => read.properties.get(*member),
                        _ => None,
                    };
                    let read = property.and_then(|property| {
                        self.native_function(*module, format!("{native}_{}", property.name))
                            .map(|read| (property, read))
                    });
                    return match read {
                        Some((property, read)) => {
                            self.natives = true;
                            let base = self.expression(function, base);
                            self.native_result(Some(&property.ty), format!("_core.{read}({base})"))
                        }
                        _ => {
                            self.unsupported(value.at, "this property of a native type");
                            "None".to_string()
                        }
                    };
                }
                let delta = DeltaOf::of(&base.ty);
                // TYPE-16: a nullable's delta has one property, `value`,
                // "null for a published null". hgraph's delta of that
                // is `Null`.
                if matches!(delta, Some(DeltaOf::Nullable)) && *member == 0 {
                    self.natives = true;
                    let base = self.expression(function, base);
                    return format!("_core.unpublished({base})");
                }
                // TYPE-16: a dictionary's delta has `updated` and
                // `removed`, read through the provider from hgraph's
                // `dict` of both (see `delta`).
                // TYPE-16: a list's delta has `updated`, `removed` and
                // `size_modified` (see `list_delta`). hgraph's bounded
                // `TSL` does not report a position that became invalid,
                // so a fixed list's `removed` is not read; its size
                // does not move, so its `size_modified` is null.
                if let Some(DeltaOf::List { fixed }) = delta {
                    let read = match (*member, fixed) {
                        (0, _) => "updated",
                        (1, false) => "removed",
                        (2, false) => "size_modified",
                        (2, true) => return "None".to_string(),
                        _ => {
                            self.unsupported(
                                value.at,
                                "the `removed` of a fixed list's delta: hgraph's bounded `TSL` \
                                 does not report a position that became invalid",
                            );
                            return "None".to_string();
                        }
                    };
                    self.natives = true;
                    let base = self.expression(function, base);
                    return format!("_core.{read}({base})");
                }
                if matches!(delta, Some(DeltaOf::Dict)) {
                    let read = if *member == 0 { "updated" } else { "removed" };
                    self.natives = true;
                    let base = self.expression(function, base);
                    return format!("_core.{read}({base})");
                }
                // TYPE-16: a struct's delta has "a property for each
                // field ... null where the field did not change". It is
                // hgraph's delta of a bundle, a `dict` of the fields
                // that changed, and a field is read through the
                // provider.
                if let Some(DeltaOf::Struct(of)) = delta
                    && let Some(field) = field_of(self.program, of, *member)
                {
                    self.natives = true;
                    let base = self.expression(function, base);
                    return format!("_core.changed({base}, {})", string(&name(&field.name)));
                }
                // TYPE-9: an `atomic<V>` of a struct holds the whole
                // struct, whose fields are read as the struct's.
                let whole = payload_of(&base.ty);
                // In a graph that field of an atomic struct's connection
                // is hgraph's `getattr_`, told the field's type: the
                // provider's class does not declare one.
                if function.mode == Mode::Graph
                    && matches!(base.ty, Type::Atomic(_))
                    && let Some(field) = field_of(self.program, whole, *member)
                {
                    let Some(of) = self.annotation(&value.ty, false, value.at) else {
                        return "None".to_string();
                    };
                    let base = self.expression(function, base);
                    self.hgraph = true;
                    return format!(
                        "_hg.getattr_[_hg.SCALAR: {of}]({base}, {})",
                        string(&name(&field.name))
                    );
                }
                // In a hook a field of a temporal struct input is
                // read through the field's own port (TYPE-9: structs
                // map "to child fields"): `quote.bid.value`.
                if matches!(function.mode, Mode::Node | Mode::Stop)
                    && let Some(port) = self.input_port(function, value)
                {
                    return self.part_value(&value.ty, &port, value.at);
                }
                let Some(field) = field_of(self.program, whole, *member) else {
                    // A property of a delta.
                    self.unsupported(value.at, "this member");
                    return "None".to_string();
                };
                let base = self.expression(function, base);
                // In a graph it is a field of a bundle's connection,
                // the child's port, taken by its name: a field may be
                // named as a port's own attribute is, `value`.
                if function.mode == Mode::Graph {
                    return format!("{base}[{}]", string(&name(&field.name)));
                }
                format!("{base}.{}", name(&field.name))
            }
            // EXPR-7: "Brackets read an element of a list or a tuple by
            // an `i64` index, and the value a dictionary has for a
            // key." ERR-6: a dictionary's brackets "are strict
            // point-in-time reads; absence fails".
            ExprKind::Index { base, index } if function.mode == Mode::Graph => {
                // TYPE-9: an element of a list's or a dictionary's
                // connection is that child's own connection, taken from
                // hgraph's `TSL` by a position known where the graph is
                // built, and from its `TSD` by a key that may be a
                // connection: a key that ticks selects another child.
                if matches!(base.ty, Type::List { .. }) && self.connects(function, index) {
                    self.unsupported(value.at, "a list's element at a position that ticks");
                    return "None".to_string();
                }
                // ERR-6: a dictionary's brackets "are strict
                // point-in-time reads; absence fails". hgraph's own
                // child selection is silent where the key is absent, so
                // the provider's node selects it and fails there, with
                // the code `value.missing_key`. It publishes where the
                // key ticks or the selected child does. A key known
                // where the graph is built is a constant connection of
                // the key's type.
                if let Type::Dict { key, value: child } = &base.ty {
                    if !publishes_whole(child) {
                        self.unsupported(value.at, "an element of a dictionary of composites");
                        return "None".to_string();
                    }
                    let connected = self.connects(function, index);
                    let Some(key) = self.annotation(key, false, value.at) else {
                        return "None".to_string();
                    };
                    let base = self.expression(function, base);
                    let index = self.expression(function, index);
                    let index = if connected {
                        index
                    } else {
                        format!("_hg.const({index}, tp=_hg.TS[{key}])")
                    };
                    self.natives = true;
                    return format!("_core.entry_of({base}, {index})");
                }
                // TYPE-9: a tuple's position is its own connection, the
                // field of its position (`tuple_annotation`), under a
                // constant index (EXPR-3).
                if matches!(base.ty, Type::Tuple(_)) {
                    let base = self.expression(function, base);
                    let key = self.position_key(function, index);
                    return format!("{base}[{key}]");
                }
                let base = self.expression(function, base);
                let index = self.expression(function, index);
                format!("{base}[{index}]")
            }
            // In a hook a position of a temporal tuple is read through
            // the position's own port (TYPE-9), as a field is.
            ExprKind::Index { base, .. }
                if matches!(function.mode, Mode::Node | Mode::Stop)
                    && matches!(base.ty, Type::Tuple(_))
                    && let Some(port) = self.input_port(function, value) =>
            {
                self.part_value(&value.ty, &port, value.at)
            }
            ExprKind::Index { base, index } => {
                let Some(read) = element_read(&base.ty) else {
                    self.unsupported(value.at, "this element");
                    return "None".to_string();
                };
                self.natives = true;
                let base = self.expression(function, base);
                let index = self.expression(function, index);
                format!("_core.{read}({base}, {index})")
            }
            // ERR-2: "`ok(value)` constructs a success and
            // `err(value)` a failure."
            ExprKind::Tagged { success, payload } => {
                self.natives = true;
                let payload = self.expression(function, payload);
                let tag = if *success { "Ok" } else { "Err" };
                format!("_core.{tag}({payload})")
            }
            // ERR-3: `is_ok(value)` and `is_err(value)` give a `bool`.
            // "In composition a test of a connection is an ordinary
            // lifted call (FN-6): it gives a connection of `bool` and
            // no fact": the provider's node, which publishes the test
            // on each tick of the result.
            ExprKind::TagTest {
                success,
                value: tested,
            } => {
                self.natives = true;
                let lifted = function.mode == Mode::Graph && self.connects(function, tested);
                let tested = self.expression(function, tested);
                let tag = if *success { "ok" } else { "err" };
                if lifted {
                    self.hgraph = true;
                    return format!("_core.lifted_is_{tag}({tested})");
                }
                let tag = if *success { "Ok" } else { "Err" };
                format!("_py.isinstance({tested}, _core.{tag})")
            }
            // ERR-3: "`binding.value` is the T and `binding.error` the
            // E", where a guard has proved which the binding is.
            ExprKind::Payload { success, result } => {
                let result = self.expression(function, result);
                let part = if *success { "value" } else { "error" };
                format!("{result}.{part}")
            }
            // ERR-4: `error(message: text, code: optional_code)`, the
            // code null when it is left out.
            ExprKind::Error { message, code } => {
                self.natives = true;
                // EXPR-2: arguments are evaluated in the order
                // written, and Python evaluates keyword arguments in
                // theirs. With `code` written first, a `?` in it is
                // met before one in `message`.
                let code_first = code
                    .as_ref()
                    .is_some_and(|code| code.at.span.start < message.at.span.start);
                let mut written = Vec::new();
                if !code_first {
                    written.push(format!("message={}", self.expression(function, message)));
                }
                written.push(match code {
                    Some(code) => format!("code={}", self.expression(function, code)),
                    None => "code=None".to_string(),
                });
                if code_first {
                    written.push(format!("message={}", self.expression(function, message)));
                }
                format!("_core.Error({})", written.join(", "))
            }
            ExprKind::ErrorPart { base, part } => {
                let base = self.expression(function, base);
                // ERR-4: the provider's `Error` has its context, an
                // `ErrorContext` or `None`, as `context`.
                let part = match part {
                    ErrorPart::Message => "message",
                    ErrorPart::Code => "code",
                    ErrorPart::Context => "context",
                    ErrorPart::Node => "node",
                    ErrorPart::Phase => "phase",
                    ErrorPart::EvaluationTime => "evaluation_time",
                };
                format!("{base}.{part}")
            }
            // ERR-5: "Postfix `?` evaluates once. Success yields T. In
            // a value function returning `result<U,E>`, failure
            // returns the same E as Err. In when, only
            // `result<T,error>` may propagate to node failure."
            ExprKind::Propagate(result) => {
                let called = match function.mode {
                    Mode::Value => {
                        self.propagates = true;
                        "propagated"
                    }
                    Mode::Node => "succeeded",
                    // The analyser rejects a `?` anywhere else
                    // (`result.propagation_phase`).
                    _ => {
                        self.unsupported(value.at, "a `?` here");
                        return "None".to_string();
                    }
                };
                self.natives = true;
                let result = self.expression(function, result);
                format!("_core.{called}({result})")
            }
            ExprKind::Capture(call) => self.capture(function, call),
            // ERR-7: "`.value` retains the normal output type": the
            // child's own output. `.error` is "a connection of `error`":
            // hgraph's stream of the child's failures, each made an
            // `error` by the provider's node.
            ExprKind::Captured { capture, error } => {
                let capture = self.expression(function, capture);
                if *error {
                    self.natives = true;
                    format!("_core.captured_error({capture}.exception)")
                } else {
                    format!("{capture}.out")
                }
            }
            _ => {
                self.unsupported(value.at, "this expression");
                "None".to_string()
            }
        }
    }

    /// `capture(callable, args...)` in a graph (ERR-7): "a temporal
    /// error boundary" around `call`, the call it names. hgraph's
    /// `try_except` gives a bundle of the child's output, `out`, and its
    /// failures, `exception`; for a child with no output it gives the
    /// failures alone, which are put in a bundle of that one field, so
    /// that `.error` reads either the same way.
    ///
    /// ERR-8: "Capture ends the failed pass without rollback or
    /// immediate retry. The child remains live for future ordinary work"
    /// and "Start/stop failures escape": hgraph catches a failure of the
    /// child's evaluation only, and the child is evaluated again on its
    /// next input. The nearest `try_except` catches a failure, once.
    ///
    /// Written here is the capture of a graph of the program, or of a
    /// source, given no wiring-time value: hgraph's `try_except` makes
    /// the child a graph of its own. Any other capture is written where
    /// it is the value of a `let` (`Emitter::captured_call`), and
    /// reported anywhere else; so is one in a test outside an eval, and
    /// one of a lifted operator or native function.
    pub(super) fn capture(&mut self, function: &Function, call: &Expr) -> String {
        let at = call.at;
        let program = self.program;
        // TST-3: a test is a composition block, and a capture there, as
        // a temporal call there, makes a connection Python has no
        // graph to make in.
        if function.mode == Mode::Test {
            self.unsupported(at, "a capture in a test, outside an eval");
            return "None".to_string();
        }
        let ExprKind::Call {
            callee:
                Callee::Function {
                    module,
                    declaration,
                    generics,
                    ..
                }
                | Callee::Candidate {
                    module,
                    declaration,
                    generics,
                    ..
                },
            arguments,
            ..
        } = &call.kind
        else {
            self.unsupported(at, "this capture");
            return "None".to_string();
        };
        let (module, declaration) = (*module, *declaration);
        let declared = &program.modules[module].declarations[declaration];
        let bodied = matches!(
            declared.kind,
            DeclarationKind::Implementation(..) | DeclarationKind::Function(_)
        );
        let Some(Detail::Callable(signature)) = program.modules[module].details.get(declaration)
        else {
            self.unsupported(at, "this capture");
            return "None".to_string();
        };
        let temporal = declared.kind.role() == Some(Role::Temporal);
        if !bodied || !(temporal || declared.kind.role() == Some(Role::Value)) {
            self.unsupported(at, "a capture of a lifted operator or native function");
            return "None".to_string();
        }
        if !temporal
            || self.is_node((module, declaration))
            || signature
                .parameters
                .iter()
                .any(|parameter| parameter.constant)
        {
            self.unsupported(
                at,
                "a capture of a node, of a lifted call or of a graph given a wiring-time \
                 value, other than as the value of a `let`",
            );
            return "None".to_string();
        }
        let python = self.called_name((module, declaration), generics, at);
        let arguments = self.arguments(
            function,
            (module, declaration),
            generics,
            signature,
            arguments,
        );
        let given = if arguments.is_empty() {
            python
        } else {
            format!("{python}, {arguments}")
        };
        self.attempted(&given, signature.result.is_none())
    }

    /// hgraph's `try_except` given `given`, its callable and then the
    /// callable's arguments (ERR-7). For a callable with no output,
    /// `outputless`, it gives the failures alone, which are put in a
    /// bundle of that one field, so that `.error` reads either the same
    /// way.
    ///
    /// ERR-8: "Start/stop failures escape, including lifecycle work
    /// triggered during evaluation". hgraph's `try_except` captures the
    /// failure of a child's `start` or `stop` that it meets while it
    /// evaluates what it captures, a branch of a `switch_` selected or
    /// left: the provider's `captured` wires beside it what raises such
    /// a failure again. ERR-1: so it does a failure that is no HGL
    /// error, which no capture takes.
    fn attempted(&mut self, given: &str, outputless: bool) -> String {
        self.hgraph = true;
        self.natives = true;
        let attempted = format!("_hg.try_except({given})");
        if outputless {
            format!("_core.captured(_hg.combine(exception={attempted}))")
        } else {
            format!("_core.captured({attempted})")
        }
    }

    /// `let seen = capture(child, value, 2)` in a graph (ERR-7), where
    /// `child` is a node, a value function lifted over a connection
    /// (FN-6), or a graph or a source given a wiring-time value. Writes
    /// what the capture needs before the statement at `depth`, and
    /// gives the capture; `None` where `call` is a graph or a source
    /// given none, which [`Emitter::capture`] writes.
    ///
    /// The capture is of a graph of the target's own, `_capture_0`,
    /// whose inputs are the child's temporal parameters and which calls
    /// the child with each wiring-time value bound; hgraph's
    /// `try_except` makes it a graph of its own. A node called there is
    /// one no call outside the capture shares: hgraph makes one node of
    /// two calls of a node with the same inputs and the same scalars in
    /// one graph, and `try_except` given a node directly would capture
    /// the failures of every call it is made of. And `try_except` takes
    /// no graph with a scalar input. "capturing a graph covers failures
    /// that leave that subgraph": the subgraph is the child's, called as
    /// it is anywhere else.
    ///
    /// EXPR-13: the arguments are evaluated once, in the order written,
    /// where the capture is: before the `let`'s own line. Each
    /// wiring-time value is held under a name of its own, which the
    /// graph reads. A connection that is a local or a literal is given
    /// as it is; any other is held too, in its place in the order. The
    /// defaults of what the call leaves out are evaluated after, in
    /// the graph's call of the child, as a call evaluates them.
    pub(super) fn captured_call(
        &mut self,
        function: &Function,
        call: &Expr,
        depth: usize,
    ) -> Option<String> {
        let at = call.at;
        let program = self.program;
        let ExprKind::Call {
            callee:
                Callee::Function {
                    module,
                    declaration,
                    generics,
                    ..
                }
                | Callee::Candidate {
                    module,
                    declaration,
                    generics,
                    ..
                },
            arguments,
            ..
        } = &call.kind
        else {
            return None;
        };
        let callee = (*module, *declaration);
        let declared = &program.modules[callee.0].declarations[callee.1];
        let Some(Detail::Callable(signature)) = program.modules[callee.0].details.get(callee.1)
        else {
            return None;
        };
        let bodied = matches!(
            declared.kind,
            DeclarationKind::Implementation(..) | DeclarationKind::Function(_)
        );
        let node = self.is_node(callee);
        // FN-6: "A selected value helper ... with temporal arguments in
        // composition is lifted". ERR-7: a value function given no
        // connection composes nothing, which the analyser rejects
        // (`capture.phase`).
        let lifted = declared.kind.role() == Some(Role::Value)
            && bodied
            && arguments
                .iter()
                .any(|argument| self.connects(function, &argument.value));
        let constants = signature
            .parameters
            .iter()
            .any(|parameter| parameter.constant);
        let temporal = declared.kind.role() == Some(Role::Temporal) && bodied;
        if !(lifted || temporal && (node || constants)) {
            return None;
        }
        if signature
            .parameters
            .iter()
            .any(|parameter| parameter.pack.is_some())
        {
            self.unsupported(at, "a capture of a call of a function with a pack");
            return Some("None".to_string());
        }
        let number = self.captured;
        self.captured += 1;
        let graph = format!("_capture_{number}");
        // Each argument as `(argument, parameter, value, input)`: the
        // position of its parameter, what it is written as, and whether
        // it is an input of the child, a connection, or its
        // configuration.
        let mut written: Vec<(&Argument, usize, String, bool)> = Vec::new();
        // What a node is given inside `_core.Held(...)`; a lifted node
        // holds its configuration itself (`bound`).
        let kept = if lifted {
            vec![false; signature.parameters.len()]
        } else {
            self.kept_configuration(callee, generics, signature)
        };
        let child = if lifted {
            let (values, bound) = self.bound(function, signature, arguments, |emitter, value| {
                emitter.connects(function, value).then(|| value.ty.clone())
            });
            let result = signature.result.as_ref().map(|_| &call.ty);
            let node = self.lifted(
                callee,
                Lifts::Function(generics),
                signature,
                &bound,
                result,
                at,
            );
            // `bound` writes each argument as `name=value`, in the order
            // of `arguments`, a pack's left out.
            let given = arguments.iter().filter_map(|argument| {
                let index = argument.parameter?;
                signature.parameters[index]
                    .pack
                    .is_none()
                    .then_some((argument, index))
            });
            for ((argument, index), value) in given.zip(values) {
                let value = value
                    .split_once('=')
                    .map_or(value.clone(), |(_, value)| value.to_string());
                let input = matches!(bound[index], Some(Bound::Input(_)));
                written.push((argument, index, value, input));
            }
            node
        } else {
            for argument in arguments {
                let Some(index) = argument.parameter else {
                    continue;
                };
                let value = self.expression(function, &argument.value);
                let value = self.held_value(kept[index], value);
                let input = !signature.parameters[index].constant;
                written.push((argument, index, value, input));
            }
            self.called_name(callee, generics, at)
        };
        // The child's graph is given its inputs by name, and the graph
        // of the capture is given them as `try_except` is.
        let mut inputs = Vec::new();
        let mut given = Vec::new();
        let mut called = Vec::new();
        for (argument, index, value, input) in written {
            let python = name(&signature.parameters[index].name);
            let plain = matches!(
                argument.value.kind,
                ExprKind::Local(_) | ExprKind::Literal(_)
            );
            let value = if !input || !plain {
                let held = format!("{graph}_{python}");
                self.line(depth, &format!("{held} = {value}"));
                held
            } else {
                value
            };
            if input {
                // A type the target does not write is reported there.
                let Some(annotation) = self.annotation(&argument.value.ty, true, at) else {
                    return Some("None".to_string());
                };
                inputs.push(format!("{python}: {annotation}"));
                given.push(format!("{python}={value}"));
                called.push(format!("{python}={python}"));
            } else {
                called.push(format!("{python}={value}"));
            }
        }
        // A lifted node binds what its call leaves out itself.
        if !lifted {
            for (index, parameter) in signature.parameters.iter().enumerate() {
                if arguments
                    .iter()
                    .any(|argument| argument.parameter == Some(index))
                {
                    continue;
                }
                if let Some(value) = self.left_out_of_call(callee, generics, index, parameter) {
                    // A node keeps a default of its configuration as it
                    // keeps one given (`kept_configuration`).
                    let value = self.held_value(kept[index], value);
                    called.push(format!("{}={value}", name(&parameter.name)));
                }
            }
        }
        let result = match &signature.result {
            Some(_) => match self.annotation(&call.ty, true, at) {
                Some(result) => result,
                None => return Some("None".to_string()),
            },
            None => "None".to_string(),
        };
        self.hgraph = true;
        self.line(depth, "@_hg.graph");
        self.line(
            depth,
            &format!("def {graph}({}) -> {result}:", inputs.join(", ")),
        );
        self.line(depth + 1, &format!("return {child}({})", called.join(", ")));
        let given = [graph].into_iter().chain(given).collect::<Vec<_>>();
        Some(self.attempted(&given.join(", "), signature.result.is_none()))
    }

    /// `Quote(bid: 1.0, ask: 2.0)`: a value of the struct `of`, a
    /// module and a declaration of it, with each field by its name.
    ///
    /// TYPE-5: "Its arguments are named, each for a field the struct
    /// has, a parent's included, in any order ... A field with a
    /// default, its own or one a struct below gives it, may be left
    /// out, and then has that default. An optional field with no
    /// default may be left out too, and is then null ... Written
    /// arguments evaluate once in source order; failure prevents later
    /// arguments. Defaults are evaluated only when omitted."
    ///
    /// The arguments are written in the order the source has them,
    /// which is the order Python evaluates them in, and then the
    /// default of each field left out.
    ///
    /// TYPE-3: "Generic applications are invariant by origin and
    /// complete canonical arguments". A value of a generic struct is
    /// made of the class for its arguments, `_core.applied(Box,
    /// "i64")`: a `Box<i64>` is then no `Box<f64>` where the two meet
    /// as values of a parent. `whole` is the construction, whose type
    /// says the arguments. Where they are not known as the module is
    /// written, in a function generic in them, the construction is
    /// reported.
    fn construct(
        &mut self,
        function: &Function,
        of: (usize, usize),
        arguments: &[Argument],
        whole: &Expr,
    ) -> String {
        let at = whole.at;
        let program = self.program;
        let Some(Detail::Struct(read)) = program.modules[of.0].details.get(of.1) else {
            self.unsupported(at, "this construction");
            return "None".to_string();
        };
        let mut written = Vec::new();
        for argument in arguments {
            let field = argument.parameter.and_then(|field| read.members.get(field));
            let Some(field) = field else {
                self.unsupported(argument.value.at, "this argument of a construction");
                continue;
            };
            let value = self.expression(function, &argument.value);
            written.push(format!("{}={value}", name(&field.name)));
        }
        for (index, field) in read.members.iter().enumerate() {
            let given = arguments
                .iter()
                .any(|argument| argument.parameter == Some(index));
            if given {
                continue;
            }
            let value = match field.default {
                Some(default) => self.field_default(default, at),
                // The analyser has held the construction to every field
                // that is not optional (TYPE-5).
                None => "None".to_string(),
            };
            written.push(format!("{}={value}", name(&field.name)));
        }
        // The class for the construction's arguments (`struct_class`).
        // A struct with no generic parameter is its own class.
        let applied = match &whole.ty {
            Type::Named { arguments, .. } if !arguments.is_empty() => self.struct_class(&whole.ty),
            _ if read.generics.is_empty() => Some(self.type_name(of.0, of.1)),
            _ => None,
        };
        let applied = applied.unwrap_or_else(|| {
            self.unsupported(
                at,
                "a generic struct made where its arguments are not known",
            );
            self.type_name(of.0, of.1)
        });
        format!("{applied}({})", written.join(", "))
    }

    /// The default the field `default` writes, where the construction
    /// at `at` leaves the field out. It is read from the analysed
    /// program, as a parameter's is (see `left_out`): a default names
    /// no field and no local (UNIT-7), so it means the same wherever it
    /// is written.
    fn field_default(&mut self, default: FieldRef, at: Location) -> String {
        let program = self.program;
        let value = program.modules[default.module]
            .defaults
            .get(default.declaration)
            .and_then(|defaults| defaults.get(default.field))
            .and_then(Option::as_ref);
        let Some(value) = value else {
            self.unsupported(at, "a default of a field left out here");
            return "None".to_string();
        };
        // A default that constructs its own struct with that same
        // field left out would be written inside itself without end.
        let key = (default.module, default.declaration, default.field);
        if self.expanding.contains(&key) {
            self.unsupported(value.at, "a default that leaves itself out");
            return "None".to_string();
        }
        let cold = Function::cold(Mode::Value);
        self.expanding.push(key);
        let written = self.expression(&cold, value);
        self.expanding.pop();
        written
    }

    fn literal(&mut self, literal: &Value, at: Location) -> String {
        match literal {
            Value::Int(value) => value.to_string(),
            Value::Float(value) => float(*value),
            Value::Bool(true) => "True".to_string(),
            Value::Bool(false) => "False".to_string(),
            Value::Str(text) => string(text),
            Value::Null => "None".to_string(),
            // LEX-13: a duration literal is a count of whole microseconds,
            // which the analyser has made of its parts. OPS-1: its
            // operators are the provider's functions on that count.
            Value::Duration(microseconds) => {
                self.datetime = true;
                format!("_dt.timedelta(microseconds={microseconds})")
            }
            Value::Temporal(parts) => self.temporal(parts, at),
            // LEX-28, TYPE-17: a tenor literal is its counts, which the
            // analyser has made of its parts: the provider's `Tenor`.
            // TEN-1: its operators are the provider's functions.
            Value::Tenor(Tenor::Moves { months, days }) => {
                self.natives = true;
                format!("_core.Tenor({months}, {days})")
            }
            Value::Tenor(Tenor::BusinessDays(count)) => {
                self.natives = true;
                format!("_core.Tenor(business={count})")
            }
        }
    }

    /// A temporal literal, by the parts it writes (LEX-11), which are
    /// what its type goes by. TYPE-2: a `date`, a `time`, a
    /// `civil_datetime` and a `datetime` are Python's `date`, `time` and
    /// `datetime`; a `timezone`, a `zoned_time` and a `zoned_datetime`
    /// are the provider's, made by its functions, which check the zone
    /// (LEX-12: "The selected run provider checks exact catalog
    /// membership and offset consistency").
    fn temporal(&mut self, parts: &Temporal, at: Location) -> String {
        if let Some(zone) = &parts.zone {
            self.natives = true;
            let zone = string(zone);
            return match (parts.date, parts.time, parts.offset) {
                (None, None, _) => format!("_core.timezone({zone})"),
                (None, Some(time), _) => {
                    self.datetime = true;
                    format!("_core.zoned_time(_dt.time({}), {zone})", time_fields(time))
                }
                (Some(date), Some(time), Some(minutes)) => {
                    if !in_calendar(date, time, minutes) {
                        self.unsupported(at, "an instant outside the years 1 to 9999");
                        return "None".to_string();
                    }
                    self.datetime = true;
                    format!(
                        "_core.zoned_datetime(_dt.datetime({}, {}), {minutes}, {zone})",
                        date_fields(date),
                        time_fields(time)
                    )
                }
                // LEX-11: a zone follows an offset or a time, or stands
                // alone, and the lexer reads no other.
                _ => {
                    self.unsupported(at, "a zone after a date with no time or no offset");
                    "None".to_string()
                }
            };
        }
        self.datetime = true;
        match (parts.date, parts.time, parts.offset) {
            (Some(date), None, _) => format!("_dt.date({})", date_fields(date)),
            (None, Some(time), _) => format!("_dt.time({})", time_fields(time)),
            (Some(date), Some(time), offset) => {
                let written = format!("_dt.datetime({}, {})", date_fields(date), time_fields(time));
                match offset {
                    // A local date and time, and an instant written in
                    // UTC: the parts are the value's own.
                    None | Some(0) => written,
                    // TYPE-2: "Instants/durations use signed 64-bit
                    // microseconds", and an instant here is a Python
                    // `datetime`, which holds the years 1 to 9999 only.
                    // One written near either end with an offset may be
                    // outside them in UTC: it is not written.
                    Some(minutes) if !in_calendar(date, time, minutes) => {
                        self.unsupported(at, "an instant outside the years 1 to 9999");
                        "None".to_string()
                    }
                    // An instant written east or west of UTC is that
                    // many minutes from what its parts say. The
                    // provider works it out, with Python's calendar.
                    Some(minutes) => {
                        self.natives = true;
                        format!("_core.instant({written}, {minutes})")
                    }
                }
            }
            // The lexer gives a literal a date, a time or a zone.
            (None, None, _) => {
                self.unsupported(at, "a temporal literal with no date and no time");
                "None".to_string()
            }
        }
    }

    // ---- Lifting ----

    /// The endpoint of `child`, a child of a temporal input of the node
    /// (TIME-1), as hgraph has it: its own child of the input's,
    /// `parts[1]` as `_core.position_of(parts, 1)`, `quote.bid` as
    /// `quote["bid"]`, and a dictionary's entry as `_core.entry_port`,
    /// one that is not valid where the key is absent, named or read
    /// alike. `None` where hgraph has no child for it: an element of an
    /// unsized list, which is a whole tuple here.
    fn child_port(&mut self, function: &Function, child: &Expr) -> Option<String> {
        match &child.kind {
            ExprKind::Local(local) if function.is_input(*local) => {
                Some(name(&function.locals[*local].name))
            }
            ExprKind::Field { base, member } => {
                let field = field_of(self.program, &base.ty, *member)?;
                let base = self.child_port(function, base)?;
                Some(format!("{base}[{}]", string(&name(&field.name))))
            }
            // TYPE-9: a tuple's position is the field of its position
            // (`tuple_annotation`), under a constant index.
            ExprKind::Index { base, index } if matches!(base.ty, Type::Tuple(_)) => {
                let key = self.position_key(function, index);
                let base = self.child_port(function, base)?;
                Some(format!("{base}[{key}]"))
            }
            ExprKind::Index { base, index } => {
                let of = match &base.ty {
                    Type::List { size: Some(_), .. } => "position_of",
                    // An absent key's entry is a child that is not valid
                    // (TIME-1), read or named alike.
                    Type::Dict { .. } => "entry_port",
                    _ => return None,
                };
                let base = self.child_port(function, base)?;
                let index = self.expression(function, index);
                self.natives = true;
                Some(format!("_core.{of}({base}, {index})"))
            }
            _ => None,
        }
    }

    /// True where `value` reads a child of a temporal input of the node,
    /// an element of a fixed list, a field, a tuple's position or a
    /// dictionary's entry, at any depth, each step one hgraph has a child
    /// for (`child_port`): an unsized list's element is no such step.
    fn reads_child(&self, function: &Function, value: &Expr) -> bool {
        fn within(emitter: &Emitter<'_>, function: &Function, value: &Expr) -> bool {
            match &value.kind {
                // The input is a collection (TIME-9): an `atomic<V>`, a `T?`
                // and a `ref<T>` have no child, as the analyser has it.
                ExprKind::Local(local) => {
                    function.is_input(*local)
                        && matches!(
                            function.locals[*local].ty,
                            Type::List { .. }
                                | Type::Tuple(_)
                                | Type::Dict { .. }
                                | Type::Named {
                                    kind: NamedKind::Struct,
                                    ..
                                }
                        )
                }
                // An `atomic<V>`'s field is computed from the whole value
                // (TYPE-9), and is no child.
                ExprKind::Field { base, member } => {
                    matches!(
                        base.ty,
                        Type::Named {
                            kind: NamedKind::Struct,
                            ..
                        }
                    ) && field_of(emitter.program, &base.ty, *member).is_some()
                        && within(emitter, function, base)
                }
                ExprKind::Index { base, .. } => {
                    matches!(
                        base.ty,
                        Type::List { size: Some(_), .. } | Type::Tuple(_) | Type::Dict { .. }
                    ) && within(emitter, function, base)
                }
                _ => false,
            }
        }
        matches!(value.kind, ExprKind::Field { .. } | ExprKind::Index { .. })
            && within(self, function, value)
    }

    /// `operation` of the child `child` names, an endpoint whether or not
    /// it is valid (END-8), written at `at`.
    fn child_operation(
        &mut self,
        function: &Function,
        operation: EndpointOperation,
        child: &Expr,
        at: Location,
    ) -> String {
        // At stop, validity and the last publication time are read, as
        // an input's are (TIME-5).
        let given = match function.mode {
            Mode::Node => true,
            Mode::Stop => matches!(
                operation,
                EndpointOperation::Valid
                    | EndpointOperation::AllValid
                    | EndpointOperation::LastModified
            ),
            _ => false,
        };
        if !given {
            self.unsupported(at, "this endpoint read in `start` or `stop`");
            return "None".to_string();
        }
        let Some(port) = self.child_port(function, child) else {
            self.unsupported(
                at,
                "an endpoint of an unsized list's element, a whole tuple here",
            );
            return "None".to_string();
        };
        self.natives = true;
        match operation {
            EndpointOperation::Valid => format!("{port}.valid"),
            EndpointOperation::AllValid => format!("{port}.all_valid"),
            EndpointOperation::Modified => format!("_core.child_modified({port})"),
            // END-8: a child's delta is read as a direct input's of its
            // type is (`language_delta`).
            EndpointOperation::DeltaValue => {
                self.language_delta(&child.ty, format!("_core.child_delta({port})"), at)
            }
            EndpointOperation::LastModified => format!("_core.child_last_modified({port})"),
            other => {
                self.unsupported(at, &format!("`{}` of a child", other.name()));
                "None".to_string()
            }
        }
    }

    /// The read `read` of a child known only when it runs (TIME-1),
    /// written at `at`: its value, read from its endpoint, or a failure
    /// with `value.unset_read` where it is not valid; where `result`,
    /// `ok` of the value or `err` of that failure's error (ERR-14).
    fn checked_child(
        &mut self,
        function: &Function,
        read: &Expr,
        result: bool,
        at: Location,
    ) -> String {
        self.natives = true;
        // An unsized list is a whole tuple here: its elements are valid
        // with it.
        if let ExprKind::Index { base, index } = &read.kind
            && growing_list(&base.ty)
            && let ExprKind::Local(local) = base.kind
            && function.is_input(local)
        {
            let port = name(&function.locals[local].name);
            let index = self.expression(function, index);
            let of = if result {
                "whole_item_result"
            } else {
                "whole_item"
            };
            return format!("_core.{of}({port}, {index})");
        }
        let Some(port) = self.child_port(function, read) else {
            self.unsupported(at, "this read of a child");
            return "None".to_string();
        };
        let value = self.part_value(&read.ty, "_port", at);
        let of = if result {
            "checked_child_result"
        } else {
            "checked_child"
        };
        format!("_core.{of}({port}, lambda _port: {value})")
    }

    /// The port of a node's temporal input that `value` reads, where it
    /// is the input or a field of a temporal struct port, as hgraph
    /// names it: `quote`, `quote["bid"]`. `None` for anything else, an
    /// `atomic<V>`'s field among them, which is read from its value.
    fn input_port(&self, function: &Function, value: &Expr) -> Option<String> {
        match &value.kind {
            ExprKind::Local(local) if function.is_input(*local) || function.is_out(*local) => {
                Some(endpoint_name(&function.locals[*local]))
            }
            ExprKind::Field { base, member } => {
                // An atomic input is one port of the whole value, read
                // in a hook as that value (TYPE-9), and has no port of
                // a field. The `?` of `atomic<V>?` marks an input that
                // may be left out (TYPE-14): its port is the atomic's.
                if let ExprKind::Local(local) = base.kind
                    && matches!(
                        function.locals[local].ty.omission_removed(),
                        Type::Atomic(_)
                    )
                {
                    return None;
                }
                let field = field_of(self.program, &base.ty, *member)?;
                let base = self.input_port(function, base)?;
                Some(format!("{base}[{}]", string(&name(&field.name))))
            }
            // A tuple's position, under a constant index, is the field
            // of its position (`tuple_annotation`).
            ExprKind::Index { base, index } if matches!(base.ty, Type::Tuple(_)) => {
                let index = constant_position(index)?;
                let base = self.input_port(function, base)?;
                Some(format!("{base}[{}]", string(&position(index))))
            }
            _ => None,
        }
    }

    /// What the port `port`, of a temporal value of `ty`, holds, read in
    /// a hook. A struct's bundle holds a value of the struct, made from
    /// each field's port in turn (TYPE-9), and a tuple's a tuple of each
    /// position's; a list's and a dictionary's children hold its
    /// elements; any other port holds its `value`. The read is at `at`,
    /// where what is not written yet is reported.
    fn port_value(&mut self, ty: &Type, port: &str, at: Location) -> String {
        self.port_value_at(ty, port, 0, at)
    }

    /// `port_value` of a port read inside `depth` comprehensions, whose
    /// names `_child{depth}` and `_key{depth}` are its own.
    fn port_value_at(&mut self, ty: &Type, port: &str, depth: usize, at: Location) -> String {
        match ty {
            // TYPE-12: a window is no ordinary value, and core has no
            // operation that reads what one holds: its values, or how
            // many. Only its endpoint is read (TIME-9). So a value that
            // holds one, a struct's or a list's, is not read whole.
            Type::RollingWindow { .. } => {
                self.unsupported(at, "what a window holds, which no core operation reads,");
                return "None".to_string();
            }
            // TYPE-9: "lists to child positions". An unsized list is a
            // whole tuple (`list_annotation`), and a list of the
            // program is a Python `list`, made of it.
            Type::List { size: None, .. } => return format!("_py.list({port}.value)"),
            // A fixed list's value is each child's, in order: hgraph's
            // own tuple of them where each child publishes whole.
            Type::List { element, .. } if publishes_whole(element) => {
                return format!("_py.list({port}.value)");
            }
            // An element of composites is read as a part is
            // (`part_value_at`): each of its own required parts where it is
            // valid, and an optional one null where it is not.
            Type::List { element, .. } => {
                let child = format!("_child{depth}");
                let read = self.part_value_at(element, &child, depth + 1, at);
                return format!("[{read} for {child} in {port}]");
            }
            // TYPE-9: "dictionaries to keyed children". A dictionary's
            // value has each key the `TSD` has, with its child's value.
            Type::Dict { value, .. } if publishes_whole(value) => {
                return format!("_py.dict({port}.value)");
            }
            Type::Dict { value, .. } => {
                let (key, child) = (format!("_key{depth}"), format!("_child{depth}"));
                let read = self.port_value_at(value, &child, depth + 1, at);
                return format!("{{{key}: {read} for {key}, {child} in {port}.items()}}");
            }
            // A set's value is a `frozenset`, as a `const` one is: hgraph
            // gives a `set`, which its `TSS` takes back as members added,
            // and a whole set published replaces what the set held
            // (TIME-7).
            Type::Set(_) => return format!("_py.frozenset({port}.value)"),
            // TYPE-9: tuples map "to child fields": a tuple of each
            // position's value, from the field of its position
            // (`tuple_annotation`).
            Type::Tuple(elements) => {
                let reads: Vec<String> = elements
                    .iter()
                    .enumerate()
                    .map(|(index, element)| {
                        let child = format!("{port}[{}]", string(&position(index)));
                        self.part_value_at(element, &child, depth, at)
                    })
                    .collect();
                return format!("({},)", reads.join(", "));
            }
            _ => {}
        }
        let (
            Some(fields),
            Type::Named {
                module,
                declaration,
                ..
            },
        ) = (fields_of(self.program, ty), ty)
        else {
            return format!("{port}.value");
        };
        // An abstract struct's value is of a struct below it, which its
        // bundle does not say (`struct_annotation`).
        let class = match self.struct_class(ty) {
            Some(class) if !inherited(self.program, *module, *declaration) => class,
            _ => {
                self.unsupported(at, &format!("a whole value of the temporal `{ty}`"));
                return "None".to_string();
            }
        };
        let fields: Vec<String> = fields
            .iter()
            .map(|(member, field_ty)| {
                let field = name(&member.name);
                let child = format!("{port}[{}]", string(&field));
                let read = self.part_value_at(field_ty, &child, depth, at);
                format!("{field}={read}")
            })
            .collect();
        format!("{class}({})", fields.join(", "))
    }

    /// What a part of a temporal struct or tuple holds, the port `port`
    /// of a field or a position of the type `ty`, read alone, `quote.ask`,
    /// or as its composite is read whole, `let held = quote`.
    ///
    /// A part that has not published has no value to give, and none is
    /// made up. The owner's decision so far, on hhenson/hgl_spec issue
    /// 126, is that a whole read needs each required part valid, and
    /// that a part whose type is optional may not be, and is then null
    /// in the whole value; how the language says so is open there. Until
    /// it says, this target reads a required part that is not valid as a
    /// failure where the read runs, with the catalogued
    /// `value.unset_read` ("Executed operation requires an unset
    /// ordinary payload"), and an optional part that is not valid as
    /// null. A fixed list of required elements that publish whole is held
    /// to each of them (`all_valid`); any other part to its own `valid`,
    /// and a struct's, a tuple's or a list's parts each in turn.
    fn part_value(&mut self, ty: &Type, port: &str, at: Location) -> String {
        self.part_value_at(ty, port, 0, at)
    }

    /// `part_value` of a port read inside `depth` comprehensions.
    fn part_value_at(&mut self, ty: &Type, port: &str, depth: usize, at: Location) -> String {
        let read = self.port_value_at(ty, port, depth, at);
        // A fixed list of required elements that publish whole is read
        // whole from its `TSL`, so it is held to each of them. One of
        // composites reads each element as a part, whose own optional
        // parts may be invalid.
        let ready = match ty {
            Type::List {
                element,
                size: Some(_),
            } if whole_scalar(element) => "all_valid",
            _ => "valid",
        };
        if publishes_whole_null(ty) {
            return format!("({read} if {port}.{ready} else None)");
        }
        self.natives = true;
        format!("({read} if {port}.{ready} else _core.unset_read())")
    }

    /// The name of the field of the tuple position that `index` names
    /// (`tuple_annotation`), as Python text: `"_0"` for a literal, and
    /// for another constant, `pair[1 - 1]` or a constant of a module, the
    /// provider's `_core.position(index)`, of the index as it is
    /// evaluated. EXPR-7: "A tuple's index is a constant within its
    /// length", which the analyser has held it to.
    fn position_key(&mut self, function: &Function, index: &Expr) -> String {
        match constant_position(index) {
            Some(at) => string(&position(at)),
            None => {
                self.natives = true;
                let index = self.expression(function, index);
                format!("_core.position({index})")
            }
        }
    }

    /// `value`, a tuple of `entries` written in a graph where one entry
    /// is a connection: the tuple's own connection, hgraph's bundle
    /// (`tuple_annotation`) made of each entry's, with no node of its
    /// own (TYPE-9: tuples map "to child fields"). An entry that is a
    /// value, known where the graph is built, is a constant connection
    /// of its position's type, which hgraph makes of a value that
    /// publishes whole and is not null. A connection of a struct whose
    /// position is an abstract struct it inherits from is narrowed to
    /// it (`narrowed`).
    fn connected_tuple(&mut self, function: &Function, value: &Expr, entries: &[Expr]) -> String {
        let Some(bundle) = self.annotation(&value.ty, true, value.at) else {
            return "None".to_string();
        };
        let mut given = Vec::new();
        for (index, entry) in entries.iter().enumerate() {
            if !self.connects(function, entry) && !whole_scalar(&entry.ty) {
                self.unsupported(entry.at, "a composite value in a tuple of connections");
                return "None".to_string();
            }
            let written = self.expression(function, entry);
            // A connection of a struct where its position is an abstract
            // struct it inherits from is that struct's bundle (TYPE-3).
            let written = match &value.ty {
                Type::Tuple(positions) if index < positions.len() => {
                    self.narrowed(function, &positions[index], entry, written)
                }
                _ => written,
            };
            given.push(format!("{}={written}", position(index)));
        }
        format!("{bundle}.from_ts({})", given.join(", "))
    }

    /// True when `value`, written in a graph, is a field of a
    /// connection of a struct or of an atomic struct (TYPE-9: tuples
    /// and structs map "to child fields"). hgraph gives the field's
    /// port: a bundle's own child, and for a `TS` of a whole struct a
    /// connection of that field of each value.
    fn connected_field(&self, function: &Function, value: &Expr) -> bool {
        let ExprKind::Field { base, .. } = &value.kind else {
            return false;
        };
        let whole = payload_of(&base.ty);
        matches!(
            whole,
            Type::Named {
                kind: NamedKind::Struct,
                ..
            }
        ) && self.connects(function, base)
    }

    /// True when `value`, written in a graph, is an element of a
    /// connection of a fixed list or of a dictionary (TYPE-9: "lists to
    /// child positions, dictionaries to keyed children"): hgraph gives
    /// the child's port. An unsized list is a whole tuple here, which
    /// has no child.
    fn connected_element(&self, function: &Function, value: &Expr) -> bool {
        let ExprKind::Index { base, .. } = &value.kind else {
            return false;
        };
        matches!(
            base.ty,
            Type::List { size: Some(_), .. } | Type::Dict { .. } | Type::Tuple(_)
        ) && self.connects(function, base)
    }

    /// True when `value`, written in a graph, is a connection: a
    /// temporal parameter or a local given one, what a temporal
    /// function gives, or what a lifted call gives (FN-6).
    fn connects(&self, function: &Function, value: &Expr) -> bool {
        match &value.kind {
            ExprKind::Local(local) => function.locals[*local].category == ValueCategory::Connection,
            ExprKind::Call {
                callee:
                    Callee::Function {
                        module,
                        declaration,
                        ..
                    }
                    | Callee::Candidate {
                        module,
                        declaration,
                        ..
                    },
                arguments,
                ..
            } => {
                let declared = &self.program.modules[*module].declarations[*declaration];
                declared.kind.role() == Some(Role::Temporal)
                    || arguments
                        .iter()
                        .any(|argument| self.connects(function, &argument.value))
            }
            ExprKind::Widen(inner) => self.connects(function, inner),
            // A tuple is a connection where one of its entries is.
            ExprKind::Tuple(entries) => entries.iter().any(|entry| self.connects(function, entry)),
            // ERR-7: a capture is "read through two connections".
            ExprKind::Capture(_) | ExprKind::Captured { .. } => true,
            // ERR-3: "In composition a test of a connection is an
            // ordinary lifted call (FN-6): it gives a connection of
            // `bool`".
            ExprKind::TagTest { value, .. } => self.connects(function, value),
            // A field of a connection is the field's own connection,
            // and so is an element of one.
            ExprKind::Field { base, .. } | ExprKind::Index { base, .. } => {
                self.connects(function, base)
            }
            // CTRL-13: an `if` "is a connection when its condition is
            // one or either branch's value is", and a block is what
            // its value is.
            ExprKind::If {
                condition,
                then,
                otherwise,
            } => {
                self.connects(function, condition)
                    || self.connects(function, then)
                    || otherwise
                        .as_ref()
                        .is_some_and(|otherwise| self.connects(function, otherwise))
            }
            ExprKind::Block {
                value: Some(value), ..
            } => self.connects(function, value),
            _ => false,
        }
    }

    /// TYPE-16: `delta<set<i64>>(added: [1, 2], removed: [3])`, a set's
    /// delta made from its properties, as hgraph's own set delta. A
    /// property left out is empty. And a nullable's, `delta<i64?>()`.
    fn delta(&mut self, function: &Function, value: &Expr, arguments: &[Argument]) -> String {
        // TYPE-16: "A nullable's delta is an optional: it holds what
        // was published, a null among them ... `delta<i64?>()` is a
        // published null", and `delta<i64?>(value: 7)` a published 7.
        // It is hgraph's own delta of such a value: `Null`, or the
        // value.
        let delta = DeltaOf::of(&value.ty);
        if let Some(DeltaOf::Nullable) = delta {
            return match arguments.first() {
                Some(argument) => {
                    let value = self.expression(function, &argument.value);
                    self.published(value)
                }
                None => {
                    self.hgraph = true;
                    "_hg.Null".to_string()
                }
            };
        }
        // TYPE-16: a struct's delta has "a property for each field
        // `name: F`, of the same name and of type `delta<F>?`: null
        // where the field did not change", and "A property left out is
        // ... null for a struct's field". It is hgraph's delta of a
        // bundle: a `dict` of the fields that changed, which the
        // provider makes from the properties given, in the order
        // written, leaving out each that is null.
        if let Some(DeltaOf::Struct(of)) = delta {
            let program = self.program;
            let mut written = Vec::new();
            for argument in arguments {
                let given = self.expression(function, &argument.value);
                if let Some(field) = argument
                    .parameter
                    .and_then(|index| field_of(program, of, index))
                {
                    written.push(format!("{}={given}", name(&field.name)));
                }
            }
            self.natives = true;
            return format!("_core.struct_delta({})", written.join(", "));
        }
        // TYPE-16: a dictionary's delta has "`updated`, a `dict<K,
        // delta<V>>` of each key added or changed with its delta, and
        // `removed`, a `set<K>`". It is hgraph's delta of a `TSD`: one
        // `dict` of each key updated with its delta, and of each
        // removed with hgraph's `REMOVE`, which the provider makes from
        // the properties, given by name in the order written (EXPR-13).
        if let Some(DeltaOf::Dict) = delta {
            const PROPERTIES: [&str; 2] = ["updated", "removed"];
            let mut written = Vec::new();
            for argument in arguments {
                let given = self.expression(function, &argument.value);
                if let Some(property) = argument.parameter.and_then(|index| PROPERTIES.get(index)) {
                    written.push(format!("{property}={given}"));
                }
            }
            self.natives = true;
            return format!("_core.dict_delta({})", written.join(", "));
        }
        // TYPE-16: a list's delta has "`updated`, each changed position
        // with its delta; `removed`, the positions that lost their
        // value ... and `size_modified`". The provider makes it as
        // hgraph's delta of a `TSL`, a `dict` of each position updated,
        // which holds the other two (see `list_delta`). hgraph's bounded
        // `TSL` takes no removed position and no size.
        if let Some(DeltaOf::List { fixed }) = delta {
            const PROPERTIES: [&str; 3] = ["updated", "removed", "size_modified"];
            let mut written = Vec::new();
            for argument in arguments {
                let given = self.expression(function, &argument.value);
                let Some(index) = argument.parameter.filter(|index| *index < PROPERTIES.len())
                else {
                    continue;
                };
                if fixed && index > 0 {
                    self.unsupported(
                        argument.value.at,
                        &format!(
                            "the `{}` of a fixed list's delta: hgraph's bounded `TSL` takes no \
                             removed position and no size",
                            PROPERTIES[index]
                        ),
                    );
                }
                written.push(format!("{}={given}", PROPERTIES[index]));
            }
            self.natives = true;
            return format!("_core.list_delta({})", written.join(", "));
        }
        let Some(DeltaOf::Set(element)) = delta else {
            self.unsupported(
                value.at,
                "a delta that is not a set's, a list's, a nullable's, a dictionary's or a struct's",
            );
            return "None".to_string();
        };
        let Some(element) = self.annotation(element, false, value.at) else {
            return "None".to_string();
        };
        // EXPR-13: the properties are evaluated in the order they are
        // written, as a call's arguments are. Python evaluates keyword
        // arguments as written, so each is given by its name in that
        // order, and one left out is empty.
        const PROPERTIES: [&str; 2] = ["added", "removed"];
        let mut given = [false; 2];
        let mut written = Vec::new();
        for argument in arguments {
            let value = self.expression(function, &argument.value);
            if let Some(index) = argument.parameter.filter(|index| *index < PROPERTIES.len()) {
                given[index] = true;
                written.push(format!("{}={value}", PROPERTIES[index]));
            }
        }
        for (property, given) in PROPERTIES.iter().zip(given) {
            if !given {
                written.push(format!("{property}=_py.frozenset()"));
            }
        }
        self.hgraph = true;
        format!("_hg.set_delta({}, tp={element})", written.join(", "))
    }

    /// A dense trace written out: a list with `None` where it has `_`
    /// (TST-6: "Dense position i denotes the simulation start plus i
    /// minimum engine steps").
    fn trace(&mut self, function: &Function, cells: &[Cell], ty: &Type) -> String {
        // A trace of a nullable: a cell may be a published null. A
        // cell is the delta of what is published (TST-5).
        let nullable = matches!(
            ty,
            Type::Trace { cell, .. }
                if is_nullable(cell) || matches!(&**cell, Type::Delta(of) if is_nullable(of))
        );
        let written: Vec<String> = cells
            .iter()
            .map(|cell| match &cell.value {
                // `_`: nothing is published.
                None => "None".to_string(),
                // A published null is hgraph's `Null`, the tick of a
                // value that is unset: `None` in a trace is silence.
                Some(Expr {
                    kind: ExprKind::Literal(Value::Null),
                    ..
                }) => {
                    self.hgraph = true;
                    "_hg.Null".to_string()
                }
                // A cell that may be null where the test runs: a
                // value of the nullable type, and not a literal of
                // its payload's or the delta it is published as.
                Some(value) if nullable && is_nullable(&value.ty) => {
                    let value = self.expression(function, value);
                    self.published(value)
                }
                // TST-5: a cell "written as a `return` is ... a T,
                // published whole, or a `delta<T>`". A whole value where
                // the delta is a type of its own is marked so: an input
                // publishes it whole, replacing what it held (TST-9), and
                // an expectation's is compared with the value the result
                // holds once its cycle has published.
                // A whole value of a struct below the cell's struct, a
                // `Circle` where the cell is a `Shape`'s (TYPE-3), would be
                // published as the struct below, which the cell's bundle
                // does not take: it is not written yet.
                Some(value) if descendant_cell(ty, &value.ty) => {
                    self.unsupported(value.at, "a whole trace cell of a struct below its type");
                    "None".to_string()
                }
                Some(value) if whole_cell(ty, &value.ty) => {
                    // A whole value of composites, a struct with a struct
                    // field or a dictionary of dictionaries, would replace
                    // each part it holds in turn, which the harness does
                    // not follow yet.
                    if !self.parts_publish_whole(&value.ty) || !self.whole_readable(&value.ty) {
                        self.unsupported(value.at, "a whole trace cell of composites");
                        return "None".to_string();
                    }
                    let fixed = matches!(value.ty, Type::List { size: Some(_), .. });
                    let value = self.expression(function, value);
                    self.harness = true;
                    if fixed {
                        format!("_harness.whole({value}, fixed=True)")
                    } else {
                        format!("_harness.whole({value})")
                    }
                }
                Some(value) => self.expression(function, value),
            })
            .collect();
        // TST-7: "Timed entries use duration offsets from run start or
        // absolute datetime keys ... no `_` occurs in timed traces."
        // The harness is given each entry as its key and its
        // publication, in the order written.
        if cells.iter().any(|cell| cell.time.is_some()) {
            self.harness = true;
            let entries: Vec<String> = cells
                .iter()
                .zip(written)
                .filter_map(|(cell, value)| {
                    let time = self.expression(function, cell.time.as_ref()?);
                    Some(format!("({time}, {value})"))
                })
                .collect();
            return format!("_harness.timed([{}])", entries.join(", "));
        }
        format!("[{}]", written.join(", "))
    }
}

/// The position a constant index names, `pair[1]`: EXPR-3 has a
/// tuple's position "under a constant index". `None` for an index that
/// is no literal integer.
fn constant_position(index: &Expr) -> Option<usize> {
    match index.kind {
        ExprKind::Literal(Value::Int(at)) => usize::try_from(at).ok(),
        _ => None,
    }
}

/// True for the literal `null`.
fn is_null(value: &Expr) -> bool {
    matches!(value.kind, ExprKind::Literal(Value::Null))
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
