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
    ErrorPart, Expr, ExprKind, Hook, Local, Stmt, Temporal, UnaryOperator, Value, ValueCategory,
};
use hgl_analyser::{
    Argument as GenericArgument, Constant, ConstantValue, Declaration, DeclarationKind, Detail,
    FieldRef, LocalKind, Member, NamedKind, Parameter, Phase, Program, Role, Scalar, Signature,
    Type,
};
use hgl_diagnostics::{Category, Code, Diagnostic, Diagnostics, Location};

use crate::instances::{Instances, instanced};
use crate::{codes, provided, python_module, test_function};

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
}

/// One top-level part of a module as it is written: a declaration or a
/// named test.
#[derive(Default)]
pub(crate) struct Piece {
    /// Its Python text.
    text: String,
    /// The first form in it that the target does not write yet. A piece
    /// with one is left out of the file.
    pub(crate) broken: Option<Diagnostic>,
    /// The functions and the types it names, each as its module and
    /// its declaration.
    pub(crate) uses: BTreeSet<(usize, usize)>,
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
        candidates: BTreeSet::new(),
        propagates: false,
        piece: Piece::default(),
        declared: Vec::new(),
        tested: Vec::new(),
    };
    // UNIT-14, RUN-1: what an application declares is written as a
    // module's is. Its run statement is not written yet.
    if let Some(at) = program.modules[index].application_at() {
        emitter.diagnostics.emit(Diagnostic::error(
            Category::Build,
            Code::Local(codes::UNSUPPORTED),
            at,
            "the hgraph target does not write an application's run statement yet".to_string(),
        ));
    }
    emitter.declarations();
    emitter.tests();
    let text = emitter.finish();
    Written {
        text,
        natives: emitter.natives,
        harness: emitter.harness,
        imports: emitter.imports,
        declarations: emitter.declared,
        tests: emitter.tested,
    }
}

/// How a parameter of a lifted function is given (FN-6: "temporal
/// arguments become inputs; scalar arguments and omitted defaults bind
/// as configuration").
#[derive(Clone)]
enum Bound {
    /// By a connection: an input of the lifted node, of this type.
    Input(Type),
    /// By a value known where the graph is built, of this type.
    Configuration(Type),
}

/// What a lifted node calls.
#[derive(Clone, Copy)]
enum Lifts<'a> {
    /// A function of the program, or a candidate with a body.
    Function,
    /// A native candidate of core: the provider's function for it, and
    /// what the call configures it with, each as `name=value`.
    Native(&'a str, &'a [String]),
}

/// Names as a Python tuple of strings: `("a", "b")`.
fn tuple(names: &[String]) -> String {
    let quoted: Vec<String> = names.iter().map(|name| format!("\"{name}\"")).collect();
    // A tuple of one needs its comma.
    format!(
        "({}{})",
        quoted.join(", "),
        if quoted.len() == 1 { "," } else { "" }
    )
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
    /// the node's configuration, its state and its inputs, and not its
    /// output.
    Stop,
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
            Mode::Value | Mode::Node | Mode::Lifecycle | Mode::Stop
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
}

/// The words a generated module leaves to Python: its keywords. A
/// built-in name such as `float` is no keyword, and a program may use
/// it: the target reaches Python's built-ins through a name of its
/// own, `_py`, and never by their bare names.
const PYTHON_WORDS: [&str; 37] = [
    "False", "None", "True", "and", "as", "assert", "async", "await", "break", "class", "continue",
    "def", "del", "elif", "else", "except", "finally", "for", "from", "global", "if", "import",
    "in", "is", "lambda", "nonlocal", "not", "or", "pass", "raise", "return", "try", "while",
    "with", "yield", "match", "case",
];

/// An HGL name as a Python one. Most are written as they are. The
/// mapping gives no two HGL names one Python name, and gives none a
/// name the target uses itself:
///
/// - A Python word, and one followed by any number of `_`, gets one
///   more `_`: `lambda` is `lambda_`, and `lambda_` is `lambda__`.
/// - Every name the target makes up starts with `_`, and a test is a
///   function `test_name`. So a name that starts with `_` or `test_`,
///   under any number of `hgl_` before it, gets one more `hgl_`:
///   `_kept` is `hgl__kept`, and `hgl__kept` is `hgl_hgl__kept`.
pub(crate) fn name(written: &str) -> String {
    if PYTHON_WORDS.contains(&written.trim_end_matches('_')) {
        return format!("{written}_");
    }
    let mut rest = written;
    while let Some(inner) = rest.strip_prefix("hgl_") {
        rest = inner;
    }
    if rest.starts_with('_') || rest.starts_with("test_") {
        return format!("hgl_{written}");
    }
    written.to_string()
}

/// The name of an enum's member as Python's `Enum` has it. A member is
/// read by name, `Side["buy"]`, so any word is one, a Python keyword
/// among them. `Enum` keeps `mro` and the names that start and end with
/// `_` for itself: a name that starts with `_` or is `mro`, under any
/// number of `hgl_`, gets one more `hgl_`. No two names give one, as
/// for [`name`]. The provider's `member` does the same for a member
/// constructed from its name.
fn member_name(written: &str) -> String {
    let mut rest = written;
    while let Some(inner) = rest.strip_prefix("hgl_") {
        rest = inner;
    }
    if rest.starts_with('_') || rest == "mro" {
        return format!("hgl_{written}");
    }
    written.to_string()
}

/// The name a generated module imports the module `path` under:
/// `_hgl_4demo_4util` for `demo.util`. Each part of the path has its
/// length before it, so that two paths never give one name, whatever
/// `_` they hold: `a_.b` is `_hgl_2a__1b`, and `a._b` is `_hgl_1a_2_b`.
fn alias(path: &str) -> String {
    let parts: Vec<String> = path
        .split('.')
        .map(|part| format!("{}{part}", part.len()))
        .collect();
    format!("_hgl_{}", parts.join("_"))
}

/// A string as a Python literal.
fn string(text: &str) -> String {
    let mut out = String::from("\"");
    for character in text.chars() {
        match character {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            // Any other control character, by its number.
            control if control.is_control() => {
                out.push_str(&format!("\\u{:04x}", control as u32));
            }
            other => out.push(other),
        }
    }
    out.push('"');
    out
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

/// The Python type of a scalar this target writes, `int` for `i64`,
/// reached through `_py`, the target's name for Python's built-ins.
fn scalar(ty: &Type) -> Option<&'static str> {
    match ty {
        Type::Scalar(Scalar::Bool) => Some("_py.bool"),
        Type::Scalar(Scalar::I64) => Some("_py.int"),
        Type::Scalar(Scalar::F64) => Some("_py.float"),
        Type::Scalar(Scalar::Str) => Some("_py.str"),
        // TYPE-2: a `duration` is elapsed time in whole microseconds,
        // which is what a `timedelta` holds.
        Type::Scalar(Scalar::Duration) => Some("_dt.timedelta"),
        // TYPE-2: a `datetime` is an instant in UTC. hgraph's instants
        // are `datetime`s in UTC that name no zone, so one is written as
        // that. A `civil_datetime` is a local date and time, which is a
        // `datetime` that names no zone too: HGL's types keep the two
        // apart, and nothing in Python has to.
        Type::Scalar(Scalar::Datetime | Scalar::CivilDatetime) => Some("_dt.datetime"),
        Type::Scalar(Scalar::Date) => Some("_dt.date"),
        Type::Scalar(Scalar::Time) => Some("_dt.time"),
        _ => None,
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

/// A type as part of a native function's name: `i64` as it is, and any
/// other character than a letter or a digit as `_`.
fn mangled(ty: &Type) -> String {
    ty.to_string()
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character
            } else {
                '_'
            }
        })
        .collect()
}

/// One `when` of a node: what its header asks, and its body.
struct Handler<'a> {
    /// The inputs whose tick wakes it (TIME-3), as local positions.
    subscribed: Vec<usize>,
    /// The inputs it requires to be valid.
    required: Vec<usize>,
    /// The rest of its condition: the conjuncts that are no selector.
    conditions: Vec<&'a Expr>,
    /// Its statements.
    body: &'a [Stmt],
}

