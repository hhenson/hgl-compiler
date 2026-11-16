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
    Argument, BinaryOperator, Body, Callee, Cell, EndpointOperation, Expr, ExprKind, Hook, Local,
    Stmt, UnaryOperator, Value,
};
use hgl_analyser::{
    Declaration, DeclarationKind, Detail, LocalKind, Phase, Program, Role, Scalar, Signature, Type,
};
use hgl_diagnostics::{Category, Code, Diagnostic, Diagnostics, Location};

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
    /// The functions it names, each as its module and its declaration.
    pub(crate) uses: BTreeSet<(usize, usize)>,
    /// For a test, where each of its statements starts: the line of
    /// the module's file, counted from 1, and the place in the source.
    /// A failure is reported by the line Python gives, and this says
    /// which statement of the test that is.
    pub(crate) statements: Vec<(usize, Location)>,
}

/// Writes module `index` of `program`.
pub(crate) fn module(program: &Program, index: usize, diagnostics: &mut Diagnostics) -> Written {
    let mut emitter = Emitter {
        program,
        module: index,
        diagnostics,
        hgraph: false,
        natives: false,
        harness: false,
        imports: BTreeSet::new(),
        expanding: Vec::new(),
        text: String::new(),
        piece: Piece::default(),
        declared: Vec::new(),
        tested: Vec::new(),
    };
    emitter.declarations();
    emitter.tests();
    let text = emitter.finish();
    Written {
        text,
        natives: emitter.natives,
        harness: emitter.harness,
        declarations: emitter.declared,
        tests: emitter.tested,
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
    /// A node's `start` or `stop`: a function of its own to hgraph,
    /// which is given the node's configuration and state, and neither
    /// its inputs nor its output.
    Lifecycle,
    /// A graph: a temporal parameter is a port, passed on as it is.
    Graph,
    /// A named test: ordinary values, and evals.
    Test,
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
        _ => None,
    }
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
    /// Where what cannot be written is reported.
    diagnostics: &'a mut Diagnostics,
    /// True once the text names something of hgraph.
    hgraph: bool,
    /// True once a native function of core is called.
    natives: bool,
    /// True once the eval harness is called.
    harness: bool,
    /// The other generated modules this one names a function of, by
    /// module path.
    imports: BTreeSet<String>,
    /// The defaults being written in place of an argument left out,
    /// each as its module, declaration and parameter: a default is not
    /// written inside itself.
    expanding: Vec<(usize, usize, usize)>,
    /// The Python of the piece being written.
    text: String,
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
        piece.text = std::mem::take(&mut self.text);
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
        for piece in self.declared.iter_mut().chain(self.tested.iter_mut()) {
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
        match (declaration.kind, signature, body) {
            (DeclarationKind::Function(Role::Value), Some(signature), Some(body)) => {
                self.value_function(declaration, signature, body);
            }
            (DeclarationKind::Function(Role::Temporal), Some(signature), Some(body)) => {
                match module.phases.get(index).copied().flatten() {
                    Some(Phase::Node) => self.node(declaration, signature, body),
                    Some(Phase::Graph) => self.graph(declaration, signature, body),
                    None => self.unsupported(declaration.at, "this function"),
                }
            }
            _ => {
                let what = match declaration.kind {
                    DeclarationKind::Struct => "a struct",
                    DeclarationKind::Enum => "an enum",
                    DeclarationKind::Operator => "an operator",
                    DeclarationKind::NativeType => "a native type",
                    kind if kind.candidate().is_some() => "an operator's candidate",
                    _ => "a native function",
                };
                self.unsupported(declaration.at, what);
            }
        }
    }

    /// `const fn name(a, b) -> T`: a plain Python function.
    fn value_function(&mut self, declaration: &Declaration, signature: &Signature, body: &Body) {
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
                self.definition_name(self.module, declaration),
                parameters.join(", ")
            ),
        );
        let function = Function {
            locals: &body.locals,
            signature: Some(signature),
            mode: Mode::Value,
        };
        self.block(&function, &body.statements, 1);
    }

    /// The annotation of a parameter or a result of a temporal
    /// function: `TS[int]` for a temporal `i64`, `int` for a `const`
    /// one. `None`, reported, for a type not written yet.
    fn annotation(&mut self, ty: &Type, temporal: bool, at: Location) -> Option<String> {
        let Some(python) = scalar(ty) else {
            self.unsupported(at, &format!("the type `{ty}`"));
            return None;
        };
        if temporal {
            self.hgraph = true;
            Some(format!("_hg.TS[{python}]"))
        } else {
            Some(python.to_string())
        }
    }

    /// The parameters of a temporal function as Python writes them, and
    /// its result's annotation.
    fn temporal_signature(
        &mut self,
        declaration: &Declaration,
        signature: &Signature,
    ) -> (Vec<String>, Option<String>) {
        if !signature.generics.is_empty() {
            self.unsupported(declaration.at, "a generic temporal function");
        }
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
    fn node(&mut self, declaration: &Declaration, signature: &Signature, body: &Body) {
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
            let annotation = match &result {
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
            let quoted: Vec<String> = locals
                .iter()
                .map(|&local| format!("\"{}\"", name(&body.locals[local].name)))
                .collect();
            // A tuple of one needs its comma.
            format!(
                "({}{})",
                quoted.join(", "),
                if quoted.len() == 1 { "," } else { "" }
            )
        };
        let mut options = Vec::new();
        if active.len() != inputs.len() && !handlers.is_empty() {
            options.push(format!("active={}", names(&active)));
        }
        if valid.len() != inputs.len() && !handlers.is_empty() {
            options.push(format!("valid={}", names(&valid)));
        }
        let python = name(&declaration.name);
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
            self.line(0, &format!("def _stop_{python}({lifecycle}):"));
            self.block(&lifecycle_hook, stop, 1);
        }
    }

    /// A graph: `@graph`, whose body wires the nodes it calls.
    fn graph(&mut self, declaration: &Declaration, signature: &Signature, body: &Body) {
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
            &format!(
                "def {}({}){returns}:",
                name(&declaration.name),
                parameters.join(", ")
            ),
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
                if function.mode == Mode::Lifecycle {
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
            other => self.unsupported(at_of(other, function.locals), "this statement"),
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
        // `not`, `if` and `float` would act on the port: only a name, a
        // literal and a call are written there.
        if function.mode == Mode::Graph
            && !matches!(
                value.kind,
                ExprKind::Literal(_) | ExprKind::Local(_) | ExprKind::Call { .. }
            )
        {
            self.unsupported(value.at, "this expression in a graph");
            return "None".to_string();
        }
        match &value.kind {
            ExprKind::Literal(literal) => self.literal(literal, value.at),
            ExprKind::Local(local) => {
                let read = self.place(function, *local);
                // In a hook a temporal input is read through its
                // payload.
                if function.mode == Mode::Node && Self::is_input(function, *local) {
                    format!("{read}.value")
                } else if function.mode == Mode::Lifecycle && Self::is_input(function, *local) {
                    self.unsupported(value.at, "a temporal input read in `start` or `stop`");
                    read
                } else {
                    read
                }
            }
            ExprKind::Endpoint {
                operation,
                inputs: Some(inputs),
            } if inputs.len() == 1 => {
                if function.mode == Mode::Lifecycle {
                    self.unsupported(value.at, "an endpoint read in `start` or `stop`");
                }
                let endpoint = &function.locals[inputs[0]];
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
            } => self.call(function, callee, arguments, value.at),
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
                // sequence that is a set is one in Python too.
                let is_set = matches!(&value.ty, Type::Set(_))
                    || matches!(&value.ty, Type::Nullable(inner) if matches!(**inner, Type::Set(_)));
                if is_set {
                    format!(
                        "_py.frozenset(({}{}))",
                        entries.join(", "),
                        if entries.len() == 1 { "," } else { "" }
                    )
                } else {
                    format!("[{}]", entries.join(", "))
                }
            }
            ExprKind::Eval(call) => self.eval(function, call),
            ExprKind::Trace(cells) => self.trace(function, cells, value.at),
            ExprKind::TraceCompare {
                left,
                right,
                negated,
            } => {
                let symbol = if *negated { "!=" } else { "==" };
                let left = self.expression(function, left);
                let right = self.expression(function, right);
                format!("{left} {symbol} {right}")
            }
            _ => {
                self.unsupported(value.at, "this expression");
                "None".to_string()
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
            Value::Duration(_) | Value::Temporal(_) => {
                self.unsupported(at, "a duration, a date or a time");
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
        let program = self.program;
        let defaults = program.modules[callee.0]
            .defaults
            .get(callee.1)
            .map_or(&[][..], Vec::as_slice);
        // A default is read with no local in scope.
        let cold = Function {
            locals: &[],
            signature: None,
            mode: Mode::Value,
        };
        for (index, parameter) in signature.parameters.iter().enumerate() {
            let given = arguments
                .iter()
                .any(|argument| argument.parameter == Some(index));
            if given || parameter.pack.is_some() {
                continue;
            }
            if let Some(Some(default)) = defaults.get(index) {
                // A default that calls its own function with that same
                // argument left out would be written inside itself
                // without end.
                let key = (callee.0, callee.1, index);
                if self.expanding.contains(&key) {
                    self.unsupported(default.at, "a default that leaves itself out");
                    written.push(format!("{}=None", name(&parameter.name)));
                    continue;
                }
                self.expanding.push(key);
                let value = self.expression(&cold, default);
                self.expanding.pop();
                written.push(format!("{}={value}", name(&parameter.name)));
            } else if matches!(parameter.ty, Type::Nullable(_)) {
                // UNIT-8: "A parameter whose type is written `T?` needs
                // no default to be omitted: it is taken to have a
                // default of `null`".
                written.push(format!("{}=None", name(&parameter.name)));
            }
        }
        written.join(", ")
    }

    /// The Python name `declaration` of `module` is defined under.
    ///
    /// FN-4 lets one name have a temporal and a value definition: "
    /// Composition and eval choose the temporal definition if one
    /// exists; runtime/value evaluation chooses the value definition."
    /// Python has one function to a name, so the value definition of
    /// such a pair is `_value_name`, which no HGL name is written as.
    fn definition_name(&self, module: usize, declaration: &Declaration) -> String {
        let python = name(&declaration.name);
        let paired = declaration.kind == DeclarationKind::Function(Role::Value)
            && self.program.modules[module]
                .declarations
                .iter()
                .any(|other| {
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
        let python = self.definition_name(module, &declared.declarations[declaration]);
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
        at: Location,
    ) -> String {
        let program = self.program;
        let (module, declaration, candidate) = match callee {
            Callee::Function {
                module,
                declaration,
                ..
            } => (*module, *declaration, false),
            Callee::Candidate {
                module,
                declaration,
                ..
            } => (*module, *declaration, true),
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
        // In a graph a symbol or a value function is lifted over the
        // connections it is given, which is not written yet.
        let lifted = function.mode == Mode::Graph
            && (candidate || declared.kind.role() == Some(Role::Value));
        if lifted {
            self.unsupported(at, "a call that is lifted over connections");
            return "None".to_string();
        }
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
        if candidate {
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
            // NUM-2: an `i64` result outside the signed 64-bit range is
            // an error, which Python's own `+` would not make it: the
            // provider's function does.
            self.natives = true;
            let arguments = self.arguments(function, (module, declaration), signature, arguments);
            return format!("_core.{native}({arguments})");
        }
        if !matches!(declared.kind, DeclarationKind::Function(_)) || Some(module) == program.core()
        {
            self.unsupported(at, &format!("a call of `{}`", declared.name));
            return "None".to_string();
        }
        let python = self.function_name(module, declaration);
        let arguments = self.arguments(function, (module, declaration), signature, arguments);
        format!("{python}({arguments})")
    }

    /// `eval(target, ...)`: the harness's `evaluate`, given the target
    /// and each argument by its parameter's name.
    fn eval(&mut self, function: &Function, call: &Expr) -> String {
        let ExprKind::Call {
            callee:
                Callee::Function {
                    module,
                    declaration,
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
        // FN-4: where a name has no temporal definition, eval takes the
        // value one and lifts it. A plain Python function is no node
        // for hgraph to run.
        let declared = &self.program.modules[*module].declarations[*declaration];
        if declared.kind.role() == Some(Role::Value) {
            self.unsupported(call.at, "an eval of a value function, which is lifted");
            return "None".to_string();
        }
        self.harness = true;
        let target = self.function_name(*module, *declaration);
        let arguments = self.arguments(function, (*module, *declaration), signature, arguments);
        if arguments.is_empty() {
            format!("_harness.evaluate({target})")
        } else {
            format!("_harness.evaluate({target}, {arguments})")
        }
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