impl<'a> Handler<'a> {
    /// Reads a handler's header. `inputs` is the node's temporal
    /// inputs, which a selector with no argument, and one left out,
    /// stand for.
    fn of(condition: Option<&'a Expr>, body: &'a [Stmt], inputs: &[usize]) -> Handler<'a> {
        // "when it stands alone in the header or appears as a
        // top-level conjunct": the conjuncts, through each `&&`.
        let mut conjuncts = Vec::new();
        let mut to_do: Vec<&'a Expr> = condition.into_iter().collect();
        while let Some(part) = to_do.pop() {
            match &part.kind {
                ExprKind::Binary {
                    operator: BinaryOperator::And,
                    left,
                    right,
                } => {
                    // Popped last first: the left is read first.
                    to_do.push(right);
                    to_do.push(left);
                }
                _ => conjuncts.push(part),
            }
        }
        // Every selector among the conjuncts adds what it names, as
        // the analyser has it: `valid(a) && valid(b)` requires both.
        // One with no argument names every input, and `valid([])`
        // names none. With no selector of a kind, its default stands.
        let mut subscribed: Option<Vec<usize>> = None;
        let mut required: Option<Vec<usize>> = None;
        let mut conditions = Vec::new();
        let add = |set: &mut Option<Vec<usize>>, named: &Option<Vec<usize>>| {
            let set = set.get_or_insert_with(Vec::new);
            for &local in named.as_deref().unwrap_or(inputs) {
                if !set.contains(&local) {
                    set.push(local);
                }
            }
        };
        for conjunct in conjuncts {
            match &conjunct.kind {
                ExprKind::Endpoint {
                    operation: EndpointOperation::Modified,
                    inputs: named,
                } => add(&mut subscribed, named),
                ExprKind::Endpoint {
                    operation: EndpointOperation::Valid,
                    inputs: named,
                } => add(&mut required, named),
                _ => conditions.push(conjunct),
            }
        }
        Handler {
            subscribed: subscribed.unwrap_or_else(|| inputs.to_vec()),
            required: required.unwrap_or_else(|| inputs.to_vec()),
            conditions,
            body,
        }
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
    /// The Python names of the candidates the module has written.
    candidates: BTreeSet<String>,
    /// True once the value function being written has a `?`: a
    /// failure then leaves the function through an exception of the
    /// provider's, which the function catches and returns (ERR-5).
    propagates: bool,
    /// What is known of the piece being written, but for its text.
    piece: Piece,
    /// The pieces written for the module's declarations.
    declared: Vec<Piece>,
    /// The pieces written for the module's named tests.
    tested: Vec<Piece>,
}

impl Emitter<'_> {
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
        self.diagnostics.emit(diagnostic);
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
        let path = &self.program.modules[self.module].path;
        let mut out = format!(
            "# Generated by the hgl compiler for hgraph, from module `{path}`. Do not edit.\n"
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
        for module in &self.imports {
            let python = python_module(module);
            out.push_str(&format!("import {python} as {}\n", alias(module)));
        }
        // The lines written so far. A piece's statements were marked
        // by the line of the piece they start on, counted from 0, and
        // are now given the line of the file, counted from 1.
        let mut lines = out.matches('\n').count();
        // The types first, then the functions, then the tests, each in
        // the order declared.
        let (types, functions): (Vec<&mut Piece>, Vec<&mut Piece>) =
            self.declared.iter_mut().partition(|piece| piece.first);
        for piece in types
            .into_iter()
            .chain(functions)
            .chain(self.tested.iter_mut())
        {
            if piece.broken.is_some() {
                // It is in no file, so its statements are on no line.
                piece.statements.clear();
                continue;
            }
            for (line, _) in &mut piece.statements {
                *line += lines + 1;
            }
            lines += piece.text.matches('\n').count();
            out.push_str(&piece.text);
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
        // with none.
        if instanced(program, self.module, index) {
            let candidate = declaration.kind.candidate().is_some();
            if candidate
                && !self
                    .candidates
                    .insert(self.definition_name(self.module, index))
            {
                self.unsupported(
                    declaration.at,
                    "a candidate with the parameter types of another of its module",
                );
                return;
            }
            let instances = self.instances;
            for instance in instances.declared(self.module, index) {
                let python = self.instance_name(self.module, index, &instance.suffix);
                self.node_or_graph(index, &python, &instance.signature, &instance.body);
                // The instances are one piece, which is left out whole
                // once one of them has a form that is not written: the
                // first such form is what is reported.
                if self.piece.broken.is_some() {
                    break;
                }
            }
            return;
        }
        match (declaration.kind, signature, body) {
            (DeclarationKind::Function(Role::Value), Some(signature), Some(body)) => {
                self.value_function(index, signature, body);
            }
            // GEN-4: a value candidate of an operator with a body of
            // its own is a function as any other, under a name of the
            // target's: an operator may have many.
            (DeclarationKind::Implementation(Role::Value, _), Some(signature), Some(body)) => {
                // Two candidates that differ only in what they require
                // would be one Python function.
                if self
                    .candidates
                    .insert(self.definition_name(self.module, index))
                {
                    self.value_function(index, signature, body);
                } else {
                    self.unsupported(
                        declaration.at,
                        "a candidate with the parameter types of another of its module",
                    );
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
                if self.candidates.insert(python.clone()) {
                    self.node_or_graph(index, &python, signature, body);
                } else {
                    self.unsupported(
                        declaration.at,
                        "a candidate with the parameter types of another of its module",
                    );
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

    /// A temporal function or a temporal candidate, the declaration
    /// `index` of the module, as the Python function `python`: a node
    /// where its body is a runtime one, and a graph where it is
    /// composition (FN-3). `signature` and `body` are the declaration's
    /// own, or an instance's.
    fn node_or_graph(&mut self, index: usize, python: &str, signature: &Signature, body: &Body) {
        let program = self.program;
        let module = &program.modules[self.module];
        match module.phases.get(index).copied().flatten() {
            Some(Phase::Node) => self.node(index, python, signature, body),
            Some(Phase::Graph) => self.graph(index, python, signature, body),
            None => self.unsupported(module.declarations[index].at, "this function"),
        }
    }

    /// The Python name of an instance of `declaration` of `module`:
    /// `_inst_`, the name the declaration would have, and the
    /// instance's generic arguments. The `_` first makes the name the
    /// target's own.
    fn instance_name(&self, module: usize, declaration: usize, suffix: &str) -> String {
        format!(
            "_inst_{}__{suffix}",
            self.definition_name(module, declaration)
        )
    }

    /// The Python name of what a call of `declaration` of `module`
    /// calls: the function, or for a declaration written by instance,
    /// its instance for the call's generic arguments `generics`.
    fn called_name(
        &mut self,
        (module, declaration): (usize, usize),
        generics: &[GenericArgument],
        at: Location,
    ) -> String {
        let program = self.program;
        if !instanced(program, module, declaration) {
            return self.function_name(module, declaration);
        }
        let instances = self.instances;
        let Some(instance) = instances.find(program, (module, declaration), generics) else {
            self.unsupported(
                at,
                "a call of a generic temporal function with generic arguments that are not known",
            );
            return "None".to_string();
        };
        let python = self.instance_name(module, declaration, &instance.suffix);
        self.piece.uses.insert((module, declaration));
        if module == self.module {
            return python;
        }
        let path = program.modules[module].path.clone();
        let named = format!("{}.{python}", alias(&path));
        self.imports.insert(path);
        named
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
        let function = Function {
            locals: &[],
            signature: None,
            mode: Mode::Value,
        };
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
        self.natives = true;
        self.piece.first = true;
        self.line(0, "");
        self.line(0, "");
        self.line(
            0,
            &format!(
                "{} = _core.struct({}, {})",
                name(&declaration.name),
                string(&declaration.name),
                tuple(&fields)
            ),
        );
    }

    /// The Python name of the enum or the struct `declaration` of
    /// `module`. One of another module is named through that module, as
    /// a function is.
    fn type_name(&mut self, module: usize, declaration: usize) -> String {
        let declared = &self.program.modules[module];
        let python = name(&declared.declarations[declaration].name);
        self.piece.uses.insert((module, declaration));
        if module == self.module {
            return python;
        }
        self.imports.insert(declared.path.clone());
        format!("{}.{python}", alias(&declared.path))
    }

    /// `const fn name(a, b) -> T`, the declaration `index` of the
    /// module: a plain Python function.
    fn value_function(&mut self, index: usize, signature: &Signature, body: &Body) {
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
        self.line(
            0,
            &format!(
                "def {}({}):",
                self.definition_name(self.module, index),
                parameters.join(", ")
            ),
        );
        let function = Function {
            locals: &body.locals,
            signature: Some(signature),
            mode: Mode::Value,
        };
        let start = self.text.len();
        self.propagates = false;
        self.block(&function, &body.statements, 1);
        if std::mem::take(&mut self.propagates) {
            // ERR-5: "In a value function returning `result<U,E>`,
            // failure returns the same E as Err." A `?` is inside an
            // expression, where Python has no `return`: the provider
            // raises, and the function returns what was raised.
            let body: String = self.text[start..]
                .lines()
                .map(|line| format!("    {line}\n"))
                .collect();
            self.text.truncate(start);
            self.line(1, "try:");
            self.text.push_str(&body);
            self.line(1, "except _core.Propagated as _left:");
            self.line(2, "return _left.failure");
        }
    }

    /// The annotation of a parameter or a result of a temporal
    /// function: `TS[int]` for a temporal `i64`, `int` for a `const`
    /// one. `None`, reported, for a type not written yet.
    fn annotation(&mut self, ty: &Type, temporal: bool, at: Location) -> Option<String> {
        // TYPE-9: temporalization maps "sets to membership". A
        // temporal `set<T>` is hgraph's `TSS[T]`, whose tick
        // is the set's delta, and a `const` one a `frozenset`.
        if let Type::Set(element) = ty {
            let element = self.annotation(element, false, at)?;
            return Some(if temporal {
                self.hgraph = true;
                format!("_hg.TSS[{element}]")
            } else {
                format!("_py.frozenset[{element}]")
            });
        }
        let python = match (scalar(ty), ty) {
            (Some(python), _) => {
                self.datetime |= python.starts_with("_dt.");
                python.to_string()
            }
            // An enum is a scalar to hgraph, as its Python `Enum` is.
            (
                None,
                Type::Named {
                    module,
                    declaration,
                    kind: NamedKind::Enum,
                    ..
                },
            ) => self.type_name(*module, *declaration),
            _ => {
                self.unsupported(at, &format!("the type `{ty}`"));
                return None;
            }
        };
        if temporal {
            self.hgraph = true;
            Some(format!("_hg.TS[{python}]"))
        } else {
            Some(python)
        }
    }

    /// The parameters of a temporal function as Python writes them, and
    /// its result's annotation.
    fn temporal_signature(
        &mut self,
        declaration: &Declaration,
        signature: &Signature,
    ) -> (Vec<String>, Option<String>) {
        let mut parameters = Vec::new();
        for parameter in &signature.parameters {
            // A `const` parameter's default is written where a call
            // leaves it out. A temporal input's is "wired as a constant
            // of its default" (UNIT-8), which is not written yet.
            if parameter.pack.is_some() {
                self.unsupported(parameter.at, "a pack of a temporal function");
            } else if parameter.has_default && !parameter.constant {
                self.unsupported(parameter.at, "a default of a temporal input");
            }
            let annotation = self
                .annotation(&parameter.ty, !parameter.constant, parameter.at)
                .unwrap_or_else(|| "_py.object".to_string());
            parameters.push(format!("{}: {annotation}", name(&parameter.name)));
        }
        let result = signature
            .result
            .as_ref()
            .and_then(|result| self.annotation(result, true, declaration.at));
        (parameters, result)
    }

    /// A node: `@compute_node`, or `@sink_node` where it has no result.
    ///
    /// TIME-3: "In a `when` header, `modified()` subscribes to all
    /// temporal inputs; `modified(a, b)` subscribes to both named inputs
    /// ... `valid()` requires all temporal inputs to be valid ...
    /// Omitting either selector uses the same default as its
    /// zero-argument form ... A selector sets subscription or admission
    /// when it stands alone in the header or appears as a top-level
    /// conjunct." TIME-4: "Aggregate subscriptions cover every handler.
    /// Node-wide validity includes only requirements common to
    /// executable handlers; other guards stay per-handler."
    ///
    /// So the node's `active=` is every input a handler subscribes to,
    /// its `valid=` the inputs every handler requires, and each handler
    /// is guarded by what it asks beyond those: that one of its own
    /// inputs ticked, that the inputs only it requires are valid, and
    /// the rest of its condition. "Conditions are checked in source
    /// order ... If that body completes without returning, checking
    /// continues with the next condition."
    ///
    /// TIME-6: "State is persistent, recordable node history." A
    /// `state` is a field of the node's `STATE`, given its value in the
    /// node's `start`. TIME-7: `inject out` is the node's own output,
    /// hgraph's `_output`.
    fn node(&mut self, index: usize, python: &str, signature: &Signature, body: &Body) {
        let program = self.program;
        let declaration = &program.modules[self.module].declarations[index];
        let function = Function {
            locals: &body.locals,
            signature: Some(signature),
            mode: Mode::Node,
        };
        let (mut parameters, result) = self.temporal_signature(declaration, signature);
        // The `const` parameters, as written for the node: a `start`
        // and a `stop` are functions of their own to hgraph, and are
        // given the configuration they may read by naming it.
        let mut lifecycle: Vec<String> = signature
            .parameters
            .iter()
            .zip(&parameters)
            .filter(|(parameter, _)| parameter.constant)
            .map(|(_, written)| written.clone())
            .collect();
        // The temporal inputs, as positions among the locals.
        let inputs: Vec<usize> = (0..body.locals.len())
            .filter(|&local| Self::is_input(&function, local))
            .collect();
        // What the body is made of: the declarations, and each hook.
        let mut states: Vec<(usize, &Option<Expr>)> = Vec::new();
        let mut start: Option<&[Stmt]> = None;
        let mut stop: Option<&[Stmt]> = None;
        let mut handlers: Vec<Handler> = Vec::new();
        let mut has_out = false;
        for statement in &body.statements {
            match statement {
                Stmt::Declare { local, value } if body.locals[*local].kind == LocalKind::State => {
                    states.push((*local, value));
                }
                Stmt::Hook {
                    hook: Hook::Start,
                    body,
                    ..
                } => start = Some(body),
                Stmt::Hook {
                    hook: Hook::Stop,
                    body,
                    ..
                } => stop = Some(body),
                Stmt::Hook {
                    hook: Hook::When,
                    condition,
                    body,
                    ..
                } => handlers.push(Handler::of(condition.as_ref(), body, &inputs)),
                Stmt::Declare { local, .. } => {
                    self.unsupported(body.locals[*local].at, "a `cache`");
                }
                Stmt::Inject { local, at } => {
                    if body.locals[*local].name == "out" {
                        has_out = true;
                    } else {
                        self.unsupported(*at, "an `inject` of a capability");
                    }
                }
                other => self.unsupported(at_of(other, &body.locals), "this statement of a node"),
            }
        }
        if has_out {
            // hgraph names the output's type after its input's:
            // `TS_OUT[int]` for `TS[int]`, `TSS_OUT[int]` for a set.
            let annotation = match &result {
                Some(result) if result.starts_with("_hg.TSS[") => {
                    result.replacen("TSS[", "TSS_OUT[", 1)
                }
                Some(result) => result.replacen("TS[", "TS_OUT[", 1),
                None => "_py.object".to_string(),
            };
            self.hgraph = true;
            parameters.push(format!("_output: {annotation} = None"));
        }
        let has_state = !states.is_empty();
        if has_state {
            parameters.push("_state: _hg.STATE = None".to_string());
            lifecycle.push("_state: _hg.STATE = None".to_string());
        }
        // TIME-5: "`stop` reads a temporal input under manual
        // admission". hgraph gives a `stop` each input it names, as the
        // node has it: the configuration, then the inputs, then the
        // state.
        let mut stopping: Vec<String> = signature
            .parameters
            .iter()
            .zip(&parameters)
            .filter(|(parameter, _)| parameter.constant)
            .map(|(_, written)| written.clone())
            .collect();
        stopping.extend(
            signature
                .parameters
                .iter()
                .zip(&parameters)
                .filter(|(parameter, _)| !parameter.constant)
                .map(|(_, written)| written.clone()),
        );
        if has_state {
            stopping.push("_state: _hg.STATE = None".to_string());
        }
        let stopping = stopping.join(", ");
        let lifecycle = lifecycle.join(", ");
        // TIME-4: what the handlers ask together.
        let active: Vec<usize> = inputs
            .iter()
            .copied()
            .filter(|input| {
                handlers
                    .iter()
                    .any(|handler| handler.subscribed.contains(input))
            })
            .collect();
        let valid: Vec<usize> = inputs
            .iter()
            .copied()
            .filter(|input| {
                handlers
                    .iter()
                    .all(|handler| handler.required.contains(input))
            })
            .collect();
        if !handlers.is_empty() && active.is_empty() {
            // TIME-3: "`modified([])` subscribes to no input: the node's
            // scheduler or alarm is then what wakes the handler."
            self.unsupported(declaration.at, "a node that no input wakes");
        }
        let names = |locals: &[usize]| -> String {
            let named: Vec<String> = locals
                .iter()
                .map(|&local| name(&body.locals[local].name))
                .collect();
            tuple(&named)
        };
        let mut options = Vec::new();
        if active.len() != inputs.len() && !handlers.is_empty() {
            options.push(format!("active={}", names(&active)));
        }
        if valid.len() != inputs.len() && !handlers.is_empty() {
            options.push(format!("valid={}", names(&valid)));
        }
        let decorator = if result.is_some() {
            "compute_node"
        } else {
            "sink_node"
        };
        self.hgraph = true;
        self.line(0, "");
        self.line(0, "");
        if options.is_empty() {
            self.line(0, &format!("@_hg.{decorator}"));
        } else {
            self.line(0, &format!("@_hg.{decorator}({})", options.join(", ")));
        }
        let returns = result.map_or(String::new(), |result| format!(" -> {result}"));
        self.line(
            0,
            &format!("def {python}({}){returns}:", parameters.join(", ")),
        );
        let before = self.text.len();
        // An endpoint as the node's code names it: an input by its
        // name, and the node's own output as hgraph's `_output`.
        let endpoint = |local: usize| {
            let declared = &body.locals[local];
            if declared.kind == LocalKind::Inject {
                "_output".to_string()
            } else {
                name(&declared.name)
            }
        };
        for handler in &handlers {
            // What this handler asks beyond what the node does. It is
            // woken by fewer inputs than the node where one the node is
            // woken by is none of its own.
            let mut guards: Vec<String> = Vec::new();
            if handler.subscribed.is_empty() && !active.is_empty() {
                self.unsupported(declaration.at, "a handler that no input wakes");
                continue;
            }
            let narrower = active
                .iter()
                .any(|input| !handler.subscribed.contains(input));
            if narrower {
                let ticked: Vec<String> = handler
                    .subscribed
                    .iter()
                    .map(|&local| format!("{}.modified", endpoint(local)))
                    .collect();
                guards.push(match ticked.as_slice() {
                    [one] => one.clone(),
                    several => format!("({})", several.join(" or ")),
                });
            }
            for &local in &handler.required {
                if !valid.contains(&local) {
                    guards.push(format!("{}.valid", endpoint(local)));
                }
            }
            for condition in &handler.conditions {
                guards.push(self.expression(&function, condition));
            }
            if guards.is_empty() {
                self.statements(&function, handler.body, 1);
            } else {
                self.line(1, &format!("if {}:", guards.join(" and ")));
                self.block(&function, handler.body, 2);
            }
        }
        if self.text.len() == before {
            self.line(1, "pass");
        }
        // `start` and `stop` are read as what they are to hgraph.
        let lifecycle_hook = Function {
            locals: &body.locals,
            signature: Some(signature),
            mode: Mode::Lifecycle,
        };
        let stop_hook = Function {
            locals: &body.locals,
            signature: Some(signature),
            mode: Mode::Stop,
        };
        // `start`: the state's first values, then the hook's own
        // statements.
        if has_state || start.is_some() {
            self.line(0, "");
            self.line(0, "");
            self.line(0, &format!("@{python}.start"));
            self.line(0, &format!("def _start_{python}({lifecycle}):"));
            let before = self.text.len();
            for (local, value) in &states {
                let field = name(&body.locals[*local].name);
                let value = match value {
                    Some(value) => self.expression(&lifecycle_hook, value),
                    None => "None".to_string(),
                };
                self.line(1, &format!("_state.{field} = {value}"));
            }
            if let Some(start) = start {
                self.statements(&lifecycle_hook, start, 1);
            }
            // A `start` with nothing in it is still a function.
            if self.text.len() == before {
                self.line(1, "pass");
            }
        }
        if let Some(stop) = stop {
            self.line(0, "");
            self.line(0, "");
            self.line(0, &format!("@{python}.stop"));
            self.line(0, &format!("def _stop_{python}({stopping}):"));
            self.block(&stop_hook, stop, 1);
        }
    }

    /// A graph: `@graph`, whose body wires the nodes it calls.
    fn graph(&mut self, index: usize, python: &str, signature: &Signature, body: &Body) {
        let program = self.program;
        let declaration = &program.modules[self.module].declarations[index];
        let function = Function {
            locals: &body.locals,
            signature: Some(signature),
            mode: Mode::Graph,
        };
        let (parameters, result) = self.temporal_signature(declaration, signature);
        self.hgraph = true;
        self.line(0, "");
        self.line(0, "");
        self.line(0, "@_hg.graph");
        let returns = result.map_or(String::new(), |result| format!(" -> {result}"));
        self.line(
            0,
            &format!("def {python}({}){returns}:", parameters.join(", ")),
        );
        self.block(&function, &body.statements, 1);
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
                    let function = Function {
                        locals: &body.locals,
                        signature: None,
                        mode: Mode::Test,
                    };
                    self.block(&function, &body.statements, 1);
                }
                // The analyser gave the test no body to write.
                None => self.unsupported(test.at, "this test"),
            }
            let piece = self.close();
            self.tested.push(piece);
        }
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
        for statement in statements {
            self.statement(function, statement, depth);
        }
        self.text[before..].lines().count()
    }

    /// What a local is written as where it is assigned or read as it
    /// is: a field of the node's state for a `state`, its name
    /// otherwise.
    fn place(&mut self, function: &Function, local: usize) -> String {
        let declared = &function.locals[local];
        match declared.kind {
            LocalKind::State => format!("_state.{}", name(&declared.name)),
            // TIME-7: "`inject out` exposes that same result for reads,
            // assignment".
            LocalKind::Inject if declared.name == "out" => {
                // hgraph gives a lifecycle hook no output: TIME-7's read
                // of `out` in `stop` has nothing to be written as.
                if matches!(function.mode, Mode::Lifecycle | Mode::Stop) {
                    self.unsupported(declared.at, "`out` in `stop`");
                }
                "_output.value".to_string()
            }
            LocalKind::Cache | LocalKind::Inject => {
                self.unsupported(declared.at, "a `cache` or an injected capability");
                "None".to_string()
            }
            _ => name(&declared.name),
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
        // A graph describes connections: Python's own `if` would test
        // a port where the graph is built, and not what flows through
        // it (CTRL-14). Only what wires is written there.
        if function.mode == Mode::Graph
            && matches!(
                statement,
                Stmt::Expr(Expr {
                    kind: ExprKind::If { .. },
                    ..
                })
            )
        {
            self.unsupported(at_of(statement, function.locals), "an `if` in a graph");
            return;
        }
        match statement {
            Stmt::Declare { local, value } => {
                if let Some(value) = value {
                    let value = self.expression(function, value);
                    let target = self.place(function, *local);
                    self.line(depth, &format!("{target} = {value}"));
                } else if matches!(function.locals[*local].ty, Type::Nullable(_)) {
                    // CTRL-11: a `var` whose type is written `T?` "is
                    // null instead, and is assigned from its
                    // declaration".
                    let target = self.place(function, *local);
                    self.line(depth, &format!("{target} = None"));
                }
                // Any other `var` declared with no value has none to
                // write: the analyser has held every read of it to an
                // assignment.
            }
            // A compound assignment's value is already the symbol with
            // the target as its left operand.
            Stmt::Assign { local, value, .. } => {
                let value = self.expression(function, value);
                let target = self.place(function, *local);
                self.line(depth, &format!("{target} = {value}"));
            }
            // EXPR-3: "An assignment may name a place inside a value, at
            // any depth". A compound assignment's value is already the
            // symbol with the place as its left operand.
            Stmt::AssignPlace { place, value, .. } => {
                let value = self.expression(function, value);
                if let Some((target, value)) = self.assigned(function, place, value) {
                    self.line(depth, &format!("{target} = {value}"));
                }
            }
            // FN-7: a `return` with a value publishes it and ends the
            // evaluation; with none, and where a body ends, nothing is
            // published. hgraph has a node that returns `None` not tick.
            Stmt::Return { value: None, .. } => self.line(depth, "return"),
            Stmt::Return {
                value: Some(value), ..
            } => {
                let value = self.expression(function, value);
                self.line(depth, &format!("return {value}"));
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
            Stmt::Expr(value) => {
                let value = self.expression(function, value);
                self.line(depth, &value);
            }
            // CTRL-12: "`while condition { ... }` repeats its body while
            // the condition holds". The form with no condition is a
            // source's.
            Stmt::While {
                condition: Some(condition),
                body,
                ..
            } if function.mode.runs() => {
                let condition = self.expression(function, condition);
                self.line(depth, &format!("while {condition}:"));
                self.block(function, body, depth + 1);
            }
            Stmt::For {
                names,
                sequence,
                body,
                at,
            } if function.mode.runs() => {
                self.for_loop(function, (names, sequence), body, *at, depth);
            }
            Stmt::Switch {
                selector,
                cases,
                default,
                ..
            } if function.mode.runs() => {
                self.switch(function, selector, cases, default.as_deref(), depth);
            }
            other => self.unsupported(at_of(other, function.locals), "this statement"),
        }
    }

    /// CTRL-12: "`for item in items` runs its body for each element of
    /// a list, in index order, or of a set, and `for key, value in
    /// entries` for each key and value of a dictionary."
    ///
    /// Python's `for` reads what it loops over once, where the loop
    /// begins: a `var` the body assigns is not what is looped over.
    fn for_loop(
        &mut self,
        function: &Function,
        (names, sequence): (&[usize], &Expr),
        body: &[Stmt],
        at: Location,
        depth: usize,
    ) {
        let over = self.expression(function, sequence);
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
    fn switch(
        &mut self,
        function: &Function,
        selector: &Expr,
        cases: &[Case],
        default: Option<&[Stmt]>,
        depth: usize,
    ) {
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
    fn if_statement(
        &mut self,
        function: &Function,
        condition: &Expr,
        then: &Expr,
        otherwise: Option<&Expr>,
        depth: usize,
    ) {
        let condition = self.expression(function, condition);
        self.line(depth, &format!("if {condition}:"));
        self.branch(function, then, depth + 1);
        if let Some(otherwise) = otherwise {
            self.line(depth, "else:");
            self.branch(function, otherwise, depth + 1);
        }
    }

    /// A branch of an `if` that stands as a statement: the statements
    /// of its block, and its last expression as one more.
    fn branch(&mut self, function: &Function, branch: &Expr, depth: usize) {
        let before = self.text.len();
        match &branch.kind {
            ExprKind::Block { statements, value } => {
                self.statements(function, statements, depth);
                if let Some(value) = value {
                    self.statement(function, &Stmt::Expr((**value).clone()), depth);
                }
            }
            // `else if`: the next `if`, as the one statement of the
            // `else`.
            _ => self.statement(function, &Stmt::Expr(branch.clone()), depth),
        }
        if self.text.len() == before {
            self.line(depth, "pass");
        }
    }

    // ---- Expressions ----

    /// True when `local` is a temporal input of the node being written:
    /// a parameter that is not `const`.
    fn is_input(function: &Function, local: usize) -> bool {
        let declared = &function.locals[local];
        declared.kind == LocalKind::Parameter
            && function.signature.is_some_and(|signature| {
                signature
                    .parameters
                    .iter()
                    .any(|parameter| parameter.name == declared.name && !parameter.constant)
            })
    }

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
            )
        {
            self.unsupported(value.at, "this expression in a graph");
            return "None".to_string();
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
                let read = self.place(function, *local);
                // In a hook a temporal input is read through its
                // payload: in a `when`, and in `stop` (TIME-5).
                if matches!(function.mode, Mode::Node | Mode::Stop)
                    && Self::is_input(function, *local)
                {
                    format!("{read}.value")
                } else if function.mode == Mode::Lifecycle && Self::is_input(function, *local) {
                    // The analyser rejects it (`input.lifecycle`).
                    self.unsupported(value.at, "a temporal input read in `start`");
                    read
                } else {
                    read
                }
            }
            ExprKind::Endpoint {
                operation,
                inputs: Some(inputs),
            } if inputs.len() == 1 => {
                let endpoint = &function.locals[inputs[0]];
                // hgraph gives `start` no endpoint, and `stop` its
                // inputs and not its output: there `valid(input)` is the
                // guard a read stands under (TIME-5).
                let given = match function.mode {
                    Mode::Lifecycle => false,
                    Mode::Stop => {
                        endpoint.kind != LocalKind::Inject && *operation == EndpointOperation::Valid
                    }
                    _ => true,
                };
                if !given {
                    self.unsupported(value.at, "this endpoint read in `start` or `stop`");
                }
                // The node's own output is hgraph's `_output`.
                let input = if endpoint.kind == LocalKind::Inject {
                    "_output".to_string()
                } else {
                    name(&endpoint.name)
                };
                match operation {
                    EndpointOperation::Valid => format!("{input}.valid"),
                    EndpointOperation::Modified => format!("{input}.modified"),
                    // END-4: "Invalidation clears validity,
                    // modification, payload, and delta."
                    EndpointOperation::Invalidate => format!("{input}.invalidate()"),
                    // END-3: what the input published this cycle.
                    EndpointOperation::DeltaValue => format!("{input}.delta_value"),
                    other => {
                        self.unsupported(value.at, &format!("`{}`", other.name()));
                        "None".to_string()
                    }
                }
            }
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
            // TST-17, RUN-3: a run in real time is not written yet.
            ExprKind::Eval {
                real_time: true, ..
            } => {
                self.unsupported(value.at, "a run in real time");
                "None".to_string()
            }
            ExprKind::Eval { call, config, .. } => self.eval(function, call, config.as_deref()),
            // TST-16: "A trace given to a `const fn` is an ordinary
            // value there: ... a `list<tuple<datetime, delta<T>>>`, one
            // tuple for each publication". The harness makes it of the
            // dense trace an eval gives.
            ExprKind::Publications(trace) => {
                let trace = self.expression(function, trace);
                format!("_harness.publications({trace})")
            }
            ExprKind::Trace(cells) => self.trace(function, cells, value.at),
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
            // TYPE-3: a struct's fields are public, and read by name.
            ExprKind::Field { base, member } => {
                let Some(field) = field_of(self.program, &base.ty, *member) else {
                    // A property of a native value, or of a delta.
                    self.unsupported(value.at, "this member");
                    return "None".to_string();
                };
                let base = self.expression(function, base);
                format!("{base}.{}", name(&field.name))
            }
            // EXPR-7: "Brackets read an element of a list or a tuple by
            // an `i64` index, and the value a dictionary has for a
            // key." ERR-6: a dictionary's brackets "are strict
            // point-in-time reads; absence fails".
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
            ExprKind::TagTest { success, value } => {
                self.natives = true;
                let value = self.expression(function, value);
                let tag = if *success { "Ok" } else { "Err" };
                format!("_py.isinstance({value}, _core.{tag})")
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
                let part = match part {
                    ErrorPart::Message => "message",
                    ErrorPart::Code => "code",
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
            _ => {
                self.unsupported(value.at, "this expression");
                "None".to_string()
            }
        }
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
        let class = self.type_name(of.0, of.1);
        let applied = match &whole.ty {
            Type::Named { arguments, .. } if !arguments.is_empty() => {
                let texts: Option<Vec<String>> = arguments
                    .iter()
                    .map(|argument| canonical_argument(program, argument))
                    .collect();
                match texts {
                    Some(texts) => {
                        self.natives = true;
                        format!("_core.applied({class}, {})", string(&texts.join(", ")))
                    }
                    None => {
                        self.unsupported(
                            at,
                            "a generic struct made where its arguments are not known",
                        );
                        class
                    }
                }
            }
            // A struct with no generic parameter is its own class.
            _ if read.generics.is_empty() => class,
            _ => {
                self.unsupported(
                    at,
                    "a generic struct made where its arguments are not known",
                );
                class
            }
        };
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
        let cold = Function {
            locals: &[],
            signature: None,
            mode: Mode::Value,
        };
        self.expanding.push(key);
        let written = self.expression(&cold, value);
        self.expanding.pop();
        written
    }

    /// What the assignment of `value` to `place` gives the place's
    /// root: the root as it is assigned, and its new value.
    ///
    /// EXPR-3: "An assignment may name a place inside a value, at any
    /// depth: a field, `point.x = 1` ... The place's root is a `var`, a
    /// `state`, a `cache`, or, in a `when`, `out`." A value is never
    /// changed (TYPE-5: "later mutation of a source cannot change an
    /// earlier retained value"), so the root is given a new one:
    /// `line.end.x = 1` makes `line` the `line` whose `end` is the old
    /// `end` with `x` 1, and `rows[0][1] = 2` makes `rows` the `rows`
    /// whose first element is the old one with its second 2. What held
    /// the old value holds it still.
    fn assigned(
        &mut self,
        function: &Function,
        place: &Expr,
        value: String,
    ) -> Option<(String, String)> {
        match &place.kind {
            ExprKind::Local(local) => Some((self.place(function, *local), value)),
            ExprKind::Field { base, member } => {
                let Some(field) = field_of(self.program, &base.ty, *member) else {
                    self.unsupported(place.at, "an assignment to this member");
                    return None;
                };
                self.natives = true;
                let held = self.expression(function, base);
                let value = format!("_core.with_field({held}, {}={value})", name(&field.name));
                self.assigned(function, base, value)
            }
            // "of a list's element type, under an `i64` index; of a
            // dictionary's value type, under a key of its key type; or
            // of a tuple's position, under a constant index. A key the
            // dictionary has not is added. An index outside a list
            // fails where it is evaluated."
            ExprKind::Index { base, index } => {
                let with = match element_read(&base.ty) {
                    Some("item") => "with_item",
                    Some(_) => "with_entry",
                    None => {
                        self.unsupported(place.at, "an assignment to this element");
                        return None;
                    }
                };
                self.natives = true;
                let held = self.expression(function, base);
                let index = self.expression(function, index);
                let value = format!("_core.{with}({held}, {index}, {value})");
                self.assigned(function, base, value)
            }
            _ => {
                self.unsupported(place.at, "an assignment to this place");
                None
            }
        }
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
        }
    }

    /// A temporal literal, by the parts it writes (LEX-11), which are
    /// what its type goes by. TYPE-2: a `date`, a `time`, a
    /// `civil_datetime` and a `datetime` are Python's `date`, `time` and
    /// `datetime`. A zoned datetime, a zoned time and a time zone are not
    /// written yet.
    fn temporal(&mut self, parts: &Temporal, at: Location) -> String {
        if parts.zone.is_some() {
            self.unsupported(at, "a zoned datetime, a zoned time or a time zone");
            return "None".to_string();
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

    /// The arguments of a call of `callee`, a module and a declaration
    /// of it: each by its parameter's name, so that the order they are
    /// written in does not matter to Python.
    ///
    /// UNIT-8: a parameter with a default may be left out. The call is
    /// then written with the default in its place, read from the
    /// analysed program: a default names no parameter and no local
    /// (UNIT-7), so it means the same wherever it is written, and a
    /// function it calls is named through its own module.
    fn arguments(
        &mut self,
        function: &Function,
        callee: (usize, usize),
        signature: &Signature,
        arguments: &[Argument],
    ) -> String {
        let mut written: Vec<String> = arguments
            .iter()
            .filter_map(|argument| {
                let parameter = signature.parameters.get(argument.parameter?)?;
                let value = self.expression(function, &argument.value);
                Some(format!("{}={value}", name(&parameter.name)))
            })
            .collect();
        for (index, parameter) in signature.parameters.iter().enumerate() {
            let given = arguments
                .iter()
                .any(|argument| argument.parameter == Some(index));
            if given || parameter.pack.is_some() {
                continue;
            }
            if let Some(value) = self.left_out(callee, index, parameter) {
                written.push(format!("{}={value}", name(&parameter.name)));
            }
        }
        written.join(", ")
    }

    /// What parameter `index` of the function `callee` is given when a
    /// call leaves it out: its default, or `None` for one written `T?`
    /// with none. Nothing for a parameter that cannot be left out.
    fn left_out(
        &mut self,
        callee: (usize, usize),
        index: usize,
        parameter: &Parameter,
    ) -> Option<String> {
        let program = self.program;
        let defaults = program.modules[callee.0]
            .defaults
            .get(callee.1)
            .map_or(&[][..], Vec::as_slice);
        if let Some(Some(default)) = defaults.get(index) {
            // A default that calls its own function with that same
            // argument left out would be written inside itself
            // without end.
            let key = (callee.0, callee.1, index);
            if self.expanding.contains(&key) {
                self.unsupported(default.at, "a default that leaves itself out");
                return Some("None".to_string());
            }
            // A default is read with no local in scope.
            let cold = Function {
                locals: &[],
                signature: None,
                mode: Mode::Value,
            };
            self.expanding.push(key);
            let value = self.expression(&cold, default);
            self.expanding.pop();
            Some(value)
        } else if matches!(parameter.ty, Type::Nullable(_)) {
            // UNIT-8: "A parameter whose type is written `T?` needs
            // no default to be omitted: it is taken to have a
            // default of `null`".
            Some("None".to_string())
        } else {
            None
        }
    }

    /// What the call `called` of a native candidate gives, as a value
    /// of the program's. `result` is the candidate's result type.
    ///
    /// A candidate that gives an enum, as core's `compare_` gives a
    /// `CmpResult` (OPS-3), gives its member's name: the provider
    /// declares no enum of the program's. The member is read from the
    /// enum by that name, as `Side("buy")` is (TYPE-7).
    fn native_result(&mut self, result: Option<&Type>, called: String) -> String {
        match result {
            Some(Type::Named {
                module,
                declaration,
                kind: NamedKind::Enum,
                ..
            }) => {
                let of = self.type_name(*module, *declaration);
                format!("_core.member({of}, {called})")
            }
            _ => called,
        }
    }

    /// The generic values a call gives a native candidate, each as
    /// `name=value` for the provider's function.
    ///
    /// NUM-9: "`zero` is a generic value parameter of the candidates
    /// that declare it, and a call gives it a compile-time constant."
    /// A value of an enum is passed as the name of its member,
    /// `"ieee"`, and an integer as itself. A generic type is not
    /// passed: a Python function is not typed.
    fn options(
        &mut self,
        signature: &Signature,
        generics: &[GenericArgument],
        at: Location,
    ) -> Vec<String> {
        let mut written = Vec::new();
        for (parameter, argument) in signature.generics.iter().zip(generics) {
            let GenericArgument::Constant(constant) = argument else {
                continue;
            };
            // The analyser says what the constant comes to: a member
            // of an enum is one however it was written (TYPE-7), and
            // none of its spelling is read here.
            let value = match &constant.value {
                ConstantValue::Member(member) => Some(string(member)),
                ConstantValue::Int(number) => Some(number.to_string()),
                _ => None,
            };
            match value {
                Some(value) => written.push(format!("{}={value}", name(&parameter.name))),
                // A value that is not a constant here, such as a
                // generic value of the function the call is in.
                None => self.unsupported(at, &format!("the option `{}`", constant.text)),
            }
        }
        written
    }

    // ---- Lifting ----

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
            _ => false,
        }
    }

    /// Gives a lifted node a name and writes it before the piece being
    /// written. `before` is the node's text up to its name and `after`
    /// the text from there on; `of` is what it lifts, for the name. A
    /// node lifted the same way twice in a piece is written once.
    ///
    /// The text is in two parts, and the name is not put in for a mark
    /// in it: a mark could as well be in a string a program wrote.
    fn lifted_node(&mut self, of: &str, before: String, after: String) -> String {
        let text = (before, after);
        if let Some((_, known)) = self.lifts.iter().find(|(written, _)| *written == text) {
            return known.clone();
        }
        // The name starts with `_`, as no name of a program's does.
        let python = format!("_lift_{}_{of}", self.lifted);
        self.lifted += 1;
        self.hgraph = true;
        self.prelude.push_str("\n\n");
        self.prelude.push_str(&text.0);
        self.prelude.push_str(&python);
        self.prelude.push_str(&text.1);
        self.lifts.push((text, python.clone()));
        python
    }

    /// FN-6: "A selected value helper, a `const fn` or a value candidate
    /// of an operator, with temporal arguments in composition is
    /// lifted: temporal arguments become inputs; scalar arguments and
    /// omitted defaults bind as configuration. ... Any input
    /// notification activates the node ... Each admitted evaluation
    /// publishes its result, including an equal result. An outputless
    /// helper lifts to a sink."
    ///
    /// Writes the node that calls the function `target`, or the
    /// provider's function when `lifts` says it is a native candidate
    /// of core, and gives the node's name. `bound` says how each
    /// parameter of `signature` is given, with `None` for one left
    /// out; `result` is what the function gives.
    ///
    /// FN-11: "A lifted argument with a declared default does not
    /// require a valid input: bind the default when that input is
    /// invalid. Otherwise, an argument whose value contract admits null
    /// does not require a valid input: bind null when it is invalid.
    /// Every other input must satisfy ordinary `valid(input)`."
    fn lifted(
        &mut self,
        target: (usize, usize),
        lifts: Lifts,
        signature: &Signature,
        bound: &[Option<Bound>],
        result: Option<&Type>,
        at: Location,
    ) -> String {
        // The node's parameters, the inputs that must be valid, how
        // many inputs there are, and the arguments of the call it
        // makes.
        let mut parameters = Vec::new();
        let mut required = Vec::new();
        let mut inputs = 0;
        let mut passed = Vec::new();
        for (index, parameter) in signature.parameters.iter().enumerate() {
            let python = name(&parameter.name);
            match bound.get(index).and_then(Option::as_ref) {
                Some(Bound::Input(ty)) => {
                    inputs += 1;
                    let annotation = self
                        .annotation(ty, true, at)
                        .unwrap_or_else(|| "_py.object".to_string());
                    parameters.push(format!("{python}: {annotation}"));
                    match self.left_out(target, index, parameter) {
                        Some(fallback) => passed.push(format!(
                            "{python}=({python}.value if {python}.valid else {fallback})"
                        )),
                        None => {
                            passed.push(format!("{python}={python}.value"));
                            required.push(python);
                        }
                    }
                }
                Some(Bound::Configuration(ty)) => {
                    let annotation = self
                        .annotation(ty, false, at)
                        .unwrap_or_else(|| "_py.object".to_string());
                    parameters.push(format!("{python}: {annotation}"));
                    passed.push(format!("{python}={python}"));
                }
                None => {
                    if let Some(value) = self.left_out(target, index, parameter) {
                        passed.push(format!("{python}={value}"));
                    }
                }
            }
        }
        let (called, of) = match lifts {
            Lifts::Native(native, configured) => {
                self.natives = true;
                passed.extend(configured.iter().cloned());
                (format!("_core.{native}"), native.to_string())
            }
            Lifts::Function => {
                let declared = &self.program.modules[target.0].declarations[target.1];
                (self.function_name(target.0, target.1), name(&declared.name))
            }
        };
        let returns = result.and_then(|result| self.annotation(result, true, at));
        // hgraph requires every input to be valid unless it is told
        // which ones.
        let options = if required.len() == inputs {
            String::new()
        } else {
            format!("(valid={})", tuple(&required))
        };
        let call = format!("{called}({})", passed.join(", "));
        let call = match lifts {
            Lifts::Native(..) => self.native_result(signature.result.as_ref(), call),
            Lifts::Function => call,
        };
        let (decorator, after) = match (result, returns) {
            (Some(_), returns) => (
                "compute_node",
                format!(
                    "({}) -> {}:\n    return {call}\n",
                    parameters.join(", "),
                    returns.unwrap_or_else(|| "_py.object".to_string()),
                ),
            ),
            (None, _) => (
                "sink_node",
                format!("({}):\n    {call}\n", parameters.join(", ")),
            ),
        };
        self.lifted_node(&of, format!("@_hg.{decorator}{options}\ndef "), after)
    }

    /// The arguments a call gives, each by its parameter's name, and
    /// how each parameter of `signature` is then bound in a lifted
    /// node: as an input where `input` gives the argument a type, as
    /// configuration otherwise.
    fn bound(
        &mut self,
        function: &Function,
        signature: &Signature,
        arguments: &[Argument],
        input: impl Fn(&Self, &Expr) -> Option<Type>,
    ) -> (Vec<String>, Vec<Option<Bound>>) {
        let mut written = Vec::new();
        let mut bound: Vec<Option<Bound>> = vec![None; signature.parameters.len()];
        for argument in arguments {
            let parameter = argument
                .parameter
                .and_then(|index| Some((index, signature.parameters.get(index)?)));
            let Some((index, parameter)) = parameter.filter(|(_, p)| p.pack.is_none()) else {
                // FN-8: a pack takes the arguments the fixed parameters
                // leave, which are not written yet.
                self.unsupported(argument.value.at, "an argument of a pack in a lifted call");
                continue;
            };
            bound[index] = Some(match input(self, &argument.value) {
                Some(ty) => Bound::Input(ty),
                None => Bound::Configuration(argument.value.ty.clone()),
            });
            let value = self.expression(function, &argument.value);
            written.push(format!("{}={value}", name(&parameter.name)));
        }
        (written, bound)
    }

    /// The Python name `declaration` of `module` is defined under.
    ///
    /// FN-4 lets one name have a temporal and a value definition: "
    /// Composition and eval choose the temporal definition if one
    /// exists; runtime/value evaluation chooses the value definition."
    /// Python has one function to a name, so the value definition of
    /// such a pair is `_value_name`, which no HGL name is written as.
    ///
    /// A candidate of an operator binds no name (UNIT-12), and an
    /// operator may have many. Each is `_impl_` and then the operator
    /// and the types of the candidate's parameters:
    /// `_impl_add__f64_i64`. The `_` first makes the name the target's
    /// own, as no name of a program is.
    ///
    /// GEN-5 lets an operator have a temporal and a value candidate "of
    /// the same parameter types", as FN-4 lets a name have both. The
    /// value one of such a pair is `_value_impl_add__f64_i64`.
    fn definition_name(&self, module: usize, index: usize) -> String {
        let declared = &self.program.modules[module];
        let declaration = &declared.declarations[index];
        if let DeclarationKind::Implementation(role, _) = declaration.kind {
            // The candidate `index` of the module as it is named.
            let named = |index: usize| -> String {
                let types: Vec<String> = match declared.details.get(index) {
                    Some(Detail::Callable(signature)) => signature
                        .parameters
                        .iter()
                        .map(|parameter| mangled(&parameter.ty))
                        .collect(),
                    _ => Vec::new(),
                };
                let operator = &declared.declarations[index].name;
                format!("_impl_{operator}_{}", types.join("_"))
            };
            let python = named(index);
            let paired = role == Role::Value
                && declared
                    .declarations
                    .iter()
                    .enumerate()
                    .any(|(other, candidate)| {
                        matches!(
                            candidate.kind,
                            DeclarationKind::Implementation(Role::Temporal, _)
                        ) && named(other) == python
                    });
            return if paired {
                format!("_value{python}")
            } else {
                python
            };
        }
        let python = name(&declaration.name);
        let paired = declaration.kind == DeclarationKind::Function(Role::Value)
            && declared.declarations.iter().any(|other| {
                other.name == declaration.name
                    && other.kind == DeclarationKind::Function(Role::Temporal)
            });
        if paired {
            format!("_value_{python}")
        } else {
            python
        }
    }

    /// The Python name of the function `declaration` of `module`. One
    /// of another module is named through that module, which is
    /// imported under a name of its own: two modules may each have a
    /// `pick`, and this one a `pick` too.
    fn function_name(&mut self, module: usize, declaration: usize) -> String {
        let declared = &self.program.modules[module];
        let python = self.definition_name(module, declaration);
        self.piece.uses.insert((module, declaration));
        if module == self.module {
            return python;
        }
        self.imports.insert(declared.path.clone());
        format!("{}.{python}", alias(&declared.path))
    }

    /// A call: of a function of the program, or of a native candidate
    /// of core, which the target's own provider has a function for.
    fn call(
        &mut self,
        function: &Function,
        callee: &Callee,
        arguments: &[Argument],
        whole: &Expr,
    ) -> String {
        let at = whole.at;
        let program = self.program;
        let (module, declaration, generics, candidate) = match callee {
            Callee::Function {
                module,
                declaration,
                generics,
                ..
            } => (*module, *declaration, generics, false),
            Callee::Candidate {
                module,
                declaration,
                generics,
                ..
            } => (*module, *declaration, generics, true),
            Callee::Required { .. } | Callee::Unread => {
                self.unsupported(at, "this call");
                return "None".to_string();
            }
        };
        let Some(Detail::Callable(signature)) = program.modules[module].details.get(declaration)
        else {
            self.unsupported(at, "this call");
            return "None".to_string();
        };
        let declared = &program.modules[module].declarations[declaration];
        // A test is a composition block (TST-3), and a temporal function
        // called in one makes a connection, which Python has no graph
        // to make in: a target is run through `eval`.
        if function.mode == Mode::Test && declared.kind.role() == Some(Role::Temporal) {
            self.unsupported(
                at,
                "a call of a temporal function in a test, outside an eval",
            );
            return "None".to_string();
        }
        // A candidate with a body of its own is a function of its
        // module, a value one, or a node or a graph, a temporal one.
        // One with none is native, and a native candidate of core is a
        // function of the target's provider.
        let bodied = matches!(declared.kind, DeclarationKind::Implementation(..));
        let native = if candidate && !bodied {
            let types: Vec<String> = signature
                .parameters
                .iter()
                .map(|parameter| mangled(&parameter.ty))
                .collect();
            let native = format!("{}_{}", declared.name, types.join("_"));
            let core = Some(module) == program.core();
            if !core || !provided().any(|provided| provided == native) {
                self.unsupported(
                    at,
                    &format!("the candidate `{}({})`", declared.name, types.join(", ")),
                );
                return "None".to_string();
            }
            Some(native)
        } else {
            None
        };
        // What a native candidate is configured with: its provider's
        // function takes each generic value by its name.
        let options = match native {
            Some(_) => self.options(signature, generics, at),
            None => Vec::new(),
        };
        // FN-6: in a graph a symbol or a value function given a
        // connection is lifted over it. "All-scalar calls execute
        // directly and invent no source timing": one given none is
        // called as it is, where the graph is built.
        // GEN-5: "a value candidate it selects for temporal arguments
        // is lifted (FN-6)". A temporal candidate is the node or the
        // graph itself.
        let lifted = function.mode == Mode::Graph
            && declared.kind.role() != Some(Role::Temporal)
            && (candidate || declared.kind.role() == Some(Role::Value))
            && arguments
                .iter()
                .any(|argument| self.connects(function, &argument.value));
        if lifted {
            let (written, bound) = self.bound(function, signature, arguments, |emitter, value| {
                emitter.connects(function, value).then(|| value.ty.clone())
            });
            let result = signature.result.as_ref().map(|_| &whole.ty);
            let lifts = match &native {
                Some(native) => Lifts::Native(native, &options),
                None => Lifts::Function,
            };
            let node = self.lifted((module, declaration), lifts, signature, &bound, result, at);
            return format!("{node}({})", written.join(", "));
        }
        if let Some(native) = native {
            // NUM-2: an `i64` result outside the signed 64-bit range is
            // an error, which Python's own `+` would not make it: the
            // provider's function does.
            self.natives = true;
            let arguments = self.arguments(function, (module, declaration), signature, arguments);
            let arguments: Vec<String> = [arguments]
                .into_iter()
                .filter(|written| !written.is_empty())
                .chain(options)
                .collect();
            let called = format!("_core.{native}({})", arguments.join(", "));
            return self.native_result(signature.result.as_ref(), called);
        }
        if !bodied && !matches!(declared.kind, DeclarationKind::Function(_)) {
            self.unsupported(at, &format!("a call of `{}`", declared.name));
            return "None".to_string();
        }
        let python = self.called_name((module, declaration), generics, at);
        let arguments = self.arguments(function, (module, declaration), signature, arguments);
        format!("{python}({arguments})")
    }

    /// `eval(target, ...)`: the harness's `evaluate`, given the target
    /// and each argument by its parameter's name.
    ///
    /// RUN-1, core's CRUN-2: a run's configuration is given last, as
    /// `_config`, a name no parameter has, so that it is evaluated
    /// after the call's arguments, as EXPR-13 has them. The harness
    /// reads its `start`, `end` and `length`.
    fn eval(&mut self, function: &Function, call: &Expr, config: Option<&Expr>) -> String {
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
        } = &call.kind
        else {
            self.unsupported(call.at, "this eval");
            return "None".to_string();
        };
        let Some(Detail::Callable(signature)) =
            self.program.modules[*module].details.get(*declaration)
        else {
            self.unsupported(call.at, "this eval");
            return "None".to_string();
        };
        self.harness = true;
        // FN-4: where a name has no temporal definition, eval takes the
        // value one, and TST-8: "A selected value helper uses ordinary
        // lifting". A trace is an input of the lifted node, and any
        // other argument its configuration.
        let declared = &self.program.modules[*module].declarations[*declaration];
        if declared.kind.role() == Some(Role::Value) {
            let (written, bound) =
                self.bound(function, signature, arguments, |_, value| match &value.ty {
                    Type::Trace { cell, .. } => Some((**cell).clone()),
                    _ => None,
                });
            let node = self.lifted(
                (*module, *declaration),
                Lifts::Function,
                signature,
                &bound,
                signature.result.as_ref(),
                call.at,
            );
            let mut given = written;
            if let Some(config) = config {
                given.push(format!("_config={}", self.expression(function, config)));
            }
            return if given.is_empty() {
                format!("_harness.evaluate({node})")
            } else {
                format!("_harness.evaluate({node}, {})", given.join(", "))
            };
        }
        let target = self.called_name((*module, *declaration), generics, call.at);
        let mut arguments = self.arguments(function, (*module, *declaration), signature, arguments);
        if let Some(config) = config {
            if !arguments.is_empty() {
                arguments.push_str(", ");
            }
            arguments.push_str(&format!("_config={}", self.expression(function, config)));
        }
        if arguments.is_empty() {
            format!("_harness.evaluate({target})")
        } else {
            format!("_harness.evaluate({target}, {arguments})")
        }
    }

    /// TYPE-16: `delta<set<i64>>(added: [1, 2], removed: [3])`, a set's
    /// delta made from its properties, as hgraph's own set delta. A
    /// property left out is empty.
    fn delta(&mut self, function: &Function, value: &Expr, arguments: &[Argument]) -> String {
        let element = match &value.ty {
            Type::Delta(of) => match &**of {
                Type::Set(element) => Some(&**element),
                _ => None,
            },
            _ => None,
        };
        let Some(element) = element else {
            self.unsupported(value.at, "a delta that is not a set's");
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
    fn trace(&mut self, function: &Function, cells: &[Cell], at: Location) -> String {
        if cells.iter().any(|cell| cell.time.is_some()) {
            self.unsupported(at, "a timed trace");
            return "None".to_string();
        }
        let written: Vec<String> = cells
            .iter()
            .map(|cell| match &cell.value {
                None => "None".to_string(),
                // hgraph's harness has `None` for no publication, so a
                // published null would read as silence.
                Some(Expr {
                    kind: ExprKind::Literal(Value::Null),
                    at,
                    ..
                }) => {
                    self.unsupported(*at, "a published null in a trace");
                    "None".to_string()
                }
                Some(value) => self.expression(function, value),
            })
            .collect();
        format!("[{}]", written.join(", "))
    }
}

/// A type as text that no other type is written as, for the class of a
/// generic struct's application (see `Emitter::construct`). A named
/// type has its module's path before it: two modules may each declare
/// a `Quote`. `None` for a type that is not whole as the module is
/// written: one that names a generic parameter, or is not known.
fn canonical(program: &Program, ty: &Type) -> Option<String> {
    let one = |ty: &Type| canonical(program, ty);
    Some(match ty {
        Type::Scalar(scalar) => scalar.as_str().to_string(),
        Type::Named {
            name,
            module,
            arguments,
            ..
        } => {
            let path = &program.modules[*module].path;
            if arguments.is_empty() {
                format!("{path}::{name}")
            } else {
                let arguments: Option<Vec<String>> = arguments
                    .iter()
                    .map(|argument| canonical_argument(program, argument))
                    .collect();
                format!("{path}::{name}<{}>", arguments?.join(", "))
            }
        }
        Type::Tuple(elements) => {
            let elements: Option<Vec<String>> = elements.iter().map(one).collect();
            format!("tuple<{}>", elements?.join(", "))
        }
        Type::List { element, size } => match size {
            Some(size) => format!("list<{}, {}>", one(element)?, canonical_constant(size)?),
            None => format!("list<{}>", one(element)?),
        },
        Type::Set(element) => format!("set<{}>", one(element)?),
        Type::Dict { key, value } => format!("dict<{}, {}>", one(key)?, one(value)?),
        Type::Rolling {
            value,
            maximum,
            minimum,
        } => match minimum {
            Some(minimum) => format!(
                "rolling<{}, {}, {}>",
                one(value)?,
                canonical_constant(maximum)?,
                canonical_constant(minimum)?
            ),
            None => format!("rolling<{}, {}>", one(value)?, canonical_constant(maximum)?),
        },
        Type::Ref(target) => format!("ref<{}>", one(target)?),
        Type::Endpoint(target) => format!("endpoint<{}>", one(target)?),
        Type::Signal => "signal".to_string(),
        Type::Atomic(payload) => format!("atomic<{}>", one(payload)?),
        Type::Result { value, error } => format!("result<{}, {}>", one(value)?, one(error)?),
        Type::Error => "error".to_string(),
        Type::Delta(shape) => format!("delta<{}>", one(shape)?),
        Type::Nullable(inner) => format!("{}?", one(inner)?),
        Type::Parameter { .. } | Type::Trace { .. } | Type::Unknown => return None,
    })
}

/// A generic argument as [`canonical`] text.
pub(crate) fn canonical_argument(program: &Program, argument: &GenericArgument) -> Option<String> {
    match argument {
        GenericArgument::Type(ty) => canonical(program, ty),
        GenericArgument::Constant(constant) => canonical_constant(constant),
    }
}

/// A constant inside a type as [`canonical`] text: what it comes to,
/// however it was written. `None` for one whose value is not known as
/// the module is written.
fn canonical_constant(constant: &Constant) -> Option<String> {
    Some(match &constant.value {
        ConstantValue::Int(number) => number.to_string(),
        ConstantValue::Duration(microseconds) => format!("{microseconds}us"),
        ConstantValue::Unbounded => "unbounded".to_string(),
        ConstantValue::Member(member) => format!(".{member}"),
        // A literal of another kind is held as it is written, and
        // compared without the spaces between its tokens.
        ConstantValue::Other(_) => constant.unspaced(),
        ConstantValue::Parameter { .. } | ConstantValue::Unevaluated => return None,
    })
}

/// The provider's function that reads an element of a value of type
/// `ty` in brackets: `item` for a list and a tuple, by position, and
/// `entry` for a dictionary, by key. `None` for a type brackets do not
/// read.
fn element_read(ty: &Type) -> Option<&'static str> {
    match ty {
        Type::List { .. } | Type::Tuple(_) => Some("item"),
        Type::Dict { .. } => Some("entry"),
        _ => None,
    }
}

/// The field at `member` among those a struct has, for a value of the
/// type `ty`. `None` where the type is no struct: a native type has
/// properties, and a delta its own.
fn field_of<'p>(program: &'p Program, ty: &Type, member: usize) -> Option<&'p Member> {
    let Type::Named {
        module,
        declaration,
        kind: NamedKind::Struct,
        ..
    } = ty
    else {
        return None;
    };
    match program.modules[*module].details.get(*declaration) {
        Some(Detail::Struct(read)) => read.members.get(member),
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
