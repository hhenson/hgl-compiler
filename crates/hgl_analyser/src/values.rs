//! Types the bodies of value functions, nodes, sources, graphs and
//! tests.
//!
//! `read` says what each written type means and `declared` checks the
//! declarations against one another. This pass reads what a body does:
//! it gives every expression of the body a type, checks each against
//! the type its place requires, and writes the body down as
//! [`hir::Body`] for the stages after the analyser.
//!
//! It is built up form by form. It reads, among the rest:
//!
//! * literals, each with the type its form gives it (TYPE-1, TYPE-2,
//!   LEX-11);
//! * the names of parameters, locals and generic value parameters;
//! * `-`, `!` and the operators with two operands, by core's numeric
//!   contract (NUM-1, NUM-6);
//! * tuples, and sequences of values;
//! * `let`, `var`, assignment and `return` (EXPR-3, EXPR-5, FN-7);
//! * calls of value functions and constructions of structs, each
//!   argument given to its parameter or field (EXPR-2, TYPE-5);
//! * members of enums, fields of struct values, and the elements of
//!   lists and dictionaries (TYPE-7, EXPR-7, ERR-6);
//! * blocks, `if`, `switch`, `for` and `while` (CTRL-10, CTRL-12), and
//!   in a graph what each composes (CTRL-13 to CTRL-15);
//! * a node's hooks, what each admits, and what a node publishes
//!   (TIME-3, TIME-7);
//! * a test's `assert`, `eval` and `run` (TST-3, TST-4, TST-17).
//!
//! A form it does not read yet is left [`ExprKind::Unread`], with no
//! type and no report: configuration given to `!`, `&&` or `||`, an
//! anonymous function's body, and a conversion written with a type's
//! name, `str(value)`. A type that is not known is reported once, where
//! it was found, and never again where it is used (ERR-9).

use std::collections::HashMap;

use hgl_diagnostics::{Category, Code, Diagnostic, Diagnostics, Location};
use hgl_lexer::literal::{decode_integer, decode_string};
use hgl_parser::{HasSpan, SyntaxKind, SyntaxNode, SyntaxToken};
use hgl_source::{FileId, Span};

use crate::constant::{Fault, Scope, Typed};
use crate::declared::{agree, is_optional, names_one_of, names_parameter, substitute_within};
use crate::hir::{
    self, BinaryOperator, Body, Callee, EndpointOperation, ErrorPart, Expr, ExprKind, Form, Hook,
    Local, LocalId, Run, Stmt, UnaryOperator, Value, ValueCategory,
};
use crate::program::{
    Binding, Declaration, DeclarationKind, LocalKind, Module, OperatorRef, Phase, Program, Role,
    StoreStatement,
};
use crate::read::{default_of, is_type, parameters};
use crate::syntax::{child, has_token, name_token, tokens, ungrouped};
use crate::types::{
    Argument, CallArguments, Constant, ConstantValue, Detail, GenericKind, GenericParameter,
    NamedKind, Pack, Scalar, Signature, StructType, TraceStyle, Type,
};
use crate::{RESULT_FORMS, codes, error, local_error, warning};

// The read of a collection's child, and the endpoint a child is (TIME-1).
mod children;

/// Types the body of every value function of the program, filling in
/// [`Module::bodies`].
pub(crate) fn check(program: &mut Program, diagnostics: &mut Diagnostics) {
    // The signature of what has none: a test, a run statement, a
    // constant.
    let empty = Signature {
        generics: Vec::new(),
        parameters: Vec::new(),
        result: None,
    };
    // UNIT-13: the value each constant is written with, read as a value
    // of the constant's type with no parameter in scope. Every module's
    // are read before any body, which may read one as a `switch` label
    // does (CTRL-6: "compared by resolved value").
    for index in 0..program.modules.len() {
        let module = &program.modules[index];
        let (bindings, written, calls) = tables(module);
        let found = Tables {
            bindings: &bindings,
            written: &written,
            calls: &calls,
        };
        let constants: Vec<Option<Expr>> = module
            .declarations
            .iter()
            .enumerate()
            .map(|(position, declaration)| {
                let Some(Detail::Constant(ty)) = module.details.get(position) else {
                    return None;
                };
                Typer::new(program, module, declaration, &empty, found, diagnostics).constant(ty)
            })
            .collect();
        program.modules[index].constants = constants;
    }
    for index in 0..program.modules.len() {
        let module = &program.modules[index];
        let (bindings, written, calls) = tables(module);
        let found = Tables {
            bindings: &bindings,
            written: &written,
            calls: &calls,
        };
        let mut bodies = Vec::with_capacity(module.declarations.len());
        for (position, declaration) in module.declarations.iter().enumerate() {
            let body = match (&declaration.kind, module.details.get(position)) {
                // FN-1: a `const fn`. GEN-4: "an `impl const fn` a value
                // candidate", whose body is a value function's.
                (
                    DeclarationKind::Function(Role::Value)
                    | DeclarationKind::Implementation(Role::Value, _),
                    Some(Detail::Callable(signature)),
                ) => Typer::new(program, module, declaration, signature, found, diagnostics).body(),
                // FN-2: a node, whose hooks are read as value code
                // (FN-3: "In a hook a temporal parameter is a value of
                // its declared type, read as in a value function"). A
                // source is a node of another shape (SOURCE-1), which
                // the phase pass holds it to.
                (
                    DeclarationKind::Function(Role::Temporal)
                    | DeclarationKind::Implementation(Role::Temporal, _),
                    Some(Detail::Callable(signature)),
                ) if module.phases.get(position) == Some(&Some(Phase::Node)) => {
                    let typer =
                        Typer::new(program, module, declaration, signature, found, diagnostics);
                    if crate::phases::is_source(declaration) {
                        typer.source()
                    } else {
                        typer.node()
                    }
                }
                // FN-2: a graph, whose body describes composition.
                (
                    DeclarationKind::Function(Role::Temporal)
                    | DeclarationKind::Implementation(Role::Temporal, _),
                    Some(Detail::Callable(signature)),
                ) if module.phases.get(position) == Some(&Some(Phase::Graph)) => {
                    Typer::new(program, module, declaration, signature, found, diagnostics).graph()
                }
                // A declaration with no body to read writes types too:
                // a struct's fields, an operator's parameters, a
                // constant's type. Each is held to the `requires` of
                // the structs it applies (GEN-3), which reading the
                // declaration does. A struct's own generic parameters
                // and its `requires` stand as a function's do (GEN-7).
                (_, detail) => {
                    let of_struct;
                    let signature = match detail {
                        Some(Detail::Callable(signature)) => signature,
                        Some(Detail::Struct(read)) => {
                            of_struct = Signature::of_struct(read);
                            &of_struct
                        }
                        _ => &empty,
                    };
                    Typer::new(program, module, declaration, signature, found, diagnostics);
                    None
                }
            };
            bodies.push(body);
        }
        // TST-3: "A test is a composition-phase block with assertions."
        // It is no declaration: it has a name, and neither a parameter
        // nor a result, so it is read as a graph of that name with an
        // empty signature.
        let mut test_bodies = Vec::with_capacity(module.tests.len());
        for test in &module.tests {
            let stand_in =
                Declaration::stand_in(&test.name, test.at, test.file, &test.syntax, true);
            let body = Typer::new(program, module, &stand_in, &empty, found, diagnostics)
                .test(test.integration);
            test_bodies.push(body);
        }
        // RUN-1: an application's run statement. It is no declaration
        // either, and is read as a graph with no parameter is.
        let run_body = module.run.as_ref().and_then(|run| {
            let stand_in = Declaration::stand_in("run", run.at, run.file, &run.syntax, false);
            Typer::new(program, module, &stand_in, &empty, found, diagnostics).run_statement()
        });
        // UNIT-14: the statements an application sets its run's store
        // with are no declaration either. They are read together, as
        // the statements of one block are.
        let store_body = module.store.first().and_then(|first| {
            let stand_in =
                Declaration::stand_in("store", first.at, first.file, &first.syntax, false);
            Typer::new(program, module, &stand_in, &empty, found, diagnostics).store(&module.store)
        });
        // UNIT-8: "Defaults remain required-constant/cold expressions
        // checked against the expected type. Any parameter but a pack
        // may have one: a value function's, const configuration, or a
        // temporal input." Each is read as a value of its parameter's
        // type, whatever its declaration is: a function, a candidate,
        // an operator, or the contract of a native one.
        let mut defaults = Vec::with_capacity(module.declarations.len());
        for (position, declaration) in module.declarations.iter().enumerate() {
            let read = match module.details.get(position) {
                Some(Detail::Callable(signature))
                    if signature
                        .parameters
                        .iter()
                        .any(|parameter| parameter.has_default) =>
                {
                    Typer::new(program, module, declaration, signature, found, diagnostics)
                        .defaults()
                }
                Some(Detail::Callable(signature)) => vec![None; signature.parameters.len()],
                // TYPE-4: a field's default is read as a value of the
                // field's type, as a parameter's is.
                Some(Detail::Struct(read)) if read.fields.iter().any(|field| field.has_default) => {
                    // A struct has no parameter and no result: what a
                    // default may name of it is its generic parameters
                    // (UNIT-7: "with its generic environment").
                    let signature = Signature::of_struct(read);
                    Typer::new(program, module, declaration, &signature, found, diagnostics)
                        .field_defaults(read)
                }
                Some(Detail::Struct(read)) => vec![None; read.fields.len()],
                _ => Vec::new(),
            };
            defaults.push(read);
        }
        program.modules[index].bodies = bodies;
        program.modules[index].test_bodies = test_bodies;
        program.modules[index].run_body = run_body;
        program.modules[index].store_body = store_body;
        program.modules[index].defaults = defaults;
    }
}

/// What the earlier passes found of `module`, by where each is written:
/// what each name resolved to, what each written type means, and the
/// generic arguments each call writes ([`Tables`]).
#[allow(clippy::type_complexity)]
fn tables(
    module: &Module,
) -> (
    HashMap<(FileId, u32), &Binding>,
    HashMap<(FileId, u32), &Type>,
    HashMap<(FileId, u32), &CallArguments>,
) {
    let bindings = module
        .uses
        .iter()
        .map(|used| ((used.at.file, used.at.span.start), &used.binding))
        .collect();
    let written = module
        .written
        .iter()
        .map(|written| ((written.at.file, written.at.span.start), &written.ty))
        .collect();
    let calls = module
        .call_arguments
        .iter()
        .map(|call| ((call.at.file, call.at.span.start), call))
        .collect();
    (bindings, written, calls)
}

/// The call an eval or a run statement takes (TST-5, RUN-1).
#[derive(Clone)]
struct Called<'a> {
    /// The name of what is called.
    name: SyntaxToken,
    /// The call's argument list.
    arguments: SyntaxNode,
    /// The generic arguments it writes after the name, when it writes
    /// them (GEN-8).
    generics: Option<&'a CallArguments>,
}

/// What a finished call selected, looked up once for the checks
/// `finished_call` runs ([`Callee::declaration`]).
struct Selected<'e> {
    /// The position of the function's or the candidate's module in
    /// `Program::modules`.
    module: usize,
    /// Its position in that module's `declarations`.
    declaration: usize,
    /// What each of its generic parameters stands for at the call.
    generics: &'e [crate::types::Argument],
    /// Its signature; `None` where what was read of it is none.
    signature: Option<&'e Signature>,
    /// The call's arguments, each with the parameter it is given to.
    arguments: &'e [hir::Argument],
}

/// What the earlier passes found of a module, each by the file and
/// offset it is written at. It holds references only, so it is copied
/// to each `Typer` (`Copy`).
#[derive(Clone, Copy)]
struct Tables<'a> {
    /// What each name resolved to (UNIT-10).
    bindings: &'a HashMap<(FileId, u32), &'a Binding>,
    /// What each written type means.
    written: &'a HashMap<(FileId, u32), &'a Type>,
    /// The generic arguments each call writes (GEN-8).
    calls: &'a HashMap<(FileId, u32), &'a CallArguments>,
}

/// Reads one value function.
struct Typer<'a> {
    program: &'a Program,
    /// The module the function is declared in.
    module: &'a Module,
    /// The file the function is written in.
    file: FileId,
    declaration: &'a Declaration,
    signature: &'a Signature,
    bindings: &'a HashMap<(FileId, u32), &'a Binding>,
    written: &'a HashMap<(FileId, u32), &'a Type>,
    /// The generic arguments each call writes, by where its callee
    /// starts (GEN-8).
    calls: &'a HashMap<(FileId, u32), &'a CallArguments>,
    /// The parameters and locals found so far.
    locals: Vec<Local>,
    /// The local each declaration is, by where its name is declared.
    declared: HashMap<Location, LocalId>,
    /// For each local, whether every path to here has given it a value
    /// (EXPR-3).
    assigned: Vec<bool>,
    /// How many more expressions of the function may be read a second
    /// time: see [`REREADS`].
    rereads: u32,
    /// True when every path to here has returned from the function
    /// (CTRL-5).
    returned: bool,
    /// True once a statement this pass does not read has been met: it
    /// may return, so the function is not said to end with no value.
    unsure: bool,
    /// True while a graph's body is read: composition, where a
    /// temporal parameter is a connection and no payload is read
    /// (FN-3).
    graph: bool,
    /// Inside a branch of an `if` whose condition is a connection, how
    /// many locals there were where the nearest such `if` began: a
    /// `var` before that is declared outside it (CTRL-14).
    temporal: Option<usize>,
    /// True once a `return` with a value stands in a branch selected at
    /// runtime: the graph's result is then the continuation's where the
    /// branch is not selected, so every path returns (CTRL-14).
    must_return: bool,
    /// Inside a composition loop that makes one child for each member,
    /// how many locals there were where the outermost such loop began:
    /// a `var` before that is declared outside it (CTRL-15).
    dynamic: Option<usize>,
    /// The hook of a node being read; `None` in a value function, and
    /// between a node's hooks.
    hook: Option<Hook>,
    /// The local that is a node's injected `out`, its result (TIME-7).
    out: Option<LocalId>,
    /// What tests have proved where the pass is reading: a binding is
    /// not null (ERR-12), or a temporal input is valid (TIME-4). A
    /// binding that is refined is immutable, and an input's validity
    /// does not change within an evaluation, so what is proved holds to
    /// the end of the block it is proved in: the list grows as tests
    /// are met, and is cut back to where it stood when a branch or a
    /// block ends.
    present: Vec<Fact>,
    /// The locals a capability has been bound to, each with the
    /// capability it is: `let bound = clock` (TIME-8: "Binding one to a
    /// `let` ... [is a use]"). The local is then the capability under
    /// another name, and is held to the same rule, whether or not core
    /// has given the capability a type.
    aliases: HashMap<LocalId, LocalId>,
    /// True while a named test's block is read (TST-3): there `assert`
    /// is a statement, and what the test may not hold is reported by the
    /// phase pass and not read.
    testing: bool,
    /// True while the block read is an integration test's (TST-17):
    /// there `run(...)` is written beside `eval(...)`.
    integration: bool,
    /// True while the statements of a source that yield are read
    /// (SOURCE-1): there a `yield` is a statement, a `return` with no
    /// value ends the source, and a `while` may have no condition.
    generating: bool,
    /// How many blocks and `if`s whose value is used are being read
    /// around what is read now: a `yield` there stands inside a value
    /// expression (SOURCE-4).
    valued: u32,
    /// The locals a capture is bound to, each with what its `.value`
    /// is (ERR-7: "What it gives is bound with `let` and read through
    /// two connections").
    captures: HashMap<LocalId, CapturedOutput>,
    /// The argument of the `capture` being read that names its
    /// callable: `required` in `capture(required, value)`. The call it
    /// names is read from the same list, less this argument.
    callable: Option<SyntaxNode>,
    /// True for the one call an `eval` stands for, while its arguments
    /// are given to its parameters (TST-5): a temporal parameter there
    /// takes a trace. The call takes it when it starts, so that what
    /// its arguments call is read as any call.
    evaluating: bool,
    /// True once `start` has used the node's scheduler or alarm, which
    /// may then wake it (TIME-3).
    driven: bool,
    /// Where the node writes `modified([])`, which subscribes to no
    /// input: the node's scheduler or alarm is what wakes the handler
    /// (TIME-3). The hooks may be written in any order, so each is
    /// judged when the whole node has been read.
    undriven: Vec<Location>,
    /// True while a `state`'s initializer is read (TIME-5).
    initializing: bool,
    /// True while a node's `cache`'s initializer is read (TIME-1).
    caching: bool,
    /// Where the temporal input stands whose child is being named as
    /// an endpoint, `parts` in `valid(parts[1])`: the child is given to
    /// the operation and not read (TIME-1, TIME-9).
    naming: Option<Location>,
    /// How many requirements deep a candidate's condition is being
    /// tested: see [`REQUIRING`].
    requiring: u32,
    /// The operator of core's module each symbol met so far stands for,
    /// by the operator's name; `None` for one the program has not.
    identities: HashMap<&'static str, Option<OperatorRef>>,
    /// The value candidates of each operator called so far, by the
    /// operator's module and declaration. A chain of symbols asks for
    /// the same operator's at every link.
    candidates: HashMap<(usize, usize), Vec<Candidate<'a>>>,
    /// The values of the module constants this function's constants
    /// have named (UNIT-13): a constant named twice is read once.
    constants: crate::constant::Constants,
    diagnostics: &'a mut Diagnostics,
}

/// How many expressions of one function are read a second time: as the
/// type their place turns out to have, or to try them as a candidate's
/// parameter (GEN-5). An expression read again may hold others that
/// are, so the count is for the whole function: past it an expression
/// keeps its first reading, a call that needs a trial is not judged,
/// and the pass ends whatever it is given.
const REREADS: u32 = 1_000;

/// How deep a requirement's test may go. Testing `requires add_(T, T)`
/// tries the candidates of `add_`, one of which may state a requirement
/// of its own, and so on; candidates that require each other would
/// never end. Past the bound a requirement is not told to hold or not,
/// and the call is not judged.
const REQUIRING: u32 = 8;

impl<'a> Typer<'a> {
    fn new(
        program: &'a Program,
        module: &'a Module,
        declaration: &'a Declaration,
        signature: &'a Signature,
        tables: Tables<'a>,
        diagnostics: &'a mut Diagnostics,
    ) -> Typer<'a> {
        let mut typer = Typer {
            program,
            module,
            file: module.files[declaration.file].file,
            declaration,
            signature,
            bindings: tables.bindings,
            written: tables.written,
            calls: tables.calls,
            locals: Vec::new(),
            declared: HashMap::new(),
            assigned: Vec::new(),
            rereads: REREADS,
            returned: false,
            unsure: false,
            graph: false,
            temporal: None,
            must_return: false,
            dynamic: None,
            hook: None,
            out: None,
            present: Vec::new(),
            aliases: HashMap::new(),
            testing: false,
            integration: false,
            generating: false,
            valued: 0,
            captures: HashMap::new(),
            callable: None,
            evaluating: false,
            driven: false,
            undriven: Vec::new(),
            initializing: false,
            caching: false,
            naming: None,
            requiring: 0,
            identities: HashMap::new(),
            candidates: HashMap::new(),
            constants: Default::default(),
            diagnostics,
        };
        for parameter in &signature.parameters {
            // What a pack holds is for the pass that reads calls.
            let ty = match parameter.pack {
                Some(_) => Type::Unknown,
                None => parameter.ty.clone(),
            };
            // FN-15: "In the function the parameter is a `var`, whose
            // first value is the place's".
            let kind = if parameter.var {
                LocalKind::Var
            } else {
                LocalKind::Parameter
            };
            typer.declare(&parameter.name, parameter.at, kind, ty, true);
        }
        typer.applications();
        typer
    }

    /// GEN-3: each type written in the declaration being read is held
    /// to the `requires` of the structs it applies, at any depth.
    ///
    /// The type is read as it is written. GEN-2: "A struct application
    /// supplies all or none". Where it writes generic arguments each is
    /// read where it stands, as a call's is; where it writes none each
    /// parameter has its default. An argument that is written is never
    /// taken for the default.
    fn applications(&mut self) {
        let within = self.declaration.syntax.span();
        let module = self.module;
        let file = self.file;
        let program = self.program;
        let written = module.written.iter().filter(|written| {
            written.at.file == file
                && written.at.span.start >= within.start
                && written.at.span.end <= within.end
        });
        for written in written {
            let mut found = Vec::new();
            struct_applications(&written.ty, &mut found);
            if !found
                .iter()
                .any(|(struct_, _)| required(program, *struct_).is_some())
            {
                continue;
            }
            // Each argument is held as the value it is (`crate::constant`),
            // however the type writes it, or as the default it takes.
            for (struct_, arguments) in &found {
                self.requires_met(written.at, *struct_, arguments);
            }
        }
    }

    fn at(&self, node: &SyntaxNode) -> Location {
        Location::new(self.file, node.span())
    }

    /// The expression of the declaration being read that `expression`
    /// was read from, as written: the outermost node at its place.
    fn syntax_of(&self, expression: &Expr) -> Option<SyntaxNode> {
        self.node_at(expression.at)
    }

    /// The outermost node of the declaration being read written at `at`.
    fn node_at(&self, at: Location) -> Option<SyntaxNode> {
        if at.file != self.file {
            return None;
        }
        self.declaration
            .syntax
            .descendants()
            .find(|node| node.span() == at.span)
    }

    /// Adds a local. `assigned` says whether it has a value from the
    /// start.
    fn declare(
        &mut self,
        name: &str,
        at: Location,
        kind: LocalKind,
        ty: Type,
        assigned: bool,
    ) -> LocalId {
        let local = self.locals.len();
        self.locals.push(Local {
            name: name.to_string(),
            at,
            kind,
            ty,
            category: ValueCategory::Value,
        });
        self.assigned.push(assigned);
        self.declared.insert(at, local);
        local
    }

    /// The body, or `None` for a function with none written.
    ///
    /// FN-7: "A body that ends in an expression returns that expression,
    /// as an arrow body does: `{ value }` and `=> value` are one body.
    /// In a function that declares no result the expression is a
    /// statement and its value is dropped ... A `const fn` that declares
    /// a result ends every path in `return value` or in its body's
    /// value: one that can reach its end with no value rejects."
    /// The body of a graph: composition (FN-2, FN-3).
    ///
    /// "There a temporal parameter, and the result of a temporal or
    /// lifted call, is a connection: the argument of a call, the
    /// operand of a symbol, a condition, a bound local, or what is
    /// returned." A `const` parameter is an ordinary value, known while
    /// the graph is built.
    ///
    /// Read so far: locals, calls and symbols, and what is returned. A
    /// conditional and a loop in composition are for a later part
    /// (CTRL-1 to CTRL-8), and are not read.
    fn graph(mut self) -> Option<Body> {
        self.graph = true;
        for (local, parameter) in self.signature.parameters.iter().enumerate() {
            self.locals[local].category = if parameter.constant {
                ValueCategory::Value
            } else {
                ValueCategory::Connection
            };
        }
        self.body()
    }

    /// The body of a node: its declarations and then its hooks (FN-2).
    /// A statement that is neither has been reported by `phases`, and
    /// is not read.
    fn node(mut self) -> Option<Body> {
        let block = child(&self.declaration.syntax, SyntaxKind::Block)?;
        let mut statements = Vec::new();
        for statement in block.children() {
            match statement.kind() {
                SyntaxKind::StateDecl | SyntaxKind::CacheDecl => {
                    // TIME-5: "State initializers cannot read current
                    // temporal inputs." TIME-1: nor can a cache's.
                    self.initializing = statement.kind() == SyntaxKind::StateDecl;
                    self.caching = statement.kind() == SyntaxKind::CacheDecl;
                    statements.push(self.local(&statement));
                    self.initializing = false;
                    self.caching = false;
                }
                SyntaxKind::InjectDecl => statements.extend(self.inject(&statement)),
                SyntaxKind::LifecycleBlock | SyntaxKind::WhenStmt => {
                    statements.push(self.hook(&statement));
                }
                _ => statements.push(self.unread(&statement)),
            }
        }
        // TIME-3: "`modified([])` subscribes to no input: the node's
        // scheduler or alarm is then what wakes the handler. Where the
        // node injects neither, or its `start` does not use the one it
        // injects, nothing wakes it: the node is dormant, and the
        // selector is warned of (`node.no_activation`, ERR-10) ... A
        // node that is dormant by design acknowledges the warning."
        if !self.driven && !self.acknowledges(codes::NODE_NO_ACTIVATION) {
            for selector in std::mem::take(&mut self.undriven) {
                self.diagnostics.emit(
                    warning(
                        Category::Phase,
                        codes::NODE_NO_ACTIVATION,
                        selector,
                        format!(
                            "`modified([])` subscribes to no input, and nothing else wakes \
                             `{}`: the node is dormant",
                            self.declaration.name
                        ),
                        "TIME-3",
                    )
                    .with_note(
                        "a scheduler or an alarm the node injects and uses in `start` wakes \
                         it: `start { schedule(scheduler, delay) }`",
                    )
                    .with_note(
                        "a node that is dormant by design says so: \
                         `@suppress_warning(\"node.no_activation\")` before the function",
                    ),
                );
            }
        }
        Some(Body {
            locals: self.locals,
            statements,
        })
    }

    /// The value a constant is written with, read as a value of
    /// `expected`, the type it is written with (UNIT-13): "Its
    /// expression is a const expression (EXPR-5) checked against T ...
    /// It reads no parameter, local, temporal value, or capability."
    /// The typer is made with no parameter, so none is in scope.
    fn constant(mut self, expected: &Type) -> Option<Expr> {
        let syntax = self.declaration.syntax.clone();
        // The expression is what follows the type: the one child that
        // is no type.
        let ty = crate::read::type_child(&syntax);
        let written = syntax.children().find(|child| Some(child) != ty.as_ref())?;
        // A type that is no ordinary value type has been reported
        // (TYPE-12), and the expression is then held to none.
        let expected = if shaped(expected) {
            &Type::Unknown
        } else {
            expected
        };
        let value = self.expression(&written, Some(expected));
        let place = format!("the constant `{}`", self.declaration.name);
        Some(self.fit(value, expected, &place))
    }

    /// The default of each parameter of the declaration (UNIT-8), read
    /// as a value of the parameter's type; `None` for a parameter with
    /// none.
    ///
    /// A default is cold: it reads no parameter and no local (UNIT-7,
    /// which the resolver holds it to), so it is read here apart from
    /// the body. A parameter whose type names a generic parameter has a
    /// type only a call gives, and its default is not held to one;
    /// neither is the default of a temporal shape that is no ordinary
    /// value type.
    fn defaults(mut self) -> Vec<Option<Expr>> {
        let signature = self.signature;
        let syntax = self.declaration.syntax.clone();
        signature
            .parameters
            .iter()
            .map(|parameter| {
                if !parameter.has_default {
                    return None;
                }
                let written = default_at(&syntax, parameter)?;
                // "An omitted temporal argument is wired as a constant
                // of its default, so the default is a value a constant
                // of the parameter's type can publish." What that is
                // for a window, an atomic or a reference is not settled
                // here: `window: rolling_window<f64, 5> = []` is read, and
                // held to no type.
                let expected = if shaped(&parameter.ty) || names_parameter(&parameter.ty) {
                    Type::Unknown
                } else {
                    parameter.ty.clone()
                };
                let value = self.expression(&written, Some(&expected));
                let place = format!("the parameter `{}`", parameter.name);
                // TYPE-12: "A `signal` input takes no default but
                // `null`": it has no payload a constant could supply.
                let signal = matches!(parameter.ty.omission_removed(), Type::Signal);
                if signal && !matches!(value.kind, ExprKind::Literal(Value::Null)) {
                    self.diagnostics.emit(error(
                        Category::Type,
                        codes::TYPE_MISMATCH,
                        value.at,
                        format!("{place} is a `signal`, which takes no default but `null`"),
                        "TYPE-12",
                    ));
                    return Some(value);
                }
                Some(self.fit(value, &expected, &place))
            })
            .collect()
    }

    /// The default of each field the struct writes, read as a value of
    /// the field's type; `None` for a field with none. `read` is the
    /// struct the typer was made for.
    ///
    /// TYPE-4: "A typed field introduction fixes its type and
    /// optionality ... An untyped inherited override changes only the
    /// construction default of a field a parent has." So `name = value`
    /// is read against the type the struct has that field with, the
    /// parent's arguments in place. UNIT-8: "Defaults remain
    /// required-constant/cold expressions checked against the expected
    /// type."
    ///
    /// A default is cold, as a parameter's is: it reads no field of the
    /// struct (UNIT-7, which the resolver holds it to). A field whose
    /// type names a generic parameter of the struct has a type only a
    /// construction gives, and its default is not held to one; neither
    /// is the default of an `atomic<...>` field, as a parameter's of
    /// that shape is not.
    fn field_defaults(mut self, read: &StructType) -> Vec<Option<Expr>> {
        let syntax = self.declaration.syntax.clone();
        let own: Vec<Location> = read.generics.iter().map(|generic| generic.at).collect();
        read.fields
            .iter()
            .map(|field| {
                if !field.has_default {
                    return None;
                }
                // The member that writes the field: its name is its
                // first token.
                let written = syntax
                    .children()
                    .filter(|member| member.kind() == SyntaxKind::StructMember)
                    .find(|member| {
                        tokens(member)
                            .next()
                            .is_some_and(|name| name.span().start == field.at.span.start)
                    })
                    .and_then(|member| default_of(&member))?;
                // The type the struct has the field with: the one
                // written, or for `name = value` the parent's.
                let declared = match &field.ty {
                    Some(ty) => Some(ty),
                    None => read
                        .members
                        .iter()
                        .find(|member| member.name == field.name)
                        .map(|member| &member.ty),
                };
                let ty = declared.map_or(Type::Unknown, |ty| placed(ty.clone(), &own));
                // "Only an optional field's default may be null": a
                // `null` on any other field is reported as that
                // (`field.null_default`), and not again as no value of
                // the field's type.
                let judged = field.null_default && !matches!(ty, Type::Nullable(_));
                let expected = if shaped(&ty) || judged {
                    Type::Unknown
                } else {
                    ty
                };
                let value = self.expression(&written, Some(&expected));
                let place = format!("the field `{}`", field.name);
                Some(self.fit(value, &expected, &place))
            })
            .collect()
    }

    /// The block of a named test (TST-3): "A test is a
    /// composition-phase block with assertions." It is read as a
    /// graph's body is, with `assert` among its statements.
    fn test(mut self, integration: bool) -> Option<Body> {
        let block = child(&self.declaration.syntax, SyntaxKind::Block)?;
        self.graph = true;
        self.testing = true;
        self.integration = integration;
        let statements = block
            .children()
            .map(|statement| self.statement(&statement))
            .collect();
        Some(Body {
            locals: self.locals,
            statements,
        })
    }

    /// The statements an application sets its run's store with
    /// (UNIT-14): "`inject global_state`, and then calls of `put` on
    /// it. Each is evaluated once, in the order written, before the
    /// run's graph is built, and its value is read as a constant's is
    /// (UNIT-13), with no parameter in scope. Any other statement
    /// there rejects (`application.statement`)."
    ///
    /// They are read as the statements of a node's `start` are, which
    /// is where a node uses a capability before its graph runs: with
    /// no parameter and no input, each is value code.
    fn store(mut self, written: &[StoreStatement]) -> Option<Body> {
        self.hook = Some(Hook::Start);
        let mut statements = Vec::new();
        for store in written {
            let Some(node) = store.syntax.children().next() else {
                continue;
            };
            if node.kind() == SyntaxKind::InjectDecl {
                // "An application injects `global_state` and no other"
                // (FN-9).
                for name in tokens(&node).filter(|token| token.kind() == SyntaxKind::Ident) {
                    if name.text() != "global_state" {
                        self.diagnostics.emit(error(
                            Category::Phase,
                            codes::APPLICATION_STATEMENT,
                            Location::new(self.file, name.span()),
                            format!(
                                "an application injects `global_state` alone, to set its \
                                 run's store, and not `{}`",
                                name.text()
                            ),
                            "UNIT-14",
                        ));
                    }
                }
                statements.extend(self.inject(&node));
                continue;
            }
            let reported = self.diagnostics.len();
            let statement = self.statement(&node);
            // A call that is itself rejected is not judged again
            // (ERR-9).
            if self.diagnostics.len() == reported && !self.puts_to_the_store(&statement) {
                self.diagnostics.emit(
                    error(
                        Category::Phase,
                        codes::APPLICATION_STATEMENT,
                        store.at,
                        "an application's statement before its run sets the run's store, and \
                         this is no `put` on it"
                            .to_string(),
                        "UNIT-14",
                    )
                    .with_note("`inject global_state`, and then `put(global_state, key, value)`"),
                );
            } else if self.diagnostics.len() == reported {
                // "its value is read as a constant's is (UNIT-13)": the
                // store is `put`'s own argument, and is read by nothing
                // the call is given. A value that reads the store,
                // `get<i64>(global_state, "limit") + 1`, is no
                // constant's.
                for at in self.reads_of_the_store(&statement) {
                    self.diagnostics.emit(
                        error(
                            Category::Phase,
                            codes::APPLICATION_STATEMENT,
                            at,
                            "what a store statement puts is read as a constant's value is, \
                             and reads no capability: the store is `put`'s own argument"
                                .to_string(),
                            "UNIT-14",
                        )
                        .with_note(
                            "a constant, or a call of a `const fn` on constants, is such a value",
                        ),
                    );
                }
            }
            statements.push(statement);
        }
        Some(Body {
            locals: self.locals,
            statements,
        })
    }

    /// Where a `put` on the store reads the store again, in what it is
    /// given beside the store itself: each use of the injected
    /// capability inside its key or its value.
    fn reads_of_the_store(&self, statement: &Stmt) -> Vec<Location> {
        let Stmt::Expr(Expr {
            kind: ExprKind::Call { arguments, .. },
            ..
        }) = statement
        else {
            return Vec::new();
        };
        let mut reads = Vec::new();
        for argument in arguments {
            // The store itself: `put`'s first parameter.
            if argument.parameter == Some(0) {
                continue;
            }
            argument.value.walk(&mut |part| {
                if let ExprKind::Local(local) = &part.kind
                    && self.locals[*local].kind == LocalKind::Inject
                {
                    reads.push(part.at);
                }
            });
        }
        reads
    }

    /// True for a statement that is a call of core's `put` on the
    /// store the unit injects: `put(global_state, "limit", 3)`.
    fn puts_to_the_store(&self, statement: &Stmt) -> bool {
        let Stmt::Expr(Expr {
            kind: ExprKind::Call {
                callee, arguments, ..
            },
            ..
        }) = statement
        else {
            return false;
        };
        let Some((module, declaration)) = callee.declaration() else {
            return false;
        };
        let put = Some(module) == self.program.core()
            && self.program.modules[module].declarations[declaration].name == "put";
        // The store is `put`'s first parameter, wherever its argument
        // is written: `put(key: "limit", store: global_state, value: 3)`.
        let on_store = arguments
            .iter()
            .find(|argument| argument.parameter == Some(0))
            .is_some_and(|argument| {
                matches!(
                    &argument.value.kind,
                    ExprKind::Local(local)
                        if self.locals[*local].kind == LocalKind::Inject
                            && self.locals[*local].name == "global_state"
                )
            });
        put && on_store
    }

    /// `assert condition`, or `assert raises("code") { ... }` (TST-4,
    /// TST-10). TST-4: "Assert requires a wiring-time bool."
    fn assertion(&mut self, node: &SyntaxNode) -> Stmt {
        let at = self.at(node);
        let Some(asserted) = node.children().next() else {
            return self.unread(node);
        };
        if asserted.kind() == SyntaxKind::RaisesAssertion {
            // TST-11: "The block has ordinary lexical test scope". Its
            // code is the phase pass's to check: it is read here as it
            // is written, a string literal between the parentheses.
            let code = asserted
                .children()
                .find(|part| part.kind() == SyntaxKind::Literal)
                .and_then(|literal| tokens(&literal).next())
                .filter(|token| token.kind() == SyntaxKind::Str)
                .and_then(|token| decode_string(token.text()).ok());
            let body = self.loop_body(child(&asserted, SyntaxKind::Block).as_ref());
            return Stmt::AssertRaises { code, body, at };
        }
        let condition = self.expression(&asserted, Some(&BOOL));
        // What is not known at all has been reported, or is for a later
        // part: an `eval`, and so a comparison with one (ERR-9). A type
        // known in part is known to be no `bool`: the list of `[]`.
        if !open(&condition.ty) && condition.ty != BOOL {
            self.diagnostics.emit(error(
                Category::Type,
                codes::TYPE_MISMATCH,
                condition.at,
                format!("an `assert` takes a `bool`, and this is `{}`", condition.ty),
                "TST-4",
            ));
        } else if self.category(&condition) == ValueCategory::Connection {
            // "Wiring-time": known when the test's graph is built, and
            // no connection, whose value is known only as the graph
            // runs.
            self.diagnostics.emit(
                error(
                    Category::Phase,
                    codes::CONNECTION_AS_CONSTANT,
                    condition.at,
                    "an `assert` takes a `bool` known when the test is built, and this is a \
                     connection"
                        .to_string(),
                    "TST-4",
                )
                .with_note("what a function publishes is observed with `eval`"),
            );
        }
        Stmt::Assert { condition, at }
    }

    /// The body of a source (SOURCE-1): "a node that produces its
    /// output and has no temporal parameters ... It declares `cache`
    /// and `state` (TIME-6), which are what it keeps from one `yield` to
    /// the next ... A source with no declaration and no hook has a body
    /// of statements, `yield` among them. One with an `inject`, a
    /// `cache`, a `state`, a `start`, or a `stop` has its declarations,
    /// then `start`, then one block that holds every `yield`, then
    /// `stop`".
    ///
    /// Its declarations and its hooks are read as a node's are. Every
    /// other statement is one that yields, or stands with those that
    /// do: it is read as value code is (FN-3: "A runtime body calls only
    /// value functions"). Which statements may stand where is the phase
    /// pass's to say.
    fn source(mut self) -> Option<Body> {
        let block = child(&self.declaration.syntax, SyntaxKind::Block)?;
        let mut statements = Vec::new();
        for statement in block.children() {
            match statement.kind() {
                SyntaxKind::StateDecl | SyntaxKind::CacheDecl => {
                    self.initializing = statement.kind() == SyntaxKind::StateDecl;
                    statements.push(self.local(&statement));
                    self.initializing = false;
                }
                SyntaxKind::InjectDecl => statements.extend(self.inject(&statement)),
                SyntaxKind::LifecycleBlock => statements.push(self.hook(&statement)),
                // "A source has no `when`": it is reported, and not
                // read.
                SyntaxKind::WhenStmt => statements.push(self.unread(&statement)),
                _ => {
                    // Each pass of the source begins here: nothing has
                    // returned.
                    self.returned = false;
                    self.generating = true;
                    statements.push(self.statement(&statement));
                    self.generating = false;
                }
            }
        }
        Some(Body {
            locals: self.locals,
            statements,
        })
    }

    /// `yield time: value` (SOURCE-1): it "writes its output and sets
    /// its next evaluation". The value is of the source's result type.
    /// What the time may be is proposed (SOURCE-2), and is not judged.
    fn yield_statement(&mut self, node: &SyntaxNode) -> Stmt {
        let at = self.at(node);
        let mut parts = node.children();
        let (Some(time), Some(value)) = (parts.next(), parts.next()) else {
            return self.unread(node);
        };
        let time = self.expression(&time, None);
        let result = self.signature.result.clone();
        let value = match &result {
            Some(result) => self.published(&value, result, "the source's result"),
            // A source with no result has nothing for a value to be:
            // it is read for what it reports itself.
            None => self.expression(&value, Some(&Type::Unknown)),
        };
        Stmt::Yield { time, value, at }
    }

    /// `inject name, ...`: each name is a capability of the node
    /// (TIME-8). `out` is its result (TIME-7): "`inject out` exposes
    /// that same result for reads, assignment, and admitted piecewise
    /// mutation." What the others are used for is for a later part.
    fn inject(&mut self, node: &SyntaxNode) -> Vec<Stmt> {
        let names: Vec<SyntaxToken> = tokens(node)
            .filter(|token| token.kind() == SyntaxKind::Ident)
            .collect();
        names
            .into_iter()
            .map(|name| {
                let at = Location::new(self.file, name.span());
                let is_out = name.text() == "out";
                // TIME-8: "`inject` binds each of the others to a local
                // of its capability's type, which core declares".
                let ty = match (&self.signature.result, is_out) {
                    (Some(result), true) => result.clone(),
                    (_, true) => Type::Unknown,
                    (_, false) => self
                        .program
                        .capability_type(name.text())
                        .unwrap_or(Type::Unknown),
                };
                // TIME-8: "A node injects `alarm` or `scheduler`, not
                // both."
                let other = match name.text() {
                    "alarm" => Some("scheduler"),
                    "scheduler" => Some("alarm"),
                    _ => None,
                };
                let earlier = other.and_then(|other| {
                    self.locals
                        .iter()
                        .find(|local| local.kind == LocalKind::Inject && local.name == other)
                        .map(|local| (other, local.at))
                });
                if let Some((other, first)) = earlier {
                    self.diagnostics.emit(
                        error(
                            Category::Injectable,
                            codes::INJECT_ALARM_AND_SCHEDULER,
                            at,
                            format!(
                                "a node injects `alarm` or `scheduler`, not both: `{other}` is \
                                 injected already"
                            ),
                            "TIME-8",
                        )
                        .with_related(first, "injected here"),
                    );
                }
                let local = self.declare(name.text(), at, LocalKind::Inject, ty, true);
                if is_out {
                    self.out = Some(local);
                }
                Stmt::Inject { local, at }
            })
            .collect()
    }

    /// `start { ... }`, `when condition { ... }`, `stop { ... }`: a hook
    /// of a node, read as value code (FN-3).
    fn hook(&mut self, node: &SyntaxNode) -> Stmt {
        let at = self.at(node);
        let hook = match tokens(node).next().map(|token| token.kind()) {
            Some(SyntaxKind::KwStart) => Hook::Start,
            Some(SyntaxKind::KwStop) => Hook::Stop,
            _ => Hook::When,
        };
        // Each hook is run from its beginning: nothing has returned.
        self.returned = false;
        let before = self.hook.replace(hook);
        let block = child(node, SyntaxKind::Block);
        // CTRL-9: what stands before the body's `{` is the condition of
        // a `when`, which may have none.
        let header = node
            .children()
            .find(|part| part.kind() != SyntaxKind::Block)
            .filter(|_| hook == Hook::When);
        let frame = self.present.len();
        // TIME-3: "`valid()` requires all temporal inputs to be valid.
        // Omitting either selector uses the same default as its
        // zero-argument form." So a `when` whose header sets no
        // admission of its own runs with every temporal input valid,
        // and one that sets it admits what its `valid` selectors name.
        // Either holds for the whole header, whatever order its
        // conjuncts are written in, and for the body.
        if hook == Hook::When {
            // TIME-3: "A header that is `scheduled()` alone is the
            // exception: it requires no input to be valid, as one that
            // writes `valid([])` does". `modified([])` alone has no such
            // exception, and the default stands.
            let alone = header
                .as_ref()
                .is_some_and(|header| self.scheduled_alone(header));
            let admitted = header
                .as_ref()
                .and_then(|header| self.admission(header))
                .unwrap_or_else(|| {
                    if alone {
                        Vec::new()
                    } else {
                        self.temporal_inputs()
                    }
                });
            self.present.extend(admitted.into_iter().map(Fact::Valid));
        }
        let reported = self.diagnostics.len();
        let condition = header.map(|condition| self.condition(&condition));
        // A header that is itself rejected says nothing sure of what it
        // admits: the reads in its body are then not judged (ERR-9).
        if self.diagnostics.len() != reported {
            let inputs = self.temporal_inputs();
            self.present.extend(inputs.iter().copied().map(Fact::Valid));
            self.present
                .extend(inputs.into_iter().map(children::all_valid_fact));
        }
        // ERR-12, TIME-4: the body is reached only where the condition
        // holds.
        if let Some(condition) = &condition {
            let (when_true, _) = self.null_tests(condition);
            self.present.extend(when_true);
        }
        let body = match block {
            Some(block) => {
                let (mut statements, last) = self.block_head(&block);
                if let Some(last) = last {
                    statements.push(Stmt::Expr(self.flow(&last, None, false).0));
                }
                statements
            }
            None => Vec::new(),
        };
        self.present.truncate(frame);
        self.hook = before;
        Stmt::Hook {
            hook,
            condition,
            body,
            at,
        }
    }

    /// True when the name at `name` may be one of the language's own
    /// forms: nothing binds it, or core's module declares it. TIME-9
    /// has core declare the endpoint operations, and in a `when` header
    /// `modified` and `valid` "are forms of the language": a name a
    /// module or a local binds is that module's or that local's.
    fn is_the_languages(&self, name: &SyntaxToken) -> bool {
        match self.bindings.get(&(self.file, name.span().start)) {
            None => true,
            Some(Binding::Declarations { module, .. }) => Some(*module) == self.program.core(),
            Some(Binding::Local { .. }) => false,
        }
    }

    /// True when a `when` header is `scheduled()` alone, in parentheses
    /// or not: the language's selector (TIME-9), and no call of a
    /// declaration a module or a local binds under that name.
    fn scheduled_alone(&self, header: &SyntaxNode) -> bool {
        let header = &ungrouped(header.clone());
        match header.kind() {
            SyntaxKind::CallExpr => header
                .children()
                .next()
                .filter(|callee| callee.kind() == SyntaxKind::NameExpr)
                .and_then(|callee| name_token(&callee))
                .is_some_and(|name| name.text() == "scheduled" && self.is_the_languages(&name)),
            _ => false,
        }
    }

    /// The inputs a `when` header admits by its own `valid` selectors;
    /// `None` when it has none, and the default stands.
    ///
    /// TIME-3: "A selector sets subscription or admission when it
    /// stands alone in the header or appears as a top-level conjunct.
    /// Selectors under negation or disjunction leave the default in
    /// place." A selector is the language's `valid`: a name a
    /// declaration binds is a call of that declaration, and no selector
    /// (TIME-9).
    fn admission(&self, header: &SyntaxNode) -> Option<Vec<LocalId>> {
        let header = &ungrouped(header.clone());
        match header.kind() {
            SyntaxKind::BinaryExpr
                if tokens(header).any(|token| token.kind() == SyntaxKind::AmpAmp) =>
            {
                // Every selector among the conjuncts adds what it names.
                let sets: Vec<Vec<LocalId>> = header
                    .children()
                    .filter_map(|operand| self.admission(&operand))
                    .collect();
                (!sets.is_empty()).then(|| sets.concat())
            }
            // `valid(a)`, and the same written with its generic
            // argument, `valid<i64>(a)`, which is the selector it is
            // without one (TIME-9).
            SyntaxKind::CallExpr
            | SyntaxKind::TypedConstructor
            | SyntaxKind::UndecidedConstructor => {
                let name = match header.kind() {
                    SyntaxKind::CallExpr => {
                        let callee = header.children().next()?;
                        match callee.kind() {
                            SyntaxKind::NameExpr => name_token(&callee)?,
                            _ => return None,
                        }
                    }
                    _ if child(header, SyntaxKind::QualifiedName).is_none() => name_token(header)?,
                    _ => return None,
                };
                // GEN-14: a form that reads two ways is the call only
                // where the resolver decided so.
                if header.kind() == SyntaxKind::UndecidedConstructor {
                    let at = self.at(header);
                    let call =
                        self.module.decisions.iter().any(|decision| {
                            decision.at == at && decision.constructor == Some(true)
                        });
                    if !call {
                        return None;
                    }
                }
                if name.text() != "valid" || !self.is_the_languages(&name) {
                    return None;
                }
                let list = child(header, SyntaxKind::ArgList)?;
                let arguments = endpoint_arguments(&list);
                if arguments.is_empty() {
                    // `valid()`: every temporal input.
                    return Some(self.temporal_inputs());
                }
                // TIME-4: `valid(parts[1])`, of a child, is a guard: "a
                // `when` header that has the test alone or as a top-level
                // conjunct guards its body". It names no input, and the
                // default admission stands with it.
                let of_child = arguments.iter().any(|entry| {
                    matches!(entry.kind(), SyntaxKind::IndexExpr | SyntaxKind::MemberExpr)
                });
                if of_child {
                    return None;
                }
                // TIME-3: "`valid(a, b)` requires both named inputs,
                // and similarly for more inputs ... `valid([])` requires
                // no inputs to be valid." What names no input, the `[]`
                // among them, admits nothing here, and any other is
                // reported where the header is read.
                Some(
                    arguments
                        .iter()
                        .filter(|entry| entry.kind() == SyntaxKind::NameExpr)
                        .filter_map(|entry| self.local_of(entry))
                        .filter(|local| self.is_temporal_input(*local))
                        .collect(),
                )
            }
            _ => None,
        }
    }

    /// A read of a temporal input's payload in a node.
    ///
    /// TIME-1: "A payload read requires admission/readability; metadata
    /// may observe an invalid endpoint." The endpoint operations are
    /// metadata, and are not read here (TIME-9): each takes the
    /// endpoint and not its payload. A read is admitted where the
    /// input is known to be valid, and nowhere else.
    ///
    /// TIME-4: "A payload read under manual admission must be dominated
    /// by a successful guard": in a `when`, the input is one its header
    /// admits, or a `valid(input)` holds where the read stands. TIME-5:
    /// "State initializers cannot read current temporal inputs. `start`
    /// reads no temporal input and gives none to an endpoint operation:
    /// it runs before any has a value. `stop` reads a temporal input
    /// under manual admission, as a `when` under `valid([])` does: no
    /// header stands over it, so each payload read is dominated by a
    /// guard".
    fn payload_read(&mut self, local: LocalId, at: Location) {
        let name = self.locals[local].name.clone();
        if self.initializing {
            self.diagnostics.emit(error(
                Category::Phase,
                codes::STATE_INPUT_READ,
                at,
                format!(
                    "`{name}` is a temporal input, and a `state` is given its first value \
                     before any input has one"
                ),
                "TIME-5",
            ));
            return;
        }
        // TIME-1: a cache's initializer admits no read either. TIME-6:
        // a cache is "initialized on every start", and TIME-5: that is
        // "before any has a value". The catalogue's `state.input_read`
        // names a state's initializer alone, so the code is local.
        if self.caching {
            self.diagnostics.emit(
                local_error(
                    Category::Phase,
                    codes::local::CACHE_INPUT_READ,
                    at,
                    format!(
                        "`{name}` is a temporal input, and a `cache` is given its first value \
                         on every start, before any input has one"
                    ),
                    "TIME-1",
                )
                .with_note("a `when` reads it, and may assign the cache"),
            );
            return;
        }
        let (unproved, rule) = match self.hook {
            Some(Hook::Start) => {
                self.input_lifecycle(at, &name);
                return;
            }
            Some(Hook::When) => ("this `when` does not require it to be", "TIME-4"),
            Some(Hook::Stop) => ("`stop` requires no input to be", "TIME-5"),
            None => return,
        };
        if self.present.contains(&Fact::Valid(local)) {
            return;
        }
        self.diagnostics.emit(
            error(
                Category::Phase,
                codes::PAYLOAD_UNGUARDED,
                at,
                format!(
                    "`{name}` may not be valid here: {unproved}, and no `valid({name})` \
                     guards the read"
                ),
                rule,
            )
            .with_note(format!(
                "a guard proves it where the read stands: `if valid({name}) {{ ... }}`"
            )),
        );
        // One report for one input on a path.
        self.present.push(Fact::Valid(local));
    }

    /// Reports a temporal input used in `start` (TIME-5): "`start`
    /// reads no temporal input and gives none to an endpoint operation:
    /// it runs before any has a value."
    fn input_lifecycle(&mut self, at: Location, name: &str) {
        self.diagnostics.emit(
            error(
                Category::Phase,
                codes::INPUT_LIFECYCLE,
                at,
                format!("`{name}` is a temporal input, and `start` runs before any has a value"),
                "TIME-5",
            )
            .with_note("a `when` reads it, and `stop` under a guard: `if valid(input) { ... }`"),
        );
    }

    /// Reports a use of the node's output where TIME-7 has none: "`out`
    /// is read in a `when` and in `stop`, and written only in a `when`:
    /// `start` neither reads nor writes it".
    fn out_lifecycle(&mut self, at: Location, message: String) {
        self.diagnostics.emit(
            error(Category::Phase, codes::OUT_LIFECYCLE, at, message, "TIME-7").with_note(
                "a node that publishes when the graph starts schedules itself in `start` and \
                 writes in its first scheduled evaluation",
            ),
        );
    }

    /// An operation on an endpoint (TIME-9): "Elsewhere each takes a
    /// temporal input of the enclosing function by its name, or `out`".
    /// `valid` and `modified` are also the selectors of a `when` header
    /// (TIME-3): "`modified()` subscribes to all temporal inputs;
    /// `modified(a, b)` subscribes to both named inputs, and similarly
    /// for more inputs ... `valid()` requires all temporal inputs to be
    /// valid; `valid(a, b)` requires both named inputs, and similarly
    /// for more inputs ... `valid([])` requires no inputs to be valid."
    fn endpoint(
        &mut self,
        operation: EndpointOperation,
        name: &SyntaxToken,
        list: &SyntaxNode,
    ) -> (ExprKind, Type) {
        let at = Location::new(self.file, name.span());
        let arguments = endpoint_arguments(list);
        // How many it takes: a selector may stand with none, and with
        // as many inputs as it names (TIME-3: "`modified(a, b)`
        // subscribes to both named inputs, and similarly for more
        // inputs").
        let (fewest, most) = match operation {
            EndpointOperation::Scheduled => (0, 0),
            EndpointOperation::Valid | EndpointOperation::Modified => (0, usize::MAX),
            _ => (1, 1),
        };
        if arguments.len() < fewest {
            self.diagnostics.emit(error(
                Category::Type,
                codes::ARGUMENT_MISSING,
                at,
                format!("`{}` is given no endpoint: it takes one", operation.name()),
                "TIME-9",
            ));
        }
        for surplus in arguments.iter().skip(most) {
            self.diagnostics.emit(error(
                Category::Type,
                codes::ARGUMENT_UNEXPECTED,
                self.at(surplus),
                format!(
                    "`{}` takes {}, and this is one more",
                    operation.name(),
                    if most == 0 {
                        "no argument"
                    } else {
                        "one endpoint"
                    }
                ),
                "TIME-9",
            ));
        }
        // The endpoints named, each by its name. `valid([])` and
        // `modified([])` name none (TIME-3); any other sequence is no
        // endpoint, and is reported as one.
        let none = matches!(
            operation,
            EndpointOperation::Valid | EndpointOperation::Modified
        ) && matches!(arguments.as_slice(), [only]
                if only.kind() == SyntaxKind::SequenceExpr && only.children().next().is_none());
        // TIME-3: "`modified([])` subscribes to no input: it stands
        // only in a node that injects `scheduler` or `alarm` and
        // schedules a wake on it in `start`, which is then what wakes
        // the handler."
        if none && operation == EndpointOperation::Modified {
            // The selector: `modified([])`, its name to its `)`.
            let selector = list.parent().map_or(at, |call| self.at(&call));
            // Once, where an argument is read a second time for the
            // candidate it is given to.
            if !self.undriven.contains(&selector) {
                self.undriven.push(selector);
            }
        }
        // TIME-9: "`scheduled()` is the handler selector for a wake by
        // the node's scheduler or alarm ... A node that injects neither
        // rejects it". With a scheduler it is the scheduler's
        // `is_scheduled_now`, and with an alarm it holds where no
        // temporal input has ticked: with neither it asks nothing.
        if operation == EndpointOperation::Scheduled {
            let wakes = self.locals.iter().any(|local| {
                local.kind == LocalKind::Inject
                    && (local.name == "scheduler" || local.name == "alarm")
            });
            if !wakes {
                // The selector: `scheduled()`, its name to its `)`.
                let selector = list.parent().map_or(at, |call| self.at(&call));
                self.diagnostics.emit(
                    error(
                        Category::Injectable,
                        codes::INJECT_NO_SCHEDULER,
                        selector,
                        "`scheduled()` asks the node's scheduler or its alarm, and this node \
                         injects neither"
                            .to_string(),
                        "TIME-9",
                    )
                    .with_note("`inject scheduler` or `inject alarm` gives the node one"),
                );
            }
        }
        // TIME-9, END-8: "A child of a collection is an endpoint whether
        // or not it is valid: `valid`, `modified`, `delta_value` and
        // `last_modified` take it, and `all_valid` where it is a
        // collection". It is given alone; `is_active`, `activate` and
        // `passivate` "operate on direct temporal inputs".
        if let [only] = arguments.as_slice()
            && matches!(
                operation,
                EndpointOperation::Valid
                    | EndpointOperation::AllValid
                    | EndpointOperation::Modified
                    | EndpointOperation::DeltaValue
                    | EndpointOperation::LastModified
            )
        {
            let reported = self.diagnostics.len();
            if let Some(found) = self.child_endpoint(only) {
                return self.child_operation(operation, at, only, found, reported);
            }
        }
        let inputs: Option<Vec<LocalId>> = if arguments.is_empty() || most == 0 {
            None
        } else if none {
            Some(Vec::new())
        } else {
            Some(
                arguments
                    .iter()
                    .take(most)
                    .filter_map(|entry| self.endpoint_named(operation, entry))
                    .collect(),
            )
        };
        // TIME-5: `start` "gives none to an endpoint operation".
        if self.hook == Some(Hook::Start)
            && let Some(inputs) = &inputs
        {
            let given: Vec<String> = inputs
                .iter()
                .filter(|input| Some(**input) != self.out)
                .map(|input| self.locals[*input].name.clone())
                .collect();
            for input in given {
                self.input_lifecycle(at, &input);
            }
        }
        // TIME-9: "`all_valid` takes only a collection; another
        // endpoint is rejected (`endpoint.argument`)". A collection is
        // "a list, a tuple, a struct and a dictionary".
        if operation == EndpointOperation::AllValid
            && let Some(inputs) = &inputs
        {
            for (input, given) in inputs.iter().zip(&arguments) {
                let ty = &self.locals[*input].ty;
                let collection = matches!(
                    ty,
                    Type::List { .. }
                        | Type::Tuple(_)
                        | Type::Dict { .. }
                        | Type::Named {
                            kind: NamedKind::Struct,
                            ..
                        }
                        | Type::Parameter { .. }
                        | Type::Unknown
                );
                if !collection {
                    self.diagnostics.emit(
                        error(
                            Category::Phase,
                            codes::ENDPOINT_ARGUMENT,
                            self.at(given),
                            format!("`all_valid` takes a collection, and `{ty}` is none"),
                            "TIME-9",
                        )
                        .with_note("a collection is a list, a tuple, a struct or a dictionary"),
                    );
                }
            }
        }
        let writes = operation == EndpointOperation::Invalidate;
        if let (Some(inputs), Some(out)) = (&inputs, self.out)
            && inputs.contains(&out)
        {
            match self.hook {
                Some(Hook::Start) => self.out_lifecycle(
                    at,
                    "`out` is neither read nor written in `start`".to_string(),
                ),
                Some(Hook::Stop) if writes => {
                    self.out_lifecycle(at, "`out` is read in `stop`, and not written".to_string());
                }
                _ => {}
            }
        }
        let ty = match operation {
            EndpointOperation::Valid
            | EndpointOperation::AllValid
            | EndpointOperation::Modified
            | EndpointOperation::Scheduled
            | EndpointOperation::IsActive => BOOL,
            // END-3: "`delta_value(x)` returns what `x` published this
            // cycle, a `delta<T>` for an endpoint of T (the language's
            // TYPE-16). For a scalar it is the value published".
            EndpointOperation::DeltaValue => {
                let of = inputs
                    .as_ref()
                    .and_then(|inputs| inputs.first())
                    .map(|input| self.locals[*input].ty.clone());
                // TYPE-16: "`ref<V>`, `signal`: None". An input that
                // may be left out is its shape under a `?`.
                let none = of.as_ref().is_some_and(|of| {
                    matches!(not_null(Some(of)), Some(Type::Ref(_) | Type::Signal))
                });
                if none {
                    self.diagnostics.emit(
                        error(
                            Category::Shape,
                            codes::DELTA_UNSUPPORTED_SHAPE,
                            at,
                            format!(
                                "`{}` has no delta",
                                of.as_ref().map_or(String::new(), ToString::to_string)
                            ),
                            "TYPE-16",
                        )
                        .with_note(
                            "a `ref` designates an endpoint and a `signal` has no payload: \
                             neither publishes a value of its own",
                        ),
                    );
                    Type::Unknown
                } else {
                    of.map_or(Type::Unknown, Type::delta)
                }
            }
            // END-6: a `datetime?`.
            EndpointOperation::LastModified => nullable(Type::Scalar(Scalar::Datetime)),
            EndpointOperation::Invalidate
            | EndpointOperation::Activate
            | EndpointOperation::Passivate => Type::Unknown,
        };
        (ExprKind::Endpoint { operation, inputs }, ty)
    }

    /// The generic arguments written on an endpoint operation, held to
    /// what the operation is: core declares each with one type, the
    /// type the endpoint holds, `valid<T>(input: endpoint<T>)` (TIME-9).
    fn endpoint_generics(
        &mut self,
        name: &SyntaxToken,
        given: Option<&CallArguments>,
        kind: &ExprKind,
    ) {
        let Some(given) = given else {
            return;
        };
        let [(at, argument)] = given.arguments.as_slice() else {
            self.diagnostics.emit(error(
                Category::Type,
                codes::GENERIC_ARGUMENT_COUNT,
                Location::new(self.file, name.span()),
                format!(
                    "`{}` takes 1 generic argument and is given {}",
                    name.text(),
                    given.arguments.len()
                ),
                "GEN-8",
            ));
            return;
        };
        let held = match argument {
            Argument::Type(held) => held,
            Argument::Constant(_) => {
                self.diagnostics.emit(error(
                    Category::Type,
                    codes::GENERIC_ARGUMENT_KIND,
                    *at,
                    format!(
                        "`{}` takes the type its endpoint holds, and this is a value",
                        name.text()
                    ),
                    "GEN-14",
                ));
                return;
            }
        };
        // A child that holds another type is no endpoint of the one
        // written either (EXPR-5).
        if let ExprKind::ChildEndpoint { child, .. } = kind {
            if whole(held) && whole(&child.ty) && child.ty != *held {
                self.diagnostics.emit(error(
                    Category::Type,
                    codes::TYPE_MISMATCH,
                    *at,
                    format!(
                        "`{}<{held}>` takes an endpoint of `{held}`, and this child holds `{}`",
                        name.text(),
                        child.ty
                    ),
                    "EXPR-5",
                ));
            }
            return;
        }
        let ExprKind::Endpoint {
            inputs: Some(inputs),
            ..
        } = kind
        else {
            return;
        };
        // An input that holds another type is no endpoint of the one
        // written (EXPR-5). What is not known has been reported.
        let other: Vec<String> = inputs
            .iter()
            .map(|input| &self.locals[*input])
            .filter(|input| whole(held) && whole(&input.ty) && input.ty != *held)
            .map(|input| format!("`{}` holds `{}`", input.name, input.ty))
            .collect();
        if !other.is_empty() {
            self.diagnostics.emit(error(
                Category::Type,
                codes::TYPE_MISMATCH,
                *at,
                format!(
                    "`{}<{held}>` takes an endpoint of `{held}`, and {}",
                    name.text(),
                    other.join(" and ")
                ),
                "EXPR-5",
            ));
        }
    }

    /// The argument of an `endpoint<T>` parameter (TIME-9): "A call
    /// gives an `endpoint<T>` parameter a temporal input of the
    /// enclosing node by its name, or `out`, and nothing else".
    ///
    /// The input is named and not read: its payload may be invalid, and
    /// the function is given the endpoint to ask. In a graph or a value
    /// function there is no enclosing node, and nothing to give.
    fn endpoint_given(&mut self, node: &SyntaxNode) -> Expr {
        let at = self.at(node);
        let named = endpoint_argument(node);
        // TIME-9: "a child of one, `parts[1]`, `quote.bid` or
        // `parts[i]`", in a hook.
        let reported = self.diagnostics.len();
        if self.hook.is_some()
            && let Some(found) = self.child_endpoint(&named)
        {
            if let Some(child) = found {
                self.child_in_start(at, &child);
                let ty = Type::Endpoint(Box::new(child.ty.clone()));
                return Expr {
                    kind: ExprKind::ChildEndpointOf(Box::new(child)),
                    ty,
                    at,
                };
            }
            if self.diagnostics.len() != reported {
                return Expr {
                    kind: ExprKind::Unread,
                    ty: Type::Unknown,
                    at,
                };
            }
        }
        let local = (named.kind() == SyntaxKind::NameExpr)
            .then(|| self.local_of(&named))
            .flatten()
            .filter(|&local| {
                self.hook.is_some() && (self.is_temporal_input(local) || Some(local) == self.out)
            });
        let Some(local) = local else {
            self.diagnostics.emit(
                error(
                    Category::Phase,
                    codes::ENDPOINT_ARGUMENT,
                    at,
                    "an `endpoint<T>` parameter takes a temporal input of the node by its \
                     name, or `out`"
                        .to_string(),
                    "TIME-9",
                )
                .with_note(if self.hook.is_some() {
                    "a state, a `const` parameter and an expression have no endpoint"
                } else {
                    "an endpoint is given in a node's hook: a graph and a value function have \
                     no input of their own to give"
                }),
            );
            return Expr {
                kind: ExprKind::Unread,
                ty: Type::Unknown,
                at,
            };
        };
        Expr {
            kind: ExprKind::EndpointOf(local),
            ty: Type::Endpoint(Box::new(self.locals[local].ty.clone())),
            at,
        }
    }

    /// The endpoint an argument of an endpoint operation names: a
    /// temporal parameter of the node, or its injected `out`. Any other
    /// is reported (`endpoint.argument`).
    fn endpoint_named(
        &mut self,
        operation: EndpointOperation,
        node: &SyntaxNode,
    ) -> Option<LocalId> {
        let local = match node.kind() {
            SyntaxKind::NameExpr => self.local_of(node),
            _ => None,
        };
        // TIME-9: only direct inputs have a subscription to change.
        let input_only = matches!(
            operation,
            EndpointOperation::Activate | EndpointOperation::Passivate
        );
        match local {
            Some(local)
                if self.is_temporal_input(local) || (!input_only && Some(local) == self.out) =>
            {
                Some(local)
            }
            _ => {
                self.diagnostics.emit(error(
                    Category::Phase,
                    codes::ENDPOINT_ARGUMENT,
                    self.at(node),
                    format!(
                        "`{}` takes a temporal input of `{}` by its name{}",
                        operation.name(),
                        self.declaration.name,
                        if input_only { "" } else { ", or `out`" }
                    ),
                    "TIME-9",
                ));
                // Read for what it reports itself. The argument is
                // rejected already, so what it reads of an input is not
                // judged as well (ERR-9).
                let frame = self.present.len();
                let inputs = self.temporal_inputs();
                self.present.extend(inputs.iter().copied().map(Fact::Valid));
                self.present
                    .extend(inputs.into_iter().map(children::all_valid_fact));
                self.expression(node, Some(&Type::Unknown));
                self.present.truncate(frame);
                None
            }
        }
    }

    fn body(mut self) -> Option<Body> {
        let syntax = &self.declaration.syntax;
        let result = self.signature.result.clone();
        let statements = if let Some(block) = child(syntax, SyntaxKind::Block) {
            let (mut statements, last) = self.block_head(&block);
            if let Some(last) = last {
                statements.push(self.final_expression(&last, result.as_ref()));
            }
            statements
        } else {
            let arrow = child(syntax, SyntaxKind::ArrowBody)?;
            match arrow.children().next() {
                Some(value) => vec![self.final_expression(&value, result.as_ref())],
                None => Vec::new(),
            }
        };
        // FN-7: "A `const fn` that declares a result ends every path in
        // `return value` or in its body's value". CTRL-14: a graph that
        // returns a value in a branch selected at runtime does too:
        // "the continuation returns a value too, and one that can reach
        // its end with none rejects."
        if let Some(result) = &result
            && !open(result)
            && !self.returned
            && !self.unsure
            && (!self.graph || self.must_return)
        {
            self.diagnostics.emit(error(
                Category::Type,
                codes::RETURN_MISSING,
                self.declaration.at,
                format!(
                    "`{}` declares a result of `{result}`, and can reach its end with no value",
                    self.declaration.name
                ),
                "FN-7",
            ));
        }
        Some(Body {
            locals: self.locals,
            statements,
        })
    }

    /// The expression a body ends in: what the function returns, when
    /// it declares a result, and a statement when it declares none
    /// (FN-7).
    fn final_expression(&mut self, node: &SyntaxNode, result: Option<&Type>) -> Stmt {
        let at = self.at(node);
        let Some(result) = result else {
            let (value, _) = self.flow(node, None, false);
            return Stmt::Expr(value);
        };
        let (value, outcome) = self.flow(node, Some(result), true);
        match outcome {
            Outcome::Value(_) => {
                let value = self.fit_input(value, result, "the function's result");
                self.returned = true;
                Stmt::Return {
                    value: Some(value),
                    at,
                }
            }
            // CTRL-10: an `if` with no `else` whose branch returns is
            // null where its condition does not hold. That is a value
            // of a result written `T?`, and is what the function then
            // returns.
            Outcome::Null if matches!(result, Type::Nullable(_)) => {
                let mut value = value;
                value.ty = result.clone();
                self.returned = true;
                Stmt::Return {
                    value: Some(value),
                    at,
                }
            }
            // Every path through it has returned, or it has no value
            // and the function ends with none. Null alone is no value
            // of a result that is not nullable.
            Outcome::Returns | Outcome::Nothing | Outcome::Null => Stmt::Expr(value),
        }
    }

    // ---- Statements ----

    fn statement(&mut self, node: &SyntaxNode) -> Stmt {
        match node.kind() {
            SyntaxKind::LocalDecl => self.local(node),
            // FN-9: "A graph injects `global_state` and no other, and
            // uses it where the graph is built". An `inject` of any
            // other capability has made the body a node's (FN-2).
            SyntaxKind::InjectDecl if self.graph && !self.testing => {
                let mut bound = self.inject(node);
                if bound.is_empty() {
                    self.unread(node)
                } else {
                    bound.swap_remove(0)
                }
            }
            SyntaxKind::AssignStmt => self.assignment(node),
            // An `assert` is a statement of a test's own (TST-3).
            SyntaxKind::AssertStmt if self.testing => self.assertion(node),
            // TST-3: a test "cannot itself declare state/cache, inject,
            // lifecycle hooks, when, yield, or return". The phase pass
            // reports a `return` there, and it is not read.
            SyntaxKind::ReturnStmt if self.testing => self.unread(node),
            // A `yield` is a statement of a source's own (SOURCE-1). In
            // a hook it is reported by the phase pass, and not read.
            SyntaxKind::YieldStmt if self.generating => {
                // SOURCE-4: "A valued return and yield inside a value
                // expression reject."
                if self.valued > 0 {
                    self.diagnostics.emit(
                        Diagnostic::error(
                            Category::Phase,
                            Code::Local(codes::local::YIELD_IN_VALUE),
                            self.at(node),
                            "a `yield` is a statement of the source, and this one stands inside \
                             a block or an `if` whose value is used"
                                .to_string(),
                        )
                        .with_rule("SOURCE-4"),
                    );
                }
                self.yield_statement(node)
            }
            // SOURCE-4: "A bare return or body completion finishes the
            // source ... A valued return ... reject[s]": a source writes
            // its output with `yield` (SOURCE-1). The value is read for
            // what it reports itself.
            SyntaxKind::ReturnStmt if self.generating => {
                let value = node
                    .children()
                    .next()
                    .map(|value| self.expression(&value, Some(&Type::Unknown)));
                if let Some(value) = &value {
                    self.diagnostics.emit(
                        Diagnostic::error(
                            Category::Phase,
                            Code::Local(codes::local::SOURCE_RETURN_VALUE),
                            value.at,
                            "a source writes its output with `yield`: its `return` takes no \
                             value"
                                .to_string(),
                        )
                        .with_rule("SOURCE-4"),
                    );
                }
                self.returned = true;
                Stmt::Return {
                    value,
                    at: self.at(node),
                }
            }
            SyntaxKind::ReturnStmt => {
                let value = node.children().next().map(|value| self.returned(&value));
                let at = self.at(node);
                // FN-7: "A value return yields an ordinary value." One
                // with nothing written yields none.
                // In a temporal function a `return` with no value "ends
                // the evaluation and publishes nothing itself".
                if let (None, Some(result)) = (&value, &self.signature.result)
                    && !open(result)
                    && self.hook.is_none()
                {
                    self.diagnostics.emit(error(
                        Category::Type,
                        codes::TYPE_MISMATCH,
                        at,
                        format!("this returns no value, and the function's result is `{result}`"),
                        "FN-7",
                    ));
                }
                // "`return value` [rejects] in a function that declares
                // no result."
                if let (Some(value), None) = (&value, &self.signature.result) {
                    self.diagnostics.emit(error(
                        Category::Type,
                        codes::TYPE_MISMATCH,
                        value.at,
                        "this returns a value, and the function declares no result".to_string(),
                        "FN-7",
                    ));
                }
                // TIME-7: "a `return` with a value stands in neither
                // `start` nor `stop`."
                if let (Some(value), Some(hook @ (Hook::Start | Hook::Stop))) = (&value, self.hook)
                {
                    self.out_lifecycle(
                        value.at,
                        format!(
                            "a `return` with a value publishes, and `{}` publishes nothing",
                            hook.as_str()
                        ),
                    );
                }
                // CTRL-14: "A `return value` in a branch of a temporal
                // `if` makes the graph's result that value while the
                // branch is selected and the continuation's otherwise".
                if value.is_some() && self.temporal.is_some() {
                    self.must_return = true;
                }
                // CTRL-15: in a loop that makes one child for each
                // member "an assignment to a `var` declared outside it
                // rejects (CTRL-8), and so does a `return`."
                if self.dynamic.is_some() {
                    self.diagnostics.emit(error(
                        Category::Phase,
                        codes::LOOP_CARRIED,
                        at,
                        "this loop makes one child for each member, and a child does not \
                         return from the graph"
                            .to_string(),
                        "CTRL-15",
                    ));
                }
                // CTRL-5: "Return targets the enclosing HGL function."
                self.returned = true;
                Stmt::Return { value, at }
            }
            // A block or an `if` on a line of its own: its value, when
            // it has one, is dropped.
            SyntaxKind::ExprStmt => match node.children().next() {
                Some(value) => Stmt::Expr(self.flow(&value, None, false).0),
                None => Stmt::Unread { at: self.at(node) },
            },
            SyntaxKind::WhileStmt => self.while_loop(node),
            SyntaxKind::ForStmt => self.for_loop(node),
            SyntaxKind::SwitchStmt => self.switch(node),
            _ => self.unread(node),
        }
    }

    /// A statement this pass does not read yet.
    fn unread(&mut self, node: &SyntaxNode) -> Stmt {
        self.forget();
        self.unsure = true;
        Stmt::Unread { at: self.at(node) }
    }

    /// A value a node publishes to its result: by a `return` in a
    /// hook, an assignment to `out`, or a source's `yield`.
    ///
    /// TIME-7: "A runtime return publishes to its result." What is
    /// published is a complete value of the result, or what the result
    /// publishes in one cycle, its `delta<T>` (TYPE-16): core's cases
    /// relay a set with `return delta_value(input)`. A result of
    /// `atomic<V>` is published whole, as a V (TYPE-9: "`atomic<V>`
    /// stops expansion at a complete ordinary V").
    ///
    /// A value written out takes its type from its place (EXPR-5), and
    /// the place here is one or the other: `(1.0, null)` is no
    /// `tuple<f64, f64>` and is its delta. So what does not read as a
    /// complete value is read again as the delta, and stands as that
    /// where it reads clean; otherwise the complete value's reading
    /// stands, with what it reports.
    fn published(&mut self, node: &SyntaxNode, result: &Type, what: &str) -> Expr {
        let whole = match result {
            Type::Atomic(value) => (**value).clone(),
            other => other.clone(),
        };
        let delta = Type::delta(result.clone());
        let mark = self.mark();
        let value = self.expression(node, Some(&whole));
        let clean = self.diagnostics.len() == mark.diagnostics;
        if clean {
            if open(&value.ty) {
                return value;
            }
            // The result's own type: a complete value, and what an
            // atomic input is read as. Or a delta, read as one whatever
            // was expected of it: `delta_value(input)`.
            if self.admits(result, &value.ty) || (delta != whole && self.admits(&delta, &value.ty))
            {
                return value;
            }
        }
        // Not a complete value as it reads: as the delta, where the
        // result has one that is another type.
        let fitted = clean && self.fits_whole(&value, &whole);
        if !fitted && delta != whole && self.may_reread() {
            self.take_back(mark, false);
            let mark = self.mark();
            let as_delta = self.expression(node, Some(&delta));
            let as_delta = self.fit(as_delta, &delta, what);
            if self.diagnostics.len() == mark.diagnostics {
                return as_delta;
            }
            self.take_back(mark, false);
            let value = self.expression(node, Some(&whole));
            return self.fit(value, &whole, what);
        }
        self.fit(value, &whole, what)
    }

    /// True when `value` is one the place of type `whole` takes: of that
    /// type, or an integer literal where an `f64` is expected (EXPR-5).
    fn fits_whole(&self, value: &Expr, whole: &Type) -> bool {
        self.admits(whole, &value.ty) || (is_integer_literal(value) && accepts_float(whole))
    }

    /// FN-7: "A value return yields an ordinary value." EXPR-5: the
    /// result type is the type expected of it.
    fn returned(&mut self, node: &SyntaxNode) -> Expr {
        let result = self.signature.result.clone();
        // TIME-7: "A runtime return publishes to its result."
        if let Some(result) = &result
            && (self.hook.is_some() || self.generating)
        {
            return self.published(node, result, "the function's result");
        }
        let value = self.expression(node, result.as_ref());
        match &result {
            Some(result) => self.fit_input(value, result, "the function's result"),
            None => value,
        }
    }

    /// EXPR-3: "Initialization fixes a binding's value/connection
    /// category and type; an annotation checks that category rather
    /// than changing it ... An uninitialized typed var supplies no value
    /// or connection."
    fn local(&mut self, node: &SyntaxNode) -> Stmt {
        let Some(name) = tokens(node).find(|token| token.kind() == SyntaxKind::Ident) else {
            return self.unread(node);
        };
        // `state` and `cache` are a node's (TIME-5, TIME-6): each is
        // assigned as a `var` is.
        let kind = match tokens(node).next().map(|token| token.kind()) {
            Some(SyntaxKind::KwVar) => LocalKind::Var,
            Some(SyntaxKind::KwState) => LocalKind::State,
            Some(SyntaxKind::KwCache) => LocalKind::Cache,
            _ => LocalKind::Let,
        };
        // The annotation, read by the pass before this one.
        let annotation = node
            .children()
            .find(|part| is_type(part.kind()))
            .and_then(|part| self.written.get(&(self.file, part.span().start)))
            .map(|ty| (*ty).clone());
        // The first value: the child that is no type.
        let value = node
            .children()
            .find(|part| !is_type(part.kind()))
            .map(|value| {
                let value = self.expression(&value, annotation.as_ref());
                match &annotation {
                    Some(annotation) => self.fit(value, annotation, "the type written for it"),
                    None => value,
                }
            });
        let ty = match (&annotation, &value) {
            (Some(annotation), _) => annotation.clone(),
            (None, Some(value)) => value.ty.clone(),
            (None, None) => Type::Unknown,
        };
        let at = Location::new(self.file, name.span());
        // CTRL-11: "A `var` whose type is written `T?` is null where it
        // is declared with no value ... it is assigned from there".
        let assigned = value.is_some() || matches!(ty, Type::Nullable(_));
        let local = self.declare(name.text(), at, kind, ty, assigned);
        // TIME-8: a local a capability is bound to is that capability.
        if let Some(capability) = value.as_ref().and_then(|value| self.capability_of(value)) {
            self.aliases.insert(local, capability);
        }
        // ERR-7: "What it gives is bound with `let`".
        if kind == LocalKind::Let
            && let Some(Expr {
                kind: ExprKind::Capture(call),
                ..
            }) = &value
        {
            let output = self.captured_output(call);
            self.captures.insert(local, output);
        }
        // EXPR-3: "Initialization fixes a binding's value/connection
        // category".
        self.locals[local].category = match &value {
            Some(value) if self.graph => self.category(value),
            // A `var` of a graph declared with no value: its first
            // assignment says what it is.
            None if self.graph => ValueCategory::Unsettled,
            _ => ValueCategory::Value,
        };
        Stmt::Declare { local, value }
    }

    /// EXPR-3: "`let` is immutable; `var` may be assigned within the
    /// same category/type ... A compound assignment reads its existing
    /// target."
    fn assignment(&mut self, node: &SyntaxNode) -> Stmt {
        let mut parts = node.children();
        let (Some(place), Some(value)) = (parts.next(), parts.next()) else {
            return self.unread(node);
        };
        // NATIVE-6: "None is assigned." A property of a native type is
        // read-only, a capability's among them (TIME-8).
        if self.assigned_property(&place) {
            return self.unread(node);
        }
        let local = match place.kind() {
            SyntaxKind::NameExpr => self.local_of(&place),
            // EXPR-3: "An assignment may name a place inside a value".
            SyntaxKind::MemberExpr | SyntaxKind::IndexExpr => {
                return self.place_assignment(node, &place, &value);
            }
            _ => None,
        };
        let Some(local) = local else {
            return self.unread(node);
        };
        let operator = tokens(node).find_map(|token| compound(token.kind()));
        // Where the assignment's operator is written: `+=`.
        let operator_at = tokens(node)
            .find(|token| compound(token.kind()).is_some())
            .map_or(self.at(node), |token| {
                Location::new(self.file, token.span())
            });
        let at = self.at(node);
        let target = self.locals[local].ty.clone();
        // TIME-7: "`inject out` exposes that same result for reads,
        // assignment, and admitted piecewise mutation ... `out` is read
        // in a `when` and in `stop`, and written only in a `when`".
        let is_out = Some(local) == self.out;
        if is_out && let Some(hook @ (Hook::Start | Hook::Stop)) = self.hook {
            self.out_lifecycle(
                self.at(&place),
                format!(
                    "`out` is written only in a `when`, and this is `{}`",
                    hook.as_str()
                ),
            );
        }
        if !is_out
            && !matches!(
                self.locals[local].kind,
                LocalKind::Var | LocalKind::State | LocalKind::Cache
            )
        {
            self.diagnostics.emit(
                error(
                    Category::Name,
                    codes::BINDING_IMMUTABLE,
                    self.at(&place),
                    format!(
                        "`{}` is {} and cannot be assigned: only a `var` can",
                        self.locals[local].name,
                        self.locals[local].kind.describe()
                    ),
                    "EXPR-3",
                )
                .with_related(self.locals[local].at, "declared here"),
            );
            // The value is still read, for what it reports itself.
            let value = self.expression(&value, Some(&target));
            return Stmt::Assign {
                local,
                operator,
                value,
                at,
            };
        }
        let value = match operator {
            // TIME-7: an assignment to `out` publishes to the result.
            None if is_out => self.published(&value, &target, "the node's result"),
            None => {
                let value = self.expression(&value, Some(&target));
                self.fit(value, &target, "the type of the `var`")
            }
            Some(operator) => {
                // `name += value` assigns what `name + value` is: the
                // target is the left operand, and is read.
                let left = self.read_local(local, &place);
                let left = Given::read(0, place.clone(), left);
                let right = self.operand(1, value.clone());
                let identity = self.symbol_operator(operator.identity());
                // What the sum is given to says what it is to be.
                let (kind, result) = self.symbol(
                    (operator, operator_at),
                    identity,
                    (left, right),
                    (Vec::new(), false, false),
                    Some(&target),
                );
                self.conforms(&result, &target, self.at(&value), "the type of the `var`");
                Expr {
                    kind,
                    ty: result,
                    at,
                }
            }
        };
        self.assigned[local] = true;
        // CTRL-8: "Dynamic composition loops do not admit loop-carried
        // accumulation". CTRL-15: "In such a loop an assignment to a
        // `var` declared outside it rejects".
        if self.dynamic.is_some_and(|outside| local < outside) {
            self.diagnostics.emit(
                error(
                    Category::Phase,
                    codes::LOOP_CARRIED,
                    self.at(&place),
                    format!(
                        "`{}` is declared outside this loop, which makes one child for each \
                         member: nothing is carried from one child to the next",
                        self.locals[local].name
                    ),
                    "CTRL-15",
                )
                .with_related(self.locals[local].at, "declared here"),
            );
        } else if self.graph {
            let given = (self.category(&value), value.at);
            self.assigned_category(local, given, false);
        }
        // TIME-8: a `var` a capability is assigned to is that
        // capability from there.
        if operator.is_none()
            && let Some(capability) = self.capability_of(&value)
        {
            self.aliases.insert(local, capability);
        }
        Stmt::Assign {
            local,
            operator,
            value,
            at,
        }
    }

    /// `point.x = 1`, `row[0] += 1`, `book["a"] = 1`: an assignment to
    /// a place inside a value.
    ///
    /// EXPR-3: "An assignment may name a place inside a value, at any
    /// depth ... The place's root is a `var`, a `state`, a `cache`, or,
    /// in a `when`, `out` (TIME-7). A `let`, a parameter, and an
    /// iteration binding are read-only all the way down. The value is
    /// of the field's type; of a list's element type, under an `i64`
    /// index; of a dictionary's value type, under a key of its key
    /// type; or of a tuple's position, under a constant index ... A set
    /// has no place to assign. A compound assignment reads the place
    /// first".
    ///
    /// The place is read as the expression it is: what a field or an
    /// element is said to be where it is read is what it takes where
    /// it is assigned, and a set, which has no element to read, has
    /// none to assign.
    ///
    /// EXPR-15: "In a graph, an assignment into a place of a `var` is
    /// copy on write ... `point.x = price` rebinds `point` to a new
    /// connection, which is `point` with its `x` from `price` ... The
    /// place, the type of its value, and a compound assignment are as
    /// EXPR-3 has them." So it is read as any other, and is then an
    /// assignment to the `var`: held to the `var`'s category, and to
    /// what a loop or a branch selected at runtime admits (CTRL-14,
    /// CTRL-15).
    fn place_assignment(
        &mut self,
        node: &SyntaxNode,
        place: &SyntaxNode,
        value: &SyntaxNode,
    ) -> Stmt {
        // "place = IDENTIFIER, { index_suffix | member_suffix }": the
        // root is the name the chain begins with.
        let mut root = place.clone();
        while matches!(root.kind(), SyntaxKind::MemberExpr | SyntaxKind::IndexExpr) {
            let Some(base) = root.children().next() else {
                return self.unread(node);
            };
            root = base;
        }
        let local = (root.kind() == SyntaxKind::NameExpr)
            .then(|| self.local_of(&root))
            .flatten();
        let Some(local) = local else {
            return self.unread(node);
        };
        let at = self.at(node);
        let operator = tokens(node).find_map(|token| compound(token.kind()));
        let operator_at = tokens(node)
            .find(|token| compound(token.kind()).is_some())
            .map_or(at, |token| Location::new(self.file, token.span()));
        // TIME-7: "`out` is read in a `when` and in `stop`, and written
        // only in a `when`". In `start` reading the place reports it.
        let is_out = Some(local) == self.out;
        if is_out && self.hook == Some(Hook::Stop) {
            self.out_lifecycle(
                self.at(place),
                "`out` is written only in a `when`, and this is `stop`".to_string(),
            );
        }
        let writable = is_out
            || matches!(
                self.locals[local].kind,
                LocalKind::Var | LocalKind::State | LocalKind::Cache
            );
        let mut target = self.expression(place, None);
        // A hook reads `out` of an atomic as its whole V (`read_local`);
        // as the root of a place it is the atomic it is declared, a part
        // that publishes whole (TYPE-9).
        if is_out {
            root_as_declared(&mut target, &self.locals);
        }
        // NATIVE-6: "None is assigned." TYPE-16: a delta that is a type
        // of its own "has properties, none is assigned". Each is
        // read-only wherever the value is reached from:
        // `items[0].size`.
        if let Some((what, rule)) = Self::read_only_inside(&target) {
            self.diagnostics.emit(error(
                Category::Name,
                codes::BINDING_IMMUTABLE,
                self.at(place),
                format!("this is a property of {what}, and a property is read and not assigned"),
                rule,
            ));
            let value = self.expression(value, Some(&target.ty));
            return Stmt::AssignPlace {
                place: target,
                operator,
                value,
                at,
            };
        }
        if !writable {
            self.diagnostics.emit(
                error(
                    Category::Name,
                    codes::BINDING_IMMUTABLE,
                    self.at(place),
                    format!(
                        "`{}` is {} and is read-only all the way down: no field or element \
                         of it is assigned",
                        self.locals[local].name,
                        self.locals[local].kind.describe()
                    ),
                    "EXPR-3",
                )
                .with_related(self.locals[local].at, "declared here")
                .with_note("copy it to a `var`, and assign into that"),
            );
            // The value is still read, for what it reports itself.
            let value = self.expression(value, Some(&target.ty));
            return Stmt::AssignPlace {
                place: target,
                operator,
                value,
                at,
            };
        }
        let value = match operator {
            None => {
                let value = self.expression(value, Some(&target.ty));
                self.fit(value, &target.ty, "the type of the place")
            }
            Some(operator) => {
                // `place += value` assigns what `place + value` is: the
                // place is the left operand, and has been read.
                let left = Given::read(0, place.clone(), target.clone());
                let right = self.operand(1, value.clone());
                let identity = self.symbol_operator(operator.identity());
                let (kind, result) = self.symbol(
                    (operator, operator_at),
                    identity,
                    (left, right),
                    (Vec::new(), false, false),
                    Some(&target.ty),
                );
                self.conforms(&result, &target.ty, self.at(value), "the type of the place");
                Expr {
                    kind,
                    ty: result,
                    at,
                }
            }
        };
        // EXPR-15: in a graph the `var` is rebound, so the assignment
        // is one to the `var`. CTRL-15: "In such a loop an assignment
        // to a `var` declared outside it rejects".
        if self.graph && self.dynamic.is_some_and(|outside| local < outside) {
            self.diagnostics.emit(
                error(
                    Category::Phase,
                    codes::LOOP_CARRIED,
                    self.at(place),
                    format!(
                        "`{}` is declared outside this loop, which makes one child for each \
                         member: nothing is carried from one child to the next",
                        self.locals[local].name
                    ),
                    "CTRL-15",
                )
                .with_related(self.locals[local].at, "declared here"),
            );
        } else if self.graph {
            // What the `var` becomes turns on the value and on every
            // index of the place: `row[index] = 3.0` with `index` a
            // connection selects the element as the graph runs.
            let mut parts = vec![&value];
            place_indices(&target, &mut parts);
            let categories: Vec<(ValueCategory, Location)> = parts
                .iter()
                .map(|part| (self.category(part), part.at))
                .collect();
            let given = categories
                .iter()
                .find(|(category, _)| *category == ValueCategory::Connection)
                .or_else(|| {
                    categories
                        .iter()
                        .find(|(category, _)| *category == ValueCategory::Unsettled)
                })
                .copied()
                .unwrap_or((ValueCategory::Value, value.at));
            self.assigned_category(local, given, true);
        }
        Stmt::AssignPlace {
            place: target,
            operator,
            value,
            at,
        }
    }

    /// The properties of the delta of `of`, each with its type, in
    /// the order TYPE-16 has them. `None` for a type whose delta is no
    /// type of its own, or is not known yet.
    ///
    /// "`set<K>`: ... the properties `added` and `removed`, each a
    /// `set<K>`"; "`dict<K, V>`: ... the properties `updated`, a
    /// `dict<K, delta<V>>` of each key added or changed with its delta,
    /// and `removed`, a `set<K>`"; "`V?`: ... an optional, with the
    /// property `value`, a `delta<V>?`: null for a published null"; "A
    /// struct: ... a property for each field `name: F`, of the same
    /// name and of type `delta<F>?`: null where the field did not
    /// change".
    fn delta_properties(&self, of: &Type) -> Option<Vec<(String, Type)>> {
        match of {
            Type::Set(_) => Some(vec![
                ("added".to_string(), of.clone()),
                ("removed".to_string(), of.clone()),
            ]),
            // "`list<V>`, `list<V, N>`: ... the properties `updated`, a
            // `dict<i64, delta<V>>` of each changed position with its
            // delta, `removed`, a `set<i64>` of the positions that lost
            // their value, and `size_modified`, an `i64?`".
            Type::List { element, .. } => {
                let position = || Box::new(Type::Scalar(Scalar::I64));
                Some(vec![
                    (
                        "updated".to_string(),
                        Type::Dict {
                            key: position(),
                            value: Box::new(Type::delta((**element).clone())),
                        },
                    ),
                    ("removed".to_string(), Type::Set(position())),
                    (
                        "size_modified".to_string(),
                        nullable(Type::Scalar(Scalar::I64)),
                    ),
                ])
            }
            Type::Dict { key, value } => Some(vec![
                (
                    "updated".to_string(),
                    Type::Dict {
                        key: key.clone(),
                        value: Box::new(Type::delta((**value).clone())),
                    },
                ),
                ("removed".to_string(), Type::Set(key.clone())),
            ]),
            Type::Nullable(payload) => Some(vec![(
                "value".to_string(),
                nullable(Type::delta((**payload).clone())),
            )]),
            Type::Named {
                module,
                declaration,
                arguments,
                kind: NamedKind::Struct,
                ..
            } => {
                let Some(Detail::Struct(read)) =
                    self.program.modules[*module].details.get(*declaration)
                else {
                    return None;
                };
                // Each field's type, with the struct's generic
                // arguments in place, as a field read has it.
                let map: Vec<(Location, &Argument)> =
                    fixed(&read.generics).zip(arguments).collect();
                let own: Vec<Location> = read.generics.iter().map(|generic| generic.at).collect();
                Some(
                    read.members
                        .iter()
                        .map(|member| {
                            let ty = placed(substitute_within(&member.ty, &map), &own);
                            // A field whose type is not known has a
                            // delta that is not known either.
                            let delta = if open(&ty) {
                                ty
                            } else {
                                nullable(Type::delta(ty))
                            };
                            (member.name.clone(), delta)
                        })
                        .collect(),
                )
            }
            _ => None,
        }
    }

    /// A property of a delta that is a type of its own (TYPE-16):
    /// `change.added`, of a set's delta; `change.updated`, of a
    /// dictionary's; `change.value`, of a nullable's; `change.bid`, of
    /// a struct's.
    fn delta_property(&mut self, base: Expr, name: &SyntaxToken, of: &Type) -> (ExprKind, Type) {
        let Some(properties) = self.delta_properties(of) else {
            return (ExprKind::Unread, Type::Unknown);
        };
        let member = properties
            .iter()
            .position(|(property, _)| property == name.text());
        let Some(member) = member else {
            self.diagnostics.emit(
                error(
                    Category::Name,
                    codes::MEMBER_UNKNOWN,
                    Location::new(self.file, name.span()),
                    format!("`{}` has no property `{}`", base.ty, name.text()),
                    "TYPE-16",
                )
                .with_note(format!("it has {}", listed(&properties))),
            );
            return (ExprKind::Unread, Type::Unknown);
        };
        let ty = properties[member].1.clone();
        let kind = ExprKind::Field {
            base: Box::new(base),
            member,
        };
        (kind, ty)
    }

    /// `delta<set<i64>>(added: [1, 2], removed: [3])`: a delta made
    /// from its properties.
    ///
    /// TYPE-16: "One that is a type of its own is constructed by its
    /// properties, by name ... A property left out is empty, or null
    /// for a struct's field, a nullable's `value`, and a list's
    /// `size_modified`. A tuple's delta is an ordinary tuple, and is
    /// written as one is." The constructor of a type that has no
    /// delta of its own is not read.
    fn delta_constructor(&mut self, node: &SyntaxNode) -> (ExprKind, Type) {
        let Some(list) = child(node, SyntaxKind::ArgList) else {
            return (ExprKind::Unread, Type::Unknown);
        };
        // The delta is recorded where the constructor starts.
        let written = self.written.get(&(self.file, node.span().start)).copied();
        let properties = match written {
            Some(Type::Delta(of)) => self.delta_properties(of),
            _ => None,
        };
        let (Some(ty), Some(properties)) = (written, properties) else {
            return self.unread_call(&list);
        };
        let ty = ty.clone();
        let mut given = vec![false; properties.len()];
        let mut arguments = Vec::new();
        // Each property given, with its value as written and its type,
        // for the constants among its entries.
        let mut written = Vec::new();
        for (name, value) in self.call_arguments(&list) {
            let position = name.as_ref().and_then(|name| {
                properties
                    .iter()
                    .position(|(property, _)| property == name.text())
            });
            let Some(position) = position else {
                // A name that is no property of the delta. An argument
                // with no name has none to report.
                if let Some(name) = &name {
                    self.diagnostics.emit(
                        error(
                            Category::Name,
                            codes::DELTA_ARGUMENT_NAME,
                            Location::new(self.file, name.span()),
                            format!("`{ty}` has no property `{}`", name.text()),
                            "TYPE-16",
                        )
                        .with_note(format!("it has {}", listed(&properties))),
                    );
                }
                arguments.push(hir::Argument {
                    parameter: None,
                    value: self.expression(&value, Some(&Type::Unknown)),
                });
                continue;
            };
            if std::mem::replace(&mut given[position], true)
                && let Some(name) = &name
            {
                self.diagnostics.emit(error(
                    Category::Name,
                    codes::DELTA_DUPLICATE_ARGUMENT,
                    Location::new(self.file, name.span()),
                    format!("`{}` is given twice", name.text()),
                    "TYPE-16",
                ));
            }
            let (property, expected) = &properties[position];
            let read = self.expression(&value, Some(expected));
            let place = format!("the property `{property}`");
            arguments.push(hir::Argument {
                parameter: Some(position),
                value: self.fit(read, expected, &place),
            });
            written.push((position, value, expected.clone()));
        }
        self.delta_overlap(&ty, &written);
        self.delta_bounds(&ty, &written);
        (ExprKind::Delta { arguments }, ty)
    }

    /// The entries of the properties `written` gives at `position` that
    /// are written out, `[a, b]` or `[k: v]`: each member, or each key,
    /// with what it is as a constant of the property's element or key
    /// type (`crate::constant`). One that is no constant is known only
    /// as the program runs, and is left out.
    fn delta_entries(
        &self,
        written: &[(usize, SyntaxNode, Type)],
        position: usize,
    ) -> Vec<(Location, ConstantValue)> {
        let mut entries = Vec::new();
        for (_, value, ty) in written.iter().filter(|(at, _, _)| *at == position) {
            let element = match ty {
                Type::Set(element) | Type::List { element, .. } => element.as_ref(),
                Type::Dict { key, .. } => key.as_ref(),
                _ => continue,
            };
            let value = ungrouped(value.clone());
            if value.kind() != SyntaxKind::SequenceExpr {
                continue;
            }
            for entry in value.children() {
                // `key: value`: the key is the first child.
                let entry = match entry.kind() {
                    SyntaxKind::SequencePair => match entry.children().next() {
                        Some(key) => key,
                        None => continue,
                    },
                    _ => entry,
                };
                if let Ok(constant) = self.read_constant(&entry, Some(element)) {
                    entries.push((self.at(&entry), constant));
                }
            }
        }
        entries
    }

    /// TYPE-16: "A `list<V, N>` keeps its bounds: a constant entry past
    /// them rejects (`delta.index_bounds`)". The catalogue's row is
    /// "Negative/out-of-fixed-range index": a constant position of any
    /// list's delta, `updated` or `removed`, that is negative rejects
    /// too. A position known only as the program runs is checked then.
    fn delta_bounds(&mut self, ty: &Type, written: &[(usize, SyntaxNode, Type)]) {
        let Type::Delta(of) = ty else {
            return;
        };
        let Type::List { size, .. } = &**of else {
            return;
        };
        let size = match size.as_ref().map(|size| &size.value) {
            Some(ConstantValue::Int(size)) => Some(*size),
            _ => None,
        };
        // A list's `updated` is keyed by position, and its `removed`
        // holds positions.
        let mut outside = Vec::new();
        for position in [0, 1] {
            for (at, value) in self.delta_entries(written, position) {
                if let ConstantValue::Int(value) = value
                    && (value < 0 || size.is_some_and(|size| value >= size))
                {
                    outside.push((at, value));
                }
            }
        }
        for (at, value) in outside {
            let bounds = match size {
                Some(0) => "it has none".to_string(),
                Some(size) => format!("its positions are 0 to {}", size - 1),
                None => "a position is 0 or more".to_string(),
            };
            self.diagnostics.emit(error(
                Category::Type,
                codes::DELTA_INDEX_BOUNDS,
                at,
                format!("`{of}` has no position {value}: {bounds}"),
                "TYPE-16",
            ));
        }
    }

    /// TYPE-16: "A delta that names one member, key or position both as
    /// added or updated and as removed is malformed. A constant one is
    /// rejected (`delta.overlap`)." A set's `added`, a dictionary's and
    /// a list's `updated`, are the first property, and `removed` the
    /// second. What is compared is each entry written as a literal; one
    /// that is not is known only as the graph runs, which "makes no
    /// check". The later of the two entries is reported.
    fn delta_overlap(&mut self, ty: &Type, written: &[(usize, SyntaxNode, Type)]) {
        let Type::Delta(of) = ty else {
            return;
        };
        if !matches!(**of, Type::Set(_) | Type::Dict { .. } | Type::List { .. }) {
            return;
        }
        // The constant a member, a key or a position is, by its value
        // (`ConstantValue::same`): `-1`, an enum's member however it is
        // written, a module's constant, `60s` and `1m` alike.
        let removed = self.delta_entries(written, 1);
        let changed = self.delta_entries(written, 0);
        for (changed, value) in changed {
            let clash = removed
                .iter()
                .find(|(_, other)| other.same(&value) == Some(true))
                .map(|(gone, _)| *gone);
            if let Some(gone) = clash {
                let later = if gone.span.start > changed.span.start {
                    gone
                } else {
                    changed
                };
                self.diagnostics.emit(
                    error(
                        Category::Type,
                        codes::DELTA_OVERLAP,
                        later,
                        format!("`{of}`'s delta names this both as changed and as removed"),
                        "TYPE-16",
                    )
                    .with_note("a delta that names one entry both ways is malformed"),
                );
            }
        }
    }

    /// What a place reaches into that takes no assignment, and the rule
    /// that says so: a native value, whose properties are read-only
    /// (NATIVE-6), or a delta that is a type of its own (TYPE-16). The
    /// place is a property of one or something inside one, however the
    /// value is reached: `items[0].size`, with `items` a list of a
    /// native type, is one. ERR-4: "`.context` is a read-only nullable
    /// record": an error's context and its parts are read-only too.
    fn read_only_inside(place: &Expr) -> Option<(String, &'static str)> {
        if let ExprKind::ErrorPart { part, .. } = &place.kind {
            return match part {
                ErrorPart::Context => Some(("`error`".to_string(), "ERR-4")),
                ErrorPart::Node | ErrorPart::Phase | ErrorPart::EvaluationTime => {
                    Some(("an error's context".to_string(), "ERR-4"))
                }
                ErrorPart::Message | ErrorPart::Code => None,
            };
        }
        let (ExprKind::Field { base, .. } | ExprKind::Index { base, .. }) = &place.kind else {
            return None;
        };
        match &base.ty {
            Type::Named {
                kind: NamedKind::Native,
                ..
            } => Some((format!("`{}`, a native type", base.ty), "NATIVE-6")),
            Type::Delta(_) => Some((format!("`{}`, a delta", base.ty), "TYPE-16")),
            _ => Self::read_only_inside(base),
        }
    }

    /// Reports an assignment whose place is a property of a native
    /// type: `clock.evaluation_time = value` (NATIVE-6, TIME-8). True
    /// when it is one.
    ///
    /// Places inside a value are not read yet, so this looks at the one
    /// form it can judge without reading: `name.property`, where the
    /// name is a local of a native type, or a capability, which is of
    /// one whether or not core has declared it.
    fn assigned_property(&mut self, place: &SyntaxNode) -> bool {
        if place.kind() != SyntaxKind::MemberExpr {
            return false;
        }
        // A place begins with a name: `place = IDENTIFIER, { index_suffix
        // | member_suffix }`.
        let Some(base) = place.children().next() else {
            return false;
        };
        let Some(local) = (base.kind() == SyntaxKind::NameExpr)
            .then(|| self.local_of(&base))
            .flatten()
        else {
            return false;
        };
        let native = match &self.locals[local].ty {
            Type::Named {
                module,
                declaration,
                ..
            } => {
                self.program.modules[*module].declarations[*declaration].kind
                    == DeclarationKind::NativeType
            }
            _ => false,
        };
        if !native && !self.is_capability(local) {
            return false;
        }
        let property = tokens(place).last();
        self.diagnostics.emit(
            error(
                Category::Name,
                codes::BINDING_IMMUTABLE,
                self.at(place),
                format!(
                    "`{}.{}` is a property of a native type, and a property is read and \
                     not assigned",
                    self.locals[local].name,
                    property.as_ref().map_or("", |name| name.text())
                ),
                "NATIVE-6",
            )
            .with_related(self.locals[local].at, "declared here"),
        );
        true
    }

    /// The category of a graph's `var` against that of a value assigned
    /// to it.
    ///
    /// CTRL-14: "A `var` keeps the category of its first value
    /// (EXPR-3): a connection assigned to one that holds an ordinary
    /// value, or the reverse, rejects (EXPR-4). Under a temporal
    /// condition a `var` declared before the `if` is assigned in a
    /// branch only where it is a connection: one that holds an ordinary
    /// value would have its value chosen at runtime, and the assignment
    /// rejects." A `var` declared with no value takes the category of
    /// the first value it is assigned, and what is not settled is not
    /// judged.
    ///
    /// `given` is the category of what is assigned, and where it is
    /// written. `inside` is true for an assignment into a place of the
    /// `var` (EXPR-15). The `var` is then rebound to itself with one
    /// place another value: a connection stays one whatever the place
    /// is given, an ordinary value wired there as a constant (FN-3),
    /// and an ordinary value given a connection, or a place a
    /// connection selects, would become one.
    fn assigned_category(
        &mut self,
        local: LocalId,
        (given, at): (ValueCategory, Location),
        inside: bool,
    ) {
        use ValueCategory::{Connection, Unsettled, Value};
        if self.locals[local].category == Unsettled && !inside {
            self.locals[local].category = given;
        }
        let held = self.locals[local].category;
        // Declared before the nearest `if` selected at runtime.
        let chosen_at_runtime = self.temporal.is_some_and(|outside| local < outside);
        let name = self.locals[local].name.clone();
        let message = match (held, given) {
            (Value, Connection) => format!(
                "`{name}` holds an ordinary value, and this is a connection: a `var` keeps \
                 the category of its first value"
            ),
            (Connection, Value) if !inside => format!(
                "`{name}` is a connection, and this is an ordinary value: a `var` keeps the \
                 category of its first value"
            ),
            (Value, Value) if chosen_at_runtime => format!(
                "`{name}` holds an ordinary value, and this branch is selected at runtime: \
                 its value would not be known while the graph is built"
            ),
            _ => return,
        };
        self.diagnostics.emit(
            error(
                Category::Type,
                codes::BINDING_CATEGORY,
                at,
                message,
                "CTRL-14",
            )
            .with_related(self.locals[local].at, "declared here"),
        );
    }

    // ---- Expressions ----

    /// The expression `node`, typed. `expected` is the type its place
    /// expects, which gives a literal or a sequence its type (EXPR-5);
    /// whether the result fits is for [`Typer::fit`].
    fn expression(&mut self, node: &SyntaxNode, expected: Option<&Type>) -> Expr {
        // TIME-9: a place that expects an endpoint is given one, and no
        // value: the argument of an `endpoint<T>` parameter.
        if let Some(Type::Endpoint(_)) = expected {
            return self.endpoint_given(node);
        }
        // TYPE-16: "A `delta<V>` is a value where a `delta<V?>` is
        // expected". A form that takes its type from its place is read
        // as the `delta<V>`: its entries and its elements are held to
        // that type, a list's delta being a dictionary. The place may
        // be one that can be left out, a `delta<V?>?`, as a struct's
        // delta has for a field. A block and an `if` keep the place's
        // own type, so that a branch may give the whole `delta<V?>`,
        // and each value in them is read here in its turn. `null` is
        // no `delta<V>`: it is left to say so, or to leave the place
        // out.
        if let Some(Type::Delta(of)) = not_null(expected)
            && let Type::Nullable(payload) = &**of
            // A literal, a tuple, a sequence and a symbol before an
            // operand; and `.member`, `ok(value)` and `err(value)`,
            // which have no type but their place's (ERR-2).
            && (matches!(
                node.kind(),
                SyntaxKind::Literal
                    | SyntaxKind::TupleExpr
                    | SyntaxKind::SequenceExpr
                    | SyntaxKind::UnaryExpr
            ) || takes_its_type(node))
            && !written_null(node)
        {
            let lifted = Type::delta((**payload).clone());
            return self.expression(node, Some(&lifted));
        }
        let at = self.at(node);
        let (kind, ty) = match node.kind() {
            SyntaxKind::Literal => self.literal(node, expected),
            SyntaxKind::NameExpr => return self.name(node),
            // "Parentheses select grouping" (EXPR-1): the expression
            // inside, where it stands.
            SyntaxKind::ParenExpr => match node.children().next() {
                Some(inner) => return self.expression(&inner, expected),
                None => (ExprKind::Unread, Type::Unknown),
            },
            SyntaxKind::TupleExpr => self.tuple(node, expected),
            SyntaxKind::SequenceExpr => self.sequence(node, expected),
            SyntaxKind::UnaryExpr => self.unary(node, expected),
            SyntaxKind::BinaryExpr => self.binary(node, expected),
            SyntaxKind::CallExpr => self.call(node, expected),
            SyntaxKind::TypedConstructor => self.typed_constructor(node, expected),
            SyntaxKind::DeltaConstructor => self.delta_constructor(node),
            SyntaxKind::UndecidedConstructor => {
                // GEN-14: the resolver decided what the node is. One
                // that is comparisons is for a later part.
                let constructor = self
                    .module
                    .decisions
                    .iter()
                    .any(|decision| decision.at == at && decision.constructor == Some(true));
                if constructor {
                    self.typed_constructor(node, expected)
                } else {
                    (ExprKind::Unread, Type::Unknown)
                }
            }
            SyntaxKind::MemberExpr => self.member(node),
            SyntaxKind::TypeCall => self.type_call(node),
            SyntaxKind::TryExpr => self.propagation(node),
            // An anonymous function's body is for a later part. What it
            // captures of a capability is reported here, and what it
            // reads of a node's inputs where it is written.
            SyntaxKind::FnExpr => {
                self.captured_capabilities(node);
                self.anonymous_reads(node);
                (ExprKind::Unread, Type::Unknown)
            }
            SyntaxKind::ImpliedMember => self.implied_member(node, expected),
            SyntaxKind::IndexExpr => self.index(node),
            // TST-3: "Eval traces are test-local values." An `eval`
            // anywhere but in a named test is not read.
            SyntaxKind::EvalExpr if self.testing => self.eval(node),
            // TST-17: "beside `eval` it writes `run(call, config)`
            // (RUN-3), whose value is the run's output as an eval's
            // is."
            SyntaxKind::RunExpr if self.testing => self.eval(node),
            // A block or an `if` whose value is used.
            SyntaxKind::Block | SyntaxKind::IfExpr => {
                let (value, outcome) = self.flow(node, expected, true);
                // CTRL-10: "A block whose last statement is an
                // expression has that expression's value and type; any
                // other block has none."
                if matches!(outcome, Outcome::Nothing) {
                    self.diagnostics.emit(error(
                        Category::Type,
                        codes::TYPE_MISMATCH,
                        at,
                        "this has no value: a block's value is its last statement, when that \
                         is an expression"
                            .to_string(),
                        "CTRL-10",
                    ));
                }
                // TYPE-14: what is null alone is of the type its place
                // expects, and rejects where nothing gives it one.
                if matches!(outcome, Outcome::Null) {
                    let mut value = value;
                    match expected {
                        Some(expected) if !open(expected) => {
                            value.ty = nullable(expected.clone());
                        }
                        Some(_) => {}
                        None => self.diagnostics.emit(error(
                            Category::Type,
                            codes::NULL_CONTEXT,
                            at,
                            "this is null when its condition does not hold, and nothing here \
                             gives that a type: write the type"
                                .to_string(),
                            "TYPE-14",
                        )),
                    }
                    return value;
                }
                return value;
            }
            _ => (ExprKind::Unread, Type::Unknown),
        };
        if matches!(kind, ExprKind::Unread) {
            self.forget();
        }
        if self.graph {
            self.constants_of(&kind);
        }
        let value = Expr { kind, ty, at };
        // TIME-1: a read in a hook of a temporal input's child.
        if matches!(node.kind(), SyntaxKind::IndexExpr | SyntaxKind::MemberExpr) {
            return self.child_read(node, value);
        }
        value
    }

    /// What a value read in a graph is (FN-3). A connection: a
    /// temporal parameter, a local given one, the result of a temporal
    /// function or candidate, or of a call given one, which is lifted
    /// (FN-6). Any other is an ordinary value, known while the graph is
    /// built, but for what an operation the graph requires gives
    /// (GEN-7): which candidate that is is selected where the generic
    /// parameters are known, and in composition it may be a temporal
    /// one (GEN-5), so with no connection among its arguments what it
    /// gives is not settled here.
    fn category(&self, value: &Expr) -> ValueCategory {
        use ValueCategory::{Connection, Unsettled, Value};
        let program = self.program;
        // Several values taken together: a connection where one is, and
        // not settled where one is not and none is a connection.
        let join = |one: ValueCategory, other: ValueCategory| match (one, other) {
            (Connection, _) | (_, Connection) => Connection,
            (Unsettled, _) | (_, Unsettled) => Unsettled,
            _ => Value,
        };
        let all = |values: &mut dyn Iterator<Item = &Expr>| {
            values.map(|value| self.category(value)).fold(Value, join)
        };
        match &value.kind {
            ExprKind::Local(local) => self.locals[*local].category,
            ExprKind::Call {
                callee, arguments, ..
            } => {
                let given = all(&mut arguments.iter().map(|argument| &argument.value));
                match callee {
                    Callee::Function {
                        module,
                        declaration,
                        ..
                    } if matches!(
                        program.modules[*module].declarations[*declaration].kind,
                        DeclarationKind::Function(Role::Temporal)
                            | DeclarationKind::NativeFunction(Role::Temporal)
                    ) =>
                    {
                        Connection
                    }
                    Callee::Candidate {
                        module,
                        declaration,
                        ..
                    } if matches!(
                        program.modules[*module].declarations[*declaration]
                            .kind
                            .candidate(),
                        Some((Role::Temporal, _))
                    ) =>
                    {
                        Connection
                    }
                    Callee::Required { .. } => join(given, Unsettled),
                    _ => given,
                }
            }
            ExprKind::Construct { arguments, .. } | ExprKind::Delta { arguments } => {
                all(&mut arguments.iter().map(|argument| &argument.value))
            }
            ExprKind::Unary { operand, .. } => self.category(operand),
            ExprKind::Binary { left, right, .. } => join(self.category(left), self.category(right)),
            ExprKind::Widen(inner) => self.category(inner),
            // OPS-6: the list converted is what it is given, lifted
            // over a connection as a conversion is (FN-6).
            ExprKind::ParentList(inner) => self.category(inner),
            ExprKind::Field { base, .. } => self.category(base),
            // An enum constructed from a connection is lifted (FN-6).
            ExprKind::EnumFrom { value, .. } => self.category(value),
            // A result form over a connection is lifted, as a value
            // function's call is (FN-6, ERR-3): what it gives is what it
            // is given.
            ExprKind::Tagged { payload, .. } => self.category(payload),
            ExprKind::TagTest { value, .. } => self.category(value),
            ExprKind::Payload { result, .. } => self.category(result),
            ExprKind::ErrorPart { base, .. } => self.category(base),
            ExprKind::Propagate(operand) => self.category(operand),
            // ERR-7: a capture is a temporal boundary, and what is read
            // of it a connection.
            ExprKind::Capture(_) | ExprKind::Captured { .. } => Connection,
            ExprKind::Error { message, code } => {
                code.iter().fold(self.category(message), |given, code| {
                    join(given, self.category(code))
                })
            }
            ExprKind::Index { base, index } => join(self.category(base), self.category(index)),
            ExprKind::Tuple(entries) | ExprKind::Sequence(entries) => all(&mut entries.iter()),
            ExprKind::Pairs(entries) => {
                all(&mut entries.iter().flat_map(|(key, value)| [key, value]))
            }
            ExprKind::Block {
                value: Some(value), ..
            } => self.category(value),
            // CTRL-13: "It is a connection when its condition is one or
            // either branch's value is". CTRL-10: "A branch that returns
            // from the function gives the `if` no value and takes no
            // part in its type (CTRL-5)", and so none in what it is. An
            // `if` no branch of which gives a value has none itself.
            ExprKind::If { condition, .. } => {
                let mut branches = value
                    .kind
                    .branch_values()
                    .into_iter()
                    .flatten()
                    .filter(|branch| gives_a_value(branch))
                    .map(|branch| self.category(branch));
                match branches.next() {
                    Some(first) => join(self.category(condition), branches.fold(first, join)),
                    None => Value,
                }
            }
            _ => Value,
        }
    }

    /// FN-3: "Where a constant is required ... a `const` parameter's
    /// argument ... a connection rejects." A connection has no value
    /// while the graph is built. TIME-2: "Composition describes a typed
    /// graph without reading live payloads."
    fn constants_of(&mut self, kind: &ExprKind) {
        let ExprKind::Call {
            callee, arguments, ..
        } = kind
        else {
            return;
        };
        let (module, declaration) = match callee {
            Callee::Function {
                module,
                declaration,
                ..
            }
            | Callee::Candidate {
                module,
                declaration,
                ..
            } => (*module, *declaration),
            // An operation the body requires: the operator's own
            // signature says which of its parameters is `const`
            // (GEN-4).
            Callee::Required {
                operator: (module, declaration),
                ..
            } => (*module, *declaration),
            Callee::Unread => return,
        };
        let Some(Detail::Callable(signature)) =
            self.program.modules[module].details.get(declaration)
        else {
            return;
        };
        for argument in arguments {
            let parameter = argument
                .parameter
                .and_then(|index| signature.parameters.get(index));
            let Some(parameter) = parameter else {
                continue;
            };
            // What is not settled is not judged: it may be a value.
            if parameter.constant && self.category(&argument.value) == ValueCategory::Connection {
                self.diagnostics.emit(error(
                    Category::Phase,
                    codes::CONNECTION_AS_CONSTANT,
                    argument.value.at,
                    format!(
                        "`{}` is a `const` parameter, and this is a connection: it has no \
                         value while the graph is built",
                        parameter.name
                    ),
                    "FN-3",
                ));
            }
        }
    }

    /// What a form this pass does not read assigns is not followed, so
    /// every `var` may have a value after it.
    fn forget(&mut self) {
        self.assigned.fill(true);
    }

    /// A literal, with the type its form gives it. `null` has the type
    /// its place expects, which is a nullable one (TYPE-14).
    fn literal(&mut self, node: &SyntaxNode, expected: Option<&Type>) -> (ExprKind, Type) {
        let Some(token) = tokens(node).next() else {
            return (ExprKind::Unread, Type::Unknown);
        };
        // A literal the lexer or `forms` has reported has no value
        // here, and no type: it is reported once (ERR-9). Its value is
        // read as every constant's is (`crate::constant::literal`).
        let read = match token.kind() {
            SyntaxKind::KwNull => {
                return match expected {
                    // TYPE-14: "`T?` denotes an ordinary value that is
                    // either a T or null."
                    Some(expected @ Type::Nullable(_)) => {
                        (ExprKind::Literal(Value::Null), expected.clone())
                    }
                    // "It does not make null an admitted value of
                    // unqualified T."
                    Some(expected) if !open(expected) => {
                        let report = error(
                            Category::Type,
                            codes::TYPE_MISMATCH,
                            self.at(node),
                            format!("`null` is no value of `{expected}`: write `{expected}?`"),
                            "TYPE-14",
                        );
                        // TYPE-16: null in a delta's place is nothing
                        // published, and a published null is the delta
                        // that holds one.
                        let optional = matches!(expected, Type::Delta(of)
                            if matches!(**of, Type::Nullable(_)));
                        self.diagnostics.emit(if optional {
                            report.with_note(format!("a published null is `{expected}()`"))
                        } else {
                            report
                        });
                        (ExprKind::Literal(Value::Null), Type::Unknown)
                    }
                    // What the place expects is not known.
                    Some(_) => (ExprKind::Literal(Value::Null), Type::Unknown),
                    // "`null` has the nullable type its place expects,
                    // and rejects where nothing gives it one."
                    None => {
                        self.diagnostics.emit(error(
                            Category::Type,
                            codes::NULL_CONTEXT,
                            self.at(node),
                            "`null` has the type its place expects, and nothing here gives \
                             one: write the type"
                                .to_string(),
                            "TYPE-14",
                        ));
                        (ExprKind::Literal(Value::Null), Type::Unknown)
                    }
                };
            }
            _ => crate::constant::literal(&token),
        };
        match read {
            Some((value, scalar)) => (ExprKind::Literal(value), Type::Scalar(scalar)),
            None => (ExprKind::Unread, Type::Unknown),
        }
    }

    /// The local a name is, when it names a parameter or a local this
    /// pass has read.
    fn local_of(&self, node: &SyntaxNode) -> Option<LocalId> {
        let name = name_token(node)?;
        match self.bindings.get(&(self.file, name.span().start))? {
            Binding::Local { at, .. } => self.declared.get(at).copied(),
            Binding::Declarations { .. } => None,
        }
    }

    /// A name: a parameter, a local, a constant of a module, or a
    /// generic value parameter. Any other, a function or a type or a
    /// name left for the prelude, is for the pass that reads calls and
    /// members.
    fn name(&mut self, node: &SyntaxNode) -> Expr {
        let at = self.at(node);
        let unread = Expr {
            kind: ExprKind::Unread,
            ty: Type::Unknown,
            at,
        };
        if let Some(local) = self.local_of(node) {
            return self.read_local(local, node);
        }
        let Some(name) = name_token(node) else {
            return unread;
        };
        // UNIT-13: "A constant is read by its name wherever an ordinary
        // value is." Its type is the one it is written with.
        if let Some((module, &[declaration])) = self.declarations(&name)
            && let Some(Detail::Constant(ty)) =
                self.program.modules[module].details.get(declaration)
        {
            return Expr {
                kind: ExprKind::Constant {
                    module,
                    declaration,
                },
                ty: ty.clone(),
                at,
            };
        }
        let Some(Binding::Local {
            at: declared,
            kind: LocalKind::GenericParameter,
        }) = self.bindings.get(&(self.file, name.span().start))
        else {
            return unread;
        };
        // GEN-13: a generic parameter that is a value has the type it
        // is declared with. One that is a type is no expression.
        let generic = self
            .signature
            .generics
            .iter()
            .find(|generic| generic.at == *declared);
        match generic.map(|generic| &generic.kind) {
            Some(GenericKind::Value(ty)) => Expr {
                kind: ExprKind::GenericValue {
                    name: name.text().to_string(),
                    at: *declared,
                },
                ty: ty.clone(),
                at,
            },
            _ => unread,
        }
    }

    /// True when `local` is a capability (TIME-8): one the node
    /// injects, other than `out`, which is its result; a parameter of a
    /// capability's type; or a local one of those has been bound to.
    fn is_capability(&self, local: LocalId) -> bool {
        self.capability(local).is_some()
    }

    /// The capability `local` is: itself, when the node injects it or it
    /// is a parameter of a capability's type; and for a local one of
    /// those has been bound to, that one.
    fn capability(&self, local: LocalId) -> Option<LocalId> {
        if let Some(capability) = self.aliases.get(&local) {
            return Some(*capability);
        }
        let own = (self.locals[local].kind == LocalKind::Inject && Some(local) != self.out)
            || self.program.is_capability_type(&self.locals[local].ty);
        own.then_some(local)
    }

    /// The capability the value of `value` is: of a read of one, that
    /// one; of a block or an `if`, the one its value is.
    fn capability_of(&self, value: &Expr) -> Option<LocalId> {
        match &value.kind {
            ExprKind::Local(local) => self.capability(*local),
            // The value of a block, or of a branch of an `if`.
            kind => kind
                .branch_values()?
                .into_iter()
                .find_map(|value| self.capability_of(value)),
        }
    }

    /// True when the function acknowledges the warning `code` (ERR-10):
    /// "At a named function, `@suppress_warning(\"node.no_activation\")`
    /// acknowledges the dormant-node warning for that declaration."
    fn acknowledges(&self, code: &str) -> bool {
        self.declaration
            .syntax
            .children()
            .filter(|annotation| annotation.kind() == SyntaxKind::WarningAnnotation)
            .filter_map(|annotation| {
                tokens(&annotation).find(|token| token.kind() == SyntaxKind::Str)
            })
            // The string's value, not its spelling: an escape, or a
            // text block, that says the code names the same warning.
            .any(|string| decode_string(string.text()).is_ok_and(|value| value == code))
    }

    /// TIME-8: a capability is not "a capture of an anonymous function
    /// (FN-14)". FN-14 has the resolver reject the use of an injected
    /// capability by its own name (`name.captured`); a `let` it is
    /// bound to, and a parameter of a capability's type, may be used
    /// there as any `let` or parameter is, and are reported here.
    fn captured_capabilities(&mut self, function: &SyntaxNode) {
        let captured: Vec<(LocalId, Location)> = function
            .descendants()
            .filter(|name| name.kind() == SyntaxKind::NameExpr)
            .filter_map(|name| {
                // A name of the function's own is no local of this
                // body, and is found by none.
                let local = self.local_of(&name)?;
                (self.locals[local].kind != LocalKind::Inject && self.is_capability(local))
                    .then(|| (local, self.at(&name)))
            })
            .collect();
        for (local, at) in captured {
            self.diagnostics.emit(
                error(
                    Category::Injectable,
                    codes::CAPABILITY_AS_VALUE,
                    at,
                    format!(
                        "`{}` is a capability, and is not retained: this is a capture of an \
                         anonymous function",
                        self.locals[local].name
                    ),
                    "TIME-8",
                )
                .with_note("give the function what it needs of the capability as an argument"),
            );
        }
    }

    /// The body of the anonymous function `function`, read where it is
    /// written in a node, for what it reads there.
    ///
    /// FN-14: "An anonymous function's scope is the scope around it
    /// plus its parameters ... its body uses only parameters, generic
    /// parameters, `let` locals, and iteration bindings." A temporal
    /// input is a parameter: in a hook it is the payload (FN-3), and
    /// TIME-1: "A payload read requires admission/readability". So each
    /// read is held to the admission where the function is written: its
    /// `when` header, the guards around it and those in the body itself
    /// (TIME-4), `start` and `stop` (TIME-5), a `state`'s or a
    /// `cache`'s initializer (TIME-5, TIME-1). An input's validity does
    /// not change within an evaluation, so a read proved where the
    /// function is written is proved wherever it is called in that
    /// evaluation.
    ///
    /// The function's parameters and result take their types from its
    /// accepting form (FN-7), which is not read, and so do its `return`
    /// and its `?` (CTRL-5, ERR-5). A parameter is of the type written
    /// for it, or of none known. What the body declares, assigns or
    /// proves is taken back, and of what the walk reports only the
    /// reads of the node's inputs stand: the rest would depend on those
    /// types, and is not judged here.
    fn anonymous_reads(&mut self, function: &SyntaxNode) {
        /// The reports of a read of a node's input where nothing admits
        /// it (TIME-1, TIME-4, TIME-5).
        const READS: [&str; 4] = [
            codes::PAYLOAD_UNGUARDED,
            codes::INPUT_LIFECYCLE,
            codes::STATE_INPUT_READ,
            codes::local::CACHE_INPUT_READ,
        ];
        if !(self.hook.is_some() || self.initializing || self.caching) {
            return;
        }
        let mark = self.mark();
        let undriven = self.undriven.len();
        let parameters = child(function, SyntaxKind::ParamList)
            .into_iter()
            .flat_map(|list| list.children())
            .filter(|parameter| parameter.kind() == SyntaxKind::LambdaParam);
        for parameter in parameters {
            let Some(name) = tokens(&parameter).find(|token| token.kind() == SyntaxKind::Ident)
            else {
                continue;
            };
            let ty = parameter
                .children()
                .find(|part| is_type(part.kind()))
                .and_then(|part| self.written.get(&(self.file, part.span().start)))
                .map_or(Type::Unknown, |ty| (*ty).clone());
            let at = Location::new(self.file, name.span());
            self.declare(name.text(), at, LocalKind::Parameter, ty, true);
        }
        let body = function.children().find_map(|part| match part.kind() {
            SyntaxKind::ArrowBody => part.children().next(),
            SyntaxKind::Block => Some(part),
            _ => None,
        });
        if let Some(body) = body {
            self.flow(&body, Some(&Type::Unknown), false);
        }
        let reads: Vec<_> = self.diagnostics.as_slice()[mark.diagnostics..]
            .iter()
            .filter(|diagnostic| READS.contains(&diagnostic.code.as_str()))
            .cloned()
            .collect();
        self.take_back(mark, false);
        // A `modified([])` of the body is no selector of the node's.
        self.undriven.truncate(undriven);
        for diagnostic in reads {
            self.diagnostics.emit(diagnostic);
        }
    }

    /// How the capability named at `node` is retained where it stands,
    /// in a few words for the message; `None` where it is used and not
    /// retained.
    ///
    /// TIME-8: "it is not the value of a `state` or a `cache`, an entry
    /// of a tuple, list, set, dictionary, or struct, what a function
    /// returns, what a node publishes, or a capture of an anonymous
    /// function (FN-14). Binding one to a `let` and giving one to a
    /// parameter of its type are uses." A capture is the resolver's to
    /// report (`name.captured`).
    fn retained(&self, node: &SyntaxNode) -> Option<&'static str> {
        // Where the capability goes: out through what only passes its
        // value on, to the place that takes it.
        let mut used = node.clone();
        loop {
            let parent = used.parent()?;
            match parent.kind() {
                // "Parentheses select grouping" (EXPR-1).
                SyntaxKind::ParenExpr => used = parent,
                // CTRL-10: "A block whose last statement is an
                // expression has that expression's value". Any other
                // statement's value is dropped, and is no use at all.
                SyntaxKind::ExprStmt => {
                    let block = parent
                        .parent()
                        .filter(|block| block.kind() == SyntaxKind::Block)?;
                    if block.children().last().as_ref() != Some(&parent) {
                        return None;
                    }
                    used = block;
                }
                // A branch of an `if` gives the `if` its value; its
                // condition does not.
                SyntaxKind::IfExpr
                    if matches!(used.kind(), SyntaxKind::Block | SyntaxKind::IfExpr) =>
                {
                    used = parent;
                }
                _ => break,
            }
        }
        let parent = used.parent()?;
        match parent.kind() {
            // FN-7: "A body that ends in an expression returns that
            // expression, as an arrow body does ... In a function that
            // declares no result the expression is a statement", whose
            // value is dropped.
            SyntaxKind::FnDecl if used.kind() == SyntaxKind::Block => {
                self.signature.result.as_ref().map(|_| "what is returned")
            }
            SyntaxKind::ArrowBody => self.signature.result.as_ref().map(|_| "what is returned"),
            SyntaxKind::StateDecl => Some("the value of a `state`"),
            SyntaxKind::CacheDecl => Some("the value of a `cache`"),
            SyntaxKind::TupleExpr => Some("an entry of a tuple"),
            SyntaxKind::SequenceExpr => Some("an entry of a list or a set"),
            // `[key: value]`: either side of a dictionary's entry.
            SyntaxKind::SequencePair => Some("an entry of a dictionary"),
            SyntaxKind::ReturnStmt => Some("what is returned"),
            // `yield time: value`: the value is what the source
            // publishes.
            SyntaxKind::YieldStmt => (parent.children().nth(1).as_ref() == Some(&used))
                .then_some("what the node publishes"),
            // `out = clock` publishes it; `kept = clock`, of a `state`
            // or a `cache`, keeps it.
            SyntaxKind::AssignStmt => {
                let place = parent.children().next()?;
                if place == used {
                    return None;
                }
                // `holder.clock = clock`, `kept[0] = clock`: the place
                // is a field or an element, which keeps it.
                if matches!(place.kind(), SyntaxKind::MemberExpr | SyntaxKind::IndexExpr) {
                    return Some("a field or an element of a value");
                }
                let target = (place.kind() == SyntaxKind::NameExpr)
                    .then(|| self.local_of(&place))
                    .flatten()?;
                if Some(target) == self.out {
                    return Some("what the node publishes");
                }
                match self.locals[target].kind {
                    LocalKind::State => Some("the value of a `state`"),
                    LocalKind::Cache => Some("the value of a `cache`"),
                    _ => None,
                }
            }
            // An argument: of a struct's construction, an entry of the
            // struct; of any other call, a use.
            SyntaxKind::Arg => {
                let call = parent.parent().and_then(|list| list.parent())?;
                // What is called: `Name(...)`, or with generic
                // arguments written `Name<T>(...)`, which is a struct's
                // construction only where the name is a struct's.
                let callee = match call.kind() {
                    SyntaxKind::CallExpr => call
                        .children()
                        .next()
                        .filter(|callee| callee.kind() == SyntaxKind::NameExpr)
                        .and_then(|callee| name_token(&callee)),
                    SyntaxKind::TypedConstructor | SyntaxKind::UndecidedConstructor => {
                        name_token(&call)
                    }
                    _ => None,
                };
                let constructs = callee
                    .and_then(|name| self.declarations(&name))
                    .is_some_and(|(module, declarations)| {
                        declarations.iter().any(|&declaration| {
                            self.program.modules[module].declarations[declaration].kind
                                == DeclarationKind::Struct
                        })
                    });
                constructs.then_some("an entry of a struct")
            }
            _ => None,
        }
    }

    /// A read of the local `local`, written at `node`. EXPR-3: "every
    /// path reaching a read must assign it."
    fn read_local(&mut self, local: LocalId, node: &SyntaxNode) -> Expr {
        let at = self.at(node);
        if !self.assigned[local] {
            self.diagnostics.emit(
                error(
                    Category::Name,
                    codes::BINDING_UNASSIGNED,
                    at,
                    format!(
                        "`{}` is read before it is given a value",
                        self.locals[local].name
                    ),
                    "EXPR-3",
                )
                .with_related(self.locals[local].at, "declared here"),
            );
            // One report for one `var`.
            self.assigned[local] = true;
        }
        if self.is_temporal_input(local)
            && (self.hook.is_some() || self.initializing || self.caching)
        {
            // TIME-1: where a child is read, `parts[1]`, the read is the
            // child's, judged where the child is (`child_read`); and a
            // child named as an endpoint, `valid(parts[1])`, is not read.
            let child = self.reads_children() && self.continues_chain(node, &self.locals[local].ty);
            if !child && self.naming != Some(at) {
                self.payload_read(local, at);
            }
        }
        self.signal_read(local, at);
        // TIME-7: "`out` is read in a `when` and in `stop` ... `start`
        // neither reads nor writes it". Nor does a `state`'s or a
        // `cache`'s initializer, which gives it its first value on
        // every start (TIME-6), before the node has published.
        if Some(local) == self.out {
            if self.hook == Some(Hook::Start) {
                self.out_lifecycle(
                    at,
                    "`out` is neither read nor written in `start`".to_string(),
                );
            } else if self.initializing || self.caching {
                self.out_lifecycle(
                    at,
                    "`out` is read in a `when` and in `stop`, and a `state` or a `cache` is \
                     given its first value on every start, before the node has published"
                        .to_string(),
                );
            }
        }
        // TIME-3: the node's scheduler or alarm used in `start` is what
        // may wake a handler that subscribes to no input. "Any use in
        // `start` counts; what a function given the capability does
        // with it is not followed."
        if self.hook == Some(Hook::Start)
            && self.capability(local).is_some_and(|capability| {
                let injected = &self.locals[capability];
                injected.kind == LocalKind::Inject
                    && matches!(injected.name.as_str(), "scheduler" | "alarm")
            })
        {
            self.driven = true;
        }
        // TIME-8: "A capability is used within the node that injects it
        // and is not retained". An injected capability other than `out`,
        // which is the node's result, and a parameter of a capability's
        // type are both one.
        if self.is_capability(local)
            && let Some(how) = self.retained(node)
        {
            self.diagnostics.emit(
                error(
                    Category::Injectable,
                    codes::CAPABILITY_AS_VALUE,
                    at,
                    format!(
                        "`{}` is a capability, and is not retained: this is {how}",
                        self.locals[local].name
                    ),
                    "TIME-8",
                )
                .with_note(
                    "a capability is bound to a `let`, given to a parameter of its type, or \
                     read through a property, `clock.evaluation_time`",
                ),
            );
        }
        // ERR-12: "Refine that same binding to T on paths proving a
        // non-null T? value".
        let ty = match &self.locals[local].ty {
            Type::Nullable(inner) if self.present.contains(&Fact::Present(local)) => {
                inner.as_ref().clone()
            }
            ty => ty.clone(),
        };
        // TYPE-9: "`atomic<V>` stops expansion at a complete ordinary
        // V", and TYPE-16: "an atomic publishes whole". In a hook an
        // atomic input's payload, and `out`'s, is that whole V, an
        // ordinary value. TYPE-14: the `?` of an input whose type is
        // no ordinary value marks one that may be left out, and no
        // null: what a hook reads of `atomic<Quote>?` is a `Quote`, and
        // of `ref<f64>?` a `ref<f64>` (`Type::omission_removed`).
        let endpoint = self.is_temporal_input(local) || Some(local) == self.out;
        let ty = if self.hook.is_some() && endpoint {
            match ty.omission_removed() {
                Type::Atomic(payload) => (**payload).clone(),
                under => under.clone(),
            }
        } else {
            ty
        };
        Expr {
            kind: ExprKind::Local(local),
            ty,
            at,
        }
    }

    /// The bindings the condition `condition` proves are not null: those
    /// where it holds, and those where it does not.
    ///
    /// ERR-12: "An immutable nullable parameter or local may be tested
    /// directly against null." A `var` may be given a null after its
    /// test, and so is proved nothing of.
    fn null_tests(&self, condition: &Expr) -> (Vec<Fact>, Vec<Fact>) {
        // ERR-12, ERR-3: "An immutable ... binding, a `let`, a
        // parameter, or an iteration binding".
        let immutable = |local: LocalId| {
            matches!(
                self.locals[local].kind,
                LocalKind::Parameter | LocalKind::Let | LocalKind::Iteration
            )
        };
        let refinable =
            |local: LocalId| immutable(local) && matches!(self.locals[local].ty, Type::Nullable(_));
        // ERR-3: "In composition a test of a connection is an ordinary
        // lifted call (FN-6): it gives a connection of `bool` and no
        // fact."
        // A binding written `result<T, E>?` that a null test has proved
        // present is read as its result (ERR-12), and is tested as one.
        // The proof may stand before the condition, or be the left
        // operand of the connective the test is the right operand of:
        // `outcome != null && is_ok(outcome)`.
        let tagged = |local: LocalId, known: &[Fact]| {
            let present = Fact::Present(local);
            let result = match &self.locals[local].ty {
                Type::Result { .. } => true,
                Type::Nullable(inner) => {
                    matches!(**inner, Type::Result { .. })
                        && (self.present.contains(&present) || known.contains(&present))
                }
                _ => false,
            };
            immutable(local) && result && self.locals[local].category == ValueCategory::Value
        };
        null_tests(
            condition,
            &Proofs {
                refinable: &refinable,
                tagged: &tagged,
                inputs: &|| self.temporal_inputs(),
                children: &|child| self.tested_child(child),
            },
            &[],
        )
    }

    /// The temporal inputs of the function being read: its parameters
    /// that are not `const` (FN-1). A parameter's local has its
    /// position among the parameters.
    fn temporal_inputs(&self) -> Vec<LocalId> {
        (0..self.signature.parameters.len())
            .filter(|&local| self.is_temporal_input(local))
            .collect()
    }

    /// True when `local` is a temporal input of a temporal function.
    fn is_temporal_input(&self, local: LocalId) -> bool {
        self.signature
            .parameters
            .get(local)
            .is_some_and(|parameter| !parameter.constant)
    }

    /// `[key: value, ...]` (EXPR-9): "`[a: b]` has one pair entry, not a
    /// standalone pair value ... The use site and expected type
    /// constrain element types, keys, ordering, uniqueness, and
    /// placeholder admission; syntax alone does not select a
    /// dictionary, timed trace, or sparse delta."
    ///
    /// So pairs are a dictionary where one is expected: each key is
    /// read as its key type and each value as its value type. Where
    /// nothing is expected, nothing says what they are
    /// (`sequence.context_type`); where another type that is known is
    /// expected, the place reports that this is none of it.
    fn pairs(
        &mut self,
        node: &SyntaxNode,
        items: &[SyntaxNode],
        expected: Option<&Type>,
        unexpected: bool,
    ) -> (ExprKind, Type) {
        let sides = match expected {
            Some(Type::Dict { key, value }) => Some(((**key).clone(), (**value).clone())),
            _ => None,
        };
        if sides.is_none() && unexpected {
            self.diagnostics.emit(
                error(
                    Category::Type,
                    codes::SEQUENCE_CONTEXT,
                    self.at(node),
                    "nothing here says what these pairs make: write the type".to_string(),
                    "EXPR-9",
                )
                .with_note("pairs are a dictionary where one is expected: `let book: dict<str, f64> = [\"a\": 1.0]`"),
            );
        }
        let unknown = Type::Unknown;
        let (key_type, value_type) = match &sides {
            Some((key, value)) => (key, value),
            None => (&unknown, &unknown),
        };
        let entries: Vec<(Expr, Expr)> = items
            .iter()
            .filter(|item| item.kind() == SyntaxKind::SequencePair)
            .filter_map(|pair| {
                let mut parts = pair.children();
                let (key, value) = (parts.next()?, parts.next()?);
                let key = self.expression(&key, Some(key_type));
                let value = self.expression(&value, Some(value_type));
                Some(if sides.is_some() {
                    (
                        self.fit(key, key_type, "a dictionary's key"),
                        self.fit(value, value_type, "a dictionary's value"),
                    )
                } else {
                    (key, value)
                })
            })
            .collect();
        let ty = match (expected, &sides) {
            (Some(expected), Some(_)) => expected.clone(),
            // A place whose type is known and is no dictionary is given
            // a dictionary of what is not known, which it reports as
            // none of what it expects.
            (Some(expected), None) if !open(expected) => Type::Dict {
                key: Box::new(Type::Unknown),
                value: Box::new(Type::Unknown),
            },
            _ => Type::Unknown,
        };
        (ExprKind::Pairs(entries), ty)
    }

    /// `(a, b)`. An expected tuple of as many elements gives each its
    /// expected type.
    fn tuple(&mut self, node: &SyntaxNode, expected: Option<&Type>) -> (ExprKind, Type) {
        let items: Vec<SyntaxNode> = node.children().collect();
        // True when nothing is expected of the tuple, which is not the
        // same as what is expected not being known.
        let unexpected = expected.is_none();
        let expected = match not_null(expected) {
            Some(Type::Tuple(expected)) if expected.len() == items.len() => Some(expected),
            _ => None,
        };
        let elements: Vec<Expr> = items
            .iter()
            .enumerate()
            .map(|(index, item)| match expected {
                Some(expected) => {
                    let expected = &expected[index];
                    let element = self.expression(item, Some(expected));
                    self.fit(element, expected, "this element of the tuple")
                }
                // Nothing is expected of the tuple, and so of its
                // elements: `(null, 1)` gives its `null` no type.
                None if unexpected => self.expression(item, None),
                // What the place expects is not known, or is no tuple
                // of this length: what an element is expected to be is
                // not known either, and nothing is reported of that.
                None => self.expression(item, Some(&Type::Unknown)),
            })
            .collect();
        // An element that does not fit has been reported: the tuple is
        // of the type expected, and is not reported again.
        let ty = match expected {
            Some(expected) => Type::Tuple(expected.clone()),
            None => Type::Tuple(elements.iter().map(|element| element.ty.clone()).collect()),
        };
        (ExprKind::Tuple(elements), ty)
    }

    /// `[a, b]`: a sequence of values (EXPR-9). "The use site and
    /// expected type constrain element types": an expected list or set
    /// gives the elements their type. With none, the values are a list
    /// of the one type they have, `f64` when some are `i64` and some
    /// `f64` (NUM-1).
    ///
    /// Pairs and `_` are for the passes that read dictionaries and test
    /// traces.
    fn sequence(&mut self, node: &SyntaxNode, expected: Option<&Type>) -> (ExprKind, Type) {
        // True when nothing is expected of the sequence, which is not
        // the same as what is expected not being known.
        let unexpected = expected.is_none();
        let items: Vec<SyntaxNode> = node.children().collect();
        // TST-5: where a trace is expected, the sequence is one
        // written out.
        if let Some(Type::Trace { cell, .. }) = not_null(expected) {
            return self.trace(&items, cell);
        }
        // EXPR-8: "Harness `_` and timed cells exist only in test
        // traces; outside them a sequence is ordinary data and a
        // placeholder is not a value." Where what is expected is not
        // known, it may be a trace, and nothing is reported (ERR-9).
        let holes: Vec<SyntaxNode> = items
            .iter()
            .flat_map(|item| match item.kind() {
                SyntaxKind::Hole => vec![item.clone()],
                SyntaxKind::SequencePair => item
                    .children()
                    .filter(|part| part.kind() == SyntaxKind::Hole)
                    .collect(),
                _ => Vec::new(),
            })
            .collect();
        if !holes.is_empty() {
            if !expected.is_some_and(open) {
                for hole in &holes {
                    self.placeholder(
                        hole,
                        "`_` is no value: it stands for no publication, in a trace of a test",
                        "EXPR-8",
                    );
                }
            }
            return (ExprKind::Unread, Type::Unknown);
        }
        // A sequence is no `null`: of a `set<i64>?` expected, the set.
        let expected = not_null(expected);
        // EXPR-9: "Entries are either all values or all colon-separated
        // pairs", which the parser holds a sequence to. Pairs, and no
        // entries where a dictionary is expected, are read as one.
        let pairs = items
            .iter()
            .any(|item| item.kind() == SyntaxKind::SequencePair);
        if pairs || (items.is_empty() && matches!(expected, Some(Type::Dict { .. }))) {
            return self.pairs(node, &items, expected, unexpected);
        }
        let element = match expected {
            Some(Type::List { element, .. }) | Some(Type::Set(element)) => Some(&**element),
            _ => None,
        };
        if let (Some(element), Some(expected)) = (element, expected) {
            let elements: Vec<Expr> = items
                .iter()
                .map(|item| {
                    let value = self.expression(item, Some(element));
                    self.fit(value, element, "an element")
                })
                .collect();
            // TYPE-10: "Size participates in identity." A list of a
            // size that is known has that many entries.
            if let Type::List {
                size: Some(size), ..
            } = expected
                && let ConstantValue::Int(size) = size.value
                && i64::try_from(elements.len()) != Ok(size)
            {
                self.diagnostics.emit(error(
                    Category::Type,
                    codes::TYPE_MISMATCH,
                    self.at(node),
                    format!(
                        "this has {} {}, and a `{expected}` has {size}",
                        elements.len(),
                        if elements.len() == 1 {
                            "entry"
                        } else {
                            "entries"
                        }
                    ),
                    "TYPE-10",
                ));
            }
            return (ExprKind::Sequence(elements), expected.clone());
        }
        // From here the place gives the entries no type. Where it
        // expects a type that is known and holds no sequence, an `i64`,
        // the sequence is no value of it. It is then a list of what is
        // not known, which the place reports as no `i64`, and its
        // entries are not judged as well. A dictionary's entries are for
        // the pass that reads dictionaries.
        let misplaced = expected
            .is_some_and(|expected| !open(expected) && !matches!(expected, Type::Dict { .. }));
        let unjudged = if misplaced {
            Type::List {
                element: Box::new(Type::Unknown),
                size: None,
            }
        } else {
            Type::Unknown
        };
        // An entry that takes its type from its place, `null`, says
        // nothing of what the sequence holds: the others are read
        // first, and it is then read as an element of their type.
        // Where what the place expects is not known, what an entry is
        // expected to be is not known either: the `[]` of `[[]]` is
        // then not reported.
        let of_entry = if unexpected {
            None
        } else {
            Some(&Type::Unknown)
        };
        // A block among the entries may assign what another reads
        // (CTRL-10), and an entry that takes its type from its place is
        // read after the others. With a block among them the others
        // are read first only to learn their type, that reading taken
        // back, and every entry is then read where it is written.
        let ordered = items.iter().any(holds_block) && self.may_reread();
        let mut read: Vec<Option<Expr>> = items
            .iter()
            .map(|item| {
                if branch_takes_its_type(item) {
                    return None;
                }
                if !ordered {
                    return Some(self.expression(item, of_entry));
                }
                let mark = self.mark();
                let entry = self.expression(item, of_entry);
                self.take_back(mark, false);
                Some(entry)
            })
            .collect();
        let known: Vec<&Type> = read.iter().flatten().map(|entry| &entry.ty).collect();
        // EXPR-9: "Entries of two types reject, and so do entries that
        // give no type: `[]`, `[null]`." Where what the place expects
        // is not known, neither is the sequence, and nothing is
        // reported.
        let Some(&first) = known.first() else {
            if unexpected {
                self.diagnostics.emit(error(
                    Category::Type,
                    codes::SEQUENCE_CONTEXT,
                    self.at(node),
                    "nothing here says what this sequence holds: write its type".to_string(),
                    "EXPR-9",
                ));
            }
            let elements = items
                .iter()
                .map(|item| self.expression(item, Some(&Type::Unknown)))
                .collect();
            return (ExprKind::Sequence(elements), unjudged);
        };
        let numbers = |ty: &Type| matches!(ty, Type::Scalar(Scalar::I64 | Scalar::F64));
        let of = if known.iter().all(|ty| numbers(ty))
            && known.iter().any(|ty| **ty == Type::Scalar(Scalar::F64))
        {
            Type::Scalar(Scalar::F64)
        } else {
            first.clone()
        };
        // "`[1, null]` [is] a `list<i64?>`": a `null` among the values
        // makes their type nullable.
        let of = if items.iter().any(branch_is_null) && !matches!(of, Type::Nullable(_)) {
            Type::Nullable(Box::new(of))
        } else {
            of
        };
        // Where what the place expects is not known, it may be what
        // gives entries of two types their one type: a `list<Base>` for
        // an `A` and a `B` that inherit from it. Such entries are not
        // judged here, and the sequence has no type. Where the place
        // is known and holds no sequence, the place reports it.
        let disagree = read.iter().flatten().any(|entry| {
            !(self.admits(&of, &entry.ty) || is_integer_literal(entry) && accepts_float(&of))
        });
        if !unexpected && disagree {
            let elements = items
                .iter()
                .enumerate()
                .map(|(position, item)| match read[position].take() {
                    Some(entry) if !ordered => entry,
                    _ => self.expression(item, Some(&Type::Unknown)),
                })
                .collect();
            return (ExprKind::Sequence(elements), unjudged);
        }
        let elements: Vec<Expr> = items
            .iter()
            .enumerate()
            .map(|(position, item)| {
                let entry = match read[position].take() {
                    Some(entry) if !ordered => entry,
                    _ => self.expression(item, Some(&of)),
                };
                self.fit(entry, &of, "an element, as the first is,")
            })
            .collect();
        let ty = Type::List {
            element: Box::new(of),
            size: None,
        };
        (ExprKind::Sequence(elements), ty)
    }

    /// `-value`, `!value`.
    ///
    /// `-` stands for core's `neg_` (the generics chapter: "Syntax binds
    /// these core identities"), and is read as the call of it: NUM-1,
    /// "Unary negation preserves the numeric type", and OPS-1 are what
    /// its candidates declare. `!` is of a bool (EXPR-6; OPS-5: "`&&`,
    /// `||`, and `!` take bools"), and is read as it is written.
    fn unary(&mut self, node: &SyntaxNode, expected: Option<&Type>) -> (ExprKind, Type) {
        let config = child(node, SyntaxKind::OperatorConfig);
        let operand = node
            .children()
            .find(|part| part.kind() != SyntaxKind::OperatorConfig);
        let (Some(token), Some(operand)) = (tokens(node).next(), operand) else {
            return (ExprKind::Unread, Type::Unknown);
        };
        match token.kind() {
            // Configuration of `!` is for the pass that reads it through
            // core's `not_`.
            SyntaxKind::Bang if config.is_some() => (ExprKind::Unread, Type::Unknown),
            SyntaxKind::Bang => {
                let operand = self.expression(&operand, Some(&BOOL));
                let operand = self.fit(operand, &BOOL, "the operand of `!`");
                let kind = ExprKind::Unary {
                    operator: UnaryOperator::Not,
                    operand: Box::new(operand),
                };
                (kind, BOOL)
            }
            SyntaxKind::Minus => {
                // LEX-7: 2^63 is an integer only as the operand of a
                // minus, where the two are the least `i64`.
                if config.is_none() && is_least_magnitude(&operand) {
                    return (
                        ExprKind::Literal(Value::Int(i64::MIN)),
                        Type::Scalar(Scalar::I64),
                    );
                }
                let operator = UnaryOperator::Negate;
                let identity = self.symbol_operator(operator.identity());
                // EXPR-13: "Unary configuration precedes its operand."
                let (mut given, faulty) = match (&config, identity) {
                    (Some(config), Some(identity)) => {
                        self.configuration(operator.symbol(), config, identity)
                    }
                    // Configuration of an operator the program does not
                    // have.
                    (Some(_), None) => return (ExprKind::Unread, Type::Unknown),
                    (None, _) => (Vec::new(), false),
                };
                let value = self.expression(&operand, None);
                let symbol = Symbol {
                    form: Form::Unary(operator),
                    text: operator.symbol(),
                    at: Location::new(self.file, token.span()),
                    rule: "OPS-1",
                    unjudged: Type::Unknown,
                    place: expected.filter(|place| !open(place)).cloned(),
                };
                let only = Given::read(0, operand, value);
                // GEN-7: `-value` under `requires neg_(T) -> T`.
                if config.is_none()
                    && let Some(operator) = identity
                    && let Some(result) = self.required(operator, &[&only], (None, expected), None)
                {
                    return self.call_required(operator, result, vec![only], symbol.form);
                }
                // An operand whose operators are not judged here: see
                // `unjudged_operand`.
                // A nullable operand is an operand as another is: no
                // candidate takes one, and the minus is then rejected
                // (ERR-12).
                let identity = identity.filter(|_| {
                    matches!(only.value.ty, Type::Nullable(_)) || !unjudged_operand(&only.value.ty)
                });
                let Some(identity) = identity else {
                    if config.is_none() {
                        let kind = ExprKind::Unary {
                            operator,
                            operand: Box::new(only.value),
                        };
                        return (kind, Type::Unknown);
                    }
                    given.push(only);
                    return (self.unselected(given, symbol.form), Type::Unknown);
                };
                given.push(only);
                if faulty {
                    return (self.unselected(given, symbol.form), symbol.unjudged);
                }
                self.call_symbol(symbol, identity, given)
            }
            _ => (ExprKind::Unread, Type::Unknown),
        }
    }

    /// `left + right`, and each other operator with two operands.
    fn binary(&mut self, node: &SyntaxNode, expected: Option<&Type>) -> (ExprKind, Type) {
        // A chain, `a + b + c`, nests to the left as deep as it is long
        // (the parser's tree depth, not its nesting). It is read from
        // its innermost link outward, in a loop, so that its length
        // costs no stack. A link that is not read, `&&` or `||` with
        // configuration, is not read inside either.
        let mut links = vec![node.clone()];
        loop {
            let last = links.last().expect("never empty");
            if unread_link(last) {
                break;
            }
            match last.children().next() {
                Some(left) if left.kind() == SyntaxKind::BinaryExpr => links.push(left),
                _ => break,
            }
        }
        // The link inside the one being read, as its left operand.
        let mut inside: Option<Expr> = None;
        loop {
            let link = links.pop().expect("never empty");
            // The type the place expects is of the whole chain, which
            // is its outermost link: a link inside is an operand, read
            // as an argument is.
            let place = if links.is_empty() { expected } else { None };
            let (kind, ty) = self.binary_link(&link, inside.take(), place);
            if links.is_empty() {
                return (kind, ty);
            }
            // As `expression` would have made it.
            if matches!(kind, ExprKind::Unread) {
                self.forget();
            }
            inside = Some(Expr {
                kind,
                ty,
                at: self.at(&link),
            });
        }
    }

    /// One link of a chain of binary operators: `left operator right`.
    /// `read` is the left operand, when it is the link inside this one
    /// and has been read.
    ///
    /// `&&` and `||` are read as they are written (EXPR-6). Every other
    /// symbol stands for an operator of core's module (the generics
    /// chapter: "Syntax binds these core identities independently of
    /// local same-named functions"), and is read as the call of it: see
    /// [`Typer::symbol`].
    fn binary_link(
        &mut self,
        node: &SyntaxNode,
        read: Option<Expr>,
        place: Option<&Type>,
    ) -> (ExprKind, Type) {
        let config = child(node, SyntaxKind::OperatorConfig);
        let mut read = read;
        let mut operands = node
            .children()
            .filter(|part| part.kind() != SyntaxKind::OperatorConfig);
        let operator = tokens(node).find_map(|token| binary_operator(token.kind()));
        let operator_at = tokens(node)
            .find(|token| binary_operator(token.kind()).is_some())
            .map_or(self.at(node), |token| {
                Location::new(self.file, token.span())
            });
        let (Some(left), Some(right), Some(operator)) =
            (operands.next(), operands.next(), operator)
        else {
            return (ExprKind::Unread, Type::Unknown);
        };
        if matches!(operator, BinaryOperator::And | BinaryOperator::Or) {
            // Configuration of these is for the pass that reads them
            // through core's `and_` and `or_`.
            if config.is_some() {
                return (ExprKind::Unread, Type::Unknown);
            }
            // EXPR-6: "Boolean connectives short-circuit left to right."
            // Their operands are bools.
            let what = format!("an operand of `{}`", operator.symbol());
            let left = match read.take() {
                Some(left) => left,
                None => self.expression(&left, Some(&BOOL)),
            };
            let left = self.fit(left, &BOOL, &what);
            // EXPR-6: the right operand is not evaluated when the left
            // decides. What it assigns is assigned only on the path
            // that evaluates it, and a `return` in it does not end
            // every path (CTRL-11, CTRL-5).
            let before = self.assigned.clone();
            let before_returned = self.returned;
            // ERR-3 has "short-circuit paths" tracked, and ERR-12 the
            // same of null tests: the right operand of `&&` is reached
            // only where the left holds, and of `||` only where it does
            // not.
            let (when_true, when_false) = self.null_tests(&left);
            let frame = self.present.len();
            self.present.extend(match operator {
                BinaryOperator::And => when_true,
                _ => when_false,
            });
            let right = self.expression(&right, Some(&BOOL));
            self.present.truncate(frame);
            self.assigned = joined(
                (&self.assigned, self.returned),
                (&before, before_returned),
                self.locals.len(),
            );
            self.returned = before_returned;
            let right = self.fit(right, &BOOL, &what);
            let kind = ExprKind::Binary {
                operator,
                left: Box::new(left),
                right: Box::new(right),
            };
            return (kind, BOOL);
        }
        // TST-4: "Harness comparison is intrinsic: compare presence,
        // exact present data, and shape/length; overload candidates
        // cannot change it."
        if self.testing
            && read.is_none()
            && config.is_none()
            && matches!(operator, BinaryOperator::Equal | BinaryOperator::NotEqual)
            && (self.gives_trace(&left) || self.gives_trace(&right))
        {
            return self.trace_comparison(operator, &left, &right);
        }
        let identity = self.symbol_operator(operator.identity());
        // The generics chapter: a symbol's "operands are read as that
        // call's arguments are". Each is read with nothing expected of
        // it, and one with no type of its own, a `.member` or a
        // sequence written out, again as the parameter of the candidate
        // selected. EXPR-13: "left operand, configuration entries in
        // their written order, then right operand".
        let left = match read.take() {
            Some(value) => Given::read(0, left, value),
            None => self.operand(0, left),
        };
        let (entries, faulty) = match (&config, identity) {
            (Some(config), Some(identity)) => {
                self.configuration(operator.symbol(), config, identity)
            }
            // Configuration of an operator the program does not have.
            (Some(_), None) => return (ExprKind::Unread, Type::Unknown),
            (None, _) => (Vec::new(), false),
        };
        let right = self.operand(1, right);
        self.symbol(
            (operator, operator_at),
            identity,
            (left, right),
            (entries, faulty, config.is_some()),
            place,
        )
    }

    /// True when the expression at `node` is one that gives a trace:
    /// an `eval`, or a local one is bound to, in parentheses or not.
    fn gives_trace(&self, node: &SyntaxNode) -> bool {
        let node = &ungrouped(node.clone());
        match node.kind() {
            SyntaxKind::EvalExpr | SyntaxKind::RunExpr => true,
            SyntaxKind::NameExpr => self
                .local_of(node)
                .is_some_and(|local| matches!(self.locals[local].ty, Type::Trace { .. })),
            _ => false,
        }
    }

    /// `left == right` or `left != right` where one side gives a trace
    /// (TST-4). TST-3: a trace is "compared with `==` or `!=` against a
    /// trace written out or another eval". The side that gives a trace
    /// says what the other is: a sequence written out is read as a
    /// trace of the same cells. TST-7: the two are of one style.
    fn trace_comparison(
        &mut self,
        operator: BinaryOperator,
        left: &SyntaxNode,
        right: &SyntaxNode,
    ) -> (ExprKind, Type) {
        // The operands are read in the order written (EXPR-13): a block
        // in one may assign what the other reads (CTRL-10). Where only
        // the right one gives a trace, what it is of is learned from a
        // reading that is taken back, and the left is then read as a
        // trace of that before the right is read where it stands.
        let left_leads = self.gives_trace(left);
        let (left, right) = if left_leads {
            let left = self.expression(left, None);
            // An eval of a target with no result has been reported,
            // and what it is compared with is not judged (ERR-9).
            let expected = match &left.ty {
                ty @ Type::Trace { .. } => ty.clone(),
                _ => Type::Unknown,
            };
            let right = self.expression(right, Some(&expected));
            let right = self.fit(right, &expected, "the trace it is compared with");
            (left, right)
        } else {
            let expected = if self.may_reread() {
                let mark = self.mark();
                let probe = self.expression(right, None);
                self.take_back(mark, false);
                match probe.ty {
                    ty @ Type::Trace { .. } => ty,
                    _ => Type::Unknown,
                }
            } else {
                Type::Unknown
            };
            let left = self.expression(left, Some(&expected));
            let right = self.expression(right, None);
            // The right says what the left is to be, now that it is
            // read where it stands.
            let expected = match &right.ty {
                ty @ Type::Trace { .. } => ty.clone(),
                _ => Type::Unknown,
            };
            let left = self.fit(left, &expected, "the trace it is compared with");
            (left, right)
        };
        // TST-7: "Within one eval all inputs and expectation use the
        // same dense/timed style", reported at the later trace.
        if let (
            Type::Trace {
                style: Some(first), ..
            },
            Type::Trace {
                style: Some(later), ..
            },
        ) = (&left.ty, &right.ty)
            && !first.same_notation(*later)
        {
            self.trace_style(right.at, *first, *later);
        }
        let kind = ExprKind::TraceCompare {
            left: Box::new(left),
            right: Box::new(right),
            negated: operator == BinaryOperator::NotEqual,
        };
        (kind, BOOL)
    }

    /// A symbol between two operands, as the call of the operator it
    /// stands for. `identity` is that operator, when the program has
    /// it; `entries` the configuration's entries, whether one of them
    /// was reported, and whether a configuration is written; `place`
    /// the type the symbol's place expects.
    ///
    /// The generics chapter: "A symbol is the call of its identity: its
    /// operands are read as that call's arguments are, the type its
    /// place expects is requested of the candidates (GEN-5), and its
    /// configuration gives the call's named arguments (EXPR-10)." The
    /// candidates of core's operators say what a symbol admits: NUM-1
    /// for numbers, OPS-1's table for the others, OPS-2 for the
    /// comparisons. "Every other pairing is a source rejection."
    ///
    /// An operand whose type is not known and a nullable one, whose
    /// payload needs its guard, are not judged here, and neither is a
    /// test against `null`: the symbol is then recorded as it is
    /// written. An operand that is a generic parameter has the
    /// operations its declaration requires (GEN-7).
    /// The generics chapter: "null presence checks ... have their own
    /// language rules; token shape alone does not make them extensible
    /// candidate calls."
    fn symbol(
        &mut self,
        (operator, at): (BinaryOperator, Location),
        identity: Option<OperatorRef>,
        (left, right): (Given, Given),
        (entries, faulty, configured): (Vec<Given>, bool, bool),
        place: Option<&Type>,
    ) -> (ExprKind, Type) {
        use BinaryOperator as Op;
        let comparison = matches!(
            operator,
            Op::Equal | Op::NotEqual | Op::Less | Op::LessEqual | Op::Greater | Op::GreaterEqual
        );
        let symbol = Symbol {
            form: Form::Binary(operator),
            text: operator.symbol(),
            at,
            rule: if comparison || matches!(operator, Op::Compare) {
                "OPS-2"
            } else {
                "OPS-1"
            },
            // What the symbol gives when no candidate is selected: a
            // comparison is a bool whichever candidate it is.
            unjudged: if comparison { BOOL } else { Type::Unknown },
            place: place.filter(|place| !open(place)).cloned(),
        };
        // A nullable operand of `==` or `!=` is a test of presence, or a
        // comparison this pass does not judge. Of any other symbol it
        // is an operand as another is: no candidate takes one, unless a
        // module declares it, and the symbol is then rejected (ERR-12).
        // An operand whose type is known only in part says nothing of
        // the candidates either: a call that leaves a generic parameter
        // for its place to say, `repeat(1)`.
        let equality = matches!(operator, Op::Equal | Op::NotEqual);
        let as_written = |operand: &Given| {
            is_null(&operand.node)
                || !operand.by_place
                    && match &operand.value.ty {
                        Type::Nullable(_) => equality,
                        ty => unjudged_operand(ty) || !whole(ty),
                    }
        };
        // GEN-7: an operand that is a generic parameter of the enclosing
        // declaration has the operations its requirements state:
        // `lhs + rhs` under `requires add_(T, T) -> T`.
        if !configured
            && !faulty
            && let Some(operator) = identity
            && let Some(result) = self.required(operator, &[&left, &right], (None, place), None)
        {
            return self.call_required(operator, result, vec![left, right], symbol.form);
        }
        let identity = identity.filter(|_| !(as_written(&left) || as_written(&right)));
        let Some(identity) = identity else {
            let (left_node, right_node) = (left.node.clone(), right.node.clone());
            let (left, right) = self.operands_as_written(left, right);
            if !configured {
                let kind = ExprKind::Binary {
                    operator,
                    left: Box::new(left),
                    right: Box::new(right),
                };
                return (kind, symbol.unjudged);
            }
            let mut given = vec![Given::read(0, left_node, left)];
            given.extend(entries);
            given.push(Given::read(1, right_node, right));
            return (self.unselected(given, symbol.form), symbol.unjudged);
        };
        let mut given = vec![left];
        given.extend(entries);
        given.push(right);
        if faulty {
            return (self.unselected(given, symbol.form), symbol.unjudged);
        }
        self.call_symbol(symbol, identity, given)
    }

    /// The operands of a symbol that is recorded as it is written, each
    /// read where it stands.
    ///
    /// TYPE-7: `.member` "names a member of the enum type its position
    /// expects": beside an operand, that operand's. "Parentheses select
    /// grouping" (EXPR-1): `(.buy)` is `.buy`. A `null` beside an
    /// operand is of that operand's type when it is nullable; beside
    /// any other, what it is of is not known here. So an operand whose
    /// first reading was taken back is read again with the other's
    /// type before it.
    fn operands_as_written(&mut self, left: Given, right: Given) -> (Expr, Expr) {
        // An operand read again: as the other's type where it takes its
        // type from it, and with nothing expected of it where it does
        // not, so that what it leaves unresolved is reported
        // (`generic.unresolved` for the `repeat(1)` of
        // `repeat(1) == repeat(2)`).
        //
        // `other` is the other operand's type where it has been read,
        // and `None` where it has not yet. A sequence or a tuple
        // written out beside an operand whose type is not known has
        // nothing to be read as, and that is the other operand's doing:
        // nothing is reported of it (ERR-9). So `eval(...) == []` says
        // nothing of its `[]` while `eval` is not read.
        let reread = |this: &mut Self, operand: &Given, other: Option<&Type>| {
            if takes_its_type(&operand.node) {
                let expected = beside(&operand.node, other.unwrap_or(&Type::Unknown));
                this.expression(&operand.node, Some(&expected))
            } else if operand.by_place && other.is_some_and(open) {
                this.expression(&operand.node, Some(&Type::Unknown))
            } else {
                this.expression(&operand.node, None)
            }
        };
        match (left.retried, right.retried) {
            (false, false) => (left.value, right.value),
            (false, true) => {
                let right = reread(self, &right, Some(&left.value.ty));
                (left.value, right)
            }
            (true, false) => {
                let left = reread(self, &left, Some(&right.value.ty));
                (left, right.value)
            }
            // One that takes its type from the other is read after it.
            (true, true) if takes_its_type(&left.node) && !takes_its_type(&right.node) => {
                let right = reread(self, &right, None);
                let left = reread(self, &left, Some(&right.ty));
                (left, right)
            }
            (true, true) => {
                let left = reread(self, &left, None);
                let right = reread(self, &right, Some(&left.ty));
                (left, right)
            }
        }
    }

    /// An operand of a symbol, read as an argument is: with nothing
    /// expected of it. A reading that reports nothing and settles no
    /// whole type is taken back, as one of an operand with no type of
    /// its own is, and the operand is read again: as the selected
    /// candidate's parameter, or where the symbol is recorded as it is
    /// written, by [`Typer::operands_as_written`].
    fn operand(&mut self, position: usize, node: SyntaxNode) -> Given {
        let by_place = typed_by_place(&node);
        let mark = self.mark();
        let value = self.expression(&node, Some(&Type::Unknown));
        let retried = (by_place || !whole(&value.ty))
            && self.diagnostics.len() == mark.diagnostics
            && self.may_reread();
        if retried {
            self.take_back(mark, true);
        }
        Given {
            key: Key::Position(position),
            by_place,
            value,
            retried,
            node,
            constant: None,
        }
    }

    /// The operator of core's module that a symbol stands for, by its
    /// name: `add_` for `+`. `None` when the program has no such
    /// operator.
    fn symbol_operator(&mut self, name: &'static str) -> Option<OperatorRef> {
        if let Some(found) = self.identities.get(name) {
            return *found;
        }
        let program = self.program;
        let found = program.core().and_then(|core| {
            let module = &program.modules[core];
            module
                .scope
                .get(name)
                .into_iter()
                .flatten()
                .copied()
                .find(|&declaration| {
                    module.declarations[declaration].kind == DeclarationKind::Operator
                })
                .map(|declaration| OperatorRef {
                    module: core,
                    declaration,
                })
        });
        self.identities.insert(name, found);
        found
    }

    /// The entries of a symbol's configuration, `{zero: .error}`, each
    /// read as a named argument of a call of `operator`, and whether
    /// one of them was reported or is of no known type.
    ///
    /// EXPR-10: "Bind these arguments to the same core identity as the
    /// named arguments of a call of it: to the operator's parameters
    /// and the options its candidates declare (GEN-6), with their
    /// roles, defaults, and ordinary argument checks."
    fn configuration(
        &mut self,
        symbol: &str,
        config: &SyntaxNode,
        operator: OperatorRef,
    ) -> (Vec<Given>, bool) {
        let program = self.program;
        let Some(Detail::Callable(public)) = program.modules[operator.module]
            .details
            .get(operator.declaration)
        else {
            return (Vec::new(), true);
        };
        let candidates = self.value_candidates(operator);
        let (selecting, options) = options_of(public, &candidates);
        let reported = self.diagnostics.len();
        let mut names: std::collections::HashSet<String> = std::collections::HashSet::new();
        let mut given = Vec::new();
        for entry in config
            .children()
            .filter(|entry| entry.kind() == SyntaxKind::ConfigArg)
        {
            // The entry's name is its first token: any word (LEX-9).
            let (Some(name), Some(value)) = (tokens(&entry).next(), entry.children().next()) else {
                continue;
            };
            let text = name.text();
            let at = Location::new(self.file, name.span());
            // The operator's own parameters are the symbol's operands.
            let operand = public
                .parameters
                .iter()
                .any(|parameter| parameter.name == text);
            if !operand && !options.contains(text) {
                self.diagnostics.emit(error(
                    Category::Name,
                    codes::ARGUMENT_UNKNOWN,
                    at,
                    format!("`{symbol}` has no option `{text}`"),
                    "EXPR-10",
                ));
            } else if operand || !names.insert(text.to_string()) {
                self.diagnostics.emit(error(
                    Category::Name,
                    codes::ARGUMENT_DUPLICATE,
                    at,
                    format!("`{text}` is given twice"),
                    "EXPR-10",
                ));
            }
            given.push(self.give(Key::Name(text.to_string()), value));
        }
        self.constants_given(&mut given, &selecting);
        let faulty = self.diagnostics.len() != reported
            || given
                .iter()
                .any(|given| !given.by_place && open(&given.value.ty));
        (given, faulty)
    }

    /// What each argument given to a generic value parameter is, as a
    /// constant where the call stands. `selecting` is the names the
    /// operator's candidates declare as generic value parameters.
    ///
    /// GEN-8: "The value a call gives a generic value parameter is a
    /// compile-time constant ... Any other rejects."
    fn constants_given(
        &mut self,
        given: &mut [Given],
        selecting: &std::collections::HashSet<&str>,
    ) {
        for given in given.iter_mut() {
            let Key::Name(name) = &given.key else {
                continue;
            };
            if !selecting.contains(name.as_str()) {
                continue;
            }
            // Its type is the candidate's to say (GEN-6): it is held to
            // that once the candidate is (`instance`).
            let read = self.typed_in(self.file, &given.node, None);
            let value = read.clone().map(|read| read.value);
            if let Some(why) = Self::not_constant(&value) {
                self.diagnostics.emit(error(
                    Category::Type,
                    codes::GENERIC_ARGUMENT_CONSTANT,
                    self.at(&given.node),
                    format!(
                        "`{name}` is a generic value parameter, and this is no compile-time \
                         constant: {why}"
                    ),
                    "GEN-8",
                ));
            }
            given.constant = Some(read);
        }
    }

    /// The call a symbol stands for: of `operator`, with `given`, the
    /// operands in their places and the configuration's entries by
    /// their names.
    ///
    /// GEN-5: "Select exactly one most-specific admitted candidate ...
    /// No match rejects; tied most-specific candidates are ambiguous."
    /// A symbol's operands select as the arguments of the call do. The
    /// type its place expects is not asked of the candidates: the
    /// result is checked against it where the symbol stands.
    fn call_symbol(
        &mut self,
        symbol: Symbol,
        operator: OperatorRef,
        given: Vec<Given>,
    ) -> (ExprKind, Type) {
        let program = self.program;
        let Some(Detail::Callable(public)) = program.modules[operator.module]
            .details
            .get(operator.declaration)
        else {
            return (self.unselected(given, symbol.form), symbol.unjudged);
        };
        let candidates = self.value_candidates(operator);
        if unreadable(public, &candidates) {
            return (self.unselected(given, symbol.form), symbol.unjudged);
        }
        let own_generics: Vec<Location> =
            public.generics.iter().map(|generic| generic.at).collect();
        let wanted = Wanted {
            place: symbol.place.as_ref(),
            among: self.result_among(operator, public, &[], &own_generics),
        };
        let (mut selected, untold) = self.admitted(operator, &candidates, &given, &wanted);
        // Which candidates admit the call cannot be told, or an entry
        // could not be tried once the function's readings were used up
        // (`REREADS`): the symbol is then not judged.
        if untold || self.rereads == 0 && given.iter().any(|given| given.by_place) {
            return (self.unselected(given, symbol.form), symbol.unjudged);
        }
        if selected.len() == 1
            && let Some(chosen) = selected.pop()
        {
            return self.called(operator, chosen, given, (symbol.form, symbol.at));
        }
        // `i64` and `f64`, and after them what the configuration gives:
        // `zero: .nan`.
        let operands: Vec<String> = given
            .iter()
            .filter(|given| matches!(given.key, Key::Position(_)))
            .map(|given| {
                // An operand with no type of its own is named as it is
                // written: `.buy`.
                if given.by_place {
                    format!("`{}`", given.node.text().to_string().trim())
                } else {
                    format!("`{}`", given.value.ty)
                }
            })
            .collect();
        let entries: Vec<String> = given
            .iter()
            .filter_map(|given| {
                let Key::Name(name) = &given.key else {
                    return None;
                };
                let value = if given.by_place {
                    given.node.text().to_string().trim().to_string()
                } else {
                    given.value.ty.to_string()
                };
                Some(format!("`{name}: {value}`"))
            })
            .collect();
        let mut takes = operands.join(" and ");
        if !entries.is_empty() {
            takes = format!("{takes} with {}", entries.join(", "));
        }
        // GEN-5: "the requested output before inputs".
        if let Some(place) = &symbol.place {
            takes = format!("{takes} to give `{place}`");
        }
        // ERR-12: "Using it where T is required without that proof
        // rejects with `nullable.required_presence`." An operand that
        // may be null is one no candidate takes.
        let nullable = given.iter().find(|given| {
            matches!(given.key, Key::Position(_))
                && !given.by_place
                && matches!(given.value.ty, Type::Nullable(_))
        });
        let diagnostic = if let (true, Some(operand)) = (selected.is_empty(), nullable) {
            error(
                Category::Type,
                codes::NULLABLE_REQUIRED_PRESENCE,
                operand.value.at,
                format!(
                    "this is `{}`, and `{}` takes no {takes}: it may be null here",
                    operand.value.ty, symbol.text
                ),
                "ERR-12",
            )
            .with_note(
                "a test against null proves an immutable binding is not: \
                 `if value == null { return ... }`",
            )
        } else if selected.is_empty() {
            error(
                Category::Operator,
                codes::OPERATOR_NO_CANDIDATE,
                symbol.at,
                format!("`{}` takes no {takes}", symbol.text),
                symbol.rule,
            )
        } else {
            let name = &program.modules[operator.module].declarations[operator.declaration].name;
            let mut diagnostic = error(
                Category::Operator,
                codes::OPERATOR_AMBIGUOUS,
                symbol.at,
                format!(
                    "{} value candidates of operator `{name}`, which `{}` stands for, take \
                     {takes} equally",
                    selected.len(),
                    symbol.text
                ),
                "GEN-5",
            );
            for chosen in &selected {
                let candidate = &program.modules[chosen.module].declarations[chosen.declaration];
                diagnostic = diagnostic.with_related(candidate.at, "this candidate");
            }
            diagnostic
        };
        self.diagnostics.emit(diagnostic);
        // What the place expects was asked of the candidates and none
        // gives it: the place is not then told the symbol is of another
        // type (ERR-9).
        let ty = match symbol.place {
            Some(_) => Type::Unknown,
            None => symbol.unjudged,
        };
        (self.unselected(given, symbol.form), ty)
    }

    // ---- Calls and constructions ----

    /// What the name that starts at `start` of `file` resolved to
    /// (UNIT-10), in whichever module the file is of: a candidate's
    /// default is written in its own module's file, which need not be
    /// the one being read.
    fn binding_at(&self, file: FileId, start: u32) -> Option<&'a Binding> {
        if let Some(found) = self.bindings.get(&(file, start)) {
            return Some(found);
        }
        self.program
            .modules
            .iter()
            .flat_map(|module| module.uses.iter())
            .find(|used| used.at.file == file && used.at.span.start == start)
            .map(|used| &used.binding)
    }

    /// The declarations a name is bound to, when it names declarations
    /// of a module and not a local.
    fn declarations(&self, name: &SyntaxToken) -> Option<(usize, &'a [usize])> {
        match self.bindings.get(&(self.file, name.span().start))? {
            Binding::Declarations {
                module,
                declarations,
            } => Some((*module, declarations.as_slice())),
            Binding::Local { .. } => None,
        }
    }

    /// `callee(arguments)`: a call of a value function, or the
    /// construction of a struct.
    ///
    /// FN-4: "runtime/value evaluation chooses the value definition."
    /// A name left for the prelude, an operator, and a callee that is
    /// itself an expression are for later parts: their arguments are
    /// read, and the call has no type.
    fn call(&mut self, node: &SyntaxNode, expected: Option<&Type>) -> (ExprKind, Type) {
        self.call_read(node, expected)
    }

    /// What every call is held to once it is built, with each argument
    /// given to its parameter. It is run where a call of a function and
    /// a call of an operator's candidate are built, and so for a call
    /// written by name, with generic arguments, by a symbol, as a link
    /// of a chain `a + b + c`, or by a compound assignment.
    fn finished_call(&mut self, kind: &ExprKind, evaluating: bool) {
        // A call of a function or a candidate: an operation a
        // declaration requires has no candidate here, and a call that
        // is not read no parameters to judge.
        let selected = match kind {
            ExprKind::Call {
                callee, arguments, ..
            } => callee.declaration().map(|(module, declaration)| Selected {
                module,
                declaration,
                generics: callee.generics(),
                signature: match self.program.modules[module].details.get(declaration) {
                    Some(Detail::Callable(signature)) => Some(signature),
                    _ => None,
                },
                arguments,
            }),
            _ => None,
        };
        let Some(selected) = selected else {
            self.store_call(kind);
            return;
        };
        self.places_given(&selected);
        self.store_call(kind);
        self.capabilities_given(&selected);
        self.fixed_arguments(&selected);
        self.traces_given(&selected, evaluating);
        self.store_keys(&selected);
        self.references_given(&selected);
    }

    /// FN-9: "A graph injects `global_state` and no other, and uses it
    /// where the graph is built: each call on it is evaluated once
    /// then ... and is given no connection (`connection.as_constant`)."
    /// A call on the store is one given the capability the graph
    /// injects: it is run where the graph is built, when a connection
    /// has no value.
    fn store_call(&mut self, kind: &ExprKind) {
        if !self.graph || self.testing {
            return;
        }
        let ExprKind::Call { arguments, .. } = kind else {
            return;
        };
        // The capability itself, or a `let` it is bound to: `let store =
        // global_state` and then `put(store, ...)` is a call on it too.
        let on_store = arguments
            .iter()
            .any(|argument| self.capability_of(&argument.value).is_some());
        if !on_store {
            return;
        }
        for argument in arguments {
            if self.category(&argument.value) == ValueCategory::Connection {
                self.diagnostics.emit(
                    error(
                        Category::Phase,
                        codes::CONNECTION_AS_CONSTANT,
                        argument.value.at,
                        "a call on the store a graph injects is run where the graph is built: \
                         this is a connection, and it takes a value known then"
                            .to_string(),
                        "FN-9",
                    )
                    .with_note(
                        "a constant or a `const` parameter is known where the graph is built",
                    ),
                );
            }
        }
    }

    /// FN-15: "A call gives a `var` parameter a place, as the left of an
    /// assignment is (EXPR-3): a `var`, a `state`, or a `cache`, or a
    /// place inside one, that holds an ordinary value ... What is
    /// read-only rejects, a `let`, a parameter that is not `var`, a
    /// constant, or an iteration binding (`binding.immutable`); so does
    /// what is no place, a literal or what a call gives
    /// (`argument.place`)." And "A function that declares one is not
    /// lifted (FN-6) ... No argument of such a call is a connection
    /// (`connection.as_constant`)".
    fn places_given(&mut self, selected: &Selected) {
        let &Selected {
            module,
            declaration,
            generics,
            arguments,
            ..
        } = selected;
        let program = self.program;
        let Some(signature) = selected.signature else {
            return;
        };
        if !signature.parameters.iter().any(|parameter| parameter.var) {
            return;
        }
        let callee = &program.modules[module].declarations[declaration].name;
        // What a parameter's type is at this call, the callee's generic
        // parameters in place.
        let own: Vec<Location> = signature
            .generics
            .iter()
            .map(|generic| generic.at)
            .collect();
        let map: Vec<(Location, &Argument)> = own.iter().copied().zip(generics).collect();
        let mut reports = Vec::new();
        // The places the call assigns: the local each is rooted in,
        // where it is written, and whether it is inside the local's
        // value.
        let mut assigned: Vec<(LocalId, Location, bool)> = Vec::new();
        for argument in arguments {
            let Some(parameter) = argument
                .parameter
                .and_then(|index| signature.parameters.get(index))
            else {
                continue;
            };
            let at = argument.value.at;
            if self.category(&argument.value) == ValueCategory::Connection {
                reports.push(error(
                    Category::Phase,
                    codes::CONNECTION_AS_CONSTANT,
                    at,
                    format!(
                        "`{callee}` has a `var` parameter and is not lifted: this is a connection, and it takes an ordinary value"
                    ),
                    "FN-15",
                ));
                continue;
            }
            if !parameter.var {
                continue;
            }
            // The root of the place: `book` of `book.orders[0]`.
            let root = argument.value.place_root();
            let takes = format!("the `var` parameter `{}` of `{callee}`", parameter.name);
            // "that holds an ordinary value of the parameter's type":
            // the call gives the place what the parameter holds, so
            // the two are one type. A place of `i64` takes no `i64?`
            // back, though an `i64` is a value of `i64?`.
            let taken = placed(substitute_within(&parameter.ty, &map), &own);
            let held = &argument.value.ty;
            let assignable = matches!(
                &root.kind,
                ExprKind::Local(local) if matches!(
                    self.locals[*local].kind,
                    LocalKind::Var | LocalKind::State | LocalKind::Cache
                )
            );
            if assignable && whole(&taken) && whole(held) && *held != taken {
                reports.push(error(
                    Category::Type,
                    codes::TYPE_MISMATCH,
                    at,
                    format!(
                        "this place is `{held}`, and {takes} is `{taken}`: the call gives \
                             the place what the parameter holds, so the two are one type"
                    ),
                    "FN-15",
                ));
                continue;
            }
            match &root.kind {
                ExprKind::Local(local) => {
                    let declared = &self.locals[*local];
                    if !matches!(
                        declared.kind,
                        LocalKind::Var | LocalKind::State | LocalKind::Cache
                    ) {
                        reports.push(
                            error(
                                Category::Name,
                                codes::BINDING_IMMUTABLE,
                                at,
                                format!(
                                    "`{}` is {} and cannot be assigned, as {takes} is: only a `var`, a `state` or a `cache` can",
                                    declared.name,
                                    declared.kind.describe()
                                ),
                                "FN-15",
                            )
                            .with_related(declared.at, "declared here"),
                        );
                    } else if let Some((what, rule)) = Self::read_only_inside(&argument.value) {
                        // NATIVE-6, TYPE-16: a property of a native
                        // value or of a delta is read-only whatever
                        // holds the value, as an assignment has it.
                        reports.push(error(
                            Category::Name,
                            codes::BINDING_IMMUTABLE,
                            at,
                            format!(
                                "this is a property of {what}, and a property is read and not assigned, as {takes} is"
                            ),
                            rule,
                        ));
                    } else {
                        // The call assigns the place, and is held to
                        // what an assignment there is.
                        let inside = !matches!(&argument.value.kind, ExprKind::Local(_));
                        assigned.push((*local, at, inside));
                    }
                }
                ExprKind::Constant { .. } => reports.push(error(
                    Category::Name,
                    codes::BINDING_IMMUTABLE,
                    at,
                    format!("a constant cannot be assigned, as {takes} is"),
                    "FN-15",
                )),
                // What is not read says nothing sure (ERR-9).
                ExprKind::Unread => {}
                _ => reports.push(
                    error(
                        Category::Type,
                        codes::ARGUMENT_PLACE,
                        at,
                        format!("{takes} takes a place, and this is none"),
                        "FN-15",
                    )
                    .with_note(
                        "a place is a `var`, a `state` or a `cache`, or a field or an element of one",
                    ),
                ),
            }
        }
        for report in reports {
            self.diagnostics.emit(report);
        }
        // In a graph an assignment is held to more (CTRL-14, CTRL-15),
        // and so is a call that assigns: a `var` declared outside a
        // loop that makes a child for each member is not carried, and
        // one that holds an ordinary value is not assigned in a branch
        // selected at runtime. What the call gives the place is an
        // ordinary value: the function is not lifted.
        if !self.graph {
            return;
        }
        for (local, at, inside) in assigned {
            if self.dynamic.is_some_and(|outside| local < outside) {
                self.diagnostics.emit(
                    error(
                        Category::Phase,
                        codes::LOOP_CARRIED,
                        at,
                        format!(
                            "`{}` is declared outside this loop, which makes one child for each member: nothing is carried from one child to the next",
                            self.locals[local].name
                        ),
                        "CTRL-15",
                    )
                    .with_related(self.locals[local].at, "declared here"),
                );
            } else {
                self.assigned_category(local, (ValueCategory::Value, at), inside);
            }
        }
    }

    /// The global store: "The key `"run.output"` is the run's own,
    /// where it keeps its output: a program that names it rejects
    /// (`store.reserved_key`)."
    ///
    /// A key is the `key` of one of the store's operations: core's own
    /// `get`, `has` and `put` (CAP-6), which are the candidates of core
    /// that take the store. A function of a program that is given the
    /// store reads and writes it through those, so its own parameters
    /// are no keys. A key written out is judged here. One that is a
    /// `const` parameter of the function it is in is known where the
    /// graph is built, which no pass reads yet.
    fn store_keys(&mut self, selected: &Selected) {
        let &Selected {
            module, arguments, ..
        } = selected;
        let Some(signature) = selected.signature else {
            return;
        };
        let Some(store) = self.program.capability_type("global_state") else {
            return;
        };
        // An operation of the store is core's, and takes the store.
        if Some(module) != self.program.core()
            || !signature
                .parameters
                .iter()
                .any(|parameter| parameter.ty == store)
        {
            return;
        }
        let reserved: Vec<Location> = arguments
            .iter()
            .filter(|argument| {
                argument
                    .parameter
                    .and_then(|index| signature.parameters.get(index))
                    .is_some_and(|parameter| parameter.constant && parameter.name == "key")
            })
            .filter(|argument| {
                matches!(
                    &argument.value.kind,
                    ExprKind::Literal(Value::Str(key)) if key == RUN_OUTPUT_KEY
                )
            })
            .map(|argument| argument.value.at)
            .collect();
        for at in reserved {
            self.diagnostics.emit(
                error(
                    Category::Injectable,
                    codes::STORE_RESERVED_KEY,
                    at,
                    format!(
                        "the key `\"{RUN_OUTPUT_KEY}\"` is the run's own: its output is kept there"
                    ),
                    "TIME-8",
                )
                .with_note("a program's own values go under another key"),
            );
        }
    }

    /// TST-3: a trace "is bound with `let`, compared ..., and given to
    /// a `const fn` as an argument. It is not a temporal argument
    /// outside an eval". `evaluating` is true for the call an `eval`
    /// stands for, whose temporal parameters take traces (TST-5).
    ///
    /// A value that holds a trace at any depth, `[seen]`, is held to
    /// the same: wrapping a trace does not take it where it may not go.
    /// A trace a `const fn` is given is the list of its publications
    /// there (TST-16), and is held to the parameter's type where the
    /// argument is read: see `publications`. A value that holds one,
    /// `[seen]`, is given as it is and not held to the parameter's type.
    fn traces_given(&mut self, selected: &Selected, evaluating: bool) {
        let &Selected {
            module,
            declaration,
            arguments,
            signature,
            ..
        } = selected;
        // FN-1: a value function, or a value candidate.
        let value = matches!(
            self.program.modules[module].declarations[declaration]
                .kind
                .role(),
            Some(Role::Value)
        );
        if value {
            return;
        }
        let constant = |index: usize| {
            signature.is_some_and(|signature| {
                signature
                    .parameters
                    .get(index)
                    .is_some_and(|parameter| parameter.constant)
            })
        };
        let escaped: Vec<Location> = arguments
            .iter()
            .filter(|argument| holds_trace(&argument.value.ty))
            .filter(|argument| {
                // An eval's own trace, given to a parameter that takes
                // one.
                let taken = evaluating
                    && matches!(argument.value.ty, Type::Trace { .. })
                    && argument.parameter.is_some_and(|index| !constant(index));
                !taken
            })
            .map(|argument| argument.value.at)
            .collect();
        for at in escaped {
            self.trace_escape(at, Takes::Parameters);
        }
    }

    /// FN-1: "a `const` parameter takes what is fixed when the graph is
    /// built. In composition that is any ordinary value. In a hook and
    /// in a value function it is a constant expression: of literals,
    /// enum members, generic value parameters, and the enclosing
    /// function's own `const` parameters."
    ///
    /// A graph's own rule, that a `const` parameter is given no
    /// connection, is `constants_of`'s. This is the other half: where
    /// values are those of one evaluation, in a hook, a source or a
    /// value function.
    fn fixed_arguments(&mut self, selected: &Selected) {
        if self.graph {
            return;
        }
        let &Selected {
            arguments,
            signature,
            ..
        } = selected;
        let Some(signature) = signature else {
            return;
        };
        let given: Vec<(Location, String)> = arguments
            .iter()
            .filter_map(|argument| {
                let parameter = signature.parameters.get(argument.parameter?)?;
                // What was not read has been reported, or is for a
                // later part (ERR-9).
                let judged = parameter.constant
                    && !matches!(argument.value.kind, ExprKind::Unread)
                    && !self.is_fixed(&argument.value);
                judged.then(|| (argument.value.at, parameter.name.clone()))
            })
            .collect();
        for (at, parameter) in given {
            self.diagnostics.emit(
                error(
                    Category::Type,
                    codes::ARGUMENT_CONSTANT,
                    at,
                    format!(
                        "`{parameter}` is a `const` parameter, and takes what is fixed when the \
                         graph is built: this is a value of one evaluation"
                    ),
                    "FN-1",
                )
                .with_note(
                    "a literal, an enum member, a generic value parameter, or a `const` \
                     parameter of the enclosing function is fixed",
                ),
            );
        }
    }

    /// True when `value` is a constant expression (FN-1): "of literals,
    /// enum members, constants (UNIT-13), generic value parameters, and
    /// the enclosing function's own `const` parameters."
    fn is_fixed(&self, value: &Expr) -> bool {
        match &value.kind {
            ExprKind::Literal(_)
            | ExprKind::EnumMember { .. }
            | ExprKind::GenericValue { .. }
            | ExprKind::Constant { .. } => true,
            // A parameter's local has its position among the parameters.
            ExprKind::Local(local) => self
                .signature
                .parameters
                .get(*local)
                .is_some_and(|parameter| parameter.constant),
            ExprKind::Widen(inner) => self.is_fixed(inner),
            // `Side(N)`, of what is fixed: the member is fixed too.
            ExprKind::EnumFrom { value, .. } => self.is_fixed(value),
            ExprKind::Unary { operand, .. } => self.is_fixed(operand),
            ExprKind::Binary { left, right, .. } => self.is_fixed(left) && self.is_fixed(right),
            // A symbol over constants: `-1`, `1 + 2`.
            ExprKind::Call {
                arguments,
                form: Form::Binary(_) | Form::Unary(_),
                ..
            } => arguments
                .iter()
                .all(|argument| self.is_fixed(&argument.value)),
            ExprKind::Tuple(entries) | ExprKind::Sequence(entries) => {
                entries.iter().all(|entry| self.is_fixed(entry))
            }
            ExprKind::Pairs(entries) => entries
                .iter()
                .all(|(key, value)| self.is_fixed(key) && self.is_fixed(value)),
            _ => false,
        }
    }

    /// TIME-8: "giving one to a parameter of its type [is a use]". A
    /// parameter written as a generic parameter or `any` is of no one
    /// type: it takes any, and what the function does with its
    /// argument, keep it in a list say, it would do with the
    /// capability. Such a parameter takes none (owner's decision of
    /// 2026-11-16 on hhenson/hgl_spec#73, question 5).
    ///
    /// The call is read, so each argument is known for the parameter it
    /// is given to: of a function, or of the candidate an operator's
    /// call or a symbol selected. A capability given to a parameter of
    /// another type written out does not fit it, and is reported as any
    /// value is (EXPR-5).
    fn capabilities_given(&mut self, selected: &Selected) {
        let &Selected {
            arguments,
            signature,
            ..
        } = selected;
        let Some(signature) = signature else {
            return;
        };
        let mut given: Vec<(Location, String, String)> = Vec::new();
        for argument in arguments {
            let (Some(parameter), Some(capability)) = (
                argument
                    .parameter
                    .and_then(|index| signature.parameters.get(index)),
                self.capability_of(&argument.value),
            ) else {
                continue;
            };
            // `T?` and `any?` take what `T` and `any` take, or null.
            let mut written = &parameter.ty;
            while let Type::Nullable(inner) = written {
                written = inner;
            }
            let open = match written {
                Type::Parameter { .. } => "a generic parameter",
                Type::Scalar(Scalar::Any) => "`any`",
                _ => continue,
            };
            given.push((
                argument.value.at,
                self.locals[capability].name.clone(),
                format!("`{}` is written as {open}", parameter.name),
            ));
        }
        for (at, capability, parameter) in given {
            self.diagnostics.emit(
                error(
                    Category::Injectable,
                    codes::CAPABILITY_AS_VALUE,
                    at,
                    format!(
                        "`{capability}` is a capability, and is given only to a parameter of \
                         its type: {parameter}"
                    ),
                    "TIME-8",
                )
                .with_note("a parameter that takes any type takes no capability"),
            );
        }
    }

    /// A call, as the form or the function its callee names.
    fn call_read(&mut self, node: &SyntaxNode, expected: Option<&Type>) -> (ExprKind, Type) {
        let (Some(callee), Some(list)) = (node.children().next(), child(node, SyntaxKind::ArgList))
        else {
            return (ExprKind::Unread, Type::Unknown);
        };
        // ERR-13: "as the callee of a call each is the form this
        // chapter defines, whatever else is in scope."
        if callee.kind() == SyntaxKind::NameExpr
            && child(&callee, SyntaxKind::QualifiedName).is_none()
            && let Some(name) = name_token(&callee)
            && let Some(form) = self.result_form(&name, &list, expected)
        {
            return form;
        }
        let found = match callee.kind() {
            SyntaxKind::NameExpr => name_token(&callee)
                .and_then(|name| self.declarations(&name).map(|found| (name, found))),
            _ => None,
        };
        let Some((name, (module, declarations))) = found else {
            // TIME-9: "The operations on an endpoint are intrinsics of
            // the language". Nothing declares one, so its name is bound
            // to nothing; it is read in a node's hooks.
            if self.hook.is_some()
                && callee.kind() == SyntaxKind::NameExpr
                && let Some(name) = name_token(&callee)
                && !self.bindings.contains_key(&(self.file, name.span().start))
                && let Some(operation) = EndpointOperation::language_form(name.text())
            {
                return self.endpoint(operation, &name, &list);
            }
            // `(fn(x) => x + 1)(7)` (FN-10), and the call of what one
            // gives, `(fn(a) => fn(b) => a + b)(1)(2)`: the function is
            // read where it is written, for what it reads (FN-14).
            if calls_anonymous(&callee) {
                self.expression(&callee, Some(&Type::Unknown));
            }
            return self.unread_call(&list);
        };
        // TIME-9: "The operations on an endpoint are core's native value
        // functions ... `modified` and `valid` are selectors in a `when`
        // header (TIME-3): there they are forms of the language, which
        // set subscription and admission." In a node's hooks each name
        // is read as the form it is, a selector or a guard among them,
        // whether core has declared the function or not: the call is of
        // core's own, and what it gives is the same.
        if self.hook.is_some()
            && Some(module) == self.program.core()
            && child(&callee, SyntaxKind::QualifiedName).is_none()
            && let Some(operation) = EndpointOperation::language_form(name.text())
        {
            return self.endpoint(operation, &name, &list);
        }
        self.call_named(&name, &list, (module, declarations), expected, None)
    }

    /// A call of what `name` is bound to: `declarations` of a module.
    /// `generics` is the generic arguments the call writes after the
    /// name, when it writes them (GEN-8).
    fn call_named(
        &mut self,
        name: &SyntaxToken,
        list: &SyntaxNode,
        (module, declarations): (usize, &[usize]),
        expected: Option<&Type>,
        generics: Option<&CallArguments>,
    ) -> (ExprKind, Type) {
        let program = self.program;
        let kind_of = |declaration: usize| program.modules[module].declarations[declaration].kind;
        // FN-4: "Composition and eval choose the temporal definition if
        // one exists; runtime/value evaluation chooses the value
        // definition." FN-3: "A composition body calls any function".
        let temporal_function = declarations.iter().copied().find(|&declaration| {
            matches!(
                kind_of(declaration),
                DeclarationKind::Function(Role::Temporal)
                    | DeclarationKind::NativeFunction(Role::Temporal)
            )
        });
        if let (true, Some(declaration)) = (self.graph, temporal_function) {
            return self.call_function(name, list, (module, declaration), expected, generics);
        }
        let value_function = declarations.iter().copied().find(|&declaration| {
            matches!(
                kind_of(declaration),
                DeclarationKind::Function(Role::Value)
                    | DeclarationKind::NativeFunction(Role::Value)
            )
        });
        if let Some(declaration) = value_function {
            return self.call_function(name, list, (module, declaration), expected, generics);
        }
        let structure = declarations
            .iter()
            .copied()
            .find(|&declaration| kind_of(declaration) == DeclarationKind::Struct);
        if let Some(declaration) = structure {
            // GEN-2: a generic struct "infers them from constructor
            // fields and expected type".
            let given = match not_null(expected) {
                Some(Type::Named {
                    module: expected_module,
                    declaration: expected_declaration,
                    arguments,
                    ..
                }) if (*expected_module, *expected_declaration) == (module, declaration) => {
                    Some(arguments.clone())
                }
                _ => None,
            };
            let settled = !expected.is_some_and(open);
            return self.construct(name, list, module, declaration, given, settled);
        }
        // TYPE-7: "A call of the enum's name constructs a member."
        let enumeration = declarations
            .iter()
            .copied()
            .find(|&declaration| kind_of(declaration) == DeclarationKind::Enum);
        if let Some(declaration) = enumeration {
            return self.enum_from(name, list, module, declaration);
        }
        let operator = declarations
            .iter()
            .copied()
            .find(|&declaration| kind_of(declaration) == DeclarationKind::Operator);
        if let Some(declaration) = operator {
            let operator = OperatorRef {
                module,
                declaration,
            };
            return self.call_operator(name, list, operator, expected, generics);
        }
        let temporal = declarations.iter().any(|&declaration| {
            matches!(
                kind_of(declaration),
                DeclarationKind::Function(Role::Temporal)
                    | DeclarationKind::NativeFunction(Role::Temporal)
            )
        });
        // NATIVE-6: "a native type is not constructed in HGL: a call of
        // its name rejects."
        let native = declarations
            .iter()
            .any(|&declaration| kind_of(declaration) == DeclarationKind::NativeType);
        if native {
            self.diagnostics.emit(
                error(
                    Category::Type,
                    codes::CALL_NOT_CALLABLE,
                    Location::new(self.file, name.span()),
                    format!(
                        "`{}` is a native type, and no call constructs one",
                        name.text()
                    ),
                    "NATIVE-6",
                )
                .with_note("a value of a native type comes from a native function"),
            );
        }
        if temporal {
            // FN-3: "A runtime body calls only value functions, and so
            // does a value function." FN-4: "A bad selected signature
            // is an error, not fallback to the other role."
            self.diagnostics.emit(error(
                Category::FunctionKind,
                codes::CALL_ROLE,
                Location::new(self.file, name.span()),
                format!(
                    "`{}` is a temporal function, and a {} calls value functions",
                    name.text(),
                    if self.hook.is_some() {
                        "hook"
                    } else if self.generating {
                        "source"
                    } else {
                        "value function"
                    }
                ),
                "FN-3",
            ));
        }
        self.unread_call(list)
    }

    /// `Side(1)` or `Side("buy")` (TYPE-7): "A call of the enum's name
    /// constructs a member. It takes one argument, by position, that is
    /// an `i64` or a `str`, and any other type rejects: `Side(1)` is the
    /// member whose number is 1, and `Side("buy")` the member of that
    /// exact name. Construction is checked: a constant that is no
    /// member's number or name rejects, and a value that is none fails
    /// where it is evaluated."
    fn enum_from(
        &mut self,
        name: &SyntaxToken,
        list: &SyntaxNode,
        module: usize,
        declaration: usize,
    ) -> (ExprKind, Type) {
        let ty = Type::Named {
            name: name.text().to_string(),
            module,
            declaration,
            arguments: Vec::new(),
            kind: NamedKind::Enum,
        };
        let Some(argument) = self.one_argument(name, list, "TYPE-7") else {
            return (ExprKind::Unread, ty);
        };
        let value = self.expression(&argument, None);
        let number = value.ty == Type::Scalar(Scalar::I64);
        let text = value.ty == Type::Scalar(Scalar::Str);
        if !number && !text {
            // What is not known has been reported (ERR-9).
            if !open(&value.ty) {
                self.diagnostics.emit(error(
                    Category::Type,
                    codes::TYPE_MISMATCH,
                    value.at,
                    format!(
                        "`{}` is constructed from a member's number, an `i64`, or its name, \
                         a `str`, and this is `{}`",
                        name.text(),
                        value.ty
                    ),
                    "TYPE-7",
                ));
            }
            return (ExprKind::Unread, ty);
        }
        let kind = match self.member_constructed(module, declaration, &argument) {
            // A constant that names a member is that member: `Side(0)`
            // is `Side.buy`, wherever a member is looked for, a `switch`
            // label and a `const` argument among them.
            Some(Ok(member)) => ExprKind::EnumMember {
                module,
                declaration,
                member,
            },
            Some(Err(missing)) => {
                self.diagnostics.emit(
                    error(
                        Category::Name,
                        codes::MEMBER_UNKNOWN,
                        value.at,
                        format!("`{}` has {missing}", name.text()),
                        "TYPE-7",
                    )
                    .with_note("a member's name is matched exactly"),
                );
                ExprKind::Unread
            }
            None => ExprKind::EnumFrom {
                module,
                declaration,
                value: Box::new(value),
            },
        };
        (kind, ty)
    }

    // ---- Result forms (ERR-2 to ERR-5) ----

    /// The call of `name` as the result form it names, when it names
    /// one (ERR-13): `ok`, `err`, `is_ok`, `is_err` or `capture`.
    fn result_form(
        &mut self,
        name: &SyntaxToken,
        list: &SyntaxNode,
        expected: Option<&Type>,
    ) -> Option<(ExprKind, Type)> {
        Some(match name.text() {
            "ok" => self.tagged(true, name, list, expected),
            "err" => self.tagged(false, name, list, expected),
            "is_ok" => self.tag_test(true, name, list),
            "is_err" => self.tag_test(false, name, list),
            "capture" => self.capture(name, list),
            _ => return None,
        })
    }

    /// The one argument a result form takes, "by position" (ERR-2,
    /// ERR-3). One with a name, a second, and none at all are each
    /// reported; any other than the first is read for what it reports
    /// itself.
    fn one_argument(
        &mut self,
        form: &SyntaxToken,
        list: &SyntaxNode,
        rule: &'static str,
    ) -> Option<SyntaxNode> {
        let mut arguments = arguments_of(list).into_iter();
        let Some((label, first)) = arguments.next() else {
            self.diagnostics.emit(error(
                Category::Type,
                codes::ARGUMENT_MISSING,
                Location::new(self.file, form.span()),
                format!("`{}` is given nothing: it takes one value", form.text()),
                rule,
            ));
            return None;
        };
        if let Some(label) = label {
            self.diagnostics.emit(error(
                Category::Name,
                codes::ARGUMENT_UNKNOWN,
                Location::new(self.file, label.span()),
                format!(
                    "`{}` takes its value by position, and `{}` names no parameter of it",
                    form.text(),
                    label.text()
                ),
                rule,
            ));
        }
        for (_, surplus) in arguments {
            self.diagnostics.emit(error(
                Category::Type,
                codes::ARGUMENT_UNEXPECTED,
                self.at(&surplus),
                format!("`{}` takes one value, and this is one more", form.text()),
                rule,
            ));
            self.expression(&surplus, Some(&Type::Unknown));
        }
        Some(first)
    }

    /// `ok(value)` or `err(value)` (ERR-2): "`ok(value)` constructs a
    /// success and `err(value)` a failure. Each takes one argument by
    /// position and no generic argument ... Both type arguments come
    /// from the complete `result<T,E>` its place expects: `value` is
    /// read as that T for `ok` and as that E for `err`. Where the place
    /// expects no complete result type the constructor rejects
    /// (`result.context_type`), and a payload that is not of its side's
    /// type rejects as any value does (EXPR-5)."
    fn tagged(
        &mut self,
        success: bool,
        name: &SyntaxToken,
        list: &SyntaxNode,
        expected: Option<&Type>,
    ) -> (ExprKind, Type) {
        let argument = self.one_argument(name, list, "ERR-2");
        // The result the place expects: of `result<T, E>?`, the result.
        let sides = match not_null(expected) {
            Some(result @ Type::Result { value, error }) if whole(result) => {
                Some((result.clone(), (**value).clone(), (**error).clone()))
            }
            _ => None,
        };
        let Some((result, value, failure)) = sides else {
            // A place whose type is not known has been reported, or is
            // for a later part: nothing is said of what it expects
            // (ERR-9).
            if !expected.is_some_and(|expected| !whole(expected) || open(expected)) {
                self.diagnostics.emit(
                    error(
                        Category::Type,
                        codes::RESULT_CONTEXT_TYPE,
                        Location::new(self.file, name.span()),
                        format!(
                            "`{}` takes both its types from the `result<T, E>` its place \
                             expects, and this place expects none",
                            name.text()
                        ),
                        "ERR-2",
                    )
                    .with_note("write the type: `let outcome: result<i64, error> = ok(1)`"),
                );
            }
            if let Some(argument) = argument {
                self.expression(&argument, Some(&Type::Unknown));
            }
            return (ExprKind::Unread, Type::Unknown);
        };
        let Some(argument) = argument else {
            return (ExprKind::Unread, result);
        };
        let side = if success { &value } else { &failure };
        let payload = self.expression(&argument, Some(side));
        let what = if success {
            "the type of a success"
        } else {
            "the type of a failure"
        };
        let payload = self.fit(payload, side, what);
        let kind = ExprKind::Tagged {
            success,
            payload: Box::new(payload),
        };
        (kind, result)
    }

    /// `is_ok(value)` or `is_err(value)` (ERR-3): they "take one
    /// `result<T,E>` by position and give a `bool`; any other type
    /// rejects."
    fn tag_test(
        &mut self,
        success: bool,
        name: &SyntaxToken,
        list: &SyntaxNode,
    ) -> (ExprKind, Type) {
        let Some(argument) = self.one_argument(name, list, "ERR-3") else {
            return (ExprKind::Unread, BOOL);
        };
        let value = self.expression(&argument, None);
        if !matches!(value.ty, Type::Result { .. }) && whole(&value.ty) && !open(&value.ty) {
            self.diagnostics.emit(error(
                Category::Type,
                codes::TYPE_MISMATCH,
                value.at,
                format!(
                    "`{}` tests a `result<T, E>`, and this is `{}`",
                    name.text(),
                    value.ty
                ),
                "ERR-3",
            ));
        }
        let kind = ExprKind::TagTest {
            success,
            value: Box::new(value),
        };
        (kind, BOOL)
    }

    /// `binding.value` or `binding.error` (ERR-3): "`binding.value` is
    /// the T and `binding.error` the E; `.value`/`.error` requires the
    /// matching fact, and without it, or on what is no such binding,
    /// rejects (`result.required_variant`)."
    fn payload(
        &mut self,
        base: Expr,
        name: &SyntaxToken,
        (value, failure): (Type, Type),
    ) -> (ExprKind, Type) {
        let at = Location::new(self.file, name.span());
        let success = match name.text() {
            "value" => true,
            "error" => false,
            other => {
                self.diagnostics.emit(error(
                    Category::Name,
                    codes::MEMBER_UNKNOWN,
                    at,
                    format!(
                        "`{}` has no `{other}`: a result has `.value` and `.error`",
                        base.ty
                    ),
                    "ERR-3",
                ));
                return (ExprKind::Unread, Type::Unknown);
            }
        };
        let fact = match base.kind {
            ExprKind::Local(local) if success => Some(Fact::Success(local)),
            ExprKind::Local(local) => Some(Fact::Failure(local)),
            _ => None,
        };
        if !fact.is_some_and(|fact| self.present.contains(&fact)) {
            let (test, tag) = if success {
                ("is_ok", "a success")
            } else {
                ("is_err", "a failure")
            };
            let message = match &base.kind {
                ExprKind::Local(local) => format!(
                    "`.{}` is read where `{}` is {tag}, and nothing here proves it one",
                    name.text(),
                    self.locals[*local].name
                ),
                _ => format!(
                    "`.{}` is read of a binding a guard has proved {tag}, and this is no \
                     binding",
                    name.text()
                ),
            };
            self.diagnostics.emit(
                error(
                    Category::Type,
                    codes::RESULT_REQUIRED_VARIANT,
                    at,
                    message,
                    "ERR-3",
                )
                .with_note(format!(
                    "bind the result with `let` and guard the read: `if {test}(outcome) {{ ... }}`"
                )),
            );
        }
        let ty = if success { value } else { failure };
        let kind = ExprKind::Payload {
            success,
            result: Box::new(base),
        };
        (kind, ty)
    }

    /// `.message`, `.code` or `.context` of an error (ERR-4):
    /// "`.message` is the `str` and `.code` the `str?`. `.context` is a
    /// read-only nullable record", an `error context?`.
    fn error_part(&mut self, base: Expr, name: &SyntaxToken) -> (ExprKind, Type) {
        let (part, ty) = match name.text() {
            "message" => (ErrorPart::Message, Type::Scalar(Scalar::Str)),
            "code" => (ErrorPart::Code, nullable(Type::Scalar(Scalar::Str))),
            "context" => (ErrorPart::Context, nullable(Type::ErrorContext)),
            other => {
                self.diagnostics.emit(error(
                    Category::Name,
                    codes::MEMBER_UNKNOWN,
                    Location::new(self.file, name.span()),
                    format!(
                        "an `error` has no `{other}`: it has `.message`, `.code` and `.context`"
                    ),
                    "ERR-4",
                ));
                return (ExprKind::Unread, Type::Unknown);
            }
        };
        let kind = ExprKind::ErrorPart {
            base: Box::new(base),
            part,
        };
        (kind, ty)
    }

    /// `.node`, `.phase` or `.evaluation_time` of an error's context
    /// (ERR-4): "a read-only nullable record of `.node: str`, `.phase:
    /// str`, one of `"evaluate"`, `"start"`, and `"stop"`, and
    /// `.evaluation_time: datetime`". `base` has been proved present.
    fn context_part(&mut self, base: Expr, name: &SyntaxToken) -> (ExprKind, Type) {
        let (part, ty) = match name.text() {
            "node" => (ErrorPart::Node, Type::Scalar(Scalar::Str)),
            "phase" => (ErrorPart::Phase, Type::Scalar(Scalar::Str)),
            "evaluation_time" => (ErrorPart::EvaluationTime, Type::Scalar(Scalar::Datetime)),
            other => {
                self.diagnostics.emit(error(
                    Category::Name,
                    codes::MEMBER_UNKNOWN,
                    Location::new(self.file, name.span()),
                    format!(
                        "an error's context has no `{other}`: it has `.node`, `.phase` and \
                         `.evaluation_time`"
                    ),
                    "ERR-4",
                ));
                return (ExprKind::Unread, Type::Unknown);
            }
        };
        let kind = ExprKind::ErrorPart {
            base: Box::new(base),
            part,
        };
        (kind, ty)
    }

    /// `error(message: text, code: optional_code)` (ERR-4): it "owns a
    /// required string message and optional string code, each given by
    /// its name: `message` is a `str`, and `code` a `str?` that is null
    /// when omitted."
    ///
    /// The other calls of a type's name, `str(value)`, `bytes(value)`
    /// and `any(value)`, are conversions, and are for a later part.
    fn type_call(&mut self, node: &SyntaxNode) -> (ExprKind, Type) {
        let word = tokens(node).next();
        let list = child(node, SyntaxKind::ArgList);
        let (Some(word), Some(list)) = (word, list) else {
            return (ExprKind::Unread, Type::Unknown);
        };
        if word.kind() != SyntaxKind::KwError {
            return self.unread_call(&list);
        }
        let text = Type::Scalar(Scalar::Str);
        let optional = nullable(Type::Scalar(Scalar::Str));
        let mut message: Option<Expr> = None;
        let mut code: Option<Expr> = None;
        for (label, value) in arguments_of(&list) {
            let Some(label) = label else {
                // "Each given by its name".
                self.diagnostics.emit(error(
                    Category::Type,
                    codes::ARGUMENT_UNEXPECTED,
                    self.at(&value),
                    "`error` takes `message` and `code`, each by its name".to_string(),
                    "ERR-4",
                ));
                self.expression(&value, Some(&Type::Unknown));
                continue;
            };
            let (slot, ty, what) = match label.text() {
                "message" => (&mut message, &text, "the message of an error"),
                "code" => (&mut code, &optional, "the code of an error"),
                other => {
                    self.diagnostics.emit(error(
                        Category::Name,
                        codes::ARGUMENT_UNKNOWN,
                        Location::new(self.file, label.span()),
                        format!("`error` takes `message` and `code`, and no `{other}`"),
                        "ERR-4",
                    ));
                    self.expression(&value, Some(&Type::Unknown));
                    continue;
                }
            };
            if slot.is_some() {
                self.diagnostics.emit(error(
                    Category::Name,
                    codes::ARGUMENT_DUPLICATE,
                    Location::new(self.file, label.span()),
                    format!("`{}` is given twice", label.text()),
                    "ERR-4",
                ));
                self.expression(&value, Some(&Type::Unknown));
                continue;
            }
            // A second `&mut self` use: the slot is filled after the
            // value is read.
            let filled = label.text() == "message";
            let value = self.expression(&value, Some(ty));
            let value = self.fit(value, ty, what);
            if filled {
                message = Some(value);
            } else {
                code = Some(value);
            }
        }
        let Some(message) = message else {
            self.diagnostics.emit(error(
                Category::Type,
                codes::ARGUMENT_MISSING,
                Location::new(self.file, word.span()),
                "`error` is given no `message`: an error owns one".to_string(),
                "ERR-4",
            ));
            return (ExprKind::Unread, Type::Error);
        };
        let kind = ExprKind::Error {
            message: Box::new(message),
            code: code.map(Box::new),
        };
        (kind, Type::Error)
    }

    /// The arguments of the call `list` is the list of. For the call a
    /// `capture` names, they are what follows the callable's name:
    /// `capture(required, value)` gives `required` the argument
    /// `value` (ERR-7).
    fn call_arguments(&self, list: &SyntaxNode) -> Vec<(Option<SyntaxToken>, SyntaxNode)> {
        arguments_of(list)
            .into_iter()
            .filter(|(_, value)| Some(value) != self.callable.as_ref())
            .collect()
    }

    /// `eval(target(...))` in a named test.
    ///
    /// TST-5: "Eval takes a call of a module function, written as any
    /// call is: `eval(target(value: [7, _, 9]))`. The call binds its
    /// arguments by the function's public positional/named rules, and
    /// its generic arguments as any call does. Temporal/value role
    /// selection follows FN-4 ... An operator is no target: a function
    /// wraps it ... Const parameters receive configuration; temporal
    /// parameters receive contextually typed publication traces. A
    /// trace is an argument of that call and of no call inside it. A
    /// second argument is the run's configuration, a `RunConfig`".
    ///
    /// TST-3: "An eval of a target that has a result is a trace of that
    /// result's type". TST-8: "an outputless target has no recorder or
    /// comparable result. Its eval stands as a statement and has no
    /// value: binding or comparing it rejects."
    ///
    /// TST-17: in an integration test `run(call, config)` is written
    /// beside it, "whose value is the run's output as an eval's is ...
    /// in a test that is no integration test it rejects." It is read
    /// as an eval is, and is one in real time (RUN-3).
    ///
    /// The call is read as any call of its function is, with a trace
    /// expected of each temporal parameter.
    fn eval(&mut self, node: &SyntaxNode) -> (ExprKind, Type) {
        let at = self.at(node);
        let real_time = node.kind() == SyntaxKind::RunExpr;
        if real_time && !self.integration {
            // "the word": the first token of the form.
            let word = match node.first_token() {
                Some(word) => Location::new(self.file, word.span()),
                None => at,
            };
            self.diagnostics.emit(
                error(
                    Category::Phase,
                    codes::RUN_POSITION,
                    word,
                    "`run` is written in a test that is no integration test".to_string(),
                    "RUN-3",
                )
                .with_note(
                    "a test evaluates in simulation, `eval(...)`; an `integration_test` may \
                     run in real time",
                ),
            );
        }
        let Some(list) = child(node, SyntaxKind::ArgList) else {
            return (ExprKind::Unread, Type::Unknown);
        };
        let given = arguments_of(&list);
        // The parser has reported an eval given nothing.
        let Some((_, argument)) = given.first().cloned() else {
            return (ExprKind::Unread, Type::Unknown);
        };
        // EXPR-1: "parentheses group", so `(call)` is the call.
        let target = ungrouped(argument.clone());
        let called = self.call_taken(&target);
        let function = called
            .as_ref()
            .and_then(|called| self.evaluated(&called.name));
        let (Some(called), Some((module, declaration))) = (called.clone(), function) else {
            // A call of a name nothing binds is left for the prelude,
            // and is not judged (ERR-9).
            if !self.unbound(called.as_ref()) {
                self.diagnostics.emit(
                    error(
                        Category::Type,
                        codes::EVAL_TARGET,
                        self.at(&argument),
                        "an eval's first argument is a call of a function of a module, and this \
                         is none"
                            .to_string(),
                        "TST-5",
                    )
                    .with_note("an operator is evaluated through a function that calls it"),
                );
            }
            // What the call was given is read all the same, with
            // nothing expected of it, and then what follows the call.
            let unread = match &called {
                Some(called) => self.unread_call(&called.arguments),
                None => (ExprKind::Unread, Type::Unknown),
            };
            self.after_call(&given, "an eval", "TST-5");
            return unread;
        };
        // FN-15: a function that declares a `var` parameter "is no
        // target of an `eval`": an eval lifts a value function (TST-8),
        // and that one is not lifted.
        if let Some(Detail::Callable(signature)) =
            self.program.modules[module].details.get(declaration)
            && signature.parameters.iter().any(|parameter| parameter.var)
        {
            self.diagnostics.emit(error(
                Category::Type,
                codes::EVAL_TARGET,
                self.at(&argument),
                format!(
                    "`{}` has a `var` parameter, and is no target of an eval",
                    called.name.text()
                ),
                "FN-15",
            ));
            // Nothing more is reported of the call (ERR-9).
            let unread = self.unread_call(&called.arguments);
            self.after_call(&given, "an eval", "TST-5");
            return unread;
        }
        let name = called.name.clone();
        self.evaluating = true;
        let (kind, ty) = self.call_function(
            &called.name,
            &called.arguments,
            (module, declaration),
            None,
            called.generics,
        );
        self.evaluating = false;
        // The call is held to what any call in a graph is: a `const`
        // parameter is given no connection (FN-3).
        self.constants_of(&kind);
        // EXPR-13: arguments are read in the order written, so the
        // call's before the configuration.
        let config = self.after_call(&given, "an eval", "TST-5");
        // TST-7: "Within one eval all inputs and expectation use the
        // same dense/timed style." The first trace that has cells says
        // which, and a later one of the other is reported.
        let mut style: Option<TraceStyle> = None;
        if let ExprKind::Call { arguments, .. } = &kind {
            for argument in arguments {
                let Type::Trace {
                    style: Some(given), ..
                } = &argument.value.ty
                else {
                    continue;
                };
                match style {
                    None => style = Some(*given),
                    Some(first) if !first.same_notation(*given) => {
                        self.trace_style(argument.value.at, first, *given);
                    }
                    Some(_) => {}
                }
            }
        }
        let has_result = matches!(
            self.program.modules[module].details.get(declaration),
            Some(Detail::Callable(signature)) if signature.result.is_some()
        );
        let call = Expr { kind, ty, at };
        if !has_result {
            // "Its eval stands as a statement and has no value". One in
            // parentheses is the same statement (EXPR-1).
            let mut holder = node.parent();
            while let Some(parent) = holder
                .as_ref()
                .filter(|parent| parent.kind() == SyntaxKind::ParenExpr)
            {
                holder = parent.parent();
            }
            let statement = holder.is_some_and(|parent| parent.kind() == SyntaxKind::ExprStmt);
            if !statement {
                self.diagnostics.emit(
                    error(
                        Category::Type,
                        codes::EVAL_NO_RESULT,
                        at,
                        format!(
                            "`{}` has no result, so this eval has no value to bind or compare",
                            name.text()
                        ),
                        "TST-8",
                    )
                    .with_note("an eval of a target with no result stands as a statement"),
                );
            }
            return (
                ExprKind::Eval {
                    call: Box::new(call),
                    config,
                    real_time,
                },
                Type::Unknown,
            );
        }
        // "Each cell is one cycle's publication": the `delta<T>` of the
        // result's type. One not known stays so.
        let cell = if open(&call.ty) {
            call.ty.clone()
        } else {
            Type::delta(call.ty.clone())
        };
        let ty = Type::Trace {
            cell: Box::new(cell),
            style,
        };
        (
            ExprKind::Eval {
                call: Box::new(call),
                config,
                real_time,
            },
            ty,
        )
    }

    /// What an eval or a run statement, `what`, is given after its
    /// call, `given` being all it is given: the run's configuration,
    /// and nothing more. "A second argument is the run's configuration"
    /// (TST-5, RUN-1); a third is reported under `rule` and read with
    /// nothing expected of it.
    fn after_call(
        &mut self,
        given: &[(Option<SyntaxToken>, SyntaxNode)],
        what: &str,
        rule: &'static str,
    ) -> Option<Box<Expr>> {
        let config = given.get(1).map(|(_, written)| self.run_config(written));
        for (_, extra) in given.iter().skip(2) {
            self.diagnostics.emit(error(
                Category::Type,
                codes::ARGUMENT_UNEXPECTED,
                self.at(extra),
                format!("{what} takes a call and a run configuration, and nothing more"),
                rule,
            ));
            self.expression(extra, None);
        }
        config
    }

    /// The call an eval or a run statement takes, `target` being its
    /// first argument: `name(arguments)`, or `name<generics>(arguments)`.
    /// GEN-14: `name<...>(...)` the parser left undecided is the call
    /// too, where the resolver decided it is one. `None` for anything
    /// else.
    fn call_taken(&self, target: &SyntaxNode) -> Option<Called<'a>> {
        let target_at = self.at(target);
        let generic = match target.kind() {
            SyntaxKind::TypedConstructor => true,
            SyntaxKind::UndecidedConstructor => self
                .module
                .decisions
                .iter()
                .any(|decision| decision.at == target_at && decision.constructor == Some(true)),
            _ => false,
        };
        let arguments = child(target, SyntaxKind::ArgList)?;
        match target.kind() {
            SyntaxKind::CallExpr => {
                let callee = target
                    .children()
                    .next()
                    .filter(|callee| callee.kind() == SyntaxKind::NameExpr)?;
                Some(Called {
                    name: name_token(&callee)?,
                    arguments,
                    generics: None,
                })
            }
            _ if generic => {
                // As `typed_constructor` finds them: the generic
                // arguments are recorded where the whole name starts.
                let name = name_token(target)?;
                let start = match child(target, SyntaxKind::QualifiedName) {
                    Some(qualified) => qualified.span().start,
                    None => name.span().start,
                };
                Some(Called {
                    name,
                    arguments,
                    generics: self.calls.get(&(self.file, start)).copied(),
                })
            }
            _ => None,
        }
    }

    /// The function `name` stands for where it is evaluated or run: a
    /// function of a module, native or not. FN-4: "Composition and eval
    /// choose the temporal definition if one exists".
    fn evaluated(&self, name: &SyntaxToken) -> Option<(usize, usize)> {
        let (module, declarations) = self.declarations(name)?;
        let kind_of =
            |declaration: usize| self.program.modules[module].declarations[declaration].kind;
        let of_role = |role: Role| {
            declarations.iter().copied().find(|&declaration| {
                matches!(
                    kind_of(declaration),
                    DeclarationKind::Function(found) | DeclarationKind::NativeFunction(found)
                        if found == role
                )
            })
        };
        of_role(Role::Temporal)
            .or_else(|| of_role(Role::Value))
            .map(|declaration| (module, declaration))
    }

    /// True for a call of a name nothing binds, which is left for the
    /// prelude and not judged (ERR-9).
    fn unbound(&self, called: Option<&Called<'a>>) -> bool {
        called.is_some_and(|called| {
            !self
                .bindings
                .contains_key(&(self.file, called.name.span().start))
        })
    }

    /// An application's run statement (RUN-1): "its run statement is
    /// `run(call, config)` or `eval(call, config)`, where the call is
    /// of an entry, written as any call is and giving the entry's const
    /// arguments, and the configuration is a value of core's
    /// `RunConfig`, or `RunConfig()` where none is written." "An entry
    /// is a temporal function with no temporal parameters."
    ///
    /// The call is read as a graph's call of the entry is: a `const`
    /// parameter takes a constant.
    fn run_statement(mut self) -> Option<Run> {
        self.graph = true;
        let form = self.declaration.syntax.children().next()?;
        let at = self.at(&form);
        let real_time = form.kind() == SyntaxKind::RunExpr;
        let list = child(&form, SyntaxKind::ArgList)?;
        let given = arguments_of(&list);
        // The parser has reported a run statement given nothing.
        let (_, argument) = given.first().cloned()?;
        // EXPR-1: "parentheses group", so `(call)` is the call.
        let target = ungrouped(argument.clone());
        let called = self.call_taken(&target);
        let function = called
            .as_ref()
            .and_then(|called| self.evaluated(&called.name));
        // "An entry is a temporal function with no temporal
        // parameters." A signature not read is not judged.
        let entry = function.filter(|&(module, declaration)| {
            let module = &self.program.modules[module];
            let temporal = module.declarations[declaration].kind.role() == Some(Role::Temporal);
            let inputs = match module.details.get(declaration) {
                Some(Detail::Callable(signature)) => signature
                    .parameters
                    .iter()
                    .any(|parameter| !parameter.constant),
                _ => false,
            };
            temporal && !inputs
        });
        let (kind, ty) = match (&called, entry) {
            (Some(called), Some(entry)) => {
                let read = self.call_function(
                    &called.name,
                    &called.arguments,
                    entry,
                    None,
                    called.generics,
                );
                // The call is held to what any call in a graph is: a
                // `const` parameter is given no connection (FN-3).
                self.constants_of(&read.0);
                read
            }
            _ => {
                if !self.unbound(called.as_ref()) {
                    self.diagnostics.emit(
                        error(
                            Category::Type,
                            codes::RUN_ENTRY,
                            self.at(&argument),
                            "a run statement's first argument is a call of an entry, and this \
                             is none"
                                .to_string(),
                            "RUN-1",
                        )
                        .with_note("an entry is a temporal function with no temporal parameters"),
                    );
                }
                // What the call was given is read all the same, with
                // nothing expected of it.
                match &called {
                    Some(called) => self.unread_call(&called.arguments),
                    None => (ExprKind::Unread, Type::Unknown),
                }
            }
        };
        let call = Expr {
            kind,
            ty,
            at: self.at(&target),
        };
        // EXPR-13: the call is read before the configuration.
        let config = self
            .after_call(&given, "a run statement", "RUN-1")
            .map(|config| *config);
        Some(Run {
            real_time,
            call,
            config,
            at,
        })
    }

    /// The configuration of a run, written as the second argument of
    /// an eval: a value of core's `RunConfig` (RUN-1, core's CRUN-2).
    fn run_config(&mut self, written: &SyntaxNode) -> Box<Expr> {
        let Some(expected) = self.program.run_config_type() else {
            return Box::new(self.expression(written, None));
        };
        let value = self.expression(written, Some(&expected));
        Box::new(self.fit(value, &expected, "the configuration of a run"))
    }

    /// Reports a trace of one style where the eval it belongs to has
    /// one of the other (TST-7).
    fn trace_style(&mut self, at: Location, first: TraceStyle, later: TraceStyle) {
        let name = |style: TraceStyle| match style {
            TraceStyle::Dense => "dense",
            TraceStyle::Timed(_) => "timed",
        };
        self.diagnostics.emit(
            error(
                Category::Type,
                codes::TRACE_STYLE,
                at,
                format!(
                    "this trace is {}, and the eval's other traces are {}: one eval uses one \
                     style, its expectation included",
                    name(later),
                    name(first)
                ),
                "TST-7",
            )
            .with_note("dense is `[7, _, 9]`, and timed `[0s: 7, 2s: 9]`"),
        );
    }

    /// A trace written out where one is expected: `[7, _, 9]`, dense,
    /// or `[0s: 7, 2s: 9]`, timed. `cell` is what a cell is to be.
    ///
    /// TST-5: "Each cell is one cycle's publication". TST-7: "Timed
    /// entries use duration offsets from run start or absolute datetime
    /// keys. Keys strictly increase; no `_` occurs in timed traces."
    fn trace(&mut self, items: &[SyntaxNode], cell: &Type) -> (ExprKind, Type) {
        let timed = items
            .iter()
            .any(|item| item.kind() == SyntaxKind::SequencePair);
        let mut cells = Vec::new();
        // The kind of key a timed trace has, and its last constant key.
        let mut key: Option<Scalar> = None;
        let mut last: Option<i64> = None;
        for item in items {
            let (time, value) = if timed {
                // A value among pairs has been reported by the parser.
                let mut parts = item.children();
                let (Some(time), Some(value)) = (parts.next(), parts.next()) else {
                    continue;
                };
                if item.kind() != SyntaxKind::SequencePair {
                    continue;
                }
                (Some(time), value)
            } else {
                (None, item.clone())
            };
            let time = time.and_then(|time| self.trace_time(&time, &mut key, &mut last));
            let value = if value.kind() == SyntaxKind::Hole {
                if timed {
                    self.placeholder(
                        &value,
                        "a timed trace has no `_`: it lists each publication with its time",
                        "TST-7",
                    );
                }
                None
            } else {
                // TST-5: "For a nullable scalar it is the optional that
                // holds what was published: a cell written as a value
                // is that value, and one written `null` a published
                // null". A cell written `null`, in parentheses or not,
                // is that delta. Any other is read as the delta: a
                // value where one is expected (TYPE-16), or the delta
                // itself, in a block or a branch as outside one.
                let optional = matches!(cell, Type::Delta(of)
                    if matches!(**of, Type::Nullable(_)));
                if optional && written_null(&value) {
                    Some(Expr {
                        kind: ExprKind::Literal(Value::Null),
                        ty: cell.clone(),
                        at: self.at(&value),
                    })
                } else if let Type::Delta(of) = cell
                    && matches!(
                        **of,
                        Type::List { .. }
                            | Type::Set(_)
                            | Type::Dict { .. }
                            | Type::Named {
                                kind: NamedKind::Struct,
                                ..
                            }
                    )
                {
                    // TST-5: "Each cell is one cycle's publication,
                    // written as a `return` is (TIME-7) ... a T,
                    // published whole, or a `delta<T>` (TYPE-16),
                    // published as that change. A list, a tuple, a
                    // struct and a dictionary are each written either
                    // way". A cell's type says which it is. A tuple's
                    // delta is a tuple of its positions, and a whole
                    // tuple reads as that.
                    Some(self.published(&value, of, "a cell of the trace"))
                } else {
                    let read = self.expression(&value, Some(cell));
                    Some(self.fit(read, cell, "a cell of the trace"))
                }
            };
            cells.push(hir::Cell { time, value });
        }
        // Where the place does not say what a cell is, the cells do:
        // the one type they have, `f64` when some are `i64` and some
        // `f64` (NUM-1).
        let cell = if open(cell) {
            let known: Vec<&Type> = cells
                .iter()
                .filter_map(|cell| cell.value.as_ref())
                .map(|value| &value.ty)
                .filter(|ty| !open(ty))
                .collect();
            let numbers = |ty: &Type| matches!(ty, Type::Scalar(Scalar::I64 | Scalar::F64));
            match known.first() {
                Some(_)
                    if known.iter().all(|ty| numbers(ty))
                        && known.iter().any(|ty| **ty == Type::Scalar(Scalar::F64)) =>
                {
                    Type::Scalar(Scalar::F64)
                }
                Some(&first) => first.clone(),
                None => cell.clone(),
            }
        } else {
            cell.clone()
        };
        let style = if timed {
            Some(TraceStyle::Timed(key.unwrap_or(Scalar::Duration)))
        } else if items.is_empty() {
            None
        } else {
            Some(TraceStyle::Dense)
        };
        let ty = Type::Trace {
            cell: Box::new(cell),
            style,
        };
        (ExprKind::Trace(cells), ty)
    }

    /// The key of a timed trace's entry: "duration offsets from run
    /// start or absolute datetime keys. Keys strictly increase" (TST-7).
    /// `key` is the kind the trace's keys have so far, and `last` its
    /// last constant key.
    fn trace_time(
        &mut self,
        node: &SyntaxNode,
        key: &mut Option<Scalar>,
        last: &mut Option<i64>,
    ) -> Option<Expr> {
        if node.kind() == SyntaxKind::Hole {
            self.placeholder(
                node,
                "a timed trace has no `_`: each entry has the time it is published at",
                "TST-7",
            );
            return None;
        }
        let time = self.expression(node, None);
        match &time.ty {
            Type::Scalar(scalar @ (Scalar::Duration | Scalar::Datetime)) => match key {
                Some(first) if first != scalar => {
                    self.diagnostics.emit(error(
                        Category::Type,
                        codes::TYPE_MISMATCH,
                        time.at,
                        format!(
                            "this key is a `{}`, and the trace's keys before it are `{}`: a \
                             timed trace's keys are all durations or all datetimes",
                            time.ty,
                            Type::Scalar(*first)
                        ),
                        "TST-7",
                    ));
                }
                Some(_) => {}
                None => *key = Some(*scalar),
            },
            ty if !open(ty) => {
                self.diagnostics.emit(error(
                    Category::Type,
                    codes::TYPE_MISMATCH,
                    time.at,
                    format!(
                        "this is `{ty}`, and a timed trace's key is a `duration` from the \
                         run's start or a `datetime`"
                    ),
                    "TST-7",
                ));
            }
            _ => {}
        }
        // "Keys strictly increase": of the keys that are constants read
        // here, durations.
        if let Ok(ConstantValue::Duration(now)) = self.read_constant(node, None) {
            if last.is_some_and(|before| now <= before) {
                self.diagnostics.emit(error(
                    Category::Type,
                    codes::TRACE_KEY_ORDER,
                    time.at,
                    "this key is not after the one before it: a timed trace's keys \
                         strictly increase"
                        .to_string(),
                    "TST-7",
                ));
            }
            *last = Some(now);
        }
        Some(time)
    }

    /// Reports a `_` where none stands (EXPR-8, TST-7).
    fn placeholder(&mut self, hole: &SyntaxNode, message: &str, rule: &'static str) {
        self.diagnostics.emit(error(
            Category::Type,
            codes::TRACE_PLACEHOLDER,
            self.at(hole),
            message.to_string(),
            rule,
        ));
    }

    /// `capture(callable, args...)` (ERR-7): "In composition,
    /// `capture(callable, args...)` creates a temporal error boundary.
    /// It names a callable and gives it the arguments its own call
    /// takes, positional or named, read, checked, and selected as that
    /// call's are (EXPR-2, FN-4, GEN-5). In a value function or a hook
    /// it rejects (`capture.phase`) ... Capture of wiring-time scalar
    /// work rejects (`capture.phase`)."
    fn capture(&mut self, name: &SyntaxToken, list: &SyntaxNode) -> (ExprKind, Type) {
        // The primary location is the call: `capture` to its `)`.
        let at = list
            .parent()
            .map_or(Location::new(self.file, name.span()), |call| self.at(&call));
        if !self.graph {
            let place = if self.hook.is_some() {
                "a hook"
            } else if self.generating {
                "a source"
            } else {
                "a value function"
            };
            self.diagnostics.emit(
                error(
                    Category::Phase,
                    codes::CAPTURE_PHASE,
                    at,
                    format!("`capture` is a boundary in composition, and this is {place}"),
                    "ERR-7",
                )
                .with_note("a graph captures a temporal call: `let seen = capture(node, input)`"),
            );
            return self.unread_call(list);
        }
        // "It names a callable": the first argument, by position.
        let first = arguments_of(list).into_iter().next();
        let Some((label, callable)) = first else {
            self.diagnostics.emit(error(
                Category::Type,
                codes::ARGUMENT_MISSING,
                at,
                "`capture` is given nothing: it names the callable it covers".to_string(),
                "ERR-7",
            ));
            return (ExprKind::Unread, Type::Unknown);
        };
        let named = (label.is_none() && callable.kind() == SyntaxKind::NameExpr)
            .then(|| name_token(&callable))
            .flatten();
        let Some(callee) = named else {
            self.diagnostics.emit(error(
                Category::Phase,
                codes::CAPTURE_PHASE,
                self.at(&callable),
                "`capture` names the callable it covers, and this is no name".to_string(),
                "ERR-7",
            ));
            return self.unread_call(list);
        };
        let Some(found) = self.declarations(&callee) else {
            // A local or a parameter is no callable. A name nothing
            // binds is left for the prelude, and is not judged.
            if self.local_of(&callable).is_some() {
                self.diagnostics.emit(error(
                    Category::Phase,
                    codes::CAPTURE_PHASE,
                    self.at(&callable),
                    format!(
                        "`capture` names the callable it covers, and `{}` is none",
                        callee.text()
                    ),
                    "ERR-7",
                ));
            }
            self.callable = Some(callable);
            let unread = self.unread_call(list);
            self.callable = None;
            return unread;
        };
        // The call it names, read as that call is: from the same list,
        // less the name.
        let outer = self.callable.replace(callable.clone());
        let (kind, ty) = self.call_named(&callee, list, found, None, None);
        self.callable = outer;
        // The call is held to what any call in a graph is: a `const`
        // parameter is given no connection (FN-3).
        self.constants_of(&kind);
        let call = Expr { kind, ty, at };
        // "It names a callable": a struct's name constructs a value,
        // and is no call that a boundary covers.
        if matches!(call.kind, ExprKind::Construct { .. }) {
            self.diagnostics.emit(error(
                Category::Phase,
                codes::CAPTURE_PHASE,
                self.at(&callable),
                format!(
                    "`capture` names the callable it covers, and `{}` is a struct: a \
                     construction is no temporal call",
                    callee.text()
                ),
                "ERR-7",
            ));
            return (ExprKind::Unread, Type::Unknown);
        }
        // "Capture of wiring-time scalar work rejects": a call that
        // composes nothing, all its arguments ordinary values.
        if matches!(call.kind, ExprKind::Call { .. })
            && self.category(&call) == ValueCategory::Value
        {
            self.diagnostics.emit(
                error(
                    Category::Phase,
                    codes::CAPTURE_PHASE,
                    at,
                    format!(
                        "`{}` is ordinary work here, done while the graph is built: there is \
                         no temporal call for `capture` to cover",
                        callee.text()
                    ),
                    "ERR-7",
                )
                .with_note("a value function is lifted, and may be captured, where it is given a connection"),
            );
        }
        (ExprKind::Capture(Box::new(call)), Type::Unknown)
    }

    /// What the `.value` of a capture of `call` is (ERR-7): "`.value`
    /// retains the normal output type; outputless calls expose only
    /// `.error`".
    fn captured_output(&self, call: &Expr) -> CapturedOutput {
        let callee = match &call.kind {
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
                ..
            } => self.program.modules[*module].details.get(*declaration),
            _ => None,
        };
        match callee {
            Some(Detail::Callable(signature)) if signature.result.is_none() => CapturedOutput::None,
            _ => CapturedOutput::Of(call.ty.clone()),
        }
    }

    /// `seen.value` or `seen.error` (ERR-7): "`.value` retains the
    /// normal output type; outputless calls expose only `.error`, a
    /// connection of `error`, and `.value` rejects there
    /// (`capture.output_field`)."
    fn captured(
        &mut self,
        base: Expr,
        name: &SyntaxToken,
        output: CapturedOutput,
    ) -> (ExprKind, Type) {
        let at = Location::new(self.file, name.span());
        let (failure, ty) = match (name.text(), output) {
            ("error", _) => (true, Type::Error),
            ("value", CapturedOutput::Of(ty)) => (false, ty),
            ("value", CapturedOutput::None) => {
                self.diagnostics.emit(
                    error(
                        Category::Type,
                        codes::CAPTURE_OUTPUT_FIELD,
                        at,
                        "the call this capture covers has no output, and so no `.value`"
                            .to_string(),
                        "ERR-7",
                    )
                    .with_note("an outputless capture is read through `.error`"),
                );
                (false, Type::Unknown)
            }
            (other, _) => {
                self.diagnostics.emit(error(
                    Category::Name,
                    codes::MEMBER_UNKNOWN,
                    at,
                    format!("a capture has no `{other}`: it is read through `.value` and `.error`"),
                    "ERR-7",
                ));
                return (ExprKind::Unread, Type::Unknown);
            }
        };
        let kind = ExprKind::Captured {
            capture: Box::new(base),
            error: failure,
        };
        (kind, ty)
    }

    /// `value?` (ERR-5): "Success yields T. In a value function
    /// returning `result<U,E>`, failure returns the same E as Err. In
    /// when, only `result<T,error>` may propagate to node failure ...
    /// Composition, lifecycle, and generator uses reject
    /// (`result.propagation_phase`), and an operand or an enclosing
    /// result of another type rejects (`result.propagation_type`) ...
    /// It does not unwrap null (ERR-12)."
    fn propagation(&mut self, node: &SyntaxNode) -> (ExprKind, Type) {
        let Some(operand) = node.children().next() else {
            return (ExprKind::Unread, Type::Unknown);
        };
        // The primary location is the `?`.
        let at = tokens(node)
            .filter(|token| token.kind() == SyntaxKind::Question)
            .last()
            .map_or(self.at(node), |mark| Location::new(self.file, mark.span()));
        let operand = self.expression(&operand, None);
        let value_function = self.declaration.kind.role() == Some(Role::Value);
        // Where it stands, when that is a place nothing propagates from.
        let forbidden = if value_function {
            None
        } else if self.graph {
            Some("in composition: a graph wires its failures, and has none to leave by")
        } else {
            match self.hook {
                Some(Hook::When) => None,
                Some(Hook::Start) => Some("in `start`"),
                Some(Hook::Stop) => Some("in `stop`"),
                None if self.generating => Some("in a source"),
                None => Some("outside a `when`"),
            }
        };
        if let Some(place) = forbidden {
            self.diagnostics.emit(
                error(
                    Category::Phase,
                    codes::RESULT_PROPAGATION_PHASE,
                    at,
                    format!("`?` does not propagate {place}"),
                    "ERR-5",
                )
                .with_note(
                    "`?` stands in a value function that returns a `result`, and in a `when`",
                ),
            );
        }
        let (value, failure) = match &operand.ty {
            Type::Result { value, error } => ((**value).clone(), (**error).clone()),
            other => {
                // What is not known has been reported, or is for a
                // later part (ERR-9).
                if whole(other) && !open(other) {
                    self.diagnostics.emit(error(
                        Category::Type,
                        codes::RESULT_PROPAGATION_TYPE,
                        at,
                        format!("`?` propagates a `result<T, E>`, and this is `{other}`"),
                        "ERR-5",
                    ));
                }
                return (ExprKind::Propagate(Box::new(operand)), Type::Unknown);
            }
        };
        if forbidden.is_none() && whole(&failure) {
            if value_function {
                // "Failure returns the same E as Err."
                match &self.signature.result {
                    Some(Type::Result { error, .. }) if **error == failure => {}
                    Some(result) if !whole(result) || open(result) => {}
                    enclosing => {
                        let returns = match enclosing {
                            Some(result) => format!("returns `{result}`"),
                            None => "returns nothing".to_string(),
                        };
                        self.diagnostics.emit(
                            error(
                                Category::Type,
                                codes::RESULT_PROPAGATION_TYPE,
                                at,
                                format!(
                                    "`?` returns a failure of `{failure}` from `{}`, which \
                                     {returns}",
                                    self.declaration.name
                                ),
                                "ERR-5",
                            )
                            .with_note(format!(
                                "the function's result is a `result<U, {failure}>` for `?` to \
                                 return its failure"
                            )),
                        );
                    }
                }
            } else if failure != Type::Error {
                // "In when, only `result<T,error>` may propagate to node
                // failure."
                self.diagnostics.emit(error(
                    Category::Type,
                    codes::RESULT_PROPAGATION_TYPE,
                    at,
                    format!(
                        "in a `when`, `?` fails the node with an `error`, and this failure is \
                         `{failure}`"
                    ),
                    "ERR-5",
                ));
            }
        }
        (ExprKind::Propagate(Box::new(operand)), value)
    }

    /// A call this pass does not read: its arguments are read for what
    /// they report themselves, with nothing expected of them.
    fn unread_call(&mut self, list: &SyntaxNode) -> (ExprKind, Type) {
        let arguments = self
            .call_arguments(list)
            .into_iter()
            .map(|(_, value)| hir::Argument {
                parameter: None,
                value: self.expression(&value, Some(&Type::Unknown)),
            })
            .collect();
        let kind = ExprKind::Call {
            callee: Callee::Unread,
            arguments,
            form: Form::Named,
        };
        (kind, Type::Unknown)
    }

    /// A call of the value function `declaration` of `module`.
    /// `given` is the generic arguments the call writes, when it writes
    /// them.
    ///
    /// GEN-2: "Resolve every needed generic from actual inputs,
    /// requested output, or solvable equality." GEN-8: "A call may
    /// supply its generic arguments as a generic constructor does,
    /// `count<i64>(3)`. It may supply the first of them and leave the
    /// rest to be resolved under GEN-2 ... A call rejects when it leaves
    /// a generic parameter unresolved." GEN-1: "Repeated parameters
    /// require equivalent types or equal constants."
    ///
    /// GEN-2: "A call resolves a generic of its callee from what it
    /// writes after the name (GEN-8), then from the type its place
    /// expects of the result, then from its arguments in declared
    /// parameter order (GEN-12), whatever order they are written in.
    /// The first of these that says what a parameter is decides it, and
    /// what follows is checked against that: an integer literal is
    /// widened to an `f64` so decided (EXPR-5), and another argument of
    /// another type rejects." So `let real: f64 = same(1, 2)` is an
    /// `f64`.
    ///
    /// Not read here: a function with a type pack or a pack parameter.
    /// A type of it that names a generic parameter is then not known.
    fn call_function(
        &mut self,
        name: &SyntaxToken,
        list: &SyntaxNode,
        (module, declaration): (usize, usize),
        expected: Option<&Type>,
        given: Option<&CallArguments>,
    ) -> (ExprKind, Type) {
        // The call an `eval` stands for is this one, and not one among
        // its arguments.
        let evaluating = std::mem::take(&mut self.evaluating);
        let target = &self.program.modules[module].declarations[declaration];
        let reads_activity = Some(module) == self.program.core()
            && target.kind == DeclarationKind::NativeFunction(Role::Value)
            && target.name == "is_active";
        let (kind, ty) = self.call_function_with(
            name,
            list,
            (module, declaration),
            expected,
            given,
            // An endpoint is never a replayable payload. Even an empty
            // trace cannot supply the receiving node's input (TIME-9).
            evaluating && !reads_activity,
        );
        // END-5, TIME-9: only the bound core function queries activity.
        // Its ordinary signature has already checked the arguments and T.
        if reads_activity
            && let ExprKind::Call { arguments, .. } = &kind
            && let [argument] = arguments.as_slice()
            && let ExprKind::EndpointOf(input) = argument.value.kind
        {
            if Some(input) == self.out {
                self.diagnostics.emit(error(
                    Category::Phase,
                    codes::ENDPOINT_ARGUMENT,
                    argument.value.at,
                    "`is_active` takes a direct temporal input of the node".to_string(),
                    "TIME-9",
                ));
            } else if self.hook == Some(Hook::Start) {
                let input_name = self.locals[input].name.clone();
                self.input_lifecycle(Location::new(self.file, name.span()), &input_name);
            }
            return (
                ExprKind::Endpoint {
                    operation: EndpointOperation::IsActive,
                    inputs: Some(vec![input]),
                },
                ty,
            );
        }
        // TIME-9: "`is_active(input)`, `activate(input)` and
        // `passivate(input)` operate on direct temporal inputs": a child
        // has no subscription of its own.
        if reads_activity
            && let ExprKind::Call { arguments, .. } = &kind
            && let [argument] = arguments.as_slice()
            && let ExprKind::ChildEndpointOf(_) = argument.value.kind
        {
            self.diagnostics.emit(error(
                Category::Phase,
                codes::ENDPOINT_ARGUMENT,
                argument.value.at,
                "`is_active` takes a direct temporal input of the node, and this is a child of \
                 one"
                .to_string(),
                "TIME-9",
            ));
            return (ExprKind::Unread, ty);
        }
        (kind, ty)
    }

    /// [`Typer::call_function`], with `evaluating` true where the call
    /// is the one an `eval` stands for (TST-5): "Const parameters
    /// receive configuration; temporal parameters receive contextually
    /// typed publication traces."
    fn call_function_with(
        &mut self,
        name: &SyntaxToken,
        list: &SyntaxNode,
        (module, declaration): (usize, usize),
        expected: Option<&Type>,
        given: Option<&CallArguments>,
        evaluating: bool,
    ) -> (ExprKind, Type) {
        let program = self.program;
        let Some(Detail::Callable(signature)) = program.modules[module].details.get(declaration)
        else {
            return self.unread_call(list);
        };
        // What a parameter takes: its type, or in an eval a trace for
        // one that is not `const`. TST-5: "Each cell is one cycle's
        // publication: it is read as the `delta<T>` (TYPE-16) of its
        // parameter's type T". A type that names a generic parameter
        // stands for its cells until they say what it is.
        let taken = |parameter: &crate::types::Parameter, ty: Type| {
            if !evaluating || parameter.constant || parameter.pack.is_some() {
                return ty;
            }
            let cell = if names_parameter(&ty) || matches!(ty, Type::Unknown) {
                ty
            } else {
                Type::delta(ty)
            };
            Type::Trace {
                cell: Box::new(cell),
                style: None,
            }
        };
        // TST-16: "A trace given to a `const fn` is an ordinary value
        // there". Not in the call an eval stands for: a `const fn` that
        // is an eval's target is lifted (FN-4), and its parameters take
        // traces as any target's do (TST-5).
        let listed = !evaluating
            && matches!(
                program.modules[module].declarations[declaration]
                    .kind
                    .role(),
                Some(Role::Value)
            );
        let generics = &signature.generics;
        let packed = generics
            .iter()
            .any(|generic| matches!(generic.kind, GenericKind::Pack { .. }))
            || signature
                .parameters
                .iter()
                .any(|parameter| parameter.pack.is_some());
        // What the call writes of the generic parameters. Arguments
        // that were rejected say nothing: the call is then read as one
        // whose generic parameters are not known (ERR-9).
        let explicit = if packed {
            None
        } else {
            self.generic_arguments(name, generics, given)
        };
        let required = |parameter: &crate::types::Parameter| {
            // UNIT-8: "A parameter whose type is written `T?` needs no
            // default to be omitted."
            // "A `var` parameter is the exception: it is always
            // given, whatever its type" (FN-15).
            parameter.var || !parameter.has_default && !matches!(parameter.ty, Type::Nullable(_))
        };
        let Some((mut found, faulty)) = explicit.filter(|_| !generics.is_empty()) else {
            // No generic parameter, or none this pass resolves: a type
            // that names one is not known.
            let known = |ty: &Type| {
                if names_parameter(ty) {
                    Type::Unknown
                } else {
                    ty.clone()
                }
            };
            let slots: Vec<Slot> = signature
                .parameters
                .iter()
                .map(|parameter| Slot {
                    name: &parameter.name,
                    ty: taken(parameter, known(&parameter.ty)),
                    required: required(parameter),
                    pack: parameter.pack,
                    listed,
                })
                .collect();
            let reported = self.diagnostics.len();
            let arguments = self.bind(name, list, &slots, Takes::Parameters);
            let ty = signature.result.as_ref().map_or(Type::Unknown, known);
            // GEN-3: a function with no generic parameter may state a
            // `requires` too, of types it names itself, and a call of
            // it is held to that. Not where something else was
            // reported of the call (ERR-9), nor where the function has
            // generic parameters this pass does not resolve.
            if generics.is_empty() && self.diagnostics.len() == reported {
                let at = Location::new(self.file, Span::new(name.span().start, list.span().end));
                self.requires_met(at, (module, declaration), &[]);
            }
            let requirements = if generics.is_empty() {
                self.answers(module, declaration, &[])
            } else {
                Vec::new()
            };
            let callee = Callee::Function {
                module,
                declaration,
                generics: Vec::new(),
                requirements,
            };
            let kind = ExprKind::Call {
                callee,
                arguments,
                form: Form::Named,
            };
            self.finished_call(&kind, evaluating);
            return (kind, ty);
        };

        // GEN-8: "A call may give a generic value parameter of its
        // callee by name, among its named arguments". What the call
        // writes decides first (GEN-2).
        let value_names: Vec<&str> = generics
            .iter()
            .filter(|generic| matches!(generic.kind, GenericKind::Value(_)))
            .map(|generic| generic.name.as_str())
            .collect();
        let callee = &program.modules[module].declarations[declaration];
        let reported = self.diagnostics.len();
        let named = self.named_generics(list, generics, &mut found);

        // The requested output: `let names: list<str> = repeat(3)` says
        // what the `T` of `repeat<T>(count: i64) -> list<T>` is. A `T`
        // result given to a `T?` place is the `T`.
        if let (Some(result), Some(expected)) = (&signature.result, expected)
            && !open(expected)
        {
            let wanted = match result {
                Type::Nullable(_) => expected,
                _ => not_null(Some(expected)).unwrap_or(expected),
            };
            infer(result, wanted, generics, &mut found);
        }
        // The inputs.
        let slots: Vec<Slot> = signature
            .parameters
            .iter()
            .map(|parameter| Slot {
                name: &parameter.name,
                ty: Type::Unknown,
                required: required(parameter),
                pack: parameter.pack,
                listed,
            })
            .collect();
        let patterns: Vec<Type> = signature
            .parameters
            .iter()
            .map(|parameter| taken(parameter, parameter.ty.clone()))
            .collect();
        let places = Places {
            slots: &slots,
            patterns: patterns.iter().collect(),
            generics,
            settled: !expected.is_some_and(open),
            aside: &value_names,
            values: &named,
            defaults: generics
                .iter()
                .map(|generic| self.default_constant(callee, generic))
                .collect(),
        };
        let Inferred {
            arguments,
            mut found,
            unknown,
            defaulted,
        } = self.infer_arguments(name, list, &places, found, Takes::Parameters);
        self.generic_values(name, callee, generics, (&named, &defaulted), &mut found);

        // GEN-8: "A call rejects when it leaves a generic parameter
        // unresolved." Not where something else was reported, or an
        // argument is not known: what would have said what the
        // parameter is may be what is missing (ERR-9).
        //
        // Where the call's place is not known, the type it expects may
        // yet say what a parameter of the result is: the call then has
        // no whole type and is read again once the place is known. A
        // parameter the result does not name is never resolved that
        // way, so it is reported here: the `N` of
        // `tag<const N: i64>(x: i64) -> i64` in `same(tag(1), 2)`.
        let place_unknown = expected.is_some_and(open);
        let unresolved: Vec<&str> = generics
            .iter()
            .zip(&found)
            .filter(|(_, found)| found.is_none())
            .filter(|(generic, _)| {
                !place_unknown
                    || !signature
                        .result
                        .as_ref()
                        .is_some_and(|result| mentions(result, &[generic.at]))
            })
            .map(|(generic, _)| generic.name.as_str())
            .collect();
        let judged = !faulty && self.diagnostics.len() == reported && !unknown;
        if judged && !unresolved.is_empty() {
            let names: Vec<String> = unresolved.iter().map(|name| format!("`{name}`")).collect();
            self.diagnostics.emit(
                error(
                    Category::Type,
                    codes::GENERIC_UNRESOLVED,
                    Location::new(self.file, name.span()),
                    format!(
                        "this call of `{}` leaves {} unresolved: no argument and no expected \
                         type says what it is",
                        name.text(),
                        names.join(", ")
                    ),
                    "GEN-8",
                )
                .with_note(format!(
                    "a call may write its generic arguments: `{}<...>(...)`",
                    name.text()
                )),
            );
        }

        // The result, with what was found in place. One that still
        // names a parameter nothing resolved is not known.
        let own: Vec<Location> = generics.iter().map(|generic| generic.at).collect();
        let resolved: Vec<Argument> = found
            .into_iter()
            .map(|found| found.unwrap_or(Argument::Type(Type::Unknown)))
            .collect();
        let map: Vec<(Location, &Argument)> = own.iter().copied().zip(&resolved).collect();
        let ty = signature.result.as_ref().map_or(Type::Unknown, |result| {
            placed(substitute_within(result, &map), &own)
        });
        // GEN-3: the callee's `requires` holds with what was resolved.
        // Not where something else was reported of the call (ERR-9).
        if judged && unresolved.is_empty() {
            let at = Location::new(self.file, Span::new(name.span().start, list.span().end));
            self.requires_met(at, (module, declaration), &resolved);
        }
        let requirements = self.answers(module, declaration, &resolved);
        let callee = Callee::Function {
            module,
            declaration,
            generics: resolved,
            requirements,
        };
        let kind = ExprKind::Call {
            callee,
            arguments,
            form: Form::Named,
        };
        self.finished_call(&kind, evaluating);
        (kind, ty)
    }

    /// The named arguments of a call that give a generic value
    /// parameter of the callee, each recorded in `found` as the
    /// constant it is. Returns each with the position of its parameter
    /// among `generics`, in the order written.
    ///
    /// GEN-8: "A call may give a generic value parameter of its callee
    /// by name, among its named arguments ... The value a call gives a
    /// generic value parameter is a compile-time constant ... Any other
    /// rejects."
    fn named_generics(
        &mut self,
        list: &SyntaxNode,
        generics: &[GenericParameter],
        found: &mut [Option<Argument>],
    ) -> Vec<(usize, SyntaxNode)> {
        let mut named = Vec::new();
        for (name, value) in self.call_arguments(list) {
            let Some(name) = name else {
                continue;
            };
            let index = generics.iter().position(|generic| {
                matches!(generic.kind, GenericKind::Value(_)) && generic.name == name.text()
            });
            let Some(index) = index else {
                continue;
            };
            // Given among the call's generic arguments too, `f<2>(N:
            // 3)`, or twice by name.
            if found[index].is_some() {
                self.diagnostics.emit(error(
                    Category::Name,
                    codes::ARGUMENT_DUPLICATE,
                    Location::new(self.file, name.span()),
                    format!("`{}` is given twice", name.text()),
                    "EXPR-2",
                ));
                // Read for what it reports itself.
                self.expression(&value, Some(&Type::Unknown));
                continue;
            }
            // Read as a value of the parameter's type where that names
            // no other parameter: one that does is held to it once that
            // is known (`generic_values`), and its type reported there.
            let expected = match &generics[index].kind {
                GenericKind::Value(ty) if !names_parameter(ty) => Some(ty),
                _ => None,
            };
            let read = self.read_constant(&value, expected);
            if let Some(why) = Self::not_constant(&read) {
                self.diagnostics.emit(error(
                    Category::Type,
                    codes::GENERIC_ARGUMENT_CONSTANT,
                    self.at(&value),
                    format!(
                        "`{}` is a generic value parameter, and this is no compile-time \
                         constant: {why}",
                        name.text()
                    ),
                    "GEN-8",
                ));
            }
            found[index] = Some(Argument::Constant(Constant {
                text: constant_text(&value),
                value: read.unwrap_or(ConstantValue::Unknown),
            }));
            named.push((index, value));
        }
        named
    }

    /// What the generic value parameters of a function are at a call,
    /// once its arguments are read: each one the call gives by name,
    /// `named`, is held to its parameter's type, and each one nothing
    /// says anything of takes its default.
    ///
    /// GEN-13: a generic value parameter has an ordinary type, which
    /// may name a type parameter, the `T` of `<T, const fallback: T>`:
    /// what the call gives `fallback` says what `T` is where the
    /// parameters have not. GEN-8: "A generic value parameter the call
    /// does not give takes its default, when it has one."
    fn generic_values(
        &mut self,
        callee: &SyntaxToken,
        declaration: &Declaration,
        generics: &[GenericParameter],
        (named, defaulted): (&[(usize, SyntaxNode)], &[usize]),
        found: &mut [Option<Argument>],
    ) {
        let own: Vec<Location> = generics.iter().map(|generic| generic.at).collect();
        // The type of the generic value parameter at `index`, with what
        // has been found in place; `Type::Unknown` where it still names
        // a parameter nothing has resolved.
        let type_of = |index: usize, found: &[Option<Argument>]| {
            let GenericKind::Value(pattern) = &generics[index].kind else {
                return Type::Unknown;
            };
            let resolved: Vec<Argument> = found
                .iter()
                .map(|found| found.clone().unwrap_or(Argument::Type(Type::Unknown)))
                .collect();
            let map: Vec<(Location, &Argument)> = own.iter().copied().zip(&resolved).collect();
            placed(substitute_within(pattern, &map), &own)
        };
        // GEN-12: in the order they are declared, whatever order the
        // call writes them in.
        let mut ordered: Vec<&(usize, SyntaxNode)> = named.iter().collect();
        ordered.sort_by_key(|(index, _)| *index);
        for (index, node) in ordered {
            let GenericKind::Value(pattern) = &generics[*index].kind else {
                continue;
            };
            let place = format!("the generic value parameter `{}`", generics[*index].name);
            let expected = type_of(*index, found);
            let expected = if whole(&expected) {
                let value = self.expression(node, Some(&expected));
                self.fit(value, &expected, &place);
                expected
            } else {
                // Its type names a parameter nothing has said anything
                // of: what it is given says.
                let value = self.expression(node, Some(&Type::Unknown));
                infer(pattern, &value.ty, generics, found);
                let expected = type_of(*index, found);
                self.fit(value, &expected, &place);
                expected
            };
            // The constant as a value of that type (EXPR-5): `fallback:
            // 1` is the `f64` one where `T` is an `f64`.
            if whole(&expected) {
                let value = self
                    .read_constant(node, Some(&expected))
                    .unwrap_or(ConstantValue::Unknown);
                found[*index] = Some(Argument::Constant(Constant {
                    text: constant_text(node),
                    value,
                }));
            }
        }
        for &index in defaulted {
            let generic = &generics[index];
            let GenericKind::Value(declared) = &generic.kind else {
                continue;
            };
            let Some(default) = generic_default(&declaration.syntax, generic) else {
                continue;
            };
            // A default that is not of the type as it is declared is
            // reported there (`signatures`). One whose type names a
            // type parameter is of it for one call and not another: the
            // `2` of `<T, const fallback: T = 2>` is no `str`.
            let expected = type_of(index, found);
            match self.unfit_default(declaration, &default, &expected, declared) {
                Some(Ok(value)) => {
                    // The default as a value of the type the call
                    // resolves (EXPR-5): the `f64` 2 where `T` is one.
                    found[index] = Some(Argument::Constant(Constant {
                        text: constant_text(&default),
                        value,
                    }));
                }
                Some(Err(described)) => {
                    self.diagnostics.emit(error(
                        Category::Type,
                        codes::TYPE_MISMATCH,
                        Location::new(self.file, callee.span()),
                        format!(
                            "this leaves `{}` to its default, {described}, and `{}` is \
                             `{expected}` here",
                            generic.name, generic.name
                        ),
                        "GEN-13",
                    ));
                }
                None => {}
            }
        }
    }

    /// The default `default` of a generic value parameter of
    /// `declaration`, declared of type `declared`, as a value of
    /// `resolved`, the type a call resolves the parameter to (GEN-13):
    /// `Ok` with it where it is one, and `Err` with what it is where it
    /// is of another type and of the one declared. `None` where that is
    /// not known, and where it is not of the type declared, which is
    /// reported where it is declared.
    fn unfit_default(
        &self,
        declaration: &Declaration,
        default: &SyntaxNode,
        resolved: &Type,
        declared: &Type,
    ) -> Option<Result<ConstantValue, String>> {
        if !whole(resolved) {
            return None;
        }
        let read = self
            .typed_in(declaration.at.file, default, Some(resolved))
            .ok()?;
        let fits_declared = names_parameter(declared)
            || crate::constant::fit(read.clone(), Some(declared), self.program).is_ok();
        match crate::constant::fit(read.clone(), Some(resolved), self.program) {
            Ok(value) => Some(Ok(value)),
            Err(Fault::Mismatch(_)) if fits_declared => {
                Some(Err(crate::constant::described(&read)))
            }
            Err(_) => None,
        }
    }

    /// What the generic arguments a call writes say of the callee's
    /// generic parameters `generics`: an entry for each parameter,
    /// `None` for one the call does not write, and whether an argument
    /// was reported where it is written. `None` when the arguments are
    /// rejected here.
    ///
    /// GEN-8: a call "may supply the first of them and leave the rest
    /// to be resolved". GEN-14: "Check each argument against the
    /// corresponding parameter kind."
    fn generic_arguments(
        &mut self,
        name: &SyntaxToken,
        generics: &[GenericParameter],
        given: Option<&CallArguments>,
    ) -> Option<(Vec<Option<Argument>>, bool)> {
        let mut found: Vec<Option<Argument>> = vec![None; generics.len()];
        let Some(given) = given else {
            return Some((found, false));
        };
        let callee = name.text();
        if given.arguments.len() > generics.len() {
            let takes = match generics.len() {
                0 => "no generic arguments".to_string(),
                1 => "1 generic argument".to_string(),
                count => format!("{count} generic arguments"),
            };
            self.diagnostics.emit(error(
                Category::Type,
                codes::GENERIC_ARGUMENT_COUNT,
                Location::new(self.file, name.span()),
                format!(
                    "`{callee}` takes {takes} and is given {}",
                    given.arguments.len()
                ),
                "GEN-8",
            ));
            return None;
        }
        let mut faulty = false;
        let mut rejected = false;
        for (position, (at, argument)) in given.arguments.iter().enumerate() {
            let generic = &generics[position];
            let mismatch = match (&generic.kind, argument) {
                // Reported where it is written.
                (_, Argument::Type(Type::Unknown)) => {
                    faulty = true;
                    None
                }
                (GenericKind::Value(_), Argument::Type(ty)) => Some(format!(
                    "`{callee}` takes a value for `{}`, and `{ty}` is a type",
                    generic.name
                )),
                (GenericKind::Type, Argument::Constant(_)) => Some(format!(
                    "`{callee}` takes a type for `{}`, and this is a value",
                    generic.name
                )),
                // A parameter whose kind its signature does not settle
                // is what the call gives it (GEN-13).
                _ => {
                    // GEN-8: "The value a call gives a generic value
                    // parameter is a compile-time constant ... Any other
                    // rejects." What it comes to has been read with the
                    // types (`read`); one not valued is read again here
                    // for what it is.
                    if let Argument::Constant(Constant {
                        value: ConstantValue::Unevaluated,
                        ..
                    }) = argument
                        && let Some(node) = self.node_at(*at)
                        && let Some(why) = Self::not_constant(&self.read_constant(&node, None))
                    {
                        self.diagnostics.emit(error(
                            Category::Type,
                            codes::GENERIC_ARGUMENT_CONSTANT,
                            *at,
                            format!(
                                "`{}` is a generic value parameter, and this is no \
                                 compile-time constant: {why}",
                                generic.name
                            ),
                            "GEN-8",
                        ));
                    }
                    found[position] = Some(argument.clone());
                    None
                }
            };
            if let Some(message) = mismatch {
                self.diagnostics.emit(error(
                    Category::Type,
                    codes::GENERIC_ARGUMENT_KIND,
                    *at,
                    message,
                    "GEN-14",
                ));
                rejected = true;
            }
        }
        (!rejected).then_some((found, faulty))
    }

    /// Reads the arguments of a call into places whose types name
    /// generic parameters, finding what those stand for from the values
    /// given (GEN-2). `found` is what is known of each parameter
    /// before; what comes back is every argument, each fitted to its
    /// place with what was found put in, and what is known after.
    ///
    /// The value of a place whose type names a parameter not yet found
    /// says what the parameter is: `1` for `first: T`, `[1]` for
    /// `values: list<T>`. Such values are read first, a call's in the
    /// order its parameters are declared in (GEN-12) and a
    /// construction's in the order written, with nothing expected of
    /// them, but for `null` and `.member`, which take their type from
    /// the place and say nothing of it.
    fn infer_arguments(
        &mut self,
        name: &SyntaxToken,
        list: &SyntaxNode,
        places: &Places,
        mut found: Vec<Option<Argument>>,
        takes: Takes,
    ) -> Inferred {
        let Places {
            slots,
            patterns,
            generics,
            settled,
            aside,
            values: named,
            defaults,
        } = places;
        // The parameters being resolved. A place's type that still
        // names one once what was found is in place is not known.
        let own: Vec<Location> = generics.iter().map(|generic| generic.at).collect();
        let assigned = self.assign(name, list, slots, takes, aside);
        let mut values: Vec<Option<Expr>> = vec![None; assigned.len()];
        // True for a value kept from its first reading that is not
        // then fitted to its place.
        let mut unfitted: Vec<bool> = vec![false; assigned.len()];
        // The values that say what a parameter is are read first, out
        // of the order written. A block among the arguments may assign
        // what another reads (CTRL-10), so with one such a reading is
        // tried and taken back, and every argument is then read where
        // it is written.
        let ordered = assigned.iter().any(|(_, node)| holds_block(node));
        // GEN-12: a call's parameters say what a generic is "in declared
        // parameter order". A struct's fields have no order of their
        // own (TYPE-5: named, "in any order"), so the first written
        // decides (GEN-2).
        let order = match takes {
            Takes::Parameters => declared_order(&assigned),
            Takes::Fields => (0..assigned.len()).collect(),
        };
        // The generic parameters an argument decided: what the call
        // writes, or its place expects, is not widened (GEN-2).
        let mut by_argument = vec![false; generics.len()];
        for position in order {
            let (slot, node) = &assigned[position];
            let Some(index) = *slot else {
                continue;
            };
            let pattern = patterns[index];
            let unfound: Vec<Location> = generics
                .iter()
                .zip(&found)
                .filter(|(_, found)| found.is_none())
                .map(|(generic, _)| generic.at)
                .collect();
            // GEN-2: "One later argument changes what an earlier
            // argument decided: an abstract struct that the type
            // decided inherits from widens the parameter to that
            // struct". So a parameter that is `T` and nothing more is
            // read here though an earlier argument has said what `T`
            // is, where that is a struct.
            let widenable = match bare_parameter(pattern, generics) {
                Some(generic) if takes == Takes::Parameters && by_argument[generic] => {
                    matches!(&found[generic], Some(Argument::Type(Type::Named { .. })))
                        .then_some(generic)
                }
                _ => None,
            };
            if takes_its_type(node) || !(mentions(pattern, &unfound) || widenable.is_some()) {
                continue;
            }
            // What the argument's type says: a parameter not yet found,
            // or a wider one for a parameter an argument decided.
            let mut learn = |this: &Self, actual: &Type, found: &mut Vec<Option<Argument>>| {
                if let Some(generic) = widenable {
                    if let Some(Argument::Type(decided)) = &found[generic]
                        && this.widens(decided, actual)
                    {
                        found[generic] = Some(Argument::Type(actual.clone()));
                    }
                    return;
                }
                let before: Vec<bool> = found.iter().map(Option::is_some).collect();
                infer(pattern, actual, generics, found);
                for (generic, was) in before.into_iter().enumerate() {
                    by_argument[generic] |= !was && found[generic].is_some();
                }
            };
            // Nothing is expected of the value while its place is not
            // known, but that the place is an endpoint: the argument of
            // `valid<T>(input: endpoint<T>)` is an endpoint of whatever
            // the input holds, and says what `T` is (TIME-9).
            // In an eval the place is a trace (TST-5): a sequence there
            // is one written out, and its cells say what it is of.
            let unknown = Type::Unknown;
            let trace = Type::Trace {
                cell: Box::new(Type::Unknown),
                style: None,
            };
            let read_as = match pattern {
                Type::Endpoint(_) => pattern,
                Type::Trace { .. } => &trace,
                _ => &unknown,
            };
            // TST-16: what a trace is where a `const fn` is given it
            // says what the parameter is, as any value's type does.
            let listed = slots[index].listed;
            let as_given = |value: hir::Expr| {
                if listed { publications(value) } else { value }
            };
            if ordered {
                if self.may_reread() {
                    let mark = self.mark();
                    let value = as_given(self.expression(node, Some(read_as)));
                    let says = self.inferred_from(pattern, &value, takes);
                    learn(self, &says, &mut found);
                    self.take_back(mark, false);
                }
                continue;
            }
            let reported = self.diagnostics.len();
            let value = as_given(self.expression(node, Some(read_as)));
            let says = self.inferred_from(pattern, &value, takes);
            learn(self, &says, &mut found);
            // A value with no whole type while nothing is known of its
            // place, `[1, "x"]`, has not been judged. Neither has one
            // that is read as the type of its place, a sequence or a
            // tuple written out: its entries and a list's size are the
            // place's to say (EXPR-5, EXPR-9, TYPE-10), so `[1, 2]` is
            // a `list<i64>` here and a `list<i64, 2>` where one is
            // expected. Each is read again below as the place's type,
            // once the parameters are found. One that reported
            // something is read once.
            let by_place = typed_by_place(node);
            let again = (by_place || !whole(&value.ty))
                && self.diagnostics.len() == reported
                && self.may_reread();
            if !again {
                values[position] = Some(value);
                // Kept as it was read, with no place: it is not held to
                // the place's type, which that reading did not know.
                unfitted[position] = by_place;
            }
        }
        // GEN-13: a generic value parameter's type may name a type
        // parameter, the `T` of `<T, const fallback: T>`. What the call
        // gives it says what that is where the places have not, in the
        // order declared (GEN-12), and before any argument is held to
        // its place: the `null` of `pick(null, fallback: 1)` is then
        // given to an `i64`. The value is read again, and held to its
        // own type, once every parameter is found.
        let mut ordered: Vec<&(usize, SyntaxNode)> = named.iter().collect();
        ordered.sort_by_key(|(index, _)| *index);
        for (index, node) in ordered {
            let GenericKind::Value(pattern) = &generics[*index].kind else {
                continue;
            };
            let unfound: Vec<Location> = generics
                .iter()
                .zip(&found)
                .filter(|(_, found)| found.is_none())
                .map(|(generic, _)| generic.at)
                .collect();
            if takes_its_type(node) || !mentions(pattern, &unfound) || !self.may_reread() {
                continue;
            }
            let mark = self.mark();
            let value = self.expression(node, Some(&Type::Unknown));
            infer(pattern, &value.ty, generics, &mut found);
            self.take_back(mark, false);
        }
        // GEN-8: "A generic value parameter the call does not give takes
        // its default, when it has one." Nothing has said what it is,
        // and it is its default before any argument is held to its
        // place: `sized([1, 2, 3])` for `sized<const N: i64 = 2>(items:
        // list<i64, N>)` gives three entries to a list of two.
        let mut defaulted = Vec::new();
        for (index, default) in defaults.iter().enumerate() {
            if found[index].is_none()
                && let Some(default) = default
            {
                found[index] = Some(Argument::Constant(default.clone()));
                defaulted.push(index);
            }
        }
        // Every argument is of its place's type with what was found in
        // place: `Pair(first: 1, second: "x")` gives `second: T` a `str`
        // where `T` is `i64`.
        let resolved: Vec<Argument> = found
            .iter()
            .map(|found| found.clone().unwrap_or(Argument::Type(Type::Unknown)))
            .collect();
        let map: Vec<(Location, &Argument)> = fixed(generics).zip(&resolved).collect();
        // The parameters nothing resolved.
        let unfound: Vec<Location> = generics
            .iter()
            .zip(&found)
            .filter(|(_, found)| found.is_none())
            .map(|(generic, _)| generic.at)
            .collect();
        let arguments: Vec<hir::Argument> = assigned
            .iter()
            .enumerate()
            .map(|(position, (slot, node))| {
                let Some(index) = *slot else {
                    return self.argument(None, node, None, takes);
                };
                let ty = substitute_within(patterns[index], &map);
                let slot = Slot {
                    ty: placed(ty, &own),
                    ..slots[index]
                };
                // A place whose type names a parameter nothing resolved
                // is not known, and where the call is settled it never
                // will be. Its argument is then read as one of which
                // nothing is expected, so that what it leaves unresolved
                // itself is reported: the `repeat(1)` of
                // `same(repeat(1), repeat(2))`. `null` and `.member`
                // have no reading of their own.
                let placeless =
                    *settled && !takes_its_type(node) && mentions(patterns[index], &unfound);
                // True for a place that is a trace an eval gives
                // (TST-5), and not one a generic parameter was made by
                // what it was given.
                let traced = matches!(patterns[index], Type::Trace { .. });
                match values[position].take() {
                    None if placeless => hir::Argument {
                        parameter: Some(index),
                        value: self.expression(node, None),
                    },
                    Some(value) if unfitted[position] => hir::Argument {
                        parameter: Some(index),
                        value,
                    },
                    // A value that holds a trace is the place's to
                    // report where the call is finished (TST-3), and is
                    // not held to its type.
                    Some(value) if !traced && holds_trace(&value.ty) => hir::Argument {
                        parameter: Some(index),
                        value,
                    },
                    Some(value) => {
                        let place = format!("the {} `{}`", takes.noun(), slot.name);
                        let value = if takes == Takes::Parameters {
                            self.fit_input(value, &slot.ty, &place)
                        } else {
                            self.fit(value, &slot.ty, &place)
                        };
                        hir::Argument {
                            parameter: Some(index),
                            value,
                        }
                    }
                    None => self.argument(Some(index), node, Some((&slot, traced)), takes),
                }
            })
            .collect();
        let unknown = assigned
            .iter()
            .zip(&arguments)
            .any(|((_, node), argument)| !takes_its_type(node) && !whole(&argument.value.ty));
        Inferred {
            arguments,
            found,
            unknown,
            defaulted,
        }
    }

    /// The output a call of `operator` requests of its candidates
    /// (GEN-5), from what is known of the operator's generic parameters
    /// so far, `found`, and the type given to each of its parameters,
    /// `supplied`. `said` is true when the call writes generic
    /// arguments, which is a request in itself.
    ///
    /// GEN-8: "An operator's takes part in the output requested, in
    /// GEN-2's order: what the call writes, then what its place
    /// expects, then its arguments, and the default where none of them
    /// says." GEN-12: the arguments in the order the operator declares
    /// its parameters. GEN-2: "an abstract struct that the type decided
    /// inherits from widens the parameter to that struct". GEN-4: the
    /// operator's own requirement of its result.
    ///
    /// A call by name and the test of a named operator requirement
    /// (GEN-16) both ask through here, so a requirement holds as the
    /// call it names would select.
    fn output_requested(
        &self,
        operator: OperatorRef,
        public: &Signature,
        mut found: Vec<Option<Argument>>,
        supplied: &[Option<&Type>],
        said: bool,
    ) -> Requested {
        let program = self.program;
        // The generic parameters an argument decided: what the call
        // writes, or its place expects, is not widened (GEN-2).
        let mut by_argument = vec![false; public.generics.len()];
        for (parameter, actual) in public.parameters.iter().zip(supplied) {
            let Some(actual) = *actual else {
                continue;
            };
            if let Some(generic) = bare_parameter(&parameter.ty, &public.generics)
                && by_argument[generic]
                && let Some(Argument::Type(decided)) = &found[generic]
                && self.widens(decided, actual)
            {
                found[generic] = Some(Argument::Type(actual.clone()));
                continue;
            }
            let before: Vec<bool> = found.iter().map(Option::is_some).collect();
            infer(&parameter.ty, actual, &public.generics, &mut found);
            for (generic, was) in before.into_iter().enumerate() {
                by_argument[generic] |= !was && found[generic].is_some();
            }
        }
        let operator_declaration =
            &program.modules[operator.module].declarations[operator.declaration];
        let operator_syntax = &operator_declaration.syntax;
        // True when the call says something of the output itself: it
        // writes generic arguments, or one of the operator's takes its
        // default.
        let mut asked = said;
        // The operator's generic value parameters that take their
        // defaults, each with the default.
        let mut defaulted: Vec<(usize, SyntaxNode)> = Vec::new();
        for (index, (generic, found)) in public.generics.iter().zip(found.iter_mut()).enumerate() {
            if found.is_none()
                && let Some(default) = generic_default(operator_syntax, generic)
            {
                *found = self
                    .default_constant(operator_declaration, generic)
                    .map(Argument::Constant);
                asked = true;
                defaulted.push((index, default));
            }
        }
        // A generic nothing has said anything of is left open: a type
        // as one not known, and a value as a constant not evaluated,
        // which agrees with any. So a `list<list<f64, cols>, rows>` with
        // `rows` 3 and `cols` not said is still asked to have three
        // rows.
        let explicit: Vec<Argument> = found
            .into_iter()
            .zip(&public.generics)
            .map(|(found, generic)| {
                found.unwrap_or_else(|| match generic.kind {
                    GenericKind::Value(_) => Argument::Constant(unsaid()),
                    _ => Argument::Type(Type::Unknown),
                })
            })
            .collect();
        let own_generics: Vec<Location> =
            public.generics.iter().map(|generic| generic.at).collect();
        let map: Vec<(Location, &Argument)> = fixed(&public.generics).zip(&explicit).collect();
        // GEN-13: a default the call takes is of its parameter's type
        // as the call resolves it. One that is not of the type as it is
        // declared is reported where it is declared, and is none of the
        // call's.
        for (index, default) in defaulted {
            let GenericKind::Value(declared) = &public.generics[index].kind else {
                continue;
            };
            let resolved = placed(substitute_within(declared, &map), &own_generics);
            if let Some(Err(described)) =
                self.unfit_default(operator_declaration, &default, &resolved, declared)
            {
                return Requested {
                    place: None,
                    among: None,
                    unfit: Some(Unfit {
                        generic: index,
                        described,
                        resolved,
                    }),
                };
            }
        }
        // The output requested, as far as it is known: a `list<T, 3>`
        // with `T` not yet said is a list of three of anything.
        let place: Option<Type> = asked
            .then(|| {
                public
                    .result
                    .as_ref()
                    .map(|result| placed(substitute_within(result, &map), &own_generics))
                    .filter(|requested| !matches!(requested, Type::Unknown))
            })
            .flatten();
        // GEN-4: an operator "owns ... generic relationships, and
        // requirements". Where its requirement says what its result may
        // be, a candidate's result is one of those.
        let among = self.result_among(operator, public, &map, &own_generics);
        Requested {
            place,
            among,
            unfit: None,
        }
    }

    /// A call of the operator `operator` by its name.
    ///
    /// GEN-5: "Match candidates against the requested output before
    /// inputs, then check their conditions. Select exactly one
    /// most-specific admitted candidate ... No match rejects; tied
    /// most-specific candidates are ambiguous ... A call in a value
    /// function or a runtime body selects among the value candidates."
    /// FN-3: "Any body calls an operator; in a runtime body or a value
    /// function the call selects a value candidate."
    ///
    /// The arguments are given to the operator's public parameters
    /// (GEN-4: the operator "owns public names/order"), and a candidate
    /// is admitted when each fits the parameter the candidate has in
    /// that place. A candidate that fits with no integer literal
    /// widened is taken before one that needs it.
    ///
    /// What is not read here: a call of an operator that has a pack, or
    /// a candidate with one. Such a call has no type.
    fn call_operator(
        &mut self,
        name: &SyntaxToken,
        list: &SyntaxNode,
        operator: OperatorRef,
        expected: Option<&Type>,
        generics: Option<&CallArguments>,
    ) -> (ExprKind, Type) {
        let program = self.program;
        let Some(Detail::Callable(public)) = program.modules[operator.module]
            .details
            .get(operator.declaration)
        else {
            return self.unread_call(list);
        };
        // GEN-8: the generic arguments the call writes are the
        // operator's: with `operator convert<T, V>(value: V) -> T`,
        // "`convert<f64>(count)` supplies T". With them in place the
        // operator's result is the output requested, when nothing of it
        // is left to resolve: `convert<f64>(count)` asks for an `f64`.
        // What they say of the operands is for the pass that resolves
        // generic candidates.
        let written: Vec<Option<Argument>> = match generics {
            None => vec![None; public.generics.len()],
            Some(_) => {
                let Some((explicit, _)) = self.generic_arguments(name, &public.generics, generics)
                else {
                    return self.unread_call(list);
                };
                explicit
            }
        };
        // GEN-8: "A generic value parameter the call does not give takes
        // its default, when it has one. An operator's takes part in the
        // output requested, in GEN-2's order: what the call writes, then
        // what its place expects, then its arguments, and the default
        // where none of them says."
        // What the call writes, before its place says anything.
        let explicit = written.clone();
        let mut found = written;
        from_place(public, expected, &mut found);
        let candidates = self.value_candidates(operator);
        if unreadable(public, &candidates) {
            return self.unread_call(list);
        }

        // EXPR-2: "Named arguments use the selected public callable's
        // parameter names; a call of an operator may also name an
        // option its candidates declare (GEN-6)." Which parameter of a
        // candidate an argument is given to, and what a call may leave
        // out, is each candidate's to say (GEN-6: "A call is matched
        // against each candidate's own signature").
        let (selecting, options) = options_of(public, &candidates);
        let reported = self.diagnostics.len();
        let written = self.operator_arguments(name, list, public, &options);
        // Each argument, read with nothing expected of it: what it
        // reports itself is reported here, once.
        let mut given: Vec<Given> = written
            .into_iter()
            .map(|(key, node)| self.give(key, node))
            .collect();
        self.constants_given(&mut given, &selecting);
        // An argument given to no parameter, or rejected itself, has
        // been reported, and one whose type is not known says nothing
        // of the candidates: the call is then not judged (ERR-9).
        let faulty = self.diagnostics.len() != reported;
        let unknown = given
            .iter()
            .any(|given| !given.by_place && open(&given.value.ty));
        if faulty || unknown {
            return (self.unselected(given, Form::Named), Type::Unknown);
        }
        // Core's OPS-6: "`convert<list<B>>(values)` is admitted too,
        // where `values` is a `list<A>` and the struct `A` inherits from
        // the abstract struct `B`; it yields the same elements." Core
        // declares no candidate for it.
        if let Some(ty) = self.parent_list(operator, public, &explicit, &given, expected) {
            // The value as `called` has an argument: read again where
            // its first reading was taken back, so that the locals it
            // declares are this reading's.
            let value = given.swap_remove(0);
            let converted = value.value.ty.clone();
            let value = self.argument_value(value, &converted, "the value converted");
            return (ExprKind::ParentList(Box::new(value)), ty);
        }
        // The generic arguments the call writes, for a requirement to
        // agree with (GEN-8).
        let written_generics: Option<Vec<Option<Argument>>> = generics.map(|_| found.clone());

        // Then its arguments, in the order the operator declares its
        // parameters (GEN-12), and its defaults: see
        // `Typer::output_requested`.
        let supplied: Vec<Option<&Type>> = public
            .parameters
            .iter()
            .enumerate()
            .map(|(index, parameter)| {
                given
                    .iter()
                    .find(|given| match &given.key {
                        Key::Position(position) => *position == index,
                        Key::Name(name) => *name == parameter.name,
                    })
                    .map(|given| &given.value.ty)
            })
            .collect();
        let Requested {
            place: requested,
            among,
            unfit,
        } = self.output_requested(operator, public, found, &supplied, generics.is_some());
        // GEN-13: a default the call takes is of its parameter's type
        // as the call resolves it: the `2` of
        // `operator choose<T, const fallback: T = 2>` is no `str`. One
        // that is not of the type as it is declared is reported there.
        if let Some(Unfit {
            generic,
            described,
            resolved,
        }) = unfit
        {
            let generic = &public.generics[generic];
            self.diagnostics.emit(error(
                Category::Type,
                codes::TYPE_MISMATCH,
                Location::new(self.file, name.span()),
                format!(
                    "this leaves `{}` to its default, {described}, and `{}` is `{resolved}` here",
                    generic.name, generic.name
                ),
                "GEN-13",
            ));
            return (self.unselected(given, Form::Named), Type::Unknown);
        }
        // GEN-7: "A generic body is checked under its stated
        // requirements": what the enclosing declaration requires of its
        // generic parameters is callable in its body. The requirement
        // is of the call as it asks: one that gives another output than
        // the call requests, its defaults included, is not this call.
        if let Some(result) = self.required(
            operator,
            &given.iter().collect::<Vec<_>>(),
            (requested.as_ref(), expected),
            written_generics.as_deref(),
        ) {
            return self.call_required(operator, result, given, Form::Named);
        }
        // GEN-5: "the requested output before inputs". EXPR-5: an
        // integer literal is an `f64` where one is expected; it selects
        // as the `i64` it is where a candidate takes one.
        let wanted = Wanted {
            place: requested
                .as_ref()
                .or(expected.filter(|expected| !open(expected))),
            among,
        };
        let (mut selected, untold) = self.admitted(operator, &candidates, &given, &wanted);
        // EXPR-12: "Resolve configuration that selects an expression's
        // source result type during source checking." An option a
        // candidate's condition reads is given here as no constant this
        // pass reads: which candidates admit the call cannot be told,
        // and the call is not judged.
        if untold {
            return (self.unselected(given, Form::Named), Type::Unknown);
        }
        // An argument could not be tried once the function's readings
        // were used up (`REREADS`): the call is then not judged.
        if self.rereads == 0 && given.iter().any(|given| given.by_place) {
            return (self.unselected(given, Form::Named), Type::Unknown);
        }
        let at = Location::new(self.file, name.span());
        if selected.len() != 1 {
            // GEN-5: "the requested output before inputs". Where what
            // the place expects is not known, it may be what tells tied
            // candidates apart: the `make()` of `[make()]` given to a
            // `list<i64>`. The tie is then not reported, and the call
            // has no type.
            if selected.len() > 1 && expected.is_some_and(open) {
                return (self.unselected(given, Form::Named), Type::Unknown);
            }
            let takes: Vec<String> = given
                .iter()
                .map(|given| {
                    let value = if given.by_place {
                        given.node.text().to_string().trim().to_string()
                    } else {
                        given.value.ty.to_string()
                    };
                    match &given.key {
                        // An option is known by its name.
                        Key::Name(name) if !public.parameters.iter().any(|p| p.name == *name) => {
                            format!("`{name}: {value}`")
                        }
                        _ => format!("`{value}`"),
                    }
                })
                .collect();
            let takes = if takes.is_empty() {
                "no argument".to_string()
            } else {
                takes.join(", ")
            };
            let diagnostic = if selected.is_empty() {
                let message = if candidates.is_empty() {
                    format!(
                        "operator `{}` has no value candidate, and a value function selects \
                         among those",
                        name.text()
                    )
                } else {
                    format!(
                        "no value candidate of operator `{}` takes {takes}{}",
                        name.text(),
                        match (wanted.place, &wanted.among) {
                            (Some(wanted), _) => format!(" and gives `{wanted}`"),
                            // What the operator's requirement admits.
                            (None, Some(among)) => {
                                let among: Vec<String> =
                                    among.iter().map(|ty| format!("`{ty}`")).collect();
                                format!(" and gives {}", among.join(" or "))
                            }
                            (None, None) => String::new(),
                        }
                    )
                };
                error(
                    Category::Operator,
                    codes::OPERATOR_NO_CANDIDATE,
                    at,
                    message,
                    "GEN-5",
                )
            } else {
                let mut diagnostic = error(
                    Category::Operator,
                    codes::OPERATOR_AMBIGUOUS,
                    at,
                    format!(
                        "{} value candidates of operator `{}` take {takes} equally",
                        selected.len(),
                        name.text()
                    ),
                    "GEN-5",
                );
                for chosen in &selected {
                    let candidate =
                        &program.modules[chosen.module].declarations[chosen.declaration];
                    diagnostic = diagnostic.with_related(candidate.at, "this candidate");
                }
                diagnostic
            };
            self.diagnostics.emit(diagnostic);
            return (self.unselected(given, Form::Named), Type::Unknown);
        }
        let Some(chosen) = selected.pop() else {
            return (self.unselected(given, Form::Named), Type::Unknown);
        };
        self.called(operator, chosen, given, (Form::Named, at))
    }

    /// The list a call of `operator` converts its one argument to, where
    /// the call is core's `convert<list<B>>(values)` of a `list<A>` and
    /// the struct `A` inherits from the abstract struct `B` (core's
    /// OPS-6). `public` is the operator's signature, `explicit` the
    /// generic arguments the call writes, in its order, and `expected`
    /// what its place expects.
    ///
    /// Core declares no candidate for it: "A list of structs to a list
    /// of their parent is not declared here." It takes no option, and
    /// "yields the same elements", so it cannot fail and is the list
    /// asked for (OPS-8: "A conversion that cannot fail returns T"); a
    /// place that expects another type, a `result` among them, is
    /// matched by no candidate (GEN-5). So is another generic argument
    /// the call writes: the value's type, `V`, is what it is given, and
    /// the output, `O`, is T. A list of a struct converted to a list of
    /// that struct, a list of a size, and a list of nullable elements
    /// are none of it.
    fn parent_list(
        &self,
        operator: OperatorRef,
        public: &Signature,
        explicit: &[Option<Argument>],
        given: &[Given],
        expected: Option<&Type>,
    ) -> Option<Type> {
        let program = self.program;
        let declared = &program.modules[operator.module].declarations[operator.declaration];
        if Some(operator.module) != program.core() || declared.name != "convert" {
            return None;
        }
        let Some(Argument::Type(
            asked @ Type::List {
                element: parent,
                size: None,
            },
        )) = explicit.first()?
        else {
            return None;
        };
        if expected.is_some_and(|expected| !self.admits(expected, asked)) {
            return None;
        }
        // The one argument, by position or by the operator's name for
        // its parameter (EXPR-2).
        let ([value], [parameter]) = (given, public.parameters.as_slice()) else {
            return None;
        };
        let named = match &value.key {
            Key::Position(position) => *position == 0,
            Key::Name(name) => *name == parameter.name,
        };
        if !named {
            return None;
        }
        let Type::List {
            element: child,
            size: None,
        } = &value.value.ty
        else {
            return None;
        };
        // Each other generic argument the call writes: the parameter's
        // type, which is the value's, or the result, which is T.
        for (generic, written) in public.generics.iter().zip(explicit).skip(1) {
            let Some(written) = written else {
                continue;
            };
            let names = |ty: &Type| matches!(ty, Type::Parameter { at, .. } if *at == generic.at);
            let stands_for = if names(&parameter.ty) {
                &value.value.ty
            } else if public.result.as_ref().is_some_and(names) {
                asked
            } else {
                return None;
            };
            if !matches!(written, Argument::Type(ty) if agree(ty, stands_for)) {
                return None;
            }
        }
        let (
            Type::Named {
                kind: NamedKind::Struct,
                ..
            },
            Type::Named {
                module,
                declaration,
                kind: NamedKind::Struct,
                ..
            },
        ) = (&**child, &**parent)
        else {
            return None;
        };
        let abstract_parent = has_token(
            &program.modules[*module].declarations[*declaration].syntax,
            SyntaxKind::KwAbstract,
        );
        (abstract_parent && !agree(child, parent) && self.inherits(child, parent))
            .then(|| asked.clone())
    }

    /// The member an enum's construction from `argument` names, where
    /// that is a constant: its place among the members, `Ok`, or what
    /// the enum has none of, `Err`. `None` for a value that is no
    /// constant read here. TYPE-7: "`Side(1)` is the member whose
    /// number is 1, and `Side("buy")` the member of that exact name."
    ///
    /// A member whose number is not known says nothing sure of which
    /// numbers the enum has, and a number is then not looked for.
    fn member_constructed(
        &self,
        module: usize,
        declaration: usize,
        argument: &SyntaxNode,
    ) -> Option<Result<usize, String>> {
        let Some(Detail::Enum(read)) = self.program.modules[module].details.get(declaration) else {
            return None;
        };
        let members = &read.members;
        match self.read_constant(argument, None).ok()? {
            ConstantValue::Int(given) if members.iter().all(|member| member.value.is_some()) => {
                Some(
                    members
                        .iter()
                        .position(|member| member.value == Some(given))
                        .ok_or_else(|| format!("no member numbered {given}")),
                )
            }
            ConstantValue::Str(given) => Some(
                members
                    .iter()
                    .position(|member| member.name == given)
                    .ok_or_else(|| format!("no member named `{given}`")),
            ),
            _ => None,
        }
    }

    /// What the compile-time constant `node`, written in `file`, reads
    /// as, with nothing yet held to its place's type
    /// (`crate::constant`). A generic value parameter of the declaration
    /// being read is held as that parameter, of its declared type.
    fn typed_in(
        &self,
        file: FileId,
        node: &SyntaxNode,
        expected: Option<&Type>,
    ) -> Result<Typed, Fault> {
        // Closures: functions written in place, which may use what is in
        // scope around them; here, `self`.
        let binding = |file: FileId, offset: u32| self.binding_at(file, offset);
        let parameter = |at: Location| {
            self.signature
                .generics
                .iter()
                .find(|generic| generic.at == at)
                .and_then(|generic| match &generic.kind {
                    GenericKind::Value(ty) => Some(ty.clone()),
                    _ => None,
                })
        };
        let scope = Scope {
            program: self.program,
            file,
            binding: &binding,
            parameter: &parameter,
            constants: &self.constants,
        };
        crate::constant::typed(node, expected, &scope)
    }

    /// What the compile-time constant `node`, written in `file`, comes
    /// to as a value of `expected`, the type its place expects (EXPR-5).
    fn read_in(
        &self,
        file: FileId,
        node: &SyntaxNode,
        expected: Option<&Type>,
    ) -> Result<ConstantValue, Fault> {
        let expected = expected.filter(|expected| whole(expected));
        self.typed_in(file, node, expected)
            .and_then(|read| crate::constant::fit(read, expected, self.program))
    }

    /// [`Typer::read_in`], for a constant written in the file being read.
    fn read_constant(
        &self,
        node: &SyntaxNode,
        expected: Option<&Type>,
    ) -> Result<ConstantValue, Fault> {
        self.read_in(self.file, node, expected)
    }

    /// The constant the default of `generic`, a generic value parameter
    /// of `declaration`, comes to (GEN-2), read as a value of the
    /// parameter's type where that is known, in the file the default is
    /// written in. A default that is not of that type is reported where
    /// it is declared (`signatures`).
    fn default_constant(
        &self,
        declaration: &Declaration,
        generic: &GenericParameter,
    ) -> Option<Constant> {
        let default = generic_default(&declaration.syntax, generic)?;
        let file = declaration.at.file;
        let expected = match &generic.kind {
            GenericKind::Value(ty) if !names_parameter(ty) => Some(ty),
            _ => None,
        };
        let value = self
            .read_in(file, &default, expected)
            .unwrap_or(ConstantValue::Unknown);
        Some(Constant {
            text: constant_text(&default),
            value,
        })
    }

    /// Why the constant read as `read` is no compile-time constant this
    /// pass has the value of, for a report: GEN-8, "The value a call
    /// gives a generic value parameter is a compile-time constant ... Any
    /// other rejects." `None` where it is one, or where what is wrong has
    /// been reported.
    fn not_constant(read: &Result<ConstantValue, Fault>) -> Option<&'static str> {
        match read {
            Err(Fault::NotConstant) => Some("it is known only when the program runs"),
            Ok(value) if !crate::constant::worked_out(value) => {
                Some("this compiler does not work out its value")
            }
            _ => None,
        }
    }

    /// The value candidates of `operator`, wherever they are declared.
    /// GEN-5: "Library membership and registration order grant no
    /// priority."
    fn value_candidates(&mut self, operator: OperatorRef) -> Vec<Candidate<'a>> {
        let key = (operator.module, operator.declaration);
        if let Some(found) = self.candidates.get(&key) {
            return found.clone();
        }
        let program = self.program;
        let mut candidates = Vec::new();
        for (module, declared) in program.modules.iter().enumerate() {
            for (declaration, candidate) in declared.declarations.iter().enumerate() {
                // GEN-5: "A call in a value function or a runtime body
                // selects among the value candidates. A call in
                // composition selects among all of them".
                let admitted = match candidate.kind.candidate() {
                    Some((Role::Value, Some(of))) => of == operator,
                    Some((Role::Temporal, Some(of))) => self.graph && of == operator,
                    _ => false,
                };
                if !admitted {
                    continue;
                }
                if let Some(Detail::Callable(signature)) = declared.details.get(declaration) {
                    candidates.push((module, declaration, signature));
                }
            }
        }
        self.candidates.insert(key, candidates.clone());
        candidates
    }

    /// One argument of an operator's call, read with nothing expected
    /// of it: what it reports itself is reported here, once.
    fn give(&mut self, key: Key, node: SyntaxNode) -> Given {
        let by_place = typed_by_place(&node);
        let mark = self.mark();
        let value = self.expression(&node, Some(&Type::Unknown));
        // An argument read as the type of its place is read again once
        // the candidate is known. Its first reading, when it reports
        // nothing, is taken back but for what it assigned, which a
        // later argument may read.
        let retried = by_place && self.diagnostics.len() == mark.diagnostics && self.may_reread();
        if retried {
            self.take_back(mark, true);
        }
        Given {
            key,
            by_place,
            value,
            retried,
            node,
            constant: None,
        }
    }

    /// The candidates of `operator` that admit a call, the most
    /// specific of them (GEN-5), and whether one could not be told to
    /// admit it or not.
    ///
    /// EXPR-5: an integer literal is an `f64` where one is expected; it
    /// selects as the `i64` it is where a candidate takes one. So the
    /// candidates are tried first with no literal widened.
    fn admitted(
        &mut self,
        operator: OperatorRef,
        candidates: &[(usize, usize, &'a Signature)],
        given: &[Given],
        wanted: &Wanted,
    ) -> (Vec<Chosen<'a>>, bool) {
        let program = self.program;
        let operator_declaration =
            &program.modules[operator.module].declarations[operator.declaration];
        let Some(Detail::Callable(public)) = program.modules[operator.module]
            .details
            .get(operator.declaration)
        else {
            return (Vec::new(), false);
        };
        let mut selected: Vec<Chosen> = Vec::new();
        let mut untold = false;
        // True when the candidates were tried with integer literals
        // widened.
        let mut widened = false;
        for widening in [false, true] {
            // True when a candidate takes the call by its result and
            // parameter types, whatever its condition says.
            let mut fits = false;
            for &(module, declaration, signature) in candidates {
                let candidate = &program.modules[module].declarations[declaration];
                // The candidate as this call takes it: its generic
                // parameters resolved (GEN-2).
                let widens = |decided: &Type, later: &Type| self.widens(decided, later);
                let defaults =
                    |generic: &GenericParameter| self.default_constant(candidate, generic);
                let Some((taken, generics, defaulted)) = instance(
                    signature,
                    public,
                    given,
                    wanted,
                    (&widens, &defaults, program),
                ) else {
                    continue;
                };
                // GEN-13: a generic value parameter has an ordinary
                // type, and the default it takes is of that type as
                // this call resolves it: the `2` of
                // `<T, const fallback: T = 2>` is no `str`. A default
                // that is not of the type as it is declared is reported
                // there, and a call that takes it is not judged
                // (ERR-9).
                let unfit = defaulted.iter().find_map(|&index| {
                    let default = generic_default(&candidate.syntax, &signature.generics[index])?;
                    let (GenericKind::Value(resolved), GenericKind::Value(declared)) =
                        (&taken.generics[index].kind, &signature.generics[index].kind)
                    else {
                        return None;
                    };
                    let read = self.typed_in(candidate.at.file, &default, None).ok()?;
                    let fits =
                        |ty: &Type| crate::constant::fit(read.clone(), Some(ty), program).is_ok();
                    (whole(resolved) && !fits(resolved)).then(|| !fits(declared))
                });
                match unfit {
                    Some(true) => {
                        untold = true;
                        continue;
                    }
                    Some(false) => continue,
                    None => {}
                }
                let admits = self.admits_call(
                    (candidate, &taken),
                    (operator_declaration, public),
                    (module, &generics),
                    (given, wanted),
                    widening,
                );
                fits |= admits != Admits::Misfit;
                match admits {
                    Admits::Yes => selected.push(Chosen {
                        module,
                        declaration,
                        declared: signature,
                        taken,
                        generics,
                    }),
                    Admits::No | Admits::Misfit => {}
                    Admits::Unknown => untold = true,
                }
            }
            // GEN-5: "Whether a literal is widened is settled by the
            // candidates' result and parameter types, before their
            // conditions: where a candidate that gives the requested
            // output takes every integer literal as an `i64`, none is
            // widened, and the conditions select among the candidates
            // that do. An option so does not change what a literal is."
            if fits {
                widened = widening;
                break;
            }
        }
        // GEN-5: "Select exactly one most-specific admitted candidate.
        // Concrete types outrank variables; structurally constrained
        // variables outrank unconstrained ones."
        most_specific(&mut selected, public.parameters.len());
        if widened && selected.len() > 1 {
            Self::prefer_widened(&mut selected, public, given);
        }
        // GEN-5: in composition "a temporal candidate outranks a value
        // candidate of the same parameter types, as composition chooses
        // the temporal definition of a pair (FN-4)."
        if self.graph && selected.len() > 1 {
            let temporal = |chosen: &Chosen| {
                matches!(
                    program.modules[chosen.module].declarations[chosen.declaration]
                        .kind
                        .candidate(),
                    Some((Role::Temporal, _))
                )
            };
            let types = |chosen: &Chosen| -> Vec<Type> {
                chosen
                    .taken
                    .parameters
                    .iter()
                    .map(|parameter| parameter.ty.clone())
                    .collect()
            };
            let outranked: Vec<bool> = selected
                .iter()
                .map(|chosen| {
                    !temporal(chosen)
                        && selected
                            .iter()
                            .any(|other| temporal(other) && types(other) == types(chosen))
                })
                .collect();
            let mut position = 0;
            selected.retain(|_| {
                position += 1;
                !outranked[position - 1]
            });
        }
        (selected, untold)
    }

    /// GEN-5: "Among candidates that tie and differ only in which
    /// integer literals they take as an `f64`, the one that takes every
    /// one of them as an `f64` is selected." So `1 + 2` where an `f64`
    /// is expected is the candidate for two `f64`, and not a tie with
    /// those that widen one literal.
    ///
    /// `selected` is the tied candidates, and is left as it is where
    /// they differ in anything else, or no one of them widens every
    /// literal.
    fn prefer_widened(selected: &mut Vec<Chosen<'a>>, public: &Signature, given: &[Given]) {
        // The type each candidate has for each argument, in the order
        // the arguments are given; `None` for one given to a generic
        // value parameter.
        let places = |chosen: &Chosen| -> Option<Vec<Option<Type>>> {
            let bound = bound(&chosen.taken, public, given)?;
            Some(
                bound
                    .iter()
                    .map(|place| match place {
                        Place::Parameter(index) => Some(chosen.taken.parameters[*index].ty.clone()),
                        Place::Generic(_) => None,
                    })
                    .collect(),
            )
        };
        let Some(taken) = selected.iter().map(places).collect::<Option<Vec<_>>>() else {
            return;
        };
        let literal: Vec<bool> = given
            .iter()
            .map(|given| !given.by_place && is_integer_literal(&given.value))
            .collect();
        // They differ only in what they take the literals as: every
        // other argument is given to one type, and they give one
        // result.
        let first = &selected[0];
        let alike = selected.iter().zip(&taken).all(|(chosen, places)| {
            chosen.taken.result == first.taken.result
                && places
                    .iter()
                    .zip(&taken[0])
                    .zip(&literal)
                    .all(|((place, other), literal)| *literal || place == other)
        });
        if !alike || !literal.contains(&true) {
            return;
        }
        // The candidates that take every literal as an `f64`.
        let widest: Vec<usize> = taken
            .iter()
            .enumerate()
            .filter(|(_, places)| {
                places
                    .iter()
                    .zip(&literal)
                    .filter(|(_, literal)| **literal)
                    .all(|(place, _)| place.as_ref().is_some_and(accepts_float))
            })
            .map(|(position, _)| position)
            .collect();
        if let [only] = widest[..] {
            let chosen = selected.swap_remove(only);
            selected.clear();
            selected.push(chosen);
        }
    }

    /// The call of the candidate `chosen` of `operator` with the
    /// arguments `given`, written as `form` says. Each argument is of
    /// the type the candidate's parameter has: the candidate was
    /// admitted, so each is given to one of its parameters.
    fn called(
        &mut self,
        operator: OperatorRef,
        chosen: Chosen,
        given: Vec<Given>,
        (form, at): (Form, Location),
    ) -> (ExprKind, Type) {
        let Chosen {
            module,
            declaration,
            taken,
            generics,
            ..
        } = chosen;
        let signature: &Signature = &taken;
        let program = self.program;
        let bound = match program.modules[operator.module]
            .details
            .get(operator.declaration)
        {
            Some(Detail::Callable(public)) => bound(signature, public, &given).unwrap_or_default(),
            _ => Vec::new(),
        };
        // FN-15: a `var` parameter "is always given: it has no default,
        // and is not left out where its type is written `T?`". The mark
        // takes no part in selecting the candidate, which admits a call
        // that leaves a `T?` out (UNIT-8): it is asked for here, of the
        // candidate the call resolved to.
        for (index, parameter) in signature.parameters.iter().enumerate() {
            if parameter.var && !bound.contains(&Place::Parameter(index)) {
                self.diagnostics.emit(error(
                    Category::Type,
                    codes::ARGUMENT_MISSING,
                    at,
                    format!(
                        "`{}` is given no `{}`, a `var` parameter, which is always given a place",
                        program.modules[module].declarations[declaration].name, parameter.name
                    ),
                    "FN-15",
                ));
            }
        }
        // EXPR-14: a symbol's configuration, `{abs: tolerance}`, given
        // to a value candidate in a graph binds where the graph is
        // built. The entries given by name to a symbol are its
        // configuration (EXPR-10); its operands are given in position.
        let lifted = self.graph
            && !matches!(form, Form::Named)
            && matches!(
                program.modules[module].declarations[declaration]
                    .kind
                    .candidate(),
                Some((Role::Value, _))
            );
        let arguments = given
            .into_iter()
            .enumerate()
            .filter_map(|(position, given)| {
                let index = match bound.get(position) {
                    Some(Place::Parameter(index)) => *index,
                    // What a generic value parameter is given is a
                    // compile-time constant, kept among the call's
                    // generic arguments and not among its arguments.
                    Some(Place::Generic(_)) => return None,
                    None => {
                        return Some(hir::Argument {
                            parameter: None,
                            value: given.value,
                        });
                    }
                };
                let parameter = &signature.parameters[index];
                let place = format!("the parameter `{}`", parameter.name);
                let named = matches!(given.key, Key::Name(_));
                let value = self.argument_value(given, &parameter.ty, &place);
                // A `const` parameter given a connection is reported as
                // one where the call is read (FN-3).
                if lifted && !parameter.constant && named {
                    self.configuration_input(form, &parameter.name, &value);
                }
                Some(hir::Argument {
                    parameter: Some(index),
                    value,
                })
            })
            .collect();
        let requirements = self.answers(module, declaration, &generics);
        let kind = ExprKind::Call {
            callee: Callee::Candidate {
                operator: (operator.module, operator.declaration),
                module,
                declaration,
                generics,
                requirements,
            },
            arguments,
            form,
        };
        self.finished_call(&kind, false);
        (kind, signature.result.clone().unwrap_or(Type::Unknown))
    }

    /// An entry of the configuration of the symbol written `form` in a
    /// graph, `value` given to the option `option` of the value
    /// candidate selected, which the graph lifts (FN-6).
    ///
    /// EXPR-14: "Scalar operator configuration binds during graph
    /// construction in temporal composition ... Configuration must not
    /// silently introduce a temporal input: time-varying temporal
    /// configuration requires a signature explicitly admitting that
    /// input." A value candidate's parameter is a scalar one: a
    /// connection there would be an input the configuration brought
    /// in. A temporal candidate's parameter admits one, and a call by
    /// name, `eq_(a, b, abs: tolerance)`, gives it explicitly (FN-6).
    /// TIME-2: "Composition describes a typed graph without reading
    /// live payloads." So the entry is given what is known where the
    /// graph is built (`connection.as_constant`).
    fn configuration_input(&mut self, form: Form, option: &str, value: &Expr) {
        if self.category(value) != ValueCategory::Connection {
            return;
        }
        let symbol = match form {
            Form::Binary(operator) => operator.symbol(),
            Form::Unary(operator) => operator.symbol(),
            Form::Named => return,
        };
        self.diagnostics.emit(
            error(
                Category::Phase,
                codes::CONNECTION_AS_CONSTANT,
                value.at,
                format!(
                    "`{option}` configures `{symbol}`, which binds it where the graph is built, \
                     and this is a connection"
                ),
                "EXPR-14",
            )
            .with_note("a constant or a `const` parameter is known where the graph is built"),
        );
    }

    /// The arguments of an operator's call as they are written, each
    /// with its position or its name. What is wrong with the call
    /// whichever candidate takes it is reported here (EXPR-2): an
    /// argument with no name after a named one; a name that neither the
    /// operator nor any candidate declares; a name given twice, or a
    /// parameter of the operator given in position and by name.
    ///
    /// Which parameter an option's argument is given to differs from
    /// candidate to candidate, so that is not judged here: a candidate
    /// the arguments do not fit is not admitted (GEN-6).
    fn operator_arguments(
        &mut self,
        callee: &SyntaxToken,
        list: &SyntaxNode,
        public: &Signature,
        options: &std::collections::HashSet<&str>,
    ) -> Vec<(Key, SyntaxNode)> {
        let mut named_seen = false;
        let mut position = 0;
        let mut names: std::collections::HashSet<String> = std::collections::HashSet::new();
        let mut arguments = Vec::new();
        for (name, value) in self.call_arguments(list) {
            let key = match name {
                None => {
                    if named_seen {
                        self.diagnostics.emit(error(
                            Category::Type,
                            codes::ARGUMENT_ORDER,
                            self.at(&value),
                            "an argument with no name follows a named one: positional \
                             arguments come first"
                                .to_string(),
                            "EXPR-2",
                        ));
                    }
                    position += 1;
                    Key::Position(position - 1)
                }
                Some(name) => {
                    named_seen = true;
                    let text = name.text();
                    let at = Location::new(self.file, name.span());
                    // The operator's parameters are the same places in
                    // every candidate, the first of each.
                    let own = public
                        .parameters
                        .iter()
                        .position(|parameter| parameter.name == text);
                    if own.is_none() && !options.contains(text) {
                        self.diagnostics.emit(error(
                            Category::Name,
                            codes::ARGUMENT_UNKNOWN,
                            at,
                            format!("`{}` has no parameter `{text}`", callee.text()),
                            "EXPR-2",
                        ));
                    } else if !names.insert(text.to_string())
                        || own.is_some_and(|index| index < position)
                    {
                        self.diagnostics.emit(error(
                            Category::Name,
                            codes::ARGUMENT_DUPLICATE,
                            at,
                            format!("`{text}` is given twice"),
                            "EXPR-2",
                        ));
                    }
                    Key::Name(text.to_string())
                }
            };
            arguments.push((key, value));
        }
        arguments
    }

    /// The types an operator's requirement admits for its result, with
    /// the call's generic arguments `map` in place: for `operator
    /// convert<T, V, O>(value: V) -> O requires O in {T, result<T,
    /// error>}` called as `convert<f64>(count)`, an `f64` and a
    /// `result<f64, error>`. `None` when the operator states none, or
    /// one of them still names a generic parameter.
    ///
    /// The requirement is read where it is `O in {types}` for the bare
    /// parameter `O` the result is, alone or joined to others by `&&`:
    /// where several are so joined, the result is in every set.
    fn result_among(
        &self,
        operator: OperatorRef,
        public: &Signature,
        map: &[(Location, &Argument)],
        own: &[Location],
    ) -> Option<Vec<Type>> {
        let Some(Type::Parameter { name, .. }) = &public.result else {
            return None;
        };
        let module = &self.program.modules[operator.module];
        let declaration = &module.declarations[operator.declaration];
        let clause = child(&declaration.syntax, SyntaxKind::RequiresClause)?;
        let mut sets = Vec::new();
        result_sets(&clause.children().next()?, name, &mut sets);
        let file = module.files[declaration.file].file;
        // The types one set admits; `None` for a set with a member that
        // is no type, or that still names a generic parameter. Such a
        // set says nothing here.
        let types_of = |set: &SyntaxNode| -> Option<Vec<Type>> {
            let mut types = Vec::new();
            for member in set.children() {
                if !is_type(member.kind()) {
                    return None;
                }
                // What the `read` pass said the type written there
                // means.
                let start = member.span().start;
                let written = module
                    .written
                    .iter()
                    .find(|written| written.at.file == file && written.at.span.start == start)?;
                let ty = placed(substitute_within(&written.ty, map), own);
                if !whole(&ty) {
                    return None;
                }
                types.push(ty);
            }
            Some(types)
        };
        // Requirements joined by `&&` all hold: the result is in every
        // set.
        let mut among: Option<Vec<Type>> = None;
        for set in &sets {
            let Some(types) = types_of(set) else {
                continue;
            };
            among = Some(match among {
                None => types,
                Some(mut among) => {
                    among.retain(|ty| types.contains(ty));
                    among
                }
            });
        }
        among
    }

    /// Whether each named operator requirement in the condition of
    /// `candidate` holds at a call that takes it with `generics` for
    /// its generic parameters: where the requirement starts, and the
    /// verdict, `None` when it cannot be told.
    ///
    /// GEN-16: "Resolve their meaning from the referenced declaration
    /// or intrinsic ... a named operator requirement tests callability
    /// with the supplied types. A result arrow constrains that
    /// operator's result type". A constraint call that names no
    /// operator is a predicate, which is not read here.
    fn requirements_hold(
        &mut self,
        candidate: &Declaration,
        parameters: &[GenericParameter],
        (module, generics): (usize, &[Argument]),
        condition: &SyntaxNode,
    ) -> Vec<(u32, Option<bool>)> {
        let calls: Vec<SyntaxNode> = condition
            .descendants()
            .filter(|node| node.kind() == SyntaxKind::ConstraintCall)
            .collect();
        if calls.is_empty() {
            return Vec::new();
        }
        let file = self.program.modules[module].files[candidate.file].file;
        let read = self.requirements_read(candidate, parameters, (module, generics), &calls);
        let mut verdicts = Vec::new();
        for (call, read) in calls.iter().zip(read) {
            let verdict = match read {
                Some((operator, operands, result)) => {
                    self.callable(operator, (file, operands), result.as_ref())
                }
                None => None,
            };
            verdicts.push((call.span().start, verdict));
        }
        verdicts
    }

    /// Reports a `requires` that does not hold where a function is
    /// called, or a struct applied, with `generics` for its generic
    /// parameters. `at` is the call or the type, and `given` what it
    /// says of the generic value parameters.
    ///
    /// GEN-3: "A call of a function whose `requires` does not hold with
    /// the generic arguments the call resolves is rejected, and so is
    /// an application of a struct whose `requires` does not hold with
    /// its arguments." Nothing is reported where it cannot be told
    /// ([`requires_holds`]). A named operator requirement is told for a
    /// generic parameter of the declaration being read too: it holds
    /// where that declaration states it (GEN-7).
    fn requires_met(
        &mut self,
        at: Location,
        (module, declaration): (usize, usize),
        generics: &[Argument],
    ) {
        let program = self.program;
        let Some((parameters, condition)) = required(program, (module, declaration)) else {
            return;
        };
        let declared = &program.modules[module].declarations[declaration];
        // GEN-16: "a named operator requirement tests callability with
        // the supplied types."
        let verdicts = self.requirements_hold(declared, parameters, (module, generics), &condition);
        let callable = |node: &SyntaxNode| -> Option<bool> {
            let start = node.span().start;
            verdicts
                .iter()
                .find(|(at, _)| *at == start)
                .and_then(|(_, verdict)| *verdict)
        };
        // What a constant the condition compares with comes to, read
        // where the declaration is written (`crate::constant`). The
        // values the call gives are `generics`, each read as a value of
        // its parameter's type where the call writes it or leaves it to
        // its default.
        let this = &*self;
        let constant = |node: &SyntaxNode, ty: Option<&Type>| -> Option<ConstantValue> {
            this.read_in(declared.at.file, node, ty).ok()
        };
        let held = requires_holds(
            program,
            (module, declaration),
            generics,
            &condition,
            &Told {
                constant: &constant,
                callable: &callable,
            },
        );
        // A construction that writes its generic arguments,
        // `Sized<str>(value: "x")`, writes a type, of which the same
        // has been reported ([`applications`]).
        let reported = self.diagnostics.as_slice().iter().any(|earlier| {
            earlier.code == Code::Catalogued(codes::REQUIRES_UNMET)
                && earlier.primary.file == at.file
                && earlier.primary.span.start == at.span.start
        });
        if held == Some(false) && !reported {
            self.diagnostics
                .emit(unmet(program, (module, declaration), at, &condition));
        }
    }

    /// Each of the constraint calls `calls`, written in the `requires`
    /// of `declared` of `module`, read with `generics` for the
    /// declaration's generic parameters: the operator it names, the
    /// types it is supplied with, each with where it is written, and
    /// the type its arrow states. `None` for one that names no
    /// operator, a predicate, and for one supplied with what is no
    /// type, or with a type that is not known.
    fn requirements_read(
        &self,
        declared: &Declaration,
        parameters: &[GenericParameter],
        (module, generics): (usize, &[Argument]),
        calls: &[SyntaxNode],
    ) -> Vec<Option<Read>> {
        let program = self.program;
        let declared_in = &program.modules[module];
        let file = declared_in.files[declared.file].file;
        let own: Vec<Location> = parameters.iter().map(|generic| generic.at).collect();
        let map: Vec<(Location, &Argument)> = own.iter().copied().zip(generics).collect();
        // What a type written in the condition means, the declaration's
        // generic parameters in place.
        let written = |node: &SyntaxNode| -> Option<Type> {
            let start = node.span().start;
            let read = declared_in
                .written
                .iter()
                .find(|written| written.at.file == file && written.at.span.start == start)?;
            let ty = placed(substitute_within(&read.ty, &map), &own);
            whole(&ty).then_some(ty)
        };
        calls
            .iter()
            .map(|call| {
                let name = name_token(call)?;
                // What the name is bound to where the declaration is
                // written.
                let start = name.span().start;
                let used = declared_in
                    .uses
                    .iter()
                    .find(|used| used.at.file == file && used.at.span.start == start)?;
                let Binding::Declarations {
                    module: operator_module,
                    declarations,
                } = &used.binding
                else {
                    return None;
                };
                let declaration = declarations.iter().copied().find(|&declaration| {
                    program.modules[*operator_module].declarations[declaration].kind
                        == DeclarationKind::Operator
                })?;
                let mut operands = Vec::new();
                let mut result = None;
                for part in call.children() {
                    match part.kind() {
                        SyntaxKind::QualifiedName => {}
                        SyntaxKind::ReturnType => {
                            result = Some(written(&part.children().next()?)?);
                        }
                        kind if is_type(kind) => {
                            let ty = written(&part)?;
                            operands.push((part, ty));
                        }
                        _ => return None,
                    }
                }
                let operator = OperatorRef {
                    module: *operator_module,
                    declaration,
                };
                Some((operator, operands, result))
            })
            .collect()
    }

    /// What answers each requirement that `declaration` of `module`
    /// states, at a call that takes it with `generics` for its generic
    /// parameters: one [`hir::Answer`] for each constraint call that
    /// holds wherever its `requires` does, in the order written.
    ///
    /// GEN-16: "a named operator requirement tests callability with the
    /// supplied types ... it is tested against the operator's value
    /// candidates, and holds where one most-specific value candidate
    /// takes the types supplied (GEN-5)". That candidate is the answer.
    /// GEN-7: where the types name a generic parameter of the
    /// declaration the call is written in, a requirement that
    /// declaration states is the answer, and what answers that is known
    /// where it is called in turn.
    ///
    /// The candidate that answers may state requirements of its own,
    /// which are answered with it. That ends: a candidate is selected
    /// only where each of its requirements was told to hold, which is
    /// tested no deeper than [`REQUIRING`].
    fn answers(
        &mut self,
        module: usize,
        declaration: usize,
        generics: &[Argument],
    ) -> Vec<hir::Answer> {
        let program = self.program;
        let declared_in = &program.modules[module];
        let declared = &declared_in.declarations[declaration];
        let Some(Detail::Callable(signature)) = declared_in.details.get(declaration) else {
            return Vec::new();
        };
        let Some(clause) = child(&declared.syntax, SyntaxKind::RequiresClause) else {
            return Vec::new();
        };
        let mut calls = Vec::new();
        if let Some(condition) = clause.children().next() {
            required_calls(&condition, &mut calls);
        }
        if calls.is_empty() {
            return Vec::new();
        }
        if self.requiring >= REQUIRING {
            return vec![hir::Answer::Unread; calls.len()];
        }
        let file = declared_in.files[declared.file].file;
        let read =
            self.requirements_read(declared, &signature.generics, (module, generics), &calls);
        read.into_iter()
            .map(|read| {
                let Some((operator, operands, result)) = read else {
                    return hir::Answer::Unread;
                };
                match self.answered(operator, (file, operands), result.as_ref()) {
                    Some(Answered::Stated(index)) => hir::Answer::Stated(index),
                    Some(Answered::Selected {
                        module,
                        declaration,
                        generics,
                    }) => {
                        self.requiring += 1;
                        let requirements = self.answers(module, declaration, &generics);
                        self.requiring -= 1;
                        hir::Answer::Candidate {
                            operator: (operator.module, operator.declaration),
                            module,
                            declaration,
                            generics,
                            requirements,
                        }
                    }
                    Some(Answered::Unmet) | None => hir::Answer::Unread,
                }
            })
            .collect()
    }

    /// Whether `operator` is callable with values of the types
    /// `operands`, to give `result` where one is stated (GEN-16).
    /// `None` when it cannot be told.
    ///
    /// A requirement names a value operation, whatever declares it:
    /// core's OPS-9 has "Addition requires a value `add_(T, T) -> T`",
    /// which a node's hook calls (FN-3), and a graph that states the
    /// same requirement may call that node's candidate. So it is tested
    /// against the operator's value candidates.
    ///
    /// Types that name a generic parameter of the declaration being
    /// read are callable where its own requirements state it (GEN-7).
    /// Otherwise, and for any other types, the operator is callable
    /// when one most-specific value candidate admits them (GEN-5). So a
    /// candidate that takes any one type, core's `eq_<T>`, answers for
    /// a generic parameter with nothing stated.
    fn callable(
        &mut self,
        operator: OperatorRef,
        supplied: (FileId, Vec<(SyntaxNode, Type)>),
        result: Option<&Type>,
    ) -> Option<bool> {
        self.answered(operator, supplied, result)
            .map(|answered| !matches!(answered, Answered::Unmet))
    }

    /// What answers a requirement that `operator` is callable with
    /// values of the types `operands`, to give `result` where one is
    /// stated: see [`Typer::callable`], which says when it holds.
    /// `None` when it cannot be told.
    fn answered(
        &mut self,
        operator: OperatorRef,
        (file, operands): (FileId, Vec<(SyntaxNode, Type)>),
        result: Option<&Type>,
    ) -> Option<Answered> {
        let own: Vec<Location> = self
            .signature
            .generics
            .iter()
            .map(|generic| generic.at)
            .collect();
        let generic = operands.iter().any(|(_, ty)| mentions(ty, &own))
            || result.is_some_and(|result| mentions(result, &own));
        let program = self.program;
        let Some(Detail::Callable(public)) = program.modules[operator.module]
            .details
            .get(operator.declaration)
        else {
            return None;
        };
        // A requirement is tested as the call it names would select
        // (GEN-8). It writes no generic argument: its arrow is what its
        // place expects, and the types it is supplied with are its
        // arguments.
        let mut found: Vec<Option<Argument>> = vec![None; public.generics.len()];
        from_place(public, result, &mut found);
        let supplied: Vec<Option<&Type>> = (0..public.parameters.len())
            .map(|index| operands.get(index).map(|(_, ty)| ty))
            .collect();
        let Requested {
            place: requested,
            among,
            unfit,
        } = self.output_requested(operator, public, found, &supplied, false);
        // GEN-13: a default the call would take is not of its
        // parameter's type as these types resolve it. The call would be
        // rejected, and so the operator is not callable with them.
        if unfit.is_some() {
            return Some(Answered::Unmet);
        }
        // The arrow is what the place of the call would expect. Where
        // the operator's defaults make the call's output another type,
        // the call does not give the arrow's: with
        // `operator count<T, const mode: i64 = 0>(value: T) -> i64`,
        // `requires count(bool) -> str` holds for no candidate.
        if let (Some(requested), Some(arrow)) = (&requested, result)
            && !self.admits(arrow, requested)
        {
            return Some(Answered::Unmet);
        }
        // What the call asks of the result: with the operator's
        // defaults, or as its arrow says.
        let asked: Option<Type> = requested.or_else(|| result.cloned());
        if generic {
            // GEN-7: what the declaration being read states is evidence
            // for the same call: this operator, these types, and the
            // same output asked. `requires window(T) -> list<T, 5>` does
            // not say the bare `window(T)`, which asks for the
            // operator's default, is callable.
            let types: Vec<&Type> = operands.iter().map(|(_, ty)| ty).collect();
            let stated = self.requirements().into_iter().find(|requirement| {
                requirement.operator == operator
                    && requirement.operands.iter().eq(types.iter().copied())
                    && self.asked_by(requirement) == Some(asked.clone())
            });
            if let Some(stated) = stated {
                return Some(Answered::Stated(stated.index));
            }
        }
        if self.requiring >= REQUIRING {
            return None;
        }
        // In a graph the candidates of a call are the temporal ones
        // too (GEN-5): a requirement is for the value ones.
        let mut candidates = self.value_candidates(operator);
        candidates.retain(|&(module, declaration, _)| {
            matches!(
                program.modules[module].declarations[declaration]
                    .kind
                    .candidate(),
                Some((Role::Value, _))
            )
            // GEN-16: "A candidate that marks a parameter `var`
            // (FN-15) answers no requirement: a requirement supplies
            // types, and no place."
            && !matches!(
                program.modules[module].details.get(declaration),
                Some(Detail::Callable(signature))
                    if signature.parameters.iter().any(|parameter| parameter.var)
            )
        });
        if unreadable(public, &candidates) {
            return None;
        }
        // Each operand as an argument in its place, of the type
        // supplied: there is no expression to read.
        let given: Vec<Given> = operands
            .into_iter()
            .enumerate()
            .map(|(position, (node, ty))| {
                let at = Location::new(file, node.span());
                let value = Expr {
                    kind: ExprKind::Unread,
                    ty,
                    at,
                };
                Given::read(position, node, value)
            })
            .collect();
        let wanted = Wanted {
            place: asked.as_ref(),
            among,
        };
        self.requiring += 1;
        let (mut selected, untold) = self.admitted(operator, &candidates, &given, &wanted);
        self.requiring -= 1;
        if untold {
            return None;
        }
        // GEN-5: "Select exactly one most-specific admitted candidate."
        Some(match (selected.pop(), selected.is_empty()) {
            (Some(chosen), true) => Answered::Selected {
                module: chosen.module,
                declaration: chosen.declaration,
                generics: chosen.generics,
            },
            _ => Answered::Unmet,
        })
    }

    /// What a requirement asks of its operator's result, as the call it
    /// names would (GEN-8): the operator's result with its arrow, the
    /// types it is supplied with and the operator's defaults in place,
    /// or its arrow where those say nothing. The outer `None` is for a
    /// requirement no call meets: a default that call would take does
    /// not fit (GEN-13), or its arrow is not what the operator's result
    /// is with those in place.
    fn asked_by(&self, requirement: &Requirement) -> Option<Option<Type>> {
        let operator = requirement.operator;
        let arrow = requirement.result.as_ref();
        let Some(Detail::Callable(public)) = self.program.modules[operator.module]
            .details
            .get(operator.declaration)
        else {
            return Some(arrow.cloned());
        };
        let mut found: Vec<Option<Argument>> = vec![None; public.generics.len()];
        from_place(public, arrow, &mut found);
        let supplied: Vec<Option<&Type>> = (0..public.parameters.len())
            .map(|index| requirement.operands.get(index))
            .collect();
        let Requested { place, unfit, .. } =
            self.output_requested(operator, public, found, &supplied, false);
        if unfit.is_some() {
            return None;
        }
        // An arrow the call's own output cannot be: see `callable`.
        if let (Some(place), Some(arrow)) = (&place, arrow)
            && !self.admits(arrow, place)
        {
            return None;
        }
        Some(place.or_else(|| arrow.cloned()))
    }

    /// The named operator requirements the enclosing declaration
    /// states: those that stand alone in its `requires`, or are joined
    /// to the rest by `&&`. One under a negation or a disjunction says
    /// nothing that holds for sure.
    ///
    /// GEN-16: "a named operator requirement tests callability with the
    /// supplied types. A result arrow constrains that operator's result
    /// type". The name is an operator when it is bound to one; a
    /// requirement supplied with what is no type is not read here.
    fn requirements(&self) -> Vec<Requirement> {
        let Some(clause) = child(&self.declaration.syntax, SyntaxKind::RequiresClause) else {
            return Vec::new();
        };
        let mut calls = Vec::new();
        if let Some(condition) = clause.children().next() {
            required_calls(&condition, &mut calls);
        }
        let program = self.program;
        // What the `read` pass said a type written here means.
        let type_at = |node: &SyntaxNode| -> Option<Type> {
            self.written
                .get(&(self.file, node.span().start))
                .map(|ty| (*ty).clone())
        };
        calls
            .iter()
            .enumerate()
            .filter_map(|(index, call)| {
                let name = name_token(call)?;
                let (module, declarations) = self.declarations(&name)?;
                let declaration = declarations.iter().copied().find(|&declaration| {
                    program.modules[module].declarations[declaration].kind
                        == DeclarationKind::Operator
                })?;
                let mut operands = Vec::new();
                let mut result = None;
                for part in call.children() {
                    match part.kind() {
                        // `alias::name`: the callee.
                        SyntaxKind::QualifiedName => {}
                        SyntaxKind::ReturnType => {
                            result = Some(type_at(&part.children().next()?)?);
                        }
                        kind if is_type(kind) => operands.push(type_at(&part)?),
                        _ => return None,
                    }
                }
                Some(Requirement {
                    index,
                    operator: OperatorRef {
                        module,
                        declaration,
                    },
                    operands,
                    result,
                })
            })
            .collect()
    }

    /// What the enclosing declaration's requirements say of a call of
    /// `operator` with the arguments `given`: `Some` when one of them
    /// is that call.
    ///
    /// GEN-7: "A generic body is checked under its stated
    /// requirements". A requirement is the call when it names the
    /// operator and is supplied with the arguments' types, each in its
    /// place: under `requires add_(T, T) -> T`, `add_(lhs, rhs)` of two
    /// `T` is a `T`, and so is `add_(rhs: b, lhs: a)`. An argument with
    /// no type of its own, a sequence or a tuple written out, is tried
    /// as the type the requirement supplies in its place: `[a] + [b]`
    /// under `requires add_(list<T>, list<T>) -> list<T>`.
    ///
    /// A requirement speaks for types of which one names a generic
    /// parameter of the declaration, its result's among them: any
    /// other call selects its candidate here, as it does in a body
    /// that has no generic parameter. `requested` is the output the
    /// call itself asks for, through the generic arguments it writes
    /// and the operator's defaults (GEN-8): a requirement that gives
    /// another is not this call. `expected` is the type the call's
    /// place expects, which chooses among requirements supplied alike.
    /// `written` is the operator's generic arguments as far as the call
    /// writes them, `make<T>()`: the requirement is one those agree
    /// with.
    fn required(
        &mut self,
        operator: OperatorRef,
        given: &[&Given],
        (requested, expected): (Option<&Type>, Option<&Type>),
        written: Option<&[Option<Argument>]>,
    ) -> Option<Required> {
        let own: Vec<Location> = self
            .signature
            .generics
            .iter()
            .map(|generic| generic.at)
            .collect();
        if own.is_empty() {
            return None;
        }
        let Some(Detail::Callable(public)) = self.program.modules[operator.module]
            .details
            .get(operator.declaration)
        else {
            return None;
        };
        // EXPR-2: "Named arguments use the selected public callable's
        // parameter names". Each argument is given to one of the
        // operator's own parameters, by its position or by that
        // parameter's name (GEN-4). A name the operator does not
        // declare is an option of a candidate, which no requirement
        // speaks for.
        let mut places = Vec::with_capacity(given.len());
        for argument in given {
            // One read as the type of its place whose first reading
            // stands was reported itself: the call is not judged here.
            if argument.by_place && !argument.retried {
                return None;
            }
            let place = match &argument.key {
                Key::Position(position) => *position,
                Key::Name(name) => public
                    .parameters
                    .iter()
                    .position(|parameter| parameter.name == *name)?,
            };
            // The operands a requirement is supplied with stand in the
            // operator's first places, each given once.
            if place >= given.len() || places.contains(&place) {
                return None;
            }
            places.push(place);
        }
        // Past the function's bound no argument is tried, and the call
        // is left to be not judged where its candidates are read.
        if self.rereads == 0 && given.iter().any(|argument| argument.retried) {
            return None;
        }
        // What the call's place expects, where that is known.
        let wanted = expected.filter(|expected| !open(expected));
        let mut first: Option<Required> = None;
        // The argument given to each place, in the operator's order.
        let mut ordered: Vec<&Given> = given.to_vec();
        for (argument, &place) in given.iter().zip(&places) {
            ordered[place] = argument;
        }
        for requirement in self.requirements() {
            // One that names a generic parameter of the declaration,
            // among the types it is supplied with or in its result:
            // `make() -> T`.
            let generic = requirement
                .operands
                .iter()
                .chain(&requirement.result)
                .any(|ty| mentions(ty, &own));
            if requirement.operator != operator
                || requirement.operands.len() != ordered.len()
                || !generic
            {
                continue;
            }
            // GEN-8: what the call requests is what it gets. Under
            // `requires window(T) -> list<T, 5>`, a `window(value)` that
            // takes the operator's default of three is another call, and
            // so is a `window<T, 5>(value)` under `requires window(T)`,
            // which asks for that default.
            let Some(asks) = self.asked_by(&requirement) else {
                continue;
            };
            if let Some(requested) = requested
                && !asks
                    .as_ref()
                    .is_some_and(|asks| self.admits(requested, asks))
            {
                continue;
            }
            // GEN-8: "A call may supply its generic arguments as a
            // generic constructor does". With them in place, what the
            // operator's own signature says of a parameter or of the
            // result is what the requirement says of it.
            if let Some(written) = written {
                // What the call does not write is left open, and agrees
                // with any: with `operator make<A, B>() -> tuple<A, B>`,
                // `make<T>()` asks for a `tuple<T, _>`, which a
                // requirement `make() -> tuple<U, T>` is not.
                let explicit: Vec<Argument> = written
                    .iter()
                    .zip(&public.generics)
                    .map(|(argument, generic)| {
                        argument.clone().unwrap_or_else(|| match generic.kind {
                            GenericKind::Value(_) => Argument::Constant(unsaid()),
                            _ => Argument::Type(Type::Unknown),
                        })
                    })
                    .collect();
                let map: Vec<(Location, &Argument)> =
                    fixed(&public.generics).zip(&explicit).collect();
                let agrees = public.parameters.iter().zip(&requirement.operands).all(
                    |(parameter, operand)| {
                        self.admits(&substitute_within(&parameter.ty, &map), operand)
                    },
                ) && match (&public.result, &requirement.result) {
                    (Some(declared), Some(result)) => {
                        self.admits(&substitute_within(declared, &map), result)
                    }
                    _ => true,
                };
                if !agrees {
                    continue;
                }
            }
            let mut supplied = true;
            for (operand, argument) in requirement.operands.iter().zip(&ordered) {
                supplied &= if argument.retried {
                    // Read again as the type supplied; what the trial
                    // reports is taken back.
                    self.fits_place(&argument.node, operand, false)
                } else {
                    *operand == argument.value.ty
                };
                if !supplied {
                    break;
                }
            }
            if !supplied {
                continue;
            }
            // GEN-5: "Match candidates against the requested output
            // before inputs". Of two requirements supplied alike, the
            // one whose arrow gives what the call's place expects is
            // the call: under `convert(T) -> i64 && convert(T) -> f64`
            // a place that expects an `f64` takes the second.
            let gives = match (wanted, requirement.result.as_ref().or(asks.as_ref())) {
                (Some(wanted), Some(result)) => self.admits(wanted, result),
                _ => true,
            };
            // What the call gives: what its arrow states, or where it
            // states none what the operator's defaults make its result.
            let required = Required {
                index: requirement.index,
                result: requirement.result.or(asks),
                operands: requirement.operands,
                places: places.clone(),
            };
            if gives {
                return Some(required);
            }
            // With none that gives it, the first supplied stands, and
            // its type is reported where it does not fit.
            first.get_or_insert(required);
        }
        first
    }

    /// The call of an operation the enclosing declaration requires
    /// (GEN-7): of the type the requirement's arrow states, and of no
    /// known type where it states none. No candidate is selected: that
    /// is done where the generic parameters are known. An argument
    /// whose first reading was taken back is read where it stands, as
    /// the type the requirement supplies.
    fn call_required(
        &mut self,
        operator: OperatorRef,
        required: Required,
        given: Vec<Given>,
        form: Form,
    ) -> (ExprKind, Type) {
        let Required {
            index,
            result,
            operands,
            places,
        } = required;
        // The operator's own parameters, in the order it declares them
        // (GEN-4).
        let parameters = match self.program.modules[operator.module]
            .details
            .get(operator.declaration)
        {
            Some(Detail::Callable(public)) => public.parameters.len(),
            _ => 0,
        };
        let arguments = given
            .into_iter()
            .zip(places)
            .map(|(given, place)| {
                let value = if given.retried {
                    let operand = &operands[place];
                    let value = self.expression(&given.node, Some(operand));
                    self.fit(value, operand, "the operand its requirement supplies")
                } else {
                    given.value
                };
                hir::Argument {
                    parameter: (place < parameters).then_some(place),
                    value,
                }
            })
            .collect();
        let kind = ExprKind::Call {
            callee: Callee::Required {
                operator: (operator.module, operator.declaration),
                requirement: index,
            },
            arguments,
            form,
        };
        (kind, result.unwrap_or(Type::Unknown))
    }

    /// The argument `given` as the value of a parameter of type `ty`,
    /// fitted to it (EXPR-5). One whose first reading was taken back
    /// (`retried`) is read again where it stands, as that type: what
    /// the first reading declared is gone, and the value read again
    /// declares its own. `place` names the parameter for messages.
    fn argument_value(&mut self, given: Given, ty: &Type, place: &str) -> Expr {
        if given.retried {
            let value = self.expression(&given.node, Some(ty));
            self.fit(value, ty, place)
        } else {
            self.fit(given.value, ty, place)
        }
    }

    /// The call of an operator for which no candidate is selected: its
    /// arguments as they were read, and no callee. One whose first
    /// reading was taken back is read where it stands.
    fn unselected(&mut self, given: Vec<Given>, form: Form) -> ExprKind {
        let arguments = given
            .into_iter()
            .map(|given| hir::Argument {
                parameter: None,
                value: if given.retried {
                    self.expression(&given.node, Some(&Type::Unknown))
                } else {
                    given.value
                },
            })
            .collect();
        ExprKind::Call {
            callee: Callee::Unread,
            arguments,
            form,
        }
    }

    /// True when the candidate `signature`, of an operator whose public
    /// signature is `public`, admits a call that gives it `given` and of
    /// which `wanted` is expected (GEN-5). With `widening`, an integer
    /// literal is admitted where an `f64` is taken (EXPR-5).
    fn admits_call(
        &mut self,
        (candidate, signature): (&Declaration, &Signature),
        (operator, public): (&Declaration, &Signature),
        (module, generics): (usize, &[Argument]),
        (given, wanted): (&[Given], &Wanted),
        widening: bool,
    ) -> Admits {
        let result_fits = match (wanted.place, &signature.result) {
            // An output requested in part, a `list<T, 3>` with `T` not
            // said, is matched as far as it is known: `admits` compares
            // what each type says and lets what is not said pass, and
            // keeps a `T` a value of a `T?` (TYPE-14).
            (Some(wanted), Some(result)) => self.admits(wanted, result),
            (Some(_), None) => false,
            (None, _) => true,
        };
        // The operator's own requirement of its result.
        let among = match (&wanted.among, &signature.result) {
            (Some(among), Some(result)) => among.contains(result),
            (Some(_), None) => false,
            (None, _) => true,
        };
        if !result_fits || !among {
            return Admits::Misfit;
        }
        // GEN-6: "A call is matched against each candidate's own
        // signature. One that names an option admits only the
        // candidates that declare it."
        let Some(bound) = bound(signature, public, given) else {
            return Admits::Misfit;
        };
        let own = public.parameters.len();
        // GEN-13: a generic value parameter has an ordinary type, and
        // what the call gives it is of that type.
        for (position, place) in bound.iter().enumerate() {
            let Place::Generic(index) = *place else {
                continue;
            };
            let GenericKind::Value(ty) = &signature.generics[index].kind else {
                continue;
            };
            let given = &given[position];
            let admitted = if given.by_place {
                self.fits_place(&given.node, ty, widening)
            } else {
                self.admits(ty, &given.value.ty)
                    || widening && is_integer_literal(&given.value) && accepts_float(ty)
            };
            if !admitted {
                return Admits::Misfit;
            }
        }
        for (index, parameter) in signature.parameters.iter().enumerate() {
            let argument = bound
                .iter()
                .position(|&place| place == Place::Parameter(index))
                .map(|position| &given[position]);
            let admitted = match argument {
                // What the call leaves out has a default, the
                // candidate's or for one of its own parameters the
                // operator's, or is written `T?` (UNIT-8). GEN-6: "a
                // candidate whose option has none is admitted only when
                // the call gives it."
                None => {
                    parameter.has_default
                        || (index < own && public.parameters[index].has_default)
                        || matches!(parameter.ty, Type::Nullable(_))
                }
                Some(given) if given.by_place => {
                    self.fits_place(&given.node, &parameter.ty, widening)
                }
                Some(given) => {
                    self.admits(&parameter.ty, &given.value.ty)
                        || widening
                            && is_integer_literal(&given.value)
                            && accepts_float(&parameter.ty)
                }
            };
            if !admitted {
                return Admits::Misfit;
            }
        }
        // GEN-6: "A candidate states in `requires` the values of a
        // const option it is for ... A call admits the candidate only
        // where that holds for the option's value at the call, a
        // default included."
        let Some(clause) = child(&candidate.syntax, SyntaxKind::RequiresClause) else {
            return Admits::Yes;
        };
        let Some(condition) = clause.children().next() else {
            return Admits::Yes;
        };
        // GEN-16: "a named operator requirement tests callability with
        // the supplied types." Each one the condition states is tested
        // here, before the rest of the condition is read.
        let callable = self.requirements_hold(
            candidate,
            &signature.generics,
            (module, generics),
            &condition,
        );
        let callable = |node: &SyntaxNode| -> Option<bool> {
            let start = node.span().start;
            callable
                .iter()
                .find(|(at, _)| *at == start)
                .and_then(|(_, verdict)| *verdict)
        };
        // The value an option has at this call: what the call gives
        // it, or its default, the candidate's and then, for one of the
        // operator's own parameters, the operator's. `None` when it is
        // no constant this pass reads. Each is a value of the option's
        // type (EXPR-5): what `instance` resolved for a generic value
        // parameter, and what is given or defaulted for a parameter.
        let this = &*self;
        let option_type = |name: &str| -> Option<Type> {
            if let Some(generic) = signature.generics.iter().position(|generic| {
                matches!(generic.kind, GenericKind::Value(_)) && generic.name == name
            }) {
                return match &signature.generics[generic].kind {
                    GenericKind::Value(ty) => Some(ty.clone()),
                    _ => None,
                };
            }
            signature
                .parameters
                .iter()
                .find(|parameter| parameter.name == name)
                .map(|parameter| parameter.ty.clone())
        };
        let value_of = |name: &str| -> Option<ConstantValue> {
            // An option that selects: a generic value parameter.
            let generic = signature.generics.iter().position(|generic| {
                matches!(generic.kind, GenericKind::Value(_)) && generic.name == name
            });
            if let Some(generic) = generic {
                return match generics.get(generic) {
                    Some(Argument::Constant(constant)) => Some(constant.value.clone()),
                    _ => None,
                };
            }
            let index = signature
                .parameters
                .iter()
                .position(|parameter| parameter.name == name)?;
            let ty = &signature.parameters[index].ty;
            let argument = bound
                .iter()
                .position(|&place| place == Place::Parameter(index));
            if let Some(position) = argument {
                return this.read_constant(&given[position].node, Some(ty)).ok();
            }
            // A default and the file it is written in: the candidate's,
            // or else the operator's.
            let default = default_at(&candidate.syntax, &signature.parameters[index])
                .map(|default| (candidate.at.file, default))
                .or_else(|| {
                    public
                        .parameters
                        .get(index)
                        .and_then(|parameter| default_at(&operator.syntax, parameter))
                        .map(|default| (operator.at.file, default))
                });
            match default {
                Some((file, default)) => this.read_in(file, &default, Some(ty)).ok(),
                // Written `T?` and left out: it is null.
                None => Some(ConstantValue::Null),
            }
        };
        // What a constant the condition compares an option with comes
        // to, read where the candidate is written.
        let constant = |node: &SyntaxNode, name: &str| -> Option<ConstantValue> {
            let ty = option_type(name)?;
            this.read_in(candidate.at.file, node, Some(&ty)).ok()
        };
        // GEN-3: "`requires` may use type/category membership,
        // equalities ...". What a generic parameter of the candidate
        // stands for at this call, and what a type written in the
        // condition means with those in place.
        let own: Vec<Location> = signature
            .generics
            .iter()
            .map(|generic| generic.at)
            .collect();
        let map: Vec<(Location, &Argument)> = own.iter().copied().zip(generics).collect();
        let type_of = |name: &str| -> Option<Type> {
            let index = signature
                .generics
                .iter()
                .position(|generic| generic.name == name)?;
            match generics.get(index)? {
                Argument::Type(ty) => Some(ty.clone()),
                Argument::Constant(_) => None,
            }
        };
        let declared_in = &self.program.modules[module];
        let file = declared_in.files[candidate.file].file;
        let written = |node: &SyntaxNode| -> Option<Type> {
            let start = node.span().start;
            let read = declared_in
                .written
                .iter()
                .find(|written| written.at.file == file && written.at.span.start == start)?;
            let ty = placed(substitute_within(&read.ty, &map), &own);
            whole(&ty).then_some(ty)
        };
        let condition_of = Condition {
            value_of: &value_of,
            constant: &constant,
            type_of: &type_of,
            written: &written,
            callable: &callable,
        };
        match holds(&condition, &condition_of) {
            Some(true) => Admits::Yes,
            Some(false) => Admits::No,
            None => Admits::Unknown,
        }
    }

    /// True when the expression at `node`, which is read as the type of
    /// its place, is a value of `ty`. It is read as one, and what that
    /// reading reports is taken back: the caller asks, and reports for
    /// itself. Without `widening`, it is no value of `ty` when an
    /// integer literal among its entries had to be widened to be one:
    /// `[1]` is then a `list<i64>` and no `list<f64>`.
    ///
    /// The expression has been read once already, with nothing expected
    /// of it, so a reading here changes nothing but the diagnostics.
    fn fits_place(&mut self, node: &SyntaxNode, ty: &Type, widening: bool) -> bool {
        // Past the function's bound nothing more is tried, and the
        // caller leaves its call unjudged.
        if !self.may_reread() {
            return true;
        }
        let mark = self.mark();
        let value = self.expression(node, Some(ty));
        let value = self.fit(value, ty, "its place");
        let fits = self.take_back(mark, false);
        fits && (widening || !widens_an_entry(&value))
    }

    /// What a reading may change, as it stands now: see
    /// [`Typer::take_back`].
    fn mark(&self) -> Mark {
        Mark {
            diagnostics: self.diagnostics.len(),
            locals: self.locals.len(),
            assigned: self.assigned.clone(),
            returned: self.returned,
            unsure: self.unsure,
            present: self.present.len(),
        }
    }

    /// Takes back what has been read since `mark`: what it reported and
    /// the locals it declared, which the reading that stands declares
    /// again. Unless `keep_flow`, also what it assigned and whether it
    /// returned (CTRL-11, CTRL-5). True when it had reported nothing.
    ///
    /// A block is an expression (CTRL-10), so reading an expression may
    /// declare a local, assign one, or return. An expression read twice,
    /// first to learn something and then where it stands, must leave
    /// one reading's worth of each.
    fn take_back(&mut self, mark: Mark, keep_flow: bool) -> bool {
        let clean = self.diagnostics.len() == mark.diagnostics;
        self.diagnostics.truncate(mark.diagnostics);
        self.locals.truncate(mark.locals);
        self.declared.retain(|_, local| *local < mark.locals);
        // What was noted of a local that is taken back goes with it:
        // its number is given to the next local declared.
        self.aliases.retain(|local, _| *local < mark.locals);
        self.captures.retain(|local, _| *local < mark.locals);
        self.present.truncate(mark.present);
        if keep_flow {
            self.assigned.truncate(mark.locals);
        } else {
            self.assigned = mark.assigned;
            self.returned = mark.returned;
            self.unsure = mark.unsure;
        }
        clean
    }

    /// True when one more expression of the function may be read a
    /// second time, which this counts.
    fn may_reread(&mut self) -> bool {
        let may = self.rereads > 0;
        self.rereads = self.rereads.saturating_sub(1);
        may
    }

    /// `Name<arguments>(fields)`: the construction of a generic struct
    /// with its arguments written.
    fn typed_constructor(
        &mut self,
        node: &SyntaxNode,
        expected: Option<&Type>,
    ) -> (ExprKind, Type) {
        // `Name<...>(...)` or `alias.Name<...>(...)`: the name is the
        // last identifier, and the type is recorded where the whole of
        // it starts.
        let start = match child(node, SyntaxKind::QualifiedName) {
            Some(qualified) => Some(qualified.span().start),
            None => name_token(node).map(|name| name.span().start),
        };
        let (Some(name), Some(start), Some(list)) =
            (name_token(node), start, child(node, SyntaxKind::ArgList))
        else {
            return (ExprKind::Unread, Type::Unknown);
        };
        // ERR-2: `ok` and `err` each take "no generic argument", and
        // no other result form has one to take. The form is read as it
        // would be without them.
        if child(node, SyntaxKind::QualifiedName).is_none() && RESULT_FORMS.contains(&name.text()) {
            self.diagnostics.emit(
                error(
                    Category::Type,
                    codes::GENERIC_ARGUMENT_COUNT,
                    Location::new(self.file, name.span()),
                    format!("`{}` takes no generic argument", name.text()),
                    "ERR-2",
                )
                .with_note("a result's types come from the `result<T, E>` its place expects"),
            );
            if let Some(form) = self.result_form(&name, &list, expected) {
                return form;
            }
        }
        // The type the `read` pass gave `Name<arguments>`.
        let written = self.written.get(&(self.file, start)).copied();
        match written {
            Some(Type::Named {
                module,
                declaration,
                arguments,
                ..
            }) if self.program.modules[*module].declarations[*declaration].kind
                == DeclarationKind::Struct =>
            {
                self.construct(
                    &name,
                    &list,
                    *module,
                    *declaration,
                    Some(arguments.clone()),
                    true,
                )
            }
            // GEN-8: "A call may supply its generic arguments as a
            // generic constructor does, `count<i64>(3)`." The `read`
            // pass kept them, the name being a function or an operator.
            _ => {
                let generics = self.calls.get(&(self.file, start)).copied();
                // TIME-9: an endpoint operation written with its generic
                // argument, `delta_value<i64>(out)`, is the form it is
                // without one, and is held to the same rules.
                if self.hook.is_some()
                    && child(node, SyntaxKind::QualifiedName).is_none()
                    && let Some(operation) = EndpointOperation::language_form(name.text())
                    && self.is_the_languages(&name)
                {
                    let read = self.endpoint(operation, &name, &list);
                    self.endpoint_generics(&name, generics, &read.0);
                    return read;
                }
                match (generics, self.declarations(&name)) {
                    (Some(generics), Some(found)) => {
                        self.call_named(&name, &list, found, expected, Some(generics))
                    }
                    _ => self.unread_call(&list),
                }
            }
        }
    }

    /// The construction of the struct `declaration` of `module`. `given`
    /// is its generic arguments, when they are written or expected.
    ///
    /// TYPE-5: "An ordinary constructor supplies complete required
    /// data." TYPE-3: "abstract values are not constructible."
    fn construct(
        &mut self,
        name: &SyntaxToken,
        list: &SyntaxNode,
        module: usize,
        declaration: usize,
        given: Option<Vec<Argument>>,
        settled: bool,
    ) -> (ExprKind, Type) {
        let program = self.program;
        let declared = &program.modules[module].declarations[declaration];
        let Some(Detail::Struct(read)) = program.modules[module].details.get(declaration) else {
            return self.unread_call(list);
        };
        let before = self.diagnostics.len();
        if has_token(&declared.syntax, SyntaxKind::KwAbstract) {
            self.diagnostics.emit(
                error(
                    Category::Type,
                    codes::STRUCT_ABSTRACT,
                    Location::new(self.file, name.span()),
                    format!(
                        "`{}` is an abstract struct and is not constructed",
                        declared.name
                    ),
                    "TYPE-3",
                )
                .with_related(declared.at, "declared here"),
            );
        }
        // Arguments of the wrong number or kind have been reported
        // where they are written (GEN-2, GEN-14): the struct made is
        // then not known, and nothing more is reported of it (ERR-9).
        let reported = given
            .as_ref()
            .is_some_and(|given| !applies(&read.generics, given));
        let given = given.filter(|_| !reported);
        // With none given, a generic struct's arguments are found from
        // the fields it is given.
        let inferring = given.is_none() && !read.generics.is_empty();
        // The struct's own parameters. A field's type that still names
        // one once the arguments are in place has been given none: with
        // no arguments given, or where a pack's place is not filled in.
        // It is then not known. A parameter of the function being read
        // is a type like any other.
        let own: Vec<Location> = read.generics.iter().map(|generic| generic.at).collect();
        // Each field's type, with the generic arguments in place when
        // they are given.
        let map: Vec<(Location, &Argument)> = match &given {
            Some(given) => fixed(&read.generics).zip(given).collect(),
            None => Vec::new(),
        };
        let slots: Vec<Slot> = read
            .members
            .iter()
            .map(|member| {
                let ty = substitute_within(&member.ty, &map);
                Slot {
                    name: &member.name,
                    // TYPE-4: "A field is optional when its type is
                    // nullable ... or an `atomic<...>` of a nullable
                    // type", the generic arguments in place: the
                    // `value: T` of a `Box<i64?>` is.
                    required: !member.has_default && is_optional(&ty) != Some(true),
                    ty: placed(ty, &own),
                    pack: None,
                    listed: false,
                }
            })
            .collect();
        // With the arguments given, each field's type is known and the
        // values are read against it.
        let (arguments, arguments_of_type) = match given {
            Some(given) => (self.bind(name, list, &slots, Takes::Fields), given),
            None if !inferring => (self.bind(name, list, &slots, Takes::Fields), Vec::new()),
            // GEN-2: "infers them from constructor fields".
            None => {
                let places = Places {
                    slots: &slots,
                    patterns: read.members.iter().map(|member| &member.ty).collect(),
                    generics: &read.generics,
                    settled,
                    aside: &[],
                    values: &[],
                    // GEN-2: "a generic value parameter that nothing
                    // says anything of takes its default".
                    defaults: read
                        .generics
                        .iter()
                        .map(|generic| self.default_constant(declared, generic))
                        .collect(),
                };
                let found = vec![None; read.generics.len()];
                let Inferred {
                    arguments,
                    mut found,
                    defaulted,
                    ..
                } = self.infer_arguments(name, list, &places, found, Takes::Fields);
                // GEN-13: a default the construction takes is of its
                // parameter's type as the construction resolves it: the
                // `2` of `struct Hold<T, const fallback: T = 2>` is no
                // `str`.
                self.generic_values(
                    name,
                    declared,
                    &read.generics,
                    (&[], &defaulted),
                    &mut found,
                );
                let found: Vec<Argument> = found
                    .into_iter()
                    .map(|found| found.unwrap_or(Argument::Type(Type::Unknown)))
                    .collect();
                (arguments, found)
            }
        };
        let ty = if reported {
            Type::Unknown
        } else {
            Type::Named {
                name: declared.name.clone(),
                module,
                declaration,
                arguments: arguments_of_type,
                kind: NamedKind::Struct,
            }
        };
        // GEN-3: the struct's `requires` holds with its arguments. Not
        // where something else was reported of the construction
        // (ERR-9).
        if let Type::Named {
            arguments: applied, ..
        } = &ty
            && self.diagnostics.len() == before
        {
            let at = Location::new(self.file, Span::new(name.span().start, list.span().end));
            self.requires_met(at, (module, declaration), applied);
        }
        // TST-3: a trace "is stored in no field", bare or inside a
        // value given to one.
        let stored: Vec<Location> = arguments
            .iter()
            .filter(|argument| holds_trace(&argument.value.ty))
            .map(|argument| argument.value.at)
            .collect();
        for at in stored {
            self.trace_escape(at, Takes::Fields);
        }
        let kind = ExprKind::Construct {
            module,
            declaration,
            arguments,
        };
        (kind, ty)
    }

    /// Gives each argument of `list` to one of `slots`: the parameters
    /// of a function, or the fields of a struct, as `takes` says.
    ///
    /// EXPR-2: "Positional arguments precede named ones; reject
    /// duplicates, unknown names, missing required arguments, and wrong
    /// types. Named arguments use the selected public callable's
    /// parameter names." TYPE-5: a constructor's "arguments are named,
    /// each for a field the struct has, a parent's included, in any
    /// order; it takes no positional argument."
    ///
    /// The arguments come back in the order written, each with the slot
    /// it is given to: `None` for one given to a pack, or to nothing.
    /// Their values are not read here.
    fn assign(
        &mut self,
        callee: &SyntaxToken,
        list: &SyntaxNode,
        slots: &[Slot],
        takes: Takes,
        aside: &[&str],
    ) -> Vec<(Option<usize>, SyntaxNode)> {
        let what = takes.noun();
        let callee_at = Location::new(self.file, callee.span());
        let has_pack = |pack: Pack| slots.iter().any(|slot| slot.pack == Some(pack));
        // Which slots have been given a value.
        let mut filled = vec![false; slots.len()];
        let mut named_seen = false;
        // Whether a constructor was given an argument with no name. Which
        // field it was meant for is not known, so no field is then
        // reported as missing (ERR-9).
        let mut unnamed_field = false;
        let mut arguments = Vec::new();
        for (name, value) in self.call_arguments(list) {
            let at = self.at(&value);
            let slot = match &name {
                // TYPE-5: "it takes no positional argument."
                None if takes == Takes::Fields => {
                    unnamed_field = true;
                    self.diagnostics.emit(error(
                        Category::Type,
                        codes::ARGUMENT_UNEXPECTED,
                        at,
                        format!(
                            "a struct's fields are given by name: `{}(field: value)`",
                            callee.text()
                        ),
                        "TYPE-5",
                    ));
                    None
                }
                None => {
                    if named_seen {
                        self.diagnostics.emit(error(
                            Category::Type,
                            codes::ARGUMENT_ORDER,
                            at,
                            "an argument with no name follows a named one: positional \
                             arguments come first"
                                .to_string(),
                            "EXPR-2",
                        ));
                    }
                    // The first slot not yet filled that is no pack; a
                    // positional pack takes what the others leave
                    // (FN-8).
                    let next = (0..slots.len())
                        .find(|&index| !filled[index] && slots[index].pack.is_none());
                    if next.is_none() && !has_pack(Pack::Positional) {
                        let fixed = slots.iter().filter(|slot| slot.pack.is_none()).count();
                        self.diagnostics.emit(error(
                            Category::Type,
                            codes::ARGUMENT_UNEXPECTED,
                            at,
                            format!(
                                "`{}` has {fixed} {what}{}, and this is one more",
                                callee.text(),
                                if fixed == 1 { "" } else { "s" }
                            ),
                            "EXPR-2",
                        ));
                    }
                    next
                }
                Some(name) => {
                    named_seen = true;
                    // GEN-8: "A name binds the callee's generic value
                    // parameter of that name, or else its parameter of
                    // that name." One that does is no place's.
                    if aside.contains(&name.text()) {
                        continue;
                    }
                    let name_at = Location::new(self.file, name.span());
                    let found = slots
                        .iter()
                        .position(|slot| slot.pack.is_none() && slot.name == name.text());
                    match found {
                        Some(index) if filled[index] => {
                            self.diagnostics.emit(error(
                                Category::Name,
                                codes::ARGUMENT_DUPLICATE,
                                name_at,
                                format!("`{}` is given twice", name.text()),
                                "EXPR-2",
                            ));
                            None
                        }
                        Some(index) => Some(index),
                        // A named pack takes the names the others
                        // leave (FN-8).
                        None if has_pack(Pack::Named) => None,
                        None => {
                            self.diagnostics.emit(error(
                                Category::Name,
                                codes::ARGUMENT_UNKNOWN,
                                name_at,
                                format!("`{}` has no {what} `{}`", callee.text(), name.text()),
                                "EXPR-2",
                            ));
                            None
                        }
                    }
                }
            };
            if let Some(index) = slot {
                filled[index] = true;
            }
            arguments.push((slot, value));
        }
        for (index, slot) in slots.iter().enumerate() {
            if slot.required && slot.pack.is_none() && !filled[index] && !unnamed_field {
                self.diagnostics.emit(error(
                    Category::Type,
                    codes::ARGUMENT_MISSING,
                    callee_at,
                    format!(
                        "`{}` is given no `{}`, a {what} with no default",
                        callee.text(),
                        slot.name
                    ),
                    "EXPR-2",
                ));
            }
        }
        arguments
    }

    /// Gives each argument of `list` to one of `slots` and reads its
    /// value, which is of the slot's type.
    fn bind(
        &mut self,
        callee: &SyntaxToken,
        list: &SyntaxNode,
        slots: &[Slot],
        takes: Takes,
    ) -> Vec<hir::Argument> {
        self.assign(callee, list, slots, takes, &[])
            .into_iter()
            .map(|(slot, value)| {
                // No generic parameter here: a slot that is a trace is
                // one an eval gives (TST-5).
                let expected = slot.map(|index| {
                    let slot = &slots[index];
                    (slot, matches!(slot.ty, Type::Trace { .. }))
                });
                self.argument(slot, &value, expected, takes)
            })
            .collect()
    }

    /// The argument written at `node`, given to `slot`. One given to a
    /// pack, or to nothing, is read for what it reports itself. With
    /// the slot is whether it is a trace an eval gives (TST-5).
    ///
    /// A value that holds a trace, given to a place that takes none,
    /// is not held to the place's type: where it may go is for the
    /// call or the construction to report once it is built (TST-3).
    fn argument(
        &mut self,
        parameter: Option<usize>,
        node: &SyntaxNode,
        slot: Option<(&Slot, bool)>,
        takes: Takes,
    ) -> hir::Argument {
        let value = match slot {
            Some((slot, traced)) => {
                let value = self.expression(node, Some(&slot.ty));
                let value = if slot.listed {
                    publications(value)
                } else {
                    value
                };
                if !traced && holds_trace(&value.ty) {
                    value
                } else {
                    let place = format!("the {} `{}`", takes.noun(), slot.name);
                    if takes == Takes::Parameters {
                        self.fit_input(value, &slot.ty, &place)
                    } else {
                        self.fit(value, &slot.ty, &place)
                    }
                }
            }
            None => self.expression(node, Some(&Type::Unknown)),
        };
        hir::Argument { parameter, value }
    }

    /// Reports a trace given to a field, or to a parameter of a
    /// function that is no `const fn` (TST-3).
    fn trace_escape(&mut self, at: Location, takes: Takes) {
        let how = match takes {
            Takes::Fields => "it is stored in no field",
            Takes::Parameters => "it is a temporal argument only of an eval",
        };
        self.diagnostics.emit(
            error(
                Category::Type,
                codes::TRACE_ESCAPE,
                at,
                format!("this is a trace, a value of the test: {how}"),
                "TST-3",
            )
            .with_note("a trace is bound with `let`, compared, and given to a `const fn`"),
        );
    }

    // ---- Members and elements ----

    /// `base.member`: a member of an enum, or a field of a struct
    /// value. Any other base is for a later part: the payload of a
    /// nullable or a result needs its guard (ERR-3, ERR-12).
    fn member(&mut self, node: &SyntaxNode) -> (ExprKind, Type) {
        // The member's name is the last token: any word (LEX-9).
        let (Some(base), Some(name)) = (node.children().next(), tokens(node).last()) else {
            return (ExprKind::Unread, Type::Unknown);
        };
        let program = self.program;
        // `Side.buy`: the base names an enum.
        if base.kind() == SyntaxKind::NameExpr
            && let Some(enum_name) = name_token(&base)
            && let Some((module, declarations)) = self.declarations(&enum_name)
            && let Some(&declaration) = declarations.iter().find(|&&declaration| {
                program.modules[module].declarations[declaration].kind == DeclarationKind::Enum
            })
        {
            return self.enum_member(&name, module, declaration);
        }
        let base = self.expression(&base, None);
        // ERR-7: a capture is "read through two connections".
        let captured = match &base.kind {
            ExprKind::Local(local) => self.captures.get(local).cloned(),
            ExprKind::Capture(call) => Some(self.captured_output(call)),
            _ => None,
        };
        if let Some(output) = captured {
            return self.captured(base, &name, output);
        }
        // ERR-3 and ERR-4: the payload of a result, and a part of an
        // error.
        match &base.ty {
            Type::Result { value, error } => {
                let sides = ((**value).clone(), (**error).clone());
                return self.payload(base, &name, sides);
            }
            Type::Error => return self.error_part(base, &name),
            Type::ErrorContext => return self.context_part(base, &name),
            // ERR-12: a member of the payload of a nullable is read
            // where a test against null has proved it present. ERR-4:
            // "Nullable code/context observations require presence
            // checking".
            Type::Nullable(inner) if has_members(inner) => {
                self.unproved_payload(&base, &format!("`.{}`", name.text()));
                return (ExprKind::Unread, Type::Unknown);
            }
            // TYPE-16: a set's delta has "the properties `added` and
            // `removed`, each a `set<K>`".
            Type::Delta(of) => return self.delta_property(base.clone(), &name, of),
            _ => {}
        }
        // TYPE-9: "`atomic<V>` stops expansion at a complete ordinary
        // V": an `atomic<Quote>` holds whole quotes, and its field is
        // the field of each, itself an atomic of the field's type, as
        // complete as the quote it is read from.
        let (whole, atomic) = match &base.ty {
            Type::Atomic(payload) => (&**payload, true),
            other => (other, false),
        };
        let Type::Named {
            module,
            declaration,
            arguments,
            kind,
            ..
        } = whole
        else {
            return (ExprKind::Unread, Type::Unknown);
        };
        if atomic && *kind != NamedKind::Struct {
            return (ExprKind::Unread, Type::Unknown);
        }
        // NATIVE-6: "`value.property` reads one as an ordinary value."
        if let Some(Detail::Native(native)) = program.modules[*module].details.get(*declaration) {
            let Some(member) = native
                .properties
                .iter()
                .position(|property| property.name == name.text())
            else {
                self.diagnostics.emit(error(
                    Category::Name,
                    codes::MEMBER_UNKNOWN,
                    Location::new(self.file, name.span()),
                    format!("`{}` has no property `{}`", base.ty, name.text()),
                    "NATIVE-6",
                ));
                return (ExprKind::Unread, Type::Unknown);
            };
            let ty = native.properties[member].ty.clone();
            let kind = ExprKind::Field {
                base: Box::new(base),
                member,
            };
            return (kind, ty);
        }
        let Some(Detail::Struct(read)) = program.modules[*module].details.get(*declaration) else {
            return (ExprKind::Unread, Type::Unknown);
        };
        let Some(member) = read
            .members
            .iter()
            .position(|member| member.name == name.text())
        else {
            self.diagnostics.emit(error(
                Category::Name,
                codes::MEMBER_UNKNOWN,
                Location::new(self.file, name.span()),
                format!("`{}` has no field `{}`", base.ty, name.text()),
                "EXPR-7",
            ));
            return (ExprKind::Unread, Type::Unknown);
        };
        // The field's type, with the value's generic arguments in
        // place. Where a pack's place is not filled in, it is not
        // known.
        let map: Vec<(Location, &Argument)> = fixed(&read.generics).zip(arguments).collect();
        let ty = substitute_within(&read.members[member].ty, &map);
        let own: Vec<Location> = read.generics.iter().map(|generic| generic.at).collect();
        let ty = placed(ty, &own);
        let ty = if atomic { Type::atomic(ty) } else { ty };
        let kind = ExprKind::Field {
            base: Box::new(base),
            member,
        };
        (kind, ty)
    }

    /// The member `name` of the enum `declaration` of `module`.
    fn enum_member(
        &mut self,
        name: &SyntaxToken,
        module: usize,
        declaration: usize,
    ) -> (ExprKind, Type) {
        let program = self.program;
        let declared = &program.modules[module].declarations[declaration];
        let ty = Type::Named {
            name: declared.name.clone(),
            module,
            declaration,
            arguments: Vec::new(),
            kind: NamedKind::Enum,
        };
        let member = match program.modules[module].details.get(declaration) {
            Some(Detail::Enum(read)) => read
                .members
                .iter()
                .position(|member| member.name == name.text()),
            _ => None,
        };
        match member {
            Some(member) => {
                let kind = ExprKind::EnumMember {
                    module,
                    declaration,
                    member,
                };
                (kind, ty)
            }
            None => {
                self.diagnostics.emit(
                    error(
                        Category::Name,
                        codes::MEMBER_UNKNOWN,
                        Location::new(self.file, name.span()),
                        format!("`{}` has no member `{}`", declared.name, name.text()),
                        "TYPE-7",
                    )
                    .with_related(declared.at, "declared here"),
                );
                // It is of the enum still: one report.
                (ExprKind::Unread, ty)
            }
        }
    }

    /// `.member` (TYPE-7): "names a member of the enum type its
    /// position expects and rejects where none is."
    fn implied_member(&mut self, node: &SyntaxNode, expected: Option<&Type>) -> (ExprKind, Type) {
        let Some(name) = tokens(node).last() else {
            return (ExprKind::Unread, Type::Unknown);
        };
        match not_null(expected) {
            Some(Type::Named {
                module,
                declaration,
                ..
            }) if self.program.modules[*module].declarations[*declaration].kind
                == DeclarationKind::Enum =>
            {
                self.enum_member(&name, *module, *declaration)
            }
            // What the place expects is not known.
            Some(expected) if open(expected) => (ExprKind::Unread, Type::Unknown),
            expected => {
                let place = match expected {
                    Some(expected) => format!("a `{expected}` is expected here"),
                    None => "nothing here says which".to_string(),
                };
                self.diagnostics.emit(error(
                    Category::Type,
                    codes::ENUM_CONTEXT,
                    self.at(node),
                    format!(
                        "`.{}` is a member of the enum its place expects, and {place}",
                        name.text()
                    ),
                    "TYPE-7",
                ));
                (ExprKind::Unread, Type::Unknown)
            }
        }
    }

    /// Reports `base`, a nullable value, read for `what` of its payload
    /// with no test against null that proves it present (ERR-12: "Using
    /// it where T is required without that proof rejects with
    /// `nullable.required_presence`"). What is read is then not known,
    /// and nothing more is reported of it (ERR-9).
    fn unproved_payload(&mut self, base: &Expr, what: &str) {
        // ERR-4: "Nullable code/context observations require presence
        // checking".
        let rule = match &base.ty {
            Type::Nullable(inner) if **inner == Type::ErrorContext => "ERR-4",
            _ => "ERR-12",
        };
        self.diagnostics.emit(
            error(
                Category::Type,
                codes::NULLABLE_REQUIRED_PRESENCE,
                base.at,
                format!(
                    "this is `{}`, and {what} of it is read where it may be null",
                    base.ty
                ),
                rule,
            )
            .with_note(
                "bind it with `let` and test it against null: `if value == null { return ... }`",
            ),
        );
    }

    /// `base[index]`: an element of a list or a tuple, or the value a
    /// dictionary has for a key (EXPR-7; ERR-6: "Ordinary/atomic
    /// dictionary brackets ... are strict point-in-time reads"). Any
    /// other base is for a later part.
    fn index(&mut self, node: &SyntaxNode) -> (ExprKind, Type) {
        let mut parts = node.children();
        let (Some(base), Some(index)) = (parts.next(), parts.next()) else {
            return (ExprKind::Unread, Type::Unknown);
        };
        let base = self.expression(&base, None);
        let (index, ty) = match &base.ty {
            Type::List { element, .. } => {
                let integer = Type::Scalar(Scalar::I64);
                let index = self.expression(&index, Some(&integer));
                (
                    self.fit(index, &integer, "the index of a list"),
                    (**element).clone(),
                )
            }
            // EXPR-7: "A tuple's index is a constant within its length:
            // it selects the element's type."
            Type::Tuple(elements) => {
                let integer = Type::Scalar(Scalar::I64);
                // UNIT-13: an index that names a constant of a module is
                // that constant's value: the index is read as every
                // constant is (`crate::constant`).
                let found = self.read_constant(&index, Some(&integer));
                let index = self.expression(&index, Some(&integer));
                // EXPR-7: "by an `i64` index". An index of another type,
                // `pair[0 / 1]` or `pair[0 == 0]`, is reported as that,
                // and is not evaluated.
                let is_integer = self.admits(&integer, &index.ty);
                let index = self.fit(index, &integer, "the index of a tuple");
                let element = match found {
                    _ if !is_integer => Some(&Type::Unknown),
                    Ok(ConstantValue::Int(value)) => usize::try_from(value)
                        .ok()
                        .and_then(|position| elements.get(position)),
                    // No compile-time constant: a local, a parameter, a
                    // call.
                    Err(Fault::NotConstant) => None,
                    // A constant whose value is not known here, a
                    // generic value parameter's or one this pass does
                    // not work out, or one reported: the element's type
                    // is then not known, and nothing is reported of it.
                    _ => Some(&Type::Unknown),
                };
                let ty = match element {
                    Some(element) => element.clone(),
                    None => {
                        self.diagnostics.emit(error(
                            Category::Type,
                            codes::TUPLE_INDEX,
                            index.at,
                            format!(
                                "a tuple's index is a constant, here one of 0 to {}: it \
                                 selects the element's type",
                                elements.len().saturating_sub(1)
                            ),
                            "EXPR-7",
                        ));
                        Type::Unknown
                    }
                };
                (index, ty)
            }
            Type::Dict { key, value } => {
                let index = self.expression(&index, Some(key));
                (
                    self.fit(index, key, "the key of the dictionary"),
                    (**value).clone(),
                )
            }
            // EXPR-7: "Brackets read an element of a list or a tuple by
            // an `i64` index, and the value a dictionary has for a
            // key." EXPR-3: "A set has no place to assign."
            Type::Set(_) => {
                self.diagnostics.emit(error(
                    Category::Type,
                    codes::TYPE_MISMATCH,
                    base.at,
                    format!(
                        "this is `{}`, and a set has no element at an index: brackets read a \
                         list, a tuple, or a dictionary",
                        base.ty
                    ),
                    "EXPR-7",
                ));
                (self.expression(&index, Some(&Type::Unknown)), Type::Unknown)
            }
            // ERR-12: an element of the payload of a nullable is read
            // where a test against null has proved it present.
            Type::Nullable(inner)
                if matches!(
                    **inner,
                    Type::List { .. } | Type::Tuple(_) | Type::Dict { .. }
                ) =>
            {
                self.unproved_payload(&base, "an element");
                (self.expression(&index, Some(&Type::Unknown)), Type::Unknown)
            }
            _ => (self.expression(&index, Some(&Type::Unknown)), Type::Unknown),
        };
        let kind = ExprKind::Index {
            base: Box::new(base),
            index: Box::new(index),
        };
        (kind, ty)
    }

    // ---- What a place expects ----

    /// The expression `value` in a place that expects `expected`: as it
    /// is when its type is the one expected, widened when it is an
    /// `i64` where an `f64` is expected (EXPR-5: "Numeric widening
    /// follows the admitted destination relationship"), and reported
    /// otherwise. `what` names the place: `the function's result`.
    fn fit(&mut self, value: Expr, expected: &Type, what: &str) -> Expr {
        // ERR-14: "A read of a collection's child that is checked when
        // it runs (TIME-1), `parts[i]`, takes its form from its place
        // (EXPR-5). Where the place expects `result<T, error>`, T the
        // child's type, it gives `ok` of the child's value, or, for a
        // child that is not valid, `err`".
        if let ExprKind::CheckedChild { result: false, .. } = &value.kind
            && let Type::Result {
                value: success,
                error,
            } = expected
            && **error == Type::Error
            && self.admits(success, &value.ty)
        {
            let at = value.at;
            let ExprKind::CheckedChild { read, .. } = value.kind else {
                return value;
            };
            return Expr {
                kind: ExprKind::CheckedChild { read, result: true },
                ty: expected.clone(),
                at,
            };
        }
        if is_integer_literal(&value) && accepts_float(expected) {
            let at = value.at;
            return Expr {
                kind: ExprKind::Widen(Box::new(value)),
                ty: Type::Scalar(Scalar::F64),
                at,
            };
        }
        self.conforms(&value.ty, expected, value.at, what);
        value
    }

    /// [`Self::fit`], for a function's argument or a graph's result. In
    /// composition a connection is taken there across a reference
    /// boundary, and by a `signal` (`references::crosses`): TYPE-12,
    /// "`ref<T>` designates an endpoint"; TIME-10, "Ordinary followers
    /// observe the selected target"; TYPE-12, "`signal` is a complete
    /// temporal input constraint with no readable payload".
    fn fit_input(&mut self, value: Expr, expected: &Type, what: &str) -> Expr {
        if self.composes(&value)
            && crate::references::crosses(expected, &value.ty, &|expected, ty| {
                self.admits(expected, ty)
            })
        {
            return value;
        }
        self.fit(value, expected, what)
    }

    /// The type `value`, given to a place declared `pattern`, says the
    /// generic parameters in `pattern` stand for. A connection given to
    /// a function's parameter in composition says it across a reference
    /// boundary (GEN-11, `references::inferred_from`): "Argument
    /// inference removes reference boundaries recursively from an
    /// otherwise unbound generic; such an input observes values. An
    /// explicit ref pattern binds its underlying type."
    fn inferred_from(&self, pattern: &Type, value: &Expr, takes: Takes) -> Type {
        if takes == Takes::Parameters && self.composes(value) {
            crate::references::inferred_from(pattern, &value.ty)
        } else {
            value.ty.clone()
        }
    }

    /// Reports a reference given to a value function outside
    /// composition: in a hook, where a `ref<T>` input is read as the
    /// designation it is. TYPE-12: "`ref<T>` designates an endpoint, not
    /// an ordinary payload type"; FN-1: a value function's parameters
    /// are ordinary value types, so one written as a generic parameter
    /// takes none. In a graph a connection given to one is lifted, and
    /// followed (TIME-10). What an endpoint operation is given is the
    /// input's endpoint, and no payload.
    fn references_given(&mut self, selected: &Selected) {
        let &Selected {
            module,
            declaration,
            arguments,
            ..
        } = selected;
        let role = self.program.modules[module].declarations[declaration]
            .kind
            .role();
        if role != Some(Role::Value) {
            return;
        }
        let given: Vec<(Location, String)> = arguments
            .iter()
            .filter(|argument| {
                !matches!(argument.value.ty, Type::Endpoint(_))
                    && !self.composes(&argument.value)
                    && crate::references::holds_ref(&argument.value.ty)
            })
            .map(|argument| (argument.value.at, argument.value.ty.to_string()))
            .collect();
        for (at, ty) in given {
            self.diagnostics.emit(
                error(
                    Category::Type,
                    codes::TYPE_NOT_ORDINARY,
                    at,
                    format!(
                        "`{ty}` is not an ordinary value type, and a value function's parameter \
                         is one"
                    ),
                    "TYPE-12",
                )
                .with_note("a `ref` designates an endpoint: a node returns it, or gives it to an endpoint operation"),
            );
        }
    }

    /// Reports a read of a `signal` input's payload in a hook. TYPE-12:
    /// "`signal` is a complete temporal input constraint with no
    /// readable payload": it is given to an endpoint operation, as
    /// `modified(tick)`, and read nowhere. A read in `start` is
    /// `input.lifecycle`, reported once.
    fn signal_read(&mut self, local: LocalId, at: Location) {
        let signal = matches!(self.locals[local].ty.omission_removed(), Type::Signal);
        let hook = matches!(self.hook, Some(Hook::When | Hook::Stop));
        if !signal || !self.is_temporal_input(local) || !hook {
            return;
        }
        self.diagnostics.emit(
            error(
                Category::Type,
                codes::TYPE_NOT_ORDINARY,
                at,
                format!(
                    "`{}` is a `signal`, which has no payload to read",
                    self.locals[local].name
                ),
                "TYPE-12",
            )
            .with_note(format!(
                "a signal is given to an endpoint operation: `modified({})`",
                self.locals[local].name
            )),
        );
    }

    /// True when `value` is a connection of a graph's body: what a
    /// temporal input or a graph's result is given in composition.
    fn composes(&self, value: &Expr) -> bool {
        self.graph
            && !self.testing
            && self.hook.is_none()
            && self.category(value) == ValueCategory::Connection
    }

    /// Reports a value of type `ty` in a place that expects `expected`
    /// when it is no value of that type.
    fn conforms(&mut self, ty: &Type, expected: &Type, at: Location, what: &str) {
        if self.admits(expected, ty) {
            return;
        }
        // ERR-12: "Using it where T is required without that proof
        // rejects with `nullable.required_presence`."
        if let Type::Nullable(payload) = ty
            && self.admits(expected, payload)
        {
            self.diagnostics.emit(
                error(
                    Category::Type,
                    codes::NULLABLE_REQUIRED_PRESENCE,
                    at,
                    format!("this is `{ty}`, and {what} is `{expected}`: it may be null here"),
                    "ERR-12",
                )
                .with_note(
                    "a test against null proves an immutable binding is not: \
                     `if value == null { return ... }`",
                ),
            );
            return;
        }
        self.diagnostics.emit(error(
            Category::Type,
            codes::TYPE_MISMATCH,
            at,
            format!("this is `{ty}`, and {what} is `{expected}`"),
            "EXPR-5",
        ));
    }

    /// True when an argument of type `later` widens a generic parameter
    /// that an earlier argument decided is `decided`.
    ///
    /// GEN-2: "One later argument changes what an earlier argument
    /// decided: an abstract struct that the type decided inherits from
    /// widens the parameter to that struct, so the order of two such
    /// arguments does not matter."
    fn widens(&self, decided: &Type, later: &Type) -> bool {
        let Type::Named {
            module,
            declaration,
            ..
        } = later
        else {
            return false;
        };
        let declared = &self.program.modules[*module].declarations[*declaration];
        declared.kind == DeclarationKind::Struct
            && has_token(&declared.syntax, SyntaxKind::KwAbstract)
            && whole(decided)
            && !agree(later, decided)
            && self.inherits(decided, later)
    }

    /// True when a value of type `ty` is a value of `expected`: the same
    /// type as far as each is known; a `T` where a `T?` is expected
    /// (TYPE-14: "either a T or null"); or a struct where an abstract
    /// struct it inherits from is expected.
    fn admits(&self, expected: &Type, ty: &Type) -> bool {
        if open(expected) || open(ty) || agree(expected, ty) {
            return true;
        }
        match expected {
            Type::Nullable(inner) => self.admits(inner, ty),
            Type::Named { .. } => self.inherits(ty, expected),
            // TYPE-16: "A `delta<V>` is a value where a `delta<V?>` is
            // expected, as a T is where a `T?` is".
            Type::Delta(of) => match &**of {
                Type::Nullable(payload) => self.admits(&Type::delta((**payload).clone()), ty),
                _ => false,
            },
            _ => false,
        }
    }

    /// True when the struct type `ty` inherits from `expected`, through
    /// any number of parents, each with the generic arguments the
    /// struct below gives it.
    ///
    /// TYPE-6: "A field whose type is an abstract struct holds every
    /// struct that inherits from it": a value of a struct is a value of
    /// the abstract structs it inherits from. TYPE-3: "Generic
    /// applications are invariant", so a `Child : Base<i64>` is no
    /// `Base<f64>`.
    fn inherits(&self, ty: &Type, expected: &Type) -> bool {
        // The parents still to look at.
        let mut to_do = vec![ty.clone()];
        // The applications of each struct whose parents have been
        // looked at. A struct reached by two lines of parents is looked
        // at once for each application, so that a ladder of diamonds
        // is walked in step with its length.
        let mut followed: HashMap<(usize, usize), Vec<Vec<Argument>>> = HashMap::new();
        // Structs that inherit from one another have been reported;
        // the count ends the walk for them.
        let mut budget = 100_000;
        while let Some(current) = to_do.pop() {
            let Type::Named {
                module,
                declaration,
                arguments,
                ..
            } = &current
            else {
                continue;
            };
            let Some(Detail::Struct(read)) =
                self.program.modules[*module].details.get(*declaration)
            else {
                continue;
            };
            let applications = followed.entry((*module, *declaration)).or_default();
            if applications.contains(arguments) {
                continue;
            }
            applications.push(arguments.clone());
            // Each generic parameter before a type pack, with the
            // argument it is given.
            let map: Vec<(Location, &Argument)> = read
                .generics
                .iter()
                .take_while(|generic| !matches!(generic.kind, GenericKind::Pack { .. }))
                .map(|generic| generic.at)
                .zip(arguments)
                .collect();
            // The struct's own parameters: one a parent still names
            // once the arguments are in place has been given none here.
            let own: Vec<Location> = read.generics.iter().map(|generic| generic.at).collect();
            for parent in &read.parents {
                let parent = substitute_within(parent, &map);
                // Where the struct passes a type pack on to its parent,
                // the pack is not put in place here: the parent may be
                // the application expected, and nothing is reported of
                // it (ERR-9). A parameter of the function being read
                // is a type like any other, and is compared.
                let may_be = names_one_of(&parent, &own) && same_declaration(expected, &parent);
                if may_be || agree(expected, &parent) {
                    return true;
                }
                if budget == 0 {
                    return false;
                }
                budget -= 1;
                to_do.push(parent);
            }
        }
        false
    }
}

/// The place `place` with its root, a local, of the type the local is
/// declared with, and not of the type a hook reads it as: an atomic's
/// V (TYPE-9), or the type under a `?` that marks omission (TYPE-14).
fn root_as_declared(place: &mut Expr, locals: &[Local]) {
    if let ExprKind::Local(local) = place.kind {
        place.ty = locals[local].ty.clone();
        return;
    }
    if let ExprKind::Field { base, .. } | ExprKind::Index { base, .. } = &mut place.kind {
        root_as_declared(base, locals);
    }
}

/// `bool`.
const BOOL: Type = Type::Scalar(Scalar::Bool);

/// True when an operator's contract is not applied to an operand of
/// this type here: it is not known, or it is nullable, and its payload
/// needs its guard first. A generic parameter is judged: it has the
/// operations its declaration's requirements state (GEN-7), and those
/// of a candidate that takes any type.
fn unjudged_operand(ty: &Type) -> bool {
    open(ty) || matches!(ty, Type::Nullable(_))
}

/// What a reading may change, at one moment: see [`Typer::take_back`].
struct Mark {
    /// How many diagnostics there are.
    diagnostics: usize,
    /// How many locals there are.
    locals: usize,
    /// What is assigned.
    assigned: Vec<bool>,
    /// Whether every path has returned.
    returned: bool,
    /// Whether a statement not read has been met.
    unsure: bool,
    /// How many bindings are proved not null.
    present: usize,
}

/// A value candidate of an operator: the positions of its module and of
/// its declaration there, and its signature.
type Candidate<'a> = (usize, usize, &'a Signature);

/// The names a call of an operator may give beside the operator's own
/// parameters: those its candidates declare as generic value
/// parameters, which select among them (GEN-6), and those with every
/// other option they declare.
fn options_of<'a>(
    public: &Signature,
    candidates: &[Candidate<'a>],
) -> (
    std::collections::HashSet<&'a str>,
    std::collections::HashSet<&'a str>,
) {
    let own = public.parameters.len();
    let selecting: std::collections::HashSet<&str> = candidates
        .iter()
        .flat_map(|(_, _, signature)| &signature.generics)
        .filter(|generic| matches!(generic.kind, GenericKind::Value(_)))
        .map(|generic| generic.name.as_str())
        .collect();
    let options = candidates
        .iter()
        .flat_map(|(_, _, signature)| signature.parameters.iter().skip(own))
        .map(|parameter| parameter.name.as_str())
        .chain(selecting.iter().copied())
        .collect();
    (selecting, options)
}

/// What the `.value` of a capture is (ERR-7).
#[derive(Debug, Clone)]
enum CapturedOutput {
    /// The call it covers has no output: there is no `.value`.
    None,
    /// The call's own output, of this type.
    Of(Type),
}

/// What a test has proved of a binding where the pass is reading.
#[derive(Debug, Clone, PartialEq)]
enum Fact {
    /// It is not null (ERR-12).
    Present(LocalId),
    /// It is valid: a temporal input whose payload may be read
    /// (TIME-4).
    Valid(LocalId),
    /// It is a success: a result whose `.value` may be read (ERR-3).
    Success(LocalId),
    /// It is a failure: a result whose `.error` may be read (ERR-3).
    Failure(LocalId),
    /// Every child of this collection is valid, one level deep:
    /// `all_valid(parts)`, which "admits every child" (TIME-1).
    AllValid(children::ChildPath),
    /// This child of a collection is valid: `valid(parts[1])`, which
    /// admits "that one" (TIME-1).
    ChildValid(children::ChildPath),
}

/// What [`null_tests`] asks of the function a condition is read in.
struct Proofs<'p> {
    /// Which bindings a null test proves anything of.
    refinable: &'p dyn Fn(LocalId) -> bool,
    /// Which bindings a tag test proves anything of, given what is
    /// known.
    tagged: &'p dyn Fn(LocalId, &[Fact]) -> bool,
    /// Every temporal input, for a `valid()` written with no argument.
    inputs: &'p dyn Fn() -> Vec<LocalId>,
    /// The child an endpoint operation's argument names when the
    /// program is compiled (TIME-1).
    children: &'p dyn Fn(&Expr) -> Option<children::ChildPath>,
}

/// What a condition proves: where it holds, and where it does not.
/// `refinable` says which bindings a null test proves anything of, and
/// `inputs` gives every temporal input, for a `valid()` written with no
/// argument.
///
/// ERR-12: a binding "tested directly against null": `value != null`,
/// `value == null`, with the `null` on either side. TIME-4: "A guard is
/// a test of `valid(input)`, on the input by its name". ERR-3: "Track
/// negation, short-circuit paths". A test of anything else, a field or
/// a call, proves nothing.
fn null_tests(condition: &Expr, proofs: &Proofs, assumed: &[Fact]) -> (Vec<Fact>, Vec<Fact>) {
    let Proofs {
        refinable,
        tagged,
        inputs,
        children,
    } = proofs;
    let both = |first: Vec<Fact>, second: Vec<Fact>| -> Vec<Fact> {
        first
            .into_iter()
            .filter(|fact| second.contains(fact))
            .collect()
    };
    let either = |mut first: Vec<Fact>, second: Vec<Fact>| -> Vec<Fact> {
        first.extend(second);
        first
    };
    match &condition.kind {
        // `valid(value)`, `valid([a, b])`, `valid()`: every endpoint
        // named is valid where it holds. Where it does not, one of them
        // is not, and nothing is proved of any.
        // TIME-9: `all_valid` is true "where every one is" valid, and
        // so the collection with them, as `valid` proves it; and
        // TIME-1: "`all_valid(parts)` admits every child".
        ExprKind::Endpoint {
            operation: operation @ (EndpointOperation::Valid | EndpointOperation::AllValid),
            inputs: named,
        } => {
            let valid = match named {
                Some(named) => named.clone(),
                None => inputs(),
            };
            let mut facts: Vec<Fact> = valid.iter().copied().map(Fact::Valid).collect();
            if *operation == EndpointOperation::AllValid {
                facts.extend(valid.into_iter().map(children::all_valid_fact));
            }
            (facts, Vec::new())
        }
        // TIME-4: "A child's guard is a test of `all_valid` of its
        // input, or of `valid` of the child as the read names it,
        // `valid(parts[1])` for `parts[1]`". One known only when the
        // program runs proves nothing here.
        ExprKind::ChildEndpoint {
            operation: operation @ (EndpointOperation::Valid | EndpointOperation::AllValid),
            child,
        } => match children(child) {
            Some(path) => (children::child_facts(*operation, path), Vec::new()),
            None => (Vec::new(), Vec::new()),
        },
        // ERR-3: a binding "gains its tag fact from a direct
        // `is_ok`/`is_err` guard on it by its name: where `is_ok` holds
        // it is a success and where it does not a failure, and `is_err`
        // the reverse."
        ExprKind::TagTest { success, value } => {
            let local = match value.kind {
                ExprKind::Local(local) if tagged(local, assumed) => local,
                _ => return (Vec::new(), Vec::new()),
            };
            let (success_where, failure_where) =
                (vec![Fact::Success(local)], vec![Fact::Failure(local)]);
            if *success {
                (success_where, failure_where)
            } else {
                (failure_where, success_where)
            }
        }
        ExprKind::Unary {
            operator: UnaryOperator::Not,
            operand,
        } => {
            let (when_true, when_false) = null_tests(operand, proofs, assumed);
            (when_false, when_true)
        }
        ExprKind::Binary {
            operator,
            left,
            right,
        } => match operator {
            BinaryOperator::Equal | BinaryOperator::NotEqual => {
                let local = match (&left.kind, &right.kind) {
                    (ExprKind::Local(local), ExprKind::Literal(Value::Null))
                    | (ExprKind::Literal(Value::Null), ExprKind::Local(local)) => *local,
                    _ => return (Vec::new(), Vec::new()),
                };
                if !refinable(local) {
                    return (Vec::new(), Vec::new());
                }
                match operator {
                    BinaryOperator::NotEqual => (vec![Fact::Present(local)], Vec::new()),
                    _ => (Vec::new(), vec![Fact::Present(local)]),
                }
            }
            // Both hold where `&&` does; one or other fails where it
            // does not.
            BinaryOperator::And => {
                // The right operand is evaluated where the left
                // holds: what the left proves is known there.
                let (left_true, left_false) = null_tests(left, proofs, assumed);
                let known = [assumed, left_true.as_slice()].concat();
                let (right_true, right_false) = null_tests(right, proofs, &known);
                (either(left_true, right_true), both(left_false, right_false))
            }
            BinaryOperator::Or => {
                // The right operand is evaluated where the left does
                // not hold.
                let (left_true, left_false) = null_tests(left, proofs, assumed);
                let known = [assumed, left_false.as_slice()].concat();
                let (right_true, right_false) = null_tests(right, proofs, &known);
                (both(left_true, right_true), either(left_false, right_false))
            }
            _ => (Vec::new(), Vec::new()),
        },
        _ => (Vec::new(), Vec::new()),
    }
}

/// True for a link of a chain of binary operators that is not read:
/// `&&` or `||` with configuration.
fn unread_link(node: &SyntaxNode) -> bool {
    child(node, SyntaxKind::OperatorConfig).is_some()
        && tokens(node)
            .find_map(|token| binary_operator(token.kind()))
            .is_some_and(|operator| matches!(operator, BinaryOperator::And | BinaryOperator::Or))
}

/// A symbol being read as the call of the operator it stands for.
struct Symbol {
    /// How the call is written.
    form: Form,
    /// The symbol: `+`.
    text: &'static str,
    /// Where it is written.
    at: Location,
    /// The rule that says what the symbol admits.
    rule: &'static str,
    /// The type of the expression where no candidate is selected.
    unjudged: Type,
    /// The type the symbol's place expects, when it is known: the
    /// output requested of the candidates (GEN-5).
    place: Option<Type>,
}

/// A requirement as it is read at a call ([`Typer::requirements_read`]):
/// the operator it names, the types it is supplied with, each with
/// where it is written, and the type its arrow states.
type Read = (OperatorRef, Vec<(SyntaxNode, Type)>, Option<Type>);

/// What answers a requirement: see [`Typer::answered`].
enum Answered {
    /// Nothing: no one value candidate takes the types supplied, or
    /// gives what the arrow states.
    Unmet,
    /// A requirement the enclosing declaration states (GEN-7): its
    /// position among that declaration's requirements.
    Stated(usize),
    /// The one most-specific value candidate that takes the types
    /// supplied (GEN-5).
    Selected {
        /// The position of its module in `Program::modules`.
        module: usize,
        /// Its position in that module's `declarations`.
        declaration: usize,
        /// What each of its generic parameters stands for.
        generics: Vec<Argument>,
    },
}

/// A named operator requirement of a declaration (GEN-16): `add_(T, T)
/// -> T` requires that `add_` is callable with two values of `T`, and
/// that it gives a `T`.
struct Requirement {
    /// Its position among the constraint calls that hold wherever the
    /// declaration's `requires` does ([`required_calls`]).
    index: usize,
    /// The operator named.
    operator: OperatorRef,
    /// The types it is supplied with, in order.
    operands: Vec<Type>,
    /// The result its arrow states, when it has one.
    result: Option<Type>,
}

/// What a call of an operator requests of its candidates: see
/// [`Typer::output_requested`].
struct Requested {
    /// The operator's result with what is known of its generic
    /// parameters in place, when the call says something of it.
    place: Option<Type>,
    /// The types the operator's own requirement admits for its result.
    among: Option<Vec<Type>>,
    /// A default the call takes that is not of its parameter's type as
    /// the call resolves it (GEN-13). Nothing is then requested.
    unfit: Option<Unfit>,
}

/// A generic value parameter's default that does not fit where a call
/// takes it.
struct Unfit {
    /// The parameter's position among the operator's generics.
    generic: usize,
    /// What the default is, for the message: `an \`i64\``.
    described: String,
    /// The parameter's type as the call resolves it.
    resolved: Type,
}

/// What the place a call stands in says of the operator's generic
/// parameters: its result is the type the place expects (GEN-8). A `T`
/// is a value of a `T?` (TYPE-14), so a place that expects an optional
/// type says what a result that is not optional is.
fn from_place(public: &Signature, expected: Option<&Type>, found: &mut [Option<Argument>]) {
    if let (Some(result), Some(expected)) = (&public.result, expected)
        && !open(expected)
    {
        let place = match result {
            Type::Nullable(_) => expected,
            _ => not_null(Some(expected)).unwrap_or(expected),
        };
        infer(result, place, &public.generics, found);
    }
}

/// What a `for` binds, and whether it makes one child for each member,
/// by what it loops over (CTRL-15).
///
/// "`for item in items` over an ordinary list, set, or dictionary
/// composes its body once for each element, with ordinary values bound
/// ... Over a connection of `list<T, N>` it composes its body `N` times,
/// `item` a connection of `T` ... Over a connection of `list<T>`,
/// `set<T>`, or `dict<K, V>` it makes one child for each member
/// (CTRL-7): `item` is a connection of `T` for a list and an ordinary
/// `T`, the member itself, for a set; `key` is an ordinary `K` and
/// `value` a connection of `V` for a dictionary."
fn bound_by_loop(over: ValueCategory, ty: &Type, names: usize) -> (Vec<ValueCategory>, bool) {
    use ValueCategory::{Connection, Unsettled, Value};
    match (over, ty) {
        (Connection, Type::List { size, .. }) => {
            // `list<T, unbounded>` grows as `list<T>` does.
            let fixed = size
                .as_ref()
                .is_some_and(|size| !matches!(size.value, ConstantValue::Unbounded));
            (vec![Connection; names], !fixed)
        }
        (Connection, Type::Set(_)) => (vec![Value; names], true),
        (Connection, Type::Dict { .. }) if names == 2 => (vec![Value, Connection], true),
        // What is looped over is not settled: neither is what is bound.
        (Unsettled, _) => (vec![Unsettled; names], false),
        _ => (vec![Value; names], false),
    }
}

/// True when an expression gives a value where it stands. A block gives
/// the one it ends in (CTRL-10), and none when it ends in a `return` or
/// any other statement; an `if` gives one where a branch does.
fn gives_a_value(value: &Expr) -> bool {
    match value.kind.branch_values() {
        Some(values) => values.into_iter().any(gives_a_value),
        None => true,
    }
}

/// The requirement of the enclosing declaration that a call is
/// (GEN-7): see [`Typer::required`].
struct Required {
    /// Its position among the declaration's requirements
    /// ([`Requirement::index`]).
    index: usize,
    /// The result its arrow states, when it has one.
    result: Option<Type>,
    /// The types it is supplied with, in the operator's order.
    operands: Vec<Type>,
    /// The place among them each argument of the call is given to, in
    /// the order the call writes them.
    places: Vec<usize>,
}

/// The constraint calls of a condition that hold wherever the condition
/// does: the condition itself, and each side of an `&&` (GEN-15).
fn required_calls(node: &SyntaxNode, calls: &mut Vec<SyntaxNode>) {
    match node.kind() {
        SyntaxKind::ConstraintCall => calls.push(node.clone()),
        SyntaxKind::ConstraintParen => {
            if let Some(inner) = node.children().next() {
                required_calls(&inner, calls);
            }
        }
        SyntaxKind::ConstraintBinary if has_token(node, SyntaxKind::AmpAmp) => {
            for side in node.children() {
                required_calls(&side, calls);
            }
        }
        _ => {}
    }
}

/// One argument of an operator's call, before a candidate is selected.
struct Given {
    /// How it is written: in a position, or by a name. Which parameter
    /// of a candidate that is, is the candidate's to say (GEN-6).
    key: Key,
    /// Where it is written.
    node: SyntaxNode,
    /// True when it is read as the type of the parameter it is given
    /// to: see [`typed_by_place`].
    by_place: bool,
    /// Its value, read with nothing expected of it.
    value: Expr,
    /// True when that reading has been taken back, the argument being
    /// read again as the selected candidate's parameter: `value` is
    /// then not what stands.
    retried: bool,
    /// The compile-time constant it is, read where the call stands, when
    /// it is given by a name a candidate declares as a generic value
    /// parameter: what it reads as, held to the candidate's type once
    /// the candidate is known (`instance`).
    constant: Option<Result<Typed, Fault>>,
}

impl Given {
    /// An operand of a symbol, which is read before the symbol's
    /// candidates are: in the place `position`, with the value it has.
    fn read(position: usize, node: SyntaxNode, value: Expr) -> Given {
        Given {
            key: Key::Position(position),
            node,
            by_place: false,
            value,
            retried: false,
            constant: None,
        }
    }
}

/// How a call writes an argument.
enum Key {
    /// With no name: the place it stands in, from 0.
    Position(usize),
    /// `name: value`.
    Name(String),
}

/// Whether a candidate admits a call.
#[derive(Clone, Copy, PartialEq)]
enum Admits {
    Yes,
    /// Its condition does not hold for the call (GEN-6).
    No,
    /// It does not take the call's arguments or give the result asked
    /// for: the types do not fit, or the call names an option it does
    /// not declare.
    Misfit,
    /// It cannot be told: a condition of the candidate reads an option
    /// whose value at the call is no constant this pass reads.
    Unknown,
}

/// What a call asks of a candidate's result (GEN-5: "the requested
/// output before inputs").
struct Wanted<'w> {
    /// The type the call's place expects, or the one the call's generic
    /// arguments make of the operator's result.
    place: Option<&'w Type>,
    /// The types the operator's own requirement admits for its result,
    /// when it states them and they are known: for
    /// `operator convert<T, V, O>(value: V) -> O
    ///     requires O in {T, result<T, error>}`
    /// called as `convert<f64>(count)`, an `f64` and a
    /// `result<f64, error>`.
    among: Option<Vec<Type>>,
}

/// True when a call of an operator is not read: the operator or a
/// candidate has a pack, or a candidate has a generic parameter that is
/// a type pack. Such a call has no type.
fn unreadable(public: &Signature, candidates: &[(usize, usize, &Signature)]) -> bool {
    let packed = |signature: &Signature| {
        signature
            .parameters
            .iter()
            .any(|parameter| parameter.pack.is_some())
    };
    packed(public)
        || candidates.iter().any(|(_, _, signature)| {
            packed(signature)
                || signature
                    .generics
                    .iter()
                    .any(|generic| matches!(generic.kind, GenericKind::Pack { .. }))
        })
}

/// What an argument of a call is given to in a candidate.
#[derive(Clone, Copy, PartialEq)]
enum Place {
    /// The parameter at this position of the candidate's.
    Parameter(usize),
    /// The generic value parameter at this position of the candidate's
    /// generic parameters: an option that selects (GEN-6).
    Generic(usize),
}

/// What each argument of a call is given to in a candidate, in the
/// order the arguments are written; `None` when the candidate does not
/// take them.
///
/// GEN-6: "An operator declares the parameters every candidate takes. A
/// candidate's shape honors them ... and it may declare further options
/// of its own". So a candidate's first parameters are the operator's,
/// place for place, and are named as the operator names them (EXPR-2);
/// those after are its options, by its own names. An argument with no
/// name is given to the parameter in its position. GEN-8: "A name binds
/// the callee's generic value parameter of that name, or else its
/// parameter of that name."
fn bound(signature: &Signature, public: &Signature, given: &[Given]) -> Option<Vec<Place>> {
    let own = public.parameters.len();
    if signature.parameters.len() < own {
        return None;
    }
    let mut taken = vec![false; signature.parameters.len()];
    let mut named = vec![false; signature.generics.len()];
    let mut places = Vec::with_capacity(given.len());
    for given in given {
        let place = match &given.key {
            Key::Position(position) => Place::Parameter(*position),
            Key::Name(name) => {
                let generic = signature.generics.iter().position(|generic| {
                    matches!(generic.kind, GenericKind::Value(_)) && generic.name == *name
                });
                match generic {
                    Some(generic) => Place::Generic(generic),
                    None => Place::Parameter(
                        public
                            .parameters
                            .iter()
                            .position(|parameter| parameter.name == *name)
                            .or_else(|| {
                                signature
                                    .parameters
                                    .iter()
                                    .skip(own)
                                    .position(|parameter| parameter.name == *name)
                                    .map(|option| option + own)
                            })?,
                    ),
                }
            }
        };
        // Each is given once.
        let slot = match place {
            Place::Parameter(index) => taken.get_mut(index)?,
            Place::Generic(index) => named.get_mut(index)?,
        };
        if *slot {
            return None;
        }
        *slot = true;
        places.push(place);
    }
    Some(places)
}

/// The default a declaration writes for its generic value parameter
/// `generic` (GEN-2).
pub(crate) fn generic_default(
    declaration: &SyntaxNode,
    generic: &GenericParameter,
) -> Option<SyntaxNode> {
    let written = child(declaration, SyntaxKind::GenericParams)?
        .children()
        .filter(|parameter| parameter.kind() == SyntaxKind::GenericParam)
        .find(|parameter| {
            tokens(parameter)
                .take_while(|token| token.kind() != SyntaxKind::Colon)
                .any(|token| token.span().start == generic.at.span.start)
        })?;
    default_of(&written)
}

/// A constant as it is written, for messages: `Constant::text`.
fn constant_text(node: &SyntaxNode) -> String {
    node.text().to_string().trim().to_string()
}

/// The set of each requirement `name in {...}` in `node`: the relation
/// itself, the sides of an `&&`, and those in parentheses. Each must
/// hold, so the name is in every one of them.
fn result_sets(node: &SyntaxNode, name: &str, sets: &mut Vec<SyntaxNode>) {
    match node.kind() {
        SyntaxKind::ConstraintRelation => {
            let operands: Vec<SyntaxNode> = node.children().collect();
            let [left, right] = operands.as_slice() else {
                return;
            };
            let named = left.kind() == SyntaxKind::NamedType
                && left.text().to_string().trim() == name
                && right.kind() == SyntaxKind::ConstraintSet
                && tokens(node).any(|token| token.text() == "in");
            if named {
                sets.push(right.clone());
            }
        }
        SyntaxKind::ConstraintBinary if has_token(node, SyntaxKind::AmpAmp) => {
            for side in node.children() {
                result_sets(&side, name, sets);
            }
        }
        SyntaxKind::ConstraintParen => {
            if let Some(inner) = node.children().next() {
                result_sets(&inner, name, sets);
            }
        }
        _ => {}
    }
}

/// True when the callee `node` is an anonymous function, in
/// parentheses or not, or a call of one, or of what such a call gives.
fn calls_anonymous(node: &SyntaxNode) -> bool {
    let node = &ungrouped(node.clone());
    match node.kind() {
        SyntaxKind::FnExpr => true,
        SyntaxKind::CallExpr => node
            .children()
            .next()
            .is_some_and(|inner| calls_anonymous(&inner)),
        _ => false,
    }
}

/// The default a declaration writes for `parameter`, one of its own.
fn default_at(declaration: &SyntaxNode, parameter: &crate::types::Parameter) -> Option<SyntaxNode> {
    let written = parameters(declaration).into_iter().find(|written| {
        tokens(written)
            .take_while(|token| token.kind() != SyntaxKind::Colon)
            .any(|token| token.span().start == parameter.at.span.start)
    })?;
    default_of(&written)
}

/// What a candidate's condition is checked against at a call.
struct Condition<'c> {
    /// The value an option has at the call: what the call gives it, or
    /// its default. `None` when it is no constant this pass reads.
    value_of: &'c dyn Fn(&str) -> Option<ConstantValue>,
    /// What a constant the condition compares the option named with
    /// comes to, as a value of the option's type (`crate::constant`).
    /// `None` when it is no constant this pass reads.
    constant: &'c dyn Fn(&SyntaxNode, &str) -> Option<ConstantValue>,
    /// What a generic parameter of the candidate stands for at the
    /// call. `None` for a name that is no generic parameter of it.
    type_of: &'c dyn Fn(&str) -> Option<Type>,
    /// What a type written in the condition means, the candidate's
    /// generic parameters in place. `None` for what is no type, or is
    /// not known.
    written: &'c dyn Fn(&SyntaxNode) -> Option<Type>,
    /// Whether a named operator requirement written in the condition
    /// holds at the call (GEN-16). `None` when it cannot be told.
    callable: &'c dyn Fn(&SyntaxNode) -> Option<bool>,
}

/// A candidate a call admits.
struct Chosen<'s> {
    /// The position of its module in `Program::modules`.
    module: usize,
    /// Its position in that module's `declarations`.
    declaration: usize,
    /// Its signature as it is declared, generic parameters and all.
    declared: &'s Signature,
    /// Its signature as this call takes it: what its generic
    /// parameters stand for put in place.
    taken: std::borrow::Cow<'s, Signature>,
    /// What each of its generic parameters stands for at this call.
    generics: Vec<Argument>,
}

/// A candidate as a call takes it: itself where it has no generic
/// parameter, and otherwise with each resolved (GEN-2) and put in
/// place. `None` when the call does not say what one of them is, or
/// does not fit the candidate's parameters at all.
///
/// GEN-5: "Match candidates against the requested output before
/// inputs". GEN-12: "A prior binding constrains every later occurrence
/// in declared parameter order." An argument whose type is not known
/// all the way down, `null`, `.member` or `[]`, says nothing.
///
/// `defaults` gives the constant a generic value parameter's default
/// comes to (GEN-2), and `program` the enums a constant given to an
/// option is held to.
#[allow(clippy::type_complexity)]
fn instance<'s>(
    signature: &'s Signature,
    public: &Signature,
    given: &[Given],
    wanted: &Wanted,
    (widens, defaults, program): (
        &dyn Fn(&Type, &Type) -> bool,
        &dyn Fn(&GenericParameter) -> Option<Constant>,
        &Program,
    ),
) -> Option<(std::borrow::Cow<'s, Signature>, Vec<Argument>, Vec<usize>)> {
    let generics = &signature.generics;
    if generics.is_empty() {
        return Some((
            std::borrow::Cow::Borrowed(signature),
            Vec::new(),
            Vec::new(),
        ));
    }
    let bound = bound(signature, public, given)?;
    let mut found: Vec<Option<Argument>> = vec![None; generics.len()];
    // GEN-2: what the call writes decides first. A generic value
    // parameter it gives by name is that, whatever an argument's type
    // would say of it: `pick(pair, N: 3)` for `value: list<i64, N>` asks
    // for a list of three, and a list of two is then no argument for it.
    for (position, place) in bound.iter().enumerate() {
        if let Place::Generic(index) = *place {
            let given = &given[position];
            // EXPR-5: a value of the option's type, as this candidate
            // declares it: `scale: 1` is the `f64` one.
            let expected = match &generics[index].kind {
                GenericKind::Value(ty) if !names_parameter(ty) => Some(ty),
                _ => None,
            };
            let value = given
                .constant
                .clone()
                .unwrap_or(Err(Fault::Reported))
                .and_then(|read| crate::constant::fit(read, expected, program))
                .unwrap_or(ConstantValue::Unknown);
            found[index] = Some(Argument::Constant(Constant {
                text: constant_text(&given.node),
                value,
            }));
        }
    }
    if let (Some(place), Some(result)) = (wanted.place, &signature.result) {
        let place = match result {
            Type::Nullable(_) => place,
            _ => not_null(Some(place)).unwrap_or(place),
        };
        infer(result, place, generics, &mut found);
    }
    // The generic parameters an argument decided: what the call writes,
    // or its place expects, is not widened (GEN-2).
    let mut by_argument = vec![false; generics.len()];
    for (index, parameter) in signature.parameters.iter().enumerate() {
        let argument = bound
            .iter()
            .position(|&place| place == Place::Parameter(index));
        let Some(position) = argument else {
            continue;
        };
        let actual = &given[position].value.ty;
        // GEN-2: "an abstract struct that the type decided inherits
        // from widens the parameter to that struct".
        if let Some(generic) = bare_parameter(&parameter.ty, generics)
            && by_argument[generic]
            && let Some(Argument::Type(decided)) = &found[generic]
            && widens(decided, actual)
        {
            found[generic] = Some(Argument::Type(actual.clone()));
            continue;
        }
        let before: Vec<bool> = found.iter().map(Option::is_some).collect();
        infer(&parameter.ty, actual, generics, &mut found);
        for (generic, was) in before.into_iter().enumerate() {
            by_argument[generic] |= !was && found[generic].is_some();
        }
    }
    // A generic value parameter's own type may name a type parameter:
    // the `T` of `<T, const fallback: T>`. What the call gives it says
    // what that is where the result and the parameters, which are
    // declared to take the call's arguments, have not.
    // GEN-12: in the order they are declared, whatever order the call
    // writes them in.
    for (index, generic) in generics.iter().enumerate() {
        let argument = bound
            .iter()
            .position(|&place| place == Place::Generic(index));
        if let (Some(position), GenericKind::Value(ty)) = (argument, &generic.kind) {
            infer(ty, &given[position].value.ty, generics, &mut found);
        }
    }
    // GEN-6: "An option that selects among candidates is a generic
    // value parameter". One that neither the call nor an argument's
    // type says anything of takes its default (GEN-2). GEN-6: "a
    // candidate whose option has none is admitted only when the call
    // gives it."
    let mut defaulted = Vec::new();
    for (index, generic) in generics.iter().enumerate() {
        if found[index].is_some() || !matches!(generic.kind, GenericKind::Value(_)) {
            continue;
        }
        if let Some(default) = defaults(generic) {
            found[index] = Some(Argument::Constant(default));
            defaulted.push(index);
        }
    }
    let resolved: Vec<Argument> = found.into_iter().collect::<Option<_>>()?;
    let own: Vec<Location> = generics.iter().map(|generic| generic.at).collect();
    let map: Vec<(Location, &Argument)> = own.iter().copied().zip(&resolved).collect();
    let mut taken = signature.clone();
    for parameter in &mut taken.parameters {
        parameter.ty = placed(substitute_within(&parameter.ty, &map), &own);
    }
    // A generic value parameter's type may name another generic
    // parameter: the `T` of `<T, const fallback: T>`.
    for generic in &mut taken.generics {
        if let GenericKind::Value(ty) = &mut generic.kind {
            *ty = placed(substitute_within(ty, &map), &own);
        }
    }
    taken.result = taken
        .result
        .as_ref()
        .map(|result| placed(substitute_within(result, &map), &own));
    Some((std::borrow::Cow::Owned(taken), resolved, defaulted))
}

/// How specific one type of a candidate's signature is beside another's
/// in the same place.
#[derive(Clone, Copy, PartialEq)]
enum Specific {
    /// As specific: the same shape, variables where the other has them.
    Same,
    /// More: concrete, or structured, where the other has a variable.
    More,
    /// Less.
    Less,
    /// Neither: more specific in one part and less in another, or of
    /// another shape altogether.
    Neither,
}

impl Specific {
    /// The two together: what holds of a whole whose parts are these.
    fn and(self, other: Specific) -> Specific {
        match (self, other) {
            (Specific::Same, other) | (other, Specific::Same) => other,
            (Specific::More, Specific::More) => Specific::More,
            (Specific::Less, Specific::Less) => Specific::Less,
            _ => Specific::Neither,
        }
    }
}

/// How specific `left` is beside `right`, each a type of a candidate's
/// signature as it is declared, so that a `Type::Parameter` in it is
/// one of that candidate's generic parameters.
///
/// GEN-5: "Concrete types outrank variables; structurally constrained
/// variables outrank unconstrained ones." GEN-12: "Compare structural
/// specificity component by component". So `tuple<i64, T>` is more
/// specific than `tuple<U, V>`, and `list<T>` than `T`.
fn specificity(left: &Type, right: &Type) -> Specific {
    let all = |pairs: &mut dyn Iterator<Item = Specific>| pairs.fold(Specific::Same, Specific::and);
    match (left, right) {
        (Type::Parameter { .. }, Type::Parameter { .. }) => Specific::Same,
        (Type::Parameter { .. }, _) => Specific::Less,
        (_, Type::Parameter { .. }) => Specific::More,
        // TYPE-14: a `T` is given where a `T?` is expected, so one
        // candidate may take `i64` where another takes `T?`. They are
        // compared as the types under the `?`.
        (Type::Nullable(left), right) if !matches!(right, Type::Nullable(_)) => {
            specificity(left, right)
        }
        (left, Type::Nullable(right)) if !matches!(left, Type::Nullable(_)) => {
            specificity(left, right)
        }
        (Type::Nullable(left), Type::Nullable(right))
        | (Type::Set(left), Type::Set(right))
        | (Type::Ref(left), Type::Ref(right))
        | (Type::Endpoint(left), Type::Endpoint(right))
        | (Type::Atomic(left), Type::Atomic(right))
        | (Type::Delta(left), Type::Delta(right))
        | (Type::List { element: left, .. }, Type::List { element: right, .. })
        | (Type::RollingWindow { value: left, .. }, Type::RollingWindow { value: right, .. }) => {
            specificity(left, right)
        }
        (Type::Tuple(left), Type::Tuple(right)) if left.len() == right.len() => all(&mut left
            .iter()
            .zip(right)
            .map(|(left, right)| specificity(left, right))),
        (
            Type::Dict { key, value },
            Type::Dict {
                key: other_key,
                value: other_value,
            },
        ) => specificity(key, other_key).and(specificity(value, other_value)),
        (
            Type::Result { value, error },
            Type::Result {
                value: other_value,
                error: other_error,
            },
        ) => specificity(value, other_value).and(specificity(error, other_error)),
        (
            Type::Named {
                module,
                declaration,
                arguments,
                ..
            },
            Type::Named {
                module: other_module,
                declaration: other_declaration,
                arguments: other_arguments,
                ..
            },
        ) if (module, declaration) == (other_module, other_declaration)
            && arguments.len() == other_arguments.len() =>
        {
            all(&mut arguments
                .iter()
                .zip(other_arguments)
                .map(|pair| match pair {
                    (Argument::Type(left), Argument::Type(right)) => specificity(left, right),
                    // A constant is as specific as a constant.
                    _ => Specific::Same,
                }))
        }
        // Two types with no variable in them: one type, where both
        // candidates admit the call.
        (left, right) if left == right => Specific::Same,
        // Of another shape. GEN-5: "Concrete types outrank variables".
        (left, right) => match (names_parameter(left), names_parameter(right)) {
            (false, true) => Specific::More,
            (true, false) => Specific::Less,
            _ => Specific::Neither,
        },
    }
}

/// Keeps of the candidates a call admits the most specific: those no
/// other outranks. One is left where the call selects; more are a tie.
///
/// GEN-5: "Select exactly one most-specific admitted candidate." The
/// operator's own parameters, `own` of them, are compared place for
/// place ([`specificity`]). A candidate outranks another when it is as
/// specific in every one and more specific in one.
fn most_specific(selected: &mut Vec<Chosen>, own: usize) {
    if selected.len() < 2 {
        return;
    }
    let outranks = |better: &Chosen, worse: &Chosen| {
        let whole = better
            .declared
            .parameters
            .iter()
            .zip(&worse.declared.parameters)
            .take(own)
            .map(|(better, worse)| specificity(&better.ty, &worse.ty))
            .fold(Specific::Same, Specific::and);
        whole == Specific::More
    };
    let outranked: Vec<bool> = selected
        .iter()
        .map(|chosen| selected.iter().any(|other| outranks(other, chosen)))
        .collect();
    let mut position = 0;
    selected.retain(|_| {
        position += 1;
        !outranked[position - 1]
    });
}

/// The generic parameters of a function or a struct that states a
/// `requires`, and the condition it states. `None` for one that states
/// none.
fn required(
    program: &Program,
    (module, declaration): (usize, usize),
) -> Option<(&[GenericParameter], SyntaxNode)> {
    let declared_in = &program.modules[module];
    let parameters: &[GenericParameter] = match declared_in.details.get(declaration)? {
        Detail::Callable(signature) => &signature.generics,
        Detail::Struct(read) => &read.generics,
        _ => return None,
    };
    let declared = &declared_in.declarations[declaration];
    let clause = child(&declared.syntax, SyntaxKind::RequiresClause)?;
    Some((parameters, clause.children().next()?))
}

/// What the place a `requires` is tested at says of it, beyond the
/// generic arguments ([`requires_holds`]).
struct Told<'t> {
    /// What a constant written in the condition comes to as a value of
    /// the type given (`crate::constant`), read where the declaration
    /// is written. `None` where it is no constant read so.
    constant: &'t dyn Fn(&SyntaxNode, Option<&Type>) -> Option<ConstantValue>,
    /// Whether a named operator requirement holds (GEN-16). `None`
    /// when it cannot be told.
    callable: &'t dyn Fn(&SyntaxNode) -> Option<bool>,
}

/// Whether `condition`, the `requires` of `declaration` of `module`,
/// holds with `generics` for the declaration's generic parameters
/// (GEN-3). `None` when it cannot be told: a generic argument is not
/// known, or names a generic parameter of the declaration it is
/// written in, which is no one type.
fn requires_holds(
    program: &Program,
    (module, declaration): (usize, usize),
    generics: &[Argument],
    condition: &SyntaxNode,
    place: &Told,
) -> Option<bool> {
    let (parameters, _) = required(program, (module, declaration))?;
    let declared_in = &program.modules[module];
    let declared = &declared_in.declarations[declaration];
    // The generic value parameter of the declaration named `name`, by
    // its position.
    let option = |name: &str| {
        parameters.iter().position(|generic| {
            matches!(generic.kind, GenericKind::Value(_)) && generic.name == name
        })
    };
    // GEN-3: the value it has is the constant the generic arguments give
    // it, a default included.
    let value_of = |name: &str| -> Option<ConstantValue> {
        match generics.get(option(name)?)? {
            Argument::Constant(constant) => Some(constant.value.clone()),
            Argument::Type(_) => None,
        }
    };
    let own: Vec<Location> = parameters.iter().map(|generic| generic.at).collect();
    let map: Vec<(Location, &Argument)> = own.iter().copied().zip(generics).collect();
    let told = |ty: Type| (whole(&ty) && !names_parameter(&ty)).then_some(ty);
    let type_of = |name: &str| -> Option<Type> {
        let index = parameters.iter().position(|generic| generic.name == name)?;
        match generics.get(index)? {
            Argument::Type(ty) => told(ty.clone()),
            Argument::Constant(_) => None,
        }
    };
    let file = declared_in.files[declared.file].file;
    let written = |node: &SyntaxNode| -> Option<Type> {
        let start = node.span().start;
        let read = declared_in
            .written
            .iter()
            .find(|written| written.at.file == file && written.at.span.start == start)?;
        told(placed(substitute_within(&read.ty, &map), &own))
    };
    // A constant the condition compares an option with is a value of
    // the option's type (EXPR-5), its generic parameters in place.
    let constant = |node: &SyntaxNode, name: &str| -> Option<ConstantValue> {
        let ty = match &parameters[option(name)?].kind {
            GenericKind::Value(ty) => placed(substitute_within(ty, &map), &own),
            _ => return None,
        };
        (place.constant)(node, Some(&ty))
    };
    holds(
        condition,
        &Condition {
            value_of: &value_of,
            constant: &constant,
            type_of: &type_of,
            written: &written,
            callable: place.callable,
        },
    )
}

/// The rejection of a call of a function, or an application of a
/// struct, at `at`, whose `requires`, `condition`, does not hold
/// (GEN-3).
fn unmet(
    program: &Program,
    (module, declaration): (usize, usize),
    at: Location,
    condition: &SyntaxNode,
) -> hgl_diagnostics::Diagnostic {
    let declared_in = &program.modules[module];
    let declared = &declared_in.declarations[declaration];
    let what = match declared.kind {
        DeclarationKind::Struct => "application",
        _ => "call",
    };
    error(
        Category::Constraint,
        codes::REQUIRES_UNMET,
        at,
        format!(
            "the `requires` of `{}` does not hold for this {what}",
            declared.name
        ),
        "GEN-3",
    )
    .with_related(
        Location::new(declared_in.files[declared.file].file, condition.span()),
        "the condition that does not hold",
    )
}

/// Every struct a type applies, at any depth, with the generic
/// arguments it gives it: none for a struct that takes none.
fn struct_applications(ty: &Type, found: &mut Vec<((usize, usize), Vec<Argument>)>) {
    match ty {
        Type::Named {
            module,
            declaration,
            arguments,
            kind,
            ..
        } => {
            if *kind == NamedKind::Struct {
                found.push(((*module, *declaration), arguments.clone()));
            }
            for argument in arguments {
                if let Argument::Type(argument) = argument {
                    struct_applications(argument, found);
                }
            }
        }
        Type::Tuple(elements) => elements
            .iter()
            .for_each(|element| struct_applications(element, found)),
        Type::List { element, .. } => struct_applications(element, found),
        Type::Set(inner)
        | Type::Ref(inner)
        | Type::Endpoint(inner)
        | Type::Atomic(inner)
        | Type::Delta(inner)
        | Type::Trace { cell: inner, .. }
        | Type::Nullable(inner) => struct_applications(inner, found),
        Type::Dict { key, value } => {
            struct_applications(key, found);
            struct_applications(value, found);
        }
        Type::RollingWindow { value, .. } => struct_applications(value, found),
        Type::Result { value, error } => {
            struct_applications(value, found);
            struct_applications(error, found);
        }
        Type::Scalar(_)
        | Type::Parameter { .. }
        | Type::Signal
        | Type::Error
        | Type::ErrorContext
        | Type::Unknown => {}
    }
}

/// Whether a candidate's condition holds at a call. `None` when it
/// cannot be told: the condition is of a form this pass does not read,
/// an option's value is no constant, or a type in it is not known.
///
/// GEN-6: a candidate compares "the option with constants: `requires
/// zero == .error`, `requires zero in {.ieee, .nan}`". GEN-3: a
/// condition "may use type/category membership, equalities": of a
/// generic parameter, `T == i64` or `T in {i64, f64}`. GEN-15:
/// "negation binds before conjunction, which binds before disjunction".
fn holds(node: &SyntaxNode, of: &Condition) -> Option<bool> {
    match node.kind() {
        SyntaxKind::ConstraintParen => holds(&node.children().next()?, of),
        SyntaxKind::ConstraintNot => holds(&node.children().next()?, of).map(|held| !held),
        // GEN-16: a named operator requirement.
        SyntaxKind::ConstraintCall => (of.callable)(node),
        SyntaxKind::ConstraintBinary => {
            let mut sides = node.children();
            let left = holds(&sides.next()?, of);
            let right = holds(&sides.next()?, of);
            let both = has_token(node, SyntaxKind::AmpAmp);
            match (left, right) {
                (Some(left), Some(right)) if both => Some(left && right),
                (Some(left), Some(right)) => Some(left || right),
                // One side settles it whatever the other is.
                (Some(false), _) | (_, Some(false)) if both => Some(false),
                (Some(true), _) | (_, Some(true)) if !both => Some(true),
                _ => None,
            }
        }
        SyntaxKind::ConstraintRelation => {
            let operands: Vec<SyntaxNode> = node.children().collect();
            let [left, right] = operands.as_slice() else {
                return None;
            };
            // The option is the lone name; the read pass has checked
            // that it names one.
            let option = |operand: &SyntaxNode| {
                (operand.kind() == SyntaxKind::NamedType)
                    .then(|| operand.text().to_string().trim().to_string())
            };
            let within = tokens(node).any(|token| token.text() == "in");
            // A generic parameter of the candidate: the condition is of
            // the type it stands for. `T == i64`, `T in {i64, f64}`.
            if let Some(ty) = option(left).and_then(|name| (of.type_of)(&name)) {
                return if has_token(node, SyntaxKind::EqEq) {
                    Some((of.written)(right)? == ty)
                } else if within && right.kind() == SyntaxKind::ConstraintSet {
                    let mut found = false;
                    for member in right.children() {
                        found |= (of.written)(&member)? == ty;
                    }
                    Some(found)
                } else {
                    None
                };
            }
            if has_token(node, SyntaxKind::EqEq) {
                let (name, constant) = match (option(left), option(right)) {
                    (Some(name), None) => (name, right),
                    (None, Some(name)) => (name, left),
                    _ => return None,
                };
                // The one comparison (`ConstantValue::same`): where the
                // value is not known, whether it holds is not either.
                (of.value_of)(&name)?.same(&(of.constant)(constant, &name)?)
            } else if within {
                let name = option(left)?;
                if right.kind() != SyntaxKind::ConstraintSet {
                    return None;
                }
                let value = (of.value_of)(&name)?;
                // GEN-3: "the membership of a constant in a set of
                // constants": it holds where it is one of them, and does
                // not where it is known to be none.
                let mut found = Some(false);
                for member in right.children() {
                    match (of.constant)(&member, &name).and_then(|member| value.same(&member)) {
                        Some(true) => return Some(true),
                        Some(false) => {}
                        None => found = None,
                    }
                }
                found
            } else {
                None
            }
        }
        _ => None,
    }
}

/// True when an expression holds a block, at any depth: reading it may
/// then declare a local, assign one, or return (CTRL-10).
fn holds_block(node: &SyntaxNode) -> bool {
    node.descendants()
        .any(|part| part.kind() == SyntaxKind::Block)
}

/// True when an integer literal was widened among the entries of a
/// sequence or a tuple written out, at any depth. A literal widened
/// inside a call by name among the entries is that call's, and is not
/// counted; one widened under a symbol that is core's own arithmetic
/// is.
fn widens_an_entry(value: &Expr) -> bool {
    match &value.kind {
        ExprKind::Widen(_) => true,
        ExprKind::Sequence(entries) | ExprKind::Tuple(entries) => {
            entries.iter().any(widens_an_entry)
        }
        // Either side of a pair: `["a": 1]` where an `f64` is asked for.
        ExprKind::Pairs(entries) => entries
            .iter()
            .any(|(key, value)| widens_an_entry(key) || widens_an_entry(value)),
        // The value of a block, and of each branch of an `if`.
        ExprKind::Block { .. } | ExprKind::If { .. } => value
            .kind
            .branch_values()
            .is_some_and(|values| values.into_iter().any(widens_an_entry)),
        // A symbol among the entries is read with the entry's type as
        // its place, so that core's own arithmetic over literals may
        // widen them: the `1` of `[-1]`, the `1` and `2` of `[1 + 2]`,
        // where an `f64` is asked for. That is the entry's widening.
        _ => {
            negated(value).is_some_and(widens_an_entry)
                || arithmetic(value)
                    .is_some_and(|(_, left, right)| widens_an_entry(left) || widens_an_entry(right))
        }
    }
}

/// True when an argument is read as the type of the parameter it is
/// given to: `null` and `.member`, which have no type of their own
/// (TYPE-14, TYPE-7), and a sequence or a tuple written out, whose
/// entries are read as the parameter's (EXPR-5, EXPR-9), so that `[1]`
/// is a `list<f64>` or a `set<i64>` where a candidate takes one. A block
/// is, when its value is; an `if`, when a branch is (CTRL-10).
fn typed_by_place(node: &SyntaxNode) -> bool {
    let node = &ungrouped(node.clone());
    match node.kind() {
        SyntaxKind::SequenceExpr | SyntaxKind::TupleExpr => true,
        // An integer literal that is a block's value is an `f64` where
        // one is expected (EXPR-5), as the block's place says.
        SyntaxKind::Block => value_of_branch(node)
            .is_some_and(|value| typed_by_place(&value) || is_integer_written(&value)),
        // The condition is first; the branches follow it.
        SyntaxKind::IfExpr => node
            .children()
            .skip(1)
            .any(|branch| typed_by_place(&branch)),
        _ => takes_its_type(node),
    }
}

/// True when an expression is an integer literal as it is written,
/// under a minus or in parentheses or not.
fn is_integer_written(node: &SyntaxNode) -> bool {
    let node = &ungrouped(node.clone());
    match node.kind() {
        SyntaxKind::Literal => tokens(node).any(|token| token.kind() == SyntaxKind::Int),
        SyntaxKind::UnaryExpr => node
            .children()
            .next()
            .is_some_and(|inner| is_integer_written(&inner)),
        _ => false,
    }
}

/// What a block or an `if` comes to.
///
/// `#[derive(Clone)]` asks the compiler to write the code that copies
/// one.
#[derive(Clone)]
enum Outcome {
    /// It has a value of this type.
    Value(Type),
    /// It has none: its last statement is no expression.
    Nothing,
    /// Every path through it returns from the function.
    Returns,
    /// It is null and nothing else: an `if` with no `else` whose branch
    /// returns (CTRL-10: "as if its `else` were null").
    Null,
}

/// One branch of an `if`, as it was read.
struct Branch {
    /// The branch: a block, or the `if` of an `else if`.
    value: Expr,
    /// What it comes to.
    outcome: Outcome,
    /// What is assigned at its end.
    assigned: Vec<bool>,
    /// True when every path through it has returned.
    returned: bool,
}

/// The value of a `switch` label, for telling two alike (CTRL-6).
use hir::LabelValue as Label;

// ---- Control flow in a value function ----

impl<'a> Typer<'a> {
    /// The statements of a block before its value, read in order, and
    /// the expression that is its value: its last statement, when that
    /// is an expression (CTRL-10).
    fn block_head(&mut self, block: &SyntaxNode) -> (Vec<Stmt>, Option<SyntaxNode>) {
        let parts: Vec<SyntaxNode> = block.children().collect();
        let last = parts
            .last()
            .filter(|last| last.kind() == SyntaxKind::ExprStmt)
            .and_then(|last| last.children().next());
        let before = if last.is_some() {
            parts.len() - 1
        } else {
            parts.len()
        };
        let statements = parts[..before]
            .iter()
            .map(|statement| self.statement(statement))
            .collect();
        (statements, last)
    }

    /// An expression that may be a block or an `if`, with what it comes
    /// to. Any other expression has its value. `used` is false when the
    /// value is dropped: a statement, or the last expression of a
    /// function that declares no result (FN-7).
    fn flow(&mut self, node: &SyntaxNode, expected: Option<&Type>, used: bool) -> (Expr, Outcome) {
        // "Parentheses select grouping" (EXPR-1).
        let node = &ungrouped(node.clone());
        // SOURCE-4: a block or an `if` whose value is used is a value
        // expression, and a `yield` inside it is reported.
        let valued = used && matches!(node.kind(), SyntaxKind::Block | SyntaxKind::IfExpr);
        self.valued += u32::from(valued);
        let read = match node.kind() {
            SyntaxKind::Block => self.block(node, expected, used),
            SyntaxKind::IfExpr => self.conditional(node, expected, used),
            _ => {
                let value = self.expression(node, expected);
                let ty = value.ty.clone();
                (value, Outcome::Value(ty))
            }
        };
        self.valued -= u32::from(valued);
        read
    }

    /// CTRL-10: "A block whose last statement is an expression has that
    /// expression's value and type; any other block has none."
    ///
    /// Where the place says what it expects, the value is checked
    /// against that here, where it is written; the block is then of the
    /// type expected and is not reported again.
    fn block(&mut self, node: &SyntaxNode, expected: Option<&Type>, used: bool) -> (Expr, Outcome) {
        let at = self.at(node);
        // ERR-12: what a null test proves inside the block holds to its
        // end.
        let frame = self.present.len();
        let (expression, outcome) = self.block_read(node, expected, used, at);
        self.present.truncate(frame);
        (expression, outcome)
    }

    /// The block `node`, read: see [`Typer::block`].
    fn block_read(
        &mut self,
        node: &SyntaxNode,
        expected: Option<&Type>,
        used: bool,
        at: Location,
    ) -> (Expr, Outcome) {
        let (statements, last) = self.block_head(node);
        let (value, outcome) = match last {
            // CTRL-5: what follows a `return` is not reached. It is read
            // for what it reports itself, and is not the block's value.
            Some(last) if self.returned => {
                let (value, _) = self.flow(&last, Some(&Type::Unknown), false);
                (Some(Box::new(value)), Outcome::Returns)
            }
            Some(last) => {
                let (value, outcome) = self.flow(&last, expected, used);
                match (outcome, expected) {
                    (Outcome::Value(_), Some(expected)) if !open(expected) => {
                        let reported = self.diagnostics.len();
                        // A branch of a graph's `if` or `switch` gives
                        // its connection across a reference as the
                        // place it stands in does (TIME-10).
                        let value = self.fit_input(value, expected, "the value of this block");
                        let ty = if self.diagnostics.len() == reported {
                            value.ty.clone()
                        } else {
                            expected.clone()
                        };
                        (Some(Box::new(value)), Outcome::Value(ty))
                    }
                    (outcome, _) => (Some(Box::new(value)), outcome),
                }
            }
            None => (None, Outcome::Nothing),
        };
        // CTRL-5: a block that has returned on every path comes to
        // that, whatever follows the `return`.
        let outcome = if self.returned {
            Outcome::Returns
        } else {
            outcome
        };
        let ty = match &outcome {
            Outcome::Value(ty) => ty.clone(),
            Outcome::Nothing | Outcome::Returns | Outcome::Null => Type::Unknown,
        };
        let kind = ExprKind::Block { statements, value };
        (Expr { kind, ty, at }, outcome)
    }

    /// CTRL-10: "In a value function the condition of an `if` or a
    /// `while` is a `bool`: a `bool?`, a number, or any other type
    /// rejects."
    fn condition(&mut self, node: &SyntaxNode) -> Expr {
        let value = self.expression(node, Some(&BOOL));
        if !open(&value.ty) && value.ty != BOOL {
            self.diagnostics.emit(error(
                Category::Type,
                codes::TYPE_MISMATCH,
                value.at,
                format!("this is `{}`, and a condition is a `bool`", value.ty),
                self.conditional_rule(),
            ));
        }
        value
    }

    /// The rule that says what an `if` is where this one stands:
    /// CTRL-13 in a graph, CTRL-10 in a value function and a hook.
    fn conditional_rule(&self) -> &'static str {
        if self.graph { "CTRL-13" } else { "CTRL-10" }
    }

    /// The rule that says what a loop and a `switch` are where this one
    /// stands: CTRL-15 in a graph, CTRL-12 in a value function and a
    /// hook.
    fn loop_rule(&self) -> &'static str {
        if self.graph { "CTRL-15" } else { "CTRL-12" }
    }

    /// One branch of an `if`, read from what is assigned before the
    /// `if`.
    fn branch(
        &mut self,
        node: &SyntaxNode,
        expected: Option<&Type>,
        used: bool,
        (before, before_returned, proved): (&[bool], bool, &[Fact]),
    ) -> Branch {
        self.assigned = restored(before, self.locals.len(), true);
        self.returned = before_returned;
        // ERR-12: what the condition proves holds in the branch, and no
        // further.
        let frame = self.present.len();
        self.present.extend_from_slice(proved);
        let (value, outcome) = self.flow(node, expected, used);
        self.present.truncate(frame);
        Branch {
            value,
            outcome,
            assigned: self.assigned.clone(),
            returned: self.returned,
        }
    }

    /// `if condition { ... } else { ... }`.
    ///
    /// CTRL-10: "An `if` whose value is used has the one type its two
    /// branches' values have, as the entries of a sequence have one
    /// (EXPR-9): a null branch makes it `T?`, and two types reject.
    /// With no `else` it is as if its `else` were null ... Where the
    /// place expects a type, each branch's value is checked against it.
    /// A branch that returns from the function gives the `if` no value
    /// and takes no part in its type (CTRL-5)."
    ///
    /// CTRL-11: a `var` "is assigned after an `if` when every branch
    /// that does not return assigns it, so not by an `if` with no
    /// `else`".
    fn conditional(
        &mut self,
        node: &SyntaxNode,
        expected: Option<&Type>,
        used: bool,
    ) -> (Expr, Outcome) {
        let at = self.at(node);
        let mut parts = node.children();
        let (Some(condition), Some(then)) = (parts.next(), parts.next()) else {
            self.forget();
            self.unsure = true;
            let unread = Expr {
                kind: ExprKind::Unread,
                ty: Type::Unknown,
                at,
            };
            return (unread, Outcome::Value(Type::Unknown));
        };
        let otherwise = parts.next();
        let condition = self.condition(&condition);
        // CTRL-1: "A wiring-time condition chooses which branch to
        // compose. A temporal condition in composition selects a child
        // graph at runtime."
        let selects = if self.graph {
            self.category(&condition)
        } else {
            ValueCategory::Value
        };
        // A branch selected at runtime: a `var` before it is declared
        // outside (CTRL-14). The nearest such `if` says which are: one
        // inside another has its own, and a `var` of the outer branch
        // is declared before the inner.
        let outer = self.temporal;
        if selects == ValueCategory::Connection {
            self.temporal = Some(self.locals.len());
        }
        // ERR-12: the bindings the condition proves are not null, where
        // it holds and where it does not.
        let (when_true, when_false) = self.null_tests(&condition);
        let before = self.assigned.clone();
        let before_returned = self.returned;
        let (in_then, in_else) = (
            (before.as_slice(), before_returned, when_true.as_slice()),
            (before.as_slice(), before_returned, when_false.as_slice()),
        );
        // The place says what a branch's value is, when it says. With
        // nothing said, a branch that is `null` or `.member` takes its
        // type from the other branch, which is then read first.
        let told = expected.is_some_and(|expected| !open(expected));
        let from_other = |first: &Outcome, second: &SyntaxNode| match first {
            Outcome::Value(ty) if !open(ty) => Some(if branch_is_null(second) {
                nullable(ty.clone())
            } else {
                ty.clone()
            }),
            _ => None,
        };
        let by_place = |branch: &SyntaxNode| branch_takes_its_type(branch);
        let (mut then, mut other) = match &otherwise {
            None => (self.branch(&then, expected, used, in_then), None),
            Some(otherwise) if !told && by_place(&then) && !by_place(otherwise) => {
                let other = self.branch(otherwise, expected, used, in_else);
                let hint = from_other(&other.outcome, &then);
                let then = self.branch(&then, hint.as_ref().or(expected), used, in_then);
                (then, Some(other))
            }
            Some(otherwise) => {
                let then = self.branch(&then, expected, used, in_then);
                let hint = if !told && by_place(otherwise) {
                    from_other(&then.outcome, otherwise)
                } else {
                    None
                };
                let other = self.branch(otherwise, hint.as_ref().or(expected), used, in_else);
                (then, Some(other))
            }
        };
        self.temporal = outer;
        // ERR-12: "including after a terminating null branch". Where
        // one branch returns, what follows the `if` is reached only
        // through the other, and what that one's side of the condition
        // proves holds to the end of the block the `if` stands in.
        let else_returned = other
            .as_ref()
            .map_or(before_returned, |other| other.returned);
        if then.returned && !else_returned {
            self.present.extend_from_slice(&when_false);
        } else if else_returned && !then.returned && other.is_some() {
            self.present.extend_from_slice(&when_true);
        }

        // What is assigned after: by every branch that does not return.
        let count = self.locals.len();
        match &other {
            Some(other) => {
                self.assigned = joined(
                    (&then.assigned, then.returned),
                    (&other.assigned, other.returned),
                    count,
                );
                self.returned = then.returned && other.returned;
            }
            None => {
                self.assigned = joined(
                    (&then.assigned, then.returned),
                    (&before, before_returned),
                    count,
                );
                self.returned = then.returned && before_returned;
            }
        }

        let outcome = match (
            then.outcome.clone(),
            other.as_ref().map(|other| other.outcome.clone()),
        ) {
            (Outcome::Returns, Some(Outcome::Returns)) => Outcome::Returns,
            (Outcome::Returns, Some(outcome)) | (outcome, Some(Outcome::Returns)) => outcome,
            (Outcome::Value(_), Some(Outcome::Value(_))) => {
                let ty = match (expected, &mut other) {
                    (Some(expected), _) if told => expected.clone(),
                    // CTRL-10 gives the one type to "an `if` whose value
                    // is used". One whose value is dropped has branches
                    // of any types.
                    _ if !used => Type::Unknown,
                    (_, Some(other)) => self.one_type(&mut then.value, &mut other.value),
                    (_, None) => then.value.ty.clone(),
                };
                Outcome::Value(ty)
            }
            // CTRL-13: with no `else`, "under a temporal condition it is
            // a connection of `T` that never ticks while the condition
            // is false (CTRL-3): `T`, not `T?`. Under a wiring-time
            // condition a branch that is an ordinary value makes it a
            // `T?`, as in a value function, and a branch that is a
            // connection makes it a connection of `T` that never ticks
            // where the branch is not composed." So a branch that is a
            // connection gives a connection of `T` whatever the
            // condition is. Otherwise, where the condition or the
            // branch is not settled, neither is the type.
            (Outcome::Value(ty), None) if self.graph => {
                use ValueCategory::{Connection, Unsettled, Value};
                match (selects, self.category(&then.value)) {
                    // A connection of `T` either way: selected at
                    // runtime, or composed where the condition holds.
                    (Connection, _) | (Value | Unsettled, Connection) => Outcome::Value(ty),
                    (Value, Value) => Outcome::Value(nullable(ty)),
                    (Unsettled, Value | Unsettled) | (Value, Unsettled) => {
                        Outcome::Value(Type::Unknown)
                    }
                }
            }
            // "With no `else` it is as if its `else` were null: its
            // value is a `T?`".
            (Outcome::Value(ty), None) => Outcome::Value(nullable(ty)),
            // The same holds where the branch written returns: the `if`
            // is then null and nothing else.
            (Outcome::Returns | Outcome::Null, None) => Outcome::Null,
            // A branch that is null alone is a null branch: it "makes
            // it `T?`".
            (Outcome::Null, Some(Outcome::Null)) => Outcome::Null,
            (Outcome::Null, Some(Outcome::Value(ty)))
            | (Outcome::Value(ty), Some(Outcome::Null)) => Outcome::Value(nullable(ty)),
            _ => Outcome::Nothing,
        };
        let ty = match &outcome {
            Outcome::Value(ty) => ty.clone(),
            Outcome::Nothing | Outcome::Returns | Outcome::Null => Type::Unknown,
        };
        let kind = ExprKind::If {
            condition: Box::new(condition),
            then: Box::new(then.value),
            otherwise: other.map(|other| Box::new(other.value)),
        };
        (Expr { kind, ty, at }, outcome)
    }

    /// The one type the values of two branches have, with nothing
    /// expected of them: the type of both; the wider of a `T` and a
    /// `T?`; an `f64` for an integer literal and an `f64`, the literal
    /// widened (EXPR-5). Two types are reported at the second branch.
    fn one_type(&mut self, then: &mut Expr, other: &mut Expr) -> Type {
        let (first, second) = (then.ty.clone(), other.ty.clone());
        if open(&first) || open(&second) {
            return Type::Unknown;
        }
        if agree(&first, &second) || self.admits(&first, &second) {
            return first;
        }
        if self.admits(&second, &first) {
            return second;
        }
        let float = Type::Scalar(Scalar::F64);
        if second == float
            && let Some(widened) = widened_block(then)
        {
            *then = widened;
            return float;
        }
        if first == float
            && let Some(widened) = widened_block(other)
        {
            *other = widened;
            return float;
        }
        self.diagnostics.emit(error(
            Category::Type,
            codes::TYPE_MISMATCH,
            other.at,
            format!("this branch is `{second}`, and the other is `{first}`"),
            self.conditional_rule(),
        ));
        first
    }

    /// The body of a loop. CTRL-11: a `var` is "never [assigned] by a
    /// loop's body, which may run no times"; nor does a `return` in it
    /// end every path.
    fn loop_body(&mut self, block: Option<&SyntaxNode>) -> Vec<Stmt> {
        let before = self.assigned.clone();
        let before_returned = self.returned;
        // ERR-12: a body may run no times, so what a null test proves
        // inside it is proved of nothing after the loop.
        let frame = self.present.len();
        let statements = match block {
            Some(block) => block
                .children()
                .map(|statement| self.statement(&statement))
                .collect(),
            None => Vec::new(),
        };
        self.present.truncate(frame);
        self.assigned = restored(&before, self.locals.len(), true);
        self.returned = before_returned;
        statements
    }

    /// CTRL-12: "`while condition { ... }` repeats its body while the
    /// condition holds; the form with no condition is a source's
    /// (SOURCE-4) and rejects there."
    fn while_loop(&mut self, node: &SyntaxNode) -> Stmt {
        let at = self.at(node);
        let condition = node
            .children()
            .find(|part| part.kind() != SyntaxKind::Block);
        let condition = match condition {
            Some(condition) => Some(self.condition(&condition)),
            // CTRL-15: "the form with no condition is a source's".
            None if self.generating => None,
            None => {
                let keyword = tokens(node)
                    .next()
                    .map_or(at, |keyword| Location::new(self.file, keyword.span()));
                self.diagnostics.emit(error(
                    Category::Phase,
                    codes::WHILE_CONDITION,
                    keyword,
                    format!(
                        "a `while` with no condition is for a source that yields: in a {} it \
                         has one",
                        if self.graph {
                            "graph"
                        } else {
                            "value function"
                        }
                    ),
                    self.loop_rule(),
                ));
                None
            }
        };
        // CTRL-15: "The condition of a `while` is an ordinary `bool`,
        // tested while the graph is built: a connection there rejects
        // (FN-3)".
        if let Some(condition) = &condition
            && self.graph
            && self.category(condition) == ValueCategory::Connection
        {
            self.diagnostics.emit(error(
                Category::Phase,
                codes::CONNECTION_AS_CONSTANT,
                condition.at,
                "the condition of a `while` is tested while the graph is built, and this is \
                 a connection"
                    .to_string(),
                "FN-3",
            ));
        }
        // ERR-12: the body is reached only where the condition holds,
        // so what it proves is not null is not null there.
        let frame = self.present.len();
        if let Some(condition) = &condition {
            let (when_true, _) = self.null_tests(condition);
            self.present.extend(when_true);
        }
        let body = self.loop_body(child(node, SyntaxKind::Block).as_ref());
        self.present.truncate(frame);
        Stmt::While {
            condition,
            body,
            at,
        }
    }

    /// CTRL-12: "`for item in items` runs its body for each element of a
    /// list, in index order, or of a set, and `for key, value in
    /// entries` for each key and value of a dictionary. One name over a
    /// dictionary, two over a list or a set, and any other type after
    /// `in` reject."
    fn for_loop(&mut self, node: &SyntaxNode) -> Stmt {
        let at = self.at(node);
        // The pattern's names: the identifiers directly inside the
        // statement, but the `in` that ends them (LEX-10).
        let mut names: Vec<SyntaxToken> = tokens(node)
            .filter(|token| token.kind() == SyntaxKind::Ident)
            .collect();
        if names.last().is_some_and(|token| token.text() == "in") {
            names.pop();
        }
        let Some(sequence) = node
            .children()
            .find(|part| part.kind() != SyntaxKind::Block)
        else {
            return self.unread(node);
        };
        let sequence = self.expression(&sequence, None);
        let types: Vec<Type> = match (&sequence.ty, names.len()) {
            (ty, count) if open(ty) => vec![Type::Unknown; count],
            (Type::List { element, .. } | Type::Set(element), 1) => vec![(**element).clone()],
            (Type::Dict { key, value }, 2) => vec![(**key).clone(), (**value).clone()],
            (ty, count) => {
                let takes = if count == 1 {
                    "`for` with one name loops over a list or a set"
                } else {
                    "`for` with two names loops over a dictionary"
                };
                self.diagnostics.emit(error(
                    Category::Type,
                    codes::TYPE_MISMATCH,
                    sequence.at,
                    format!("this is `{ty}`, and {takes}"),
                    self.loop_rule(),
                ));
                vec![Type::Unknown; count]
            }
        };
        // CTRL-15: what the loop binds, and whether it makes one child
        // for each member.
        let over = if self.graph {
            self.category(&sequence)
        } else {
            ValueCategory::Value
        };
        let (categories, children) = bound_by_loop(over, &sequence.ty, names.len());
        let outside = self.locals.len();
        let names: Vec<LocalId> = names
            .iter()
            .zip(types)
            .map(|(name, ty)| {
                let at = Location::new(self.file, name.span());
                self.declare(name.text(), at, LocalKind::Iteration, ty, true)
            })
            .collect();
        for (local, category) in names.iter().zip(categories) {
            self.locals[*local].category = category;
        }
        // Each such loop says which `var`s are outside it: one declared
        // in an enclosing loop's body is outside the loop nested there.
        // A loop that makes no children keeps the boundary of the one
        // it stands in.
        let outer = self.dynamic;
        if children {
            self.dynamic = Some(outside);
        }
        let body = self.loop_body(child(node, SyntaxKind::Block).as_ref());
        self.dynamic = outer;
        Stmt::For {
            names,
            sequence,
            body,
            at,
        }
    }

    /// How many members the enum `ty` has; `None` for a type that is no
    /// enum.
    fn enum_members(&self, ty: &Type) -> Option<usize> {
        let Type::Named {
            module,
            declaration,
            ..
        } = ty
        else {
            return None;
        };
        match self.program.modules[*module].details.get(*declaration) {
            Some(Detail::Enum(read)) => Some(read.members.len()),
            _ => None,
        }
    }

    /// CTRL-12: "A `switch` there selects on a `bool`, `i64`, `str`, or
    /// enum value. Each label is a constant of the selector's type."
    /// CTRL-6: "Duplicate values reject, including differently spelled
    /// equal enum members. There is no fallthrough ... Enum coverage
    /// can establish exhaustiveness". CTRL-11: a `var` is assigned
    /// "after a `switch` when every case that does not return assigns
    /// it and the cases leave no value out, with a `default` or by
    /// covering an enum".
    fn switch(&mut self, node: &SyntaxNode) -> Stmt {
        let at = self.at(node);
        let Some(selector) = node.children().find(|part| {
            !matches!(
                part.kind(),
                SyntaxKind::SwitchCase | SyntaxKind::SwitchDefault
            )
        }) else {
            return self.unread(node);
        };
        let selector = self.expression(&selector, None);
        // CTRL-15: "a `switch` over an ordinary value composes the one
        // case selected, and over a connection each case is a child
        // graph selected at runtime ... a case's assignments and returns
        // follow CTRL-14."
        let outer = self.temporal;
        if self.graph && self.category(&selector) == ValueCategory::Connection {
            self.temporal = Some(self.locals.len());
        }
        let members = self.enum_members(&selector.ty);
        let selects = open(&selector.ty)
            || members.is_some()
            || matches!(
                selector.ty,
                Type::Scalar(Scalar::Bool | Scalar::I64 | Scalar::Str)
            );
        if !selects {
            self.diagnostics.emit(error(
                Category::Type,
                codes::TYPE_MISMATCH,
                selector.at,
                format!(
                    "this is `{}`, and a `switch` selects on a `bool`, `i64`, `str` or enum \
                     value",
                    selector.ty
                ),
                self.loop_rule(),
            ));
        }
        // What a label is expected to be.
        let of = if selects {
            selector.ty.clone()
        } else {
            Type::Unknown
        };
        let before = self.assigned.clone();
        let before_returned = self.returned;
        // What is assigned at the end of each case, and whether it has
        // returned.
        let mut ends: Vec<(Vec<bool>, bool)> = Vec::new();
        let mut seen: Vec<(ConstantValue, Location)> = Vec::new();
        let mut cases = Vec::new();
        let mut default = None;
        for part in node.children() {
            match part.kind() {
                SyntaxKind::SwitchCase => {
                    let mut inside = part.children();
                    let Some(written) = inside.next() else {
                        continue;
                    };
                    self.assigned = restored(&before, self.locals.len(), false);
                    self.returned = before_returned;
                    let label = self.expression(&written, Some(&of));
                    let label = self.fit(label, &of, "the selector");
                    // CTRL-6: "Switch labels are typed source constants,
                    // compared by resolved value": each is read as a
                    // value of the selector's type (`crate::constant`).
                    let read = self.read_constant(&written, Some(&of));
                    // FN-3: "Where a constant is required, a `switch`
                    // label ... a connection rejects."
                    if self.graph && self.category(&label) == ValueCategory::Connection {
                        self.diagnostics.emit(error(
                            Category::Phase,
                            codes::CONNECTION_AS_CONSTANT,
                            label.at,
                            "a `switch` label is a constant, and this is a connection".to_string(),
                            "FN-3",
                        ));
                    } else if matches!(read, Err(Fault::NotConstant)) {
                        // UNIT-13: a constant is "a compile-time constant
                        // where ... a `switch` label (CTRL-12) requires
                        // one". CTRL-12: "Each label is a constant of
                        // the selector's type". GEN-8 says what such a
                        // constant is: "a literal, an enum member however
                        // it is written (TYPE-7) ... a constant (UNIT-13),
                        // a generic value parameter in scope, or a
                        // constant expression of those". One that is a
                        // constant whose value this pass does not work
                        // out, `-{shifted: true} 1` of a candidate of the
                        // program's own, is a label: whether it repeats
                        // another is left unjudged (CTRL-6, "compared by
                        // resolved value").
                        self.diagnostics.emit(local_error(
                            Category::Type,
                            codes::local::SWITCH_LABEL_CONSTANT,
                            label.at,
                            "a `switch` label is a compile-time constant, and this is known \
                             only when the program runs"
                                .to_string(),
                            self.loop_rule(),
                        ));
                    }
                    let constant = read.ok();
                    let value = constant
                        .as_ref()
                        .and_then(|constant| self::label(self.program, constant, &of));
                    if let Some(constant) = constant {
                        match seen
                            .iter()
                            .find(|(other, _)| other.same(&constant) == Some(true))
                        {
                            Some((_, first)) => self.diagnostics.emit(
                                error(
                                    Category::Type,
                                    codes::SWITCH_DUPLICATE_LABEL,
                                    label.at,
                                    "this label has the value of an earlier one".to_string(),
                                    "CTRL-6",
                                )
                                .with_related(*first, "first here"),
                            ),
                            None => seen.push((constant, label.at)),
                        }
                    }
                    // ERR-12: what a null test proves in one case is
                    // proved of no other, and of nothing after the
                    // `switch`: another case may be the one selected.
                    let frame = self.present.len();
                    let body = inside.map(|statement| self.statement(&statement)).collect();
                    self.present.truncate(frame);
                    ends.push((self.assigned.clone(), self.returned));
                    cases.push(hir::Case { label, value, body });
                }
                SyntaxKind::SwitchDefault => {
                    self.assigned = restored(&before, self.locals.len(), false);
                    self.returned = before_returned;
                    let frame = self.present.len();
                    let body = part
                        .children()
                        .map(|statement| self.statement(&statement))
                        .collect();
                    self.present.truncate(frame);
                    ends.push((self.assigned.clone(), self.returned));
                    default = Some(body);
                }
                _ => {}
            }
        }
        let covered = seen
            .iter()
            .filter(|(label, _)| matches!(label, ConstantValue::Member(_)))
            .count();
        let exhaustive = default.is_some() || members.is_some_and(|count| covered == count);
        let count = self.locals.len();
        if exhaustive && !ends.is_empty() {
            self.assigned = (0..count)
                .map(|local| {
                    // A local a later case declares is not assigned
                    // at the end of an earlier one.
                    ends.iter().all(|(assigned, returned)| {
                        *returned || assigned.get(local).copied().unwrap_or(false)
                    })
                })
                .collect();
            self.returned = ends.iter().all(|(_, returned)| *returned);
        } else {
            self.assigned = restored(&before, count, false);
            self.returned = before_returned;
        }
        self.temporal = outer;
        Stmt::Switch {
            selector,
            cases,
            default,
            at,
        }
    }
}

/// What was assigned at `before`, for as many locals as there are now.
/// One declared since is `since`: assigned, when its block has ended
/// and it is in scope no more; not assigned, when it is a `switch`
/// case's, which is in scope after the `switch` (UNIT-12) on paths that
/// did not declare it.
fn restored(before: &[bool], count: usize, since: bool) -> Vec<bool> {
    let mut assigned = before.to_vec();
    assigned.resize(count, since);
    assigned
}

/// What is assigned where two paths meet: a local both assign, a path
/// that has returned counting as assigning all (CTRL-5: "Definite
/// assignment ignores paths that have already returned").
fn joined(first: (&[bool], bool), second: (&[bool], bool), count: usize) -> Vec<bool> {
    let on = |(assigned, returned): (&[bool], bool), local: usize| {
        returned || assigned.get(local).copied().unwrap_or(true)
    };
    (0..count)
        .map(|local| on(first, local) && on(second, local))
        .collect()
}

/// The expression that is the value of a branch written as a block: its
/// last statement, when that is an expression.
fn value_of_branch(branch: &SyntaxNode) -> Option<SyntaxNode> {
    if branch.kind() != SyntaxKind::Block {
        return None;
    }
    branch
        .children()
        .last()
        .filter(|last| last.kind() == SyntaxKind::ExprStmt)
        .and_then(|last| last.children().next())
}

/// True when a branch's value takes its type from its place: `null` or
/// `.member`, as the value of a block, of a branch of the `if` of an
/// `else if`, or in parentheses.
fn branch_takes_its_type(branch: &SyntaxNode) -> bool {
    let branch = &ungrouped(branch.clone());
    match branch.kind() {
        SyntaxKind::Block => {
            value_of_branch(branch).is_some_and(|value| branch_takes_its_type(&value))
        }
        // The condition is first; the branches follow it.
        SyntaxKind::IfExpr => branch
            .children()
            .skip(1)
            .any(|inner| branch_takes_its_type(&inner)),
        _ => takes_its_type(branch),
    }
}

/// True when a branch's value, found as [`branch_takes_its_type`] finds
/// it, is `null`.
fn branch_is_null(branch: &SyntaxNode) -> bool {
    let branch = &ungrouped(branch.clone());
    match branch.kind() {
        SyntaxKind::Block => value_of_branch(branch).is_some_and(|value| branch_is_null(&value)),
        SyntaxKind::IfExpr => branch
            .children()
            .skip(1)
            .any(|inner| branch_is_null(&inner)),
        _ => is_null(branch),
    }
}

/// `T?` for a `T`; a type that is nullable already, or not known, as it
/// is.
fn nullable(ty: Type) -> Type {
    if open(&ty) || matches!(ty, Type::Nullable(_)) {
        ty
    } else {
        Type::Nullable(Box::new(ty))
    }
}

/// The block `branch` with its value, an integer literal, widened to an
/// `f64` (EXPR-5); `None` when its value is no integer literal.
fn widened_block(branch: &Expr) -> Option<Expr> {
    let ExprKind::Block {
        statements,
        value: Some(value),
    } = &branch.kind
    else {
        return None;
    };
    if !is_integer_literal(value) {
        return None;
    }
    let float = Type::Scalar(Scalar::F64);
    let widened = Expr {
        kind: ExprKind::Widen(value.clone()),
        ty: float.clone(),
        at: value.at,
    };
    Some(Expr {
        kind: ExprKind::Block {
            statements: statements.clone(),
            value: Some(Box::new(widened)),
        },
        ty: float,
        at: branch.at,
    })
}

/// A constant as the stages after the analyser hold a `switch` label's
/// value (`hir::Case::value`) and a key a child is read by: of the
/// types a label may be (CTRL-12), and a float. A member of an enum is
/// by its position among the members of `ty`, the selector's enum.
/// `None` for one of another type, or not valued.
fn label(program: &Program, value: &ConstantValue, ty: &Type) -> Option<Label> {
    Some(match value {
        ConstantValue::Int(value) => Label::Int(*value),
        ConstantValue::Bool(value) => Label::Bool(*value),
        ConstantValue::Str(value) => Label::Str(value.clone()),
        ConstantValue::Float(value) => return Label::float(*value),
        ConstantValue::Member(name) => {
            let Some(Type::Named {
                module,
                declaration,
                ..
            }) = not_null(Some(ty))
            else {
                return None;
            };
            let Some(Detail::Enum(read)) = program.modules[*module].details.get(*declaration)
            else {
                return None;
            };
            Label::Member(
                read.members
                    .iter()
                    .position(|member| member.name == *name)?,
            )
        }
        _ => return None,
    })
}

/// True when a value of `ty` has members that `value.member` reads: a
/// struct's fields, a native type's properties, a result's payloads, an
/// error's and its context's parts, and a delta's properties. An
/// `atomic<S>` under a `?` is an input that may be omitted, and is not
/// one (TYPE-14).
fn has_members(ty: &Type) -> bool {
    matches!(
        ty,
        Type::Named {
            kind: NamedKind::Struct | NamedKind::Native,
            ..
        } | Type::Result { .. }
            | Type::Error
            | Type::ErrorContext
            | Type::Delta(_)
    )
}

/// True when a type is known all the way down: no part of it is
/// `Type::Unknown`.
fn whole(ty: &Type) -> bool {
    !ty.any_part(&|part| matches!(part, Type::Unknown), &|_| false)
}

/// What an argument may be given to: a parameter of a function, or a
/// field of a struct.
struct Slot<'a> {
    /// Its name.
    name: &'a str,
    /// Its type; not known when it names a generic parameter that
    /// nothing has given an argument.
    ty: Type,
    /// True when a call that gives it nothing is rejected.
    required: bool,
    /// The pack it is, when it is one (FN-8).
    pack: Option<Pack>,
    /// True for a parameter of a `const fn`: a trace given to it is the
    /// list of its publications there (TST-16).
    listed: bool,
}

/// The places a call's arguments are given to, where their types name
/// generic parameters to be found from the arguments (GEN-2).
struct Places<'s> {
    /// Each place: its name, whether it must be given, the pack it is.
    slots: &'s [Slot<'s>],
    /// The type each place is declared with, generic parameters and
    /// all. One for each of `slots`.
    patterns: Vec<&'s Type>,
    /// The generic parameters being found.
    generics: &'s [GenericParameter],
    /// True when nothing later says more of these places: the call's
    /// own place is known, or it has none. False while the call is read
    /// before its place is, which may yet say what a parameter is.
    settled: bool,
    /// The names that are no place's: a named argument under one gives
    /// a generic value parameter of the callee (GEN-8), and is read
    /// with the call's generic arguments.
    aside: &'s [&'s str],
    /// What the call gives those: each with the position of its
    /// parameter among `generics`.
    values: &'s [(usize, SyntaxNode)],
    /// The default of each generic value parameter that has one (GEN-2):
    /// an entry for each of `generics`, or none at all.
    defaults: Vec<Option<Constant>>,
}

/// What [`Typer::infer_arguments`] read.
struct Inferred {
    /// Every argument, each fitted to its place.
    arguments: Vec<hir::Argument>,
    /// What is known of each generic parameter.
    found: Vec<Option<Argument>>,
    /// True when an argument other than `null` and `.member`, which
    /// have no type of their own, has no whole type.
    unknown: bool,
    /// The generic value parameters that took their defaults, by their
    /// positions among the generic parameters.
    defaulted: Vec<usize>,
}

/// What takes the arguments of a call.
///
/// `#[derive(Clone, Copy, PartialEq)]` asks the compiler to write the
/// code that copies one of these and compares two of them.
#[derive(Clone, Copy, PartialEq)]
enum Takes {
    /// The parameters of a function: positional arguments, then named
    /// ones (EXPR-2).
    Parameters,
    /// The fields of a struct: named arguments only (TYPE-5).
    Fields,
}

impl Takes {
    /// The word a message uses for one of them.
    fn noun(self) -> &'static str {
        match self {
            Takes::Parameters => "parameter",
            Takes::Fields => "field",
        }
    }
}

/// The positions of a call's arguments in the order their places are
/// declared in, those given to no place last.
///
/// GEN-12: "A prior binding constrains every later occurrence in
/// declared parameter order." So in `same(right: 1, left: 2.0)` it is
/// `left` that says what `T` is.
fn declared_order(assigned: &[(Option<usize>, SyntaxNode)]) -> Vec<usize> {
    let mut order: Vec<usize> = (0..assigned.len()).collect();
    // A stable sort: two arguments of one place keep their written
    // order.
    order.sort_by_key(|&position| assigned[position].0.unwrap_or(usize::MAX));
    order
}

/// The key of the global store under which a run keeps its output
/// (core's CRUN-8).
const RUN_OUTPUT_KEY: &str = "run.output";

/// TST-16: "A trace given to a `const fn` is an ordinary value there:
/// for a result type T a `list<tuple<datetime, delta<T>>>`, one tuple
/// for each publication, in time order."
///
/// A value that is a trace becomes that list. A trace's cell is already
/// the `delta<T>` of what it is of (TST-5). Any other value, one that
/// holds a trace among them, is as it was.
fn publications(value: hir::Expr) -> hir::Expr {
    let Type::Trace { cell, .. } = &value.ty else {
        return value;
    };
    let ty = Type::List {
        element: Box::new(Type::Tuple(vec![
            Type::Scalar(Scalar::Datetime),
            (**cell).clone(),
        ])),
        size: None,
    };
    let at = value.at;
    hir::Expr {
        kind: ExprKind::Publications(Box::new(value)),
        ty,
        at,
    }
}

/// True when `ty` is a trace or holds one at any depth: the
/// `list<trace of i64>` of `[seen]`.
fn holds_trace(ty: &Type) -> bool {
    ty.any_part(&|part| matches!(part, Type::Trace { .. }), &|_| false)
}

/// An argument of an endpoint operation or an `endpoint<T>` parameter as
/// every check reads it: EXPR-1, "Parentheses select grouping", so
/// `valid((quote.bid))` names what `valid(quote.bid)` does, and grouping
/// never changes what an argument is, its admission or its guard.
fn endpoint_argument(node: &SyntaxNode) -> SyntaxNode {
    ungrouped(node.clone())
}

/// The arguments of the endpoint operation whose list is `list`, each
/// as `endpoint_argument` reads it.
fn endpoint_arguments(list: &SyntaxNode) -> Vec<SyntaxNode> {
    arguments_of(list)
        .into_iter()
        .map(|(_, value)| endpoint_argument(&value))
        .collect()
}

/// The arguments of an argument list, in order: each with its name,
/// when it has one, and its value.
fn arguments_of(list: &SyntaxNode) -> Vec<(Option<SyntaxToken>, SyntaxNode)> {
    list.children()
        .filter(|argument| argument.kind() == SyntaxKind::Arg)
        .filter_map(|argument| {
            let value = argument.children().next()?;
            // `name: value`: the name is the first token, any word
            // (LEX-9).
            let name = has_token(&argument, SyntaxKind::Colon)
                .then(|| tokens(&argument).next())
                .flatten();
            Some((name, value))
        })
        .collect()
}

/// Where each generic parameter before a type pack is declared: the
/// ones an argument is given to place for place (GEN-2).
fn fixed(generics: &[GenericParameter]) -> impl Iterator<Item = Location> + '_ {
    generics
        .iter()
        .take_while(|generic| !matches!(generic.kind, GenericKind::Pack { .. }))
        .map(|generic| generic.at)
}

/// True when `given` are arguments the generic parameters `generics`
/// can be given: as many, where there is no type pack, and each a type
/// or a value as its parameter is (GEN-2, GEN-14).
fn applies(generics: &[GenericParameter], given: &[Argument]) -> bool {
    let packed = generics
        .iter()
        .any(|generic| matches!(generic.kind, GenericKind::Pack { .. }));
    (packed || generics.len() == given.len())
        && generics
            .iter()
            .zip(given)
            .all(|(generic, given)| match (&generic.kind, given) {
                (GenericKind::Value(_), Argument::Type(ty)) => matches!(ty, Type::Unknown),
                (GenericKind::Type | GenericKind::Plain, Argument::Constant(_)) => false,
                _ => true,
            })
}

/// The position among `generics` of the type parameter a type is, when
/// it is one and nothing more: the `T` of `value: T`, and not of
/// `values: list<T>`.
fn bare_parameter(ty: &Type, generics: &[GenericParameter]) -> Option<usize> {
    match ty {
        Type::Parameter { at, .. } => generics.iter().position(|generic| generic.at == *at),
        _ => None,
    }
}

/// Finds what generic parameters stand for from a field's type,
/// `pattern`, and the type of the value the field is given, `actual`:
/// `T` is `i64` when `list<T>` is given a `list<i64>`, and `N` is 2
/// when `list<i64, N>` is given a `list<i64, 2>`. `found` has an entry
/// for each of `generics`; one already found is left as it is, and a
/// type that is not known finds nothing.
fn infer(
    pattern: &Type,
    actual: &Type,
    generics: &[GenericParameter],
    found: &mut [Option<Argument>],
) {
    match (pattern, actual) {
        (_, Type::Unknown) => {}
        (Type::Parameter { at, .. }, actual) => {
            find(*at, Argument::Type(actual.clone()), generics, found);
        }
        (Type::Nullable(pattern), Type::Nullable(actual)) => {
            infer(pattern, actual, generics, found)
        }
        // A `T` given where a `T?` is expected (TYPE-14).
        (Type::Nullable(pattern), actual) => infer(pattern, actual, generics, found),
        (
            Type::Named {
                module,
                declaration,
                arguments,
                ..
            },
            Type::Named {
                module: actual_module,
                declaration: actual_declaration,
                arguments: actual_arguments,
                ..
            },
        ) if (module, declaration) == (actual_module, actual_declaration) => {
            for pair in arguments.iter().zip(actual_arguments) {
                match pair {
                    (Argument::Type(pattern), Argument::Type(actual)) => {
                        infer(pattern, actual, generics, found);
                    }
                    (Argument::Constant(pattern), Argument::Constant(actual)) => {
                        infer_constant(pattern, actual, generics, found);
                    }
                    _ => {}
                }
            }
        }
        (Type::Tuple(patterns), Type::Tuple(actuals)) if patterns.len() == actuals.len() => {
            for (pattern, actual) in patterns.iter().zip(actuals) {
                infer(pattern, actual, generics, found);
            }
        }
        (
            Type::List { element, size },
            Type::List {
                element: actual,
                size: actual_size,
            },
        ) => {
            infer(element, actual, generics, found);
            if let (Some(size), Some(actual_size)) = (size, actual_size) {
                infer_constant(size, actual_size, generics, found);
            }
        }
        (Type::Set(pattern), Type::Set(actual))
        | (Type::Ref(pattern), Type::Ref(actual))
        | (Type::Endpoint(pattern), Type::Endpoint(actual))
        | (Type::Atomic(pattern), Type::Atomic(actual))
        | (Type::Delta(pattern), Type::Delta(actual))
        | (Type::Trace { cell: pattern, .. }, Type::Trace { cell: actual, .. }) => {
            infer(pattern, actual, generics, found)
        }
        // TYPE-9: "For non-composite S, `atomic<S>` normalizes to S
        // before matching". An `atomic<T>` given a value of a type that
        // is its own atomic says that `T` is that type.
        (Type::Atomic(pattern), actual)
            if !matches!(Type::atomic(actual.clone()), Type::Atomic(_)) =>
        {
            infer(pattern, actual, generics, found)
        }
        (
            Type::Dict { key, value },
            Type::Dict {
                key: actual_key,
                value: actual_value,
            },
        ) => {
            infer(key, actual_key, generics, found);
            infer(value, actual_value, generics, found);
        }
        (
            Type::RollingWindow {
                value,
                maximum,
                minimum,
            },
            Type::RollingWindow {
                value: actual,
                maximum: actual_maximum,
                minimum: actual_minimum,
            },
        ) => {
            infer(value, actual, generics, found);
            infer_constant(maximum, actual_maximum, generics, found);
            // TYPE-11: "Omitted minimum equals maximum."
            if let Some(minimum) = minimum {
                let actual_minimum = actual_minimum.as_ref().unwrap_or(actual_maximum);
                infer_constant(minimum, actual_minimum, generics, found);
            }
        }
        (
            Type::Result { value, error },
            Type::Result {
                value: actual_value,
                error: actual_error,
            },
        ) => {
            infer(value, actual_value, generics, found);
            infer(error, actual_error, generics, found);
        }
        _ => {}
    }
}

/// [`infer`], for a size or a constant argument: a generic value
/// parameter written there is the constant given in its place.
fn infer_constant(
    pattern: &Constant,
    actual: &Constant,
    generics: &[GenericParameter],
    found: &mut [Option<Argument>],
) {
    // A value nothing has said anything of says nothing here either:
    // an argument, or a default, may yet say what the parameter is.
    if is_unsaid(actual) {
        return;
    }
    infer_value(&pattern.value, actual, generics, found);
}

/// [`infer_constant`], for the value `pattern` is, at any depth inside a
/// list or a tuple: `[N, 1]` given `[2, 1]` says `N` is 2.
fn infer_value(
    pattern: &ConstantValue,
    actual: &Constant,
    generics: &[GenericParameter],
    found: &mut [Option<Argument>],
) {
    match (pattern, &actual.value) {
        (ConstantValue::Parameter { at, .. }, _) => {
            find(*at, Argument::Constant(actual.clone()), generics, found);
        }
        (ConstantValue::List(patterns), ConstantValue::List(values))
        | (ConstantValue::Tuple(patterns), ConstantValue::Tuple(values))
            if patterns.len() == values.len() =>
        {
            for (pattern, value) in patterns.iter().zip(values) {
                // An element has no text of its own: it is written as
                // the value it is.
                let element = Constant {
                    text: value.key().unwrap_or_else(|| actual.text.clone()),
                    value: value.clone(),
                };
                infer_value(pattern, &element, generics, found);
            }
        }
        _ => {}
    }
}

/// The constant that stands for a generic value nothing has said
/// anything of, in an output requested in part: it agrees with any, and
/// decides nothing.
fn unsaid() -> Constant {
    Constant {
        text: "_".to_string(),
        value: ConstantValue::Unknown,
    }
}

/// True for [`unsaid`]. No constant a source writes is `_`.
fn is_unsaid(constant: &Constant) -> bool {
    constant.text == "_" && matches!(constant.value, ConstantValue::Unknown)
}

/// Records that the generic parameter declared at `at` stands for
/// `argument`, unless something has been found for it already or the
/// argument is a type with a part that is not known.
fn find(
    at: Location,
    argument: Argument,
    generics: &[GenericParameter],
    found: &mut [Option<Argument>],
) {
    // A type that is not known all the way down decides nothing: the
    // `tuple<list<_>, i64>` of `([], 0)` would let any list through in
    // its unknown part. A later argument may say the whole of it, and
    // this one is then read again as that.
    if let Argument::Type(ty) = &argument
        && !whole(ty)
    {
        return;
    }
    let position = generics.iter().position(|generic| generic.at == at);
    if let Some(position) = position
        && found[position].is_none()
    {
        found[position] = Some(argument);
    }
}

/// True when nothing can be said against a value of this type: it is
/// not known, or it is `any`, whose typing is for a later pass
/// (TYPE-8).
fn open(ty: &Type) -> bool {
    matches!(ty, Type::Unknown | Type::Scalar(Scalar::Any))
}

/// True when an `i64` is widened in a place that expects `expected`: an
/// `f64`, or an `f64?`.
fn accepts_float(expected: &Type) -> bool {
    match expected {
        Type::Scalar(Scalar::F64) => true,
        Type::Nullable(inner) => accepts_float(inner),
        // TYPE-16: an `f64` is a value where a `delta<f64?>` is
        // expected.
        Type::Delta(of) => matches!(&**of, Type::Nullable(payload) if accepts_float(payload)),
        _ => false,
    }
}

/// A type once generic arguments are put in place of the parameters
/// declared at `own`: not known when it still names one of them, which
/// has then been given no argument.
fn placed(ty: Type, own: &[Location]) -> Type {
    if mentions(&ty, own) {
        Type::Unknown
    } else {
        ty
    }
}

/// True when `ty` names one of the generic parameters declared at
/// `own`: as a type, or as a value in a size or a constant argument.
fn mentions(ty: &Type, own: &[Location]) -> bool {
    names_one_of(ty, own) || names_value_of(ty, own)
}

/// True when a size or a constant argument inside `ty` names a generic
/// value parameter declared at one of `own`: the `N` of `list<i64, N>`,
/// of `list<i64, N + 1>`, of `Box<[N]>`.
fn names_value_of(ty: &Type, own: &[Location]) -> bool {
    ty.any_part(&|_| false, &|constant| constant.value.names_one_of(own))
}

/// What is expected of an expression that takes its type from its
/// place, `node`, beside an operand of type `other`: for `.member`, the
/// operand's type; for `null`, the operand's type when it is nullable.
fn beside(node: &SyntaxNode, other: &Type) -> Type {
    if is_implied_member(node) || matches!(other, Type::Nullable(_)) {
        other.clone()
    } else {
        Type::Unknown
    }
}

/// True when an expression is the literal `null`, in parentheses or not.
fn is_null(node: &SyntaxNode) -> bool {
    takes_its_type(node) && !is_implied_member(node)
}

/// EXPR-5: "An integer literal is an `f64` where one is expected." True
/// when an expression is one, under a minus or not.
fn is_integer_literal(value: &Expr) -> bool {
    match &value.kind {
        ExprKind::Literal(Value::Int(_)) => true,
        // `-1`, as it is written or as the call of core's `neg_` it
        // stands for.
        _ => negated(value).is_some_and(is_integer_literal),
    }
}

/// True for the call a symbol stands for where it is core's own and
/// nothing more: of a candidate the operator's own module declares,
/// with no argument beside its `operands` operands. What a module's own
/// candidate computes, or a call given an option as an argument, is not
/// known to this pass. An option that is a generic value parameter is
/// no argument: it selects the candidate, a division's `zero` say, and
/// what that computes is core's.
///
/// A symbol recorded as it is written, an [`ExprKind::Unary`] or an
/// [`ExprKind::Binary`], is core's own.
fn is_plain(kind: &ExprKind, operands: usize) -> bool {
    match kind {
        ExprKind::Call {
            callee: Callee::Candidate {
                operator, module, ..
            },
            arguments,
            ..
        } => operator.0 == *module && arguments.len() == operands,
        ExprKind::Call { .. } => false,
        _ => true,
    }
}

/// The operand of a minus that is core's negation: `-value` as it is
/// written, or the plain call of `neg_` it stands for (see
/// [`is_plain`]).
fn negated(value: &Expr) -> Option<&Expr> {
    match value.kind.unary() {
        Some((UnaryOperator::Negate, operand)) if is_plain(&value.kind, 1) => Some(operand),
        _ => None,
    }
}

/// The operator and operands of a symbol between two that is core's
/// arithmetic: as it is written, or the plain call it stands for (see
/// [`is_plain`]).
fn arithmetic(value: &Expr) -> Option<(BinaryOperator, &Expr, &Expr)> {
    value.kind.binary().filter(|_| is_plain(&value.kind, 2))
}

/// True when an expression is `.member`, in parentheses or not.
fn is_implied_member(node: &SyntaxNode) -> bool {
    ungrouped(node.clone()).kind() == SyntaxKind::ImpliedMember
}

/// True for the literal `null`, in parentheses or not.
fn written_null(node: &SyntaxNode) -> bool {
    let node = &ungrouped(node.clone());
    match node.kind() {
        SyntaxKind::Literal => tokens(node).any(|token| token.kind() == SyntaxKind::KwNull),
        _ => false,
    }
}

/// True when an expression takes its type from its place and has none
/// of its own: `null`, `.member`, and `ok(value)` or `err(value)`,
/// whose "type arguments come from the complete `result<T,E>` its place
/// expects" (ERR-2), in parentheses or not.
fn takes_its_type(node: &SyntaxNode) -> bool {
    let node = &ungrouped(node.clone());
    match node.kind() {
        SyntaxKind::ImpliedMember => true,
        SyntaxKind::Literal => tokens(node).any(|token| token.kind() == SyntaxKind::KwNull),
        // ERR-13: the callee `ok` or `err` is the constructor, whatever
        // else is in scope.
        SyntaxKind::CallExpr => node
            .children()
            .next()
            .filter(|callee| {
                callee.kind() == SyntaxKind::NameExpr
                    && child(callee, SyntaxKind::QualifiedName).is_none()
            })
            .and_then(|callee| name_token(&callee))
            .is_some_and(|name| matches!(name.text(), "ok" | "err")),
        _ => false,
    }
}

/// True when two types are applications of one struct, whatever their
/// generic arguments.
fn same_declaration(left: &Type, right: &Type) -> bool {
    match (left, right) {
        (
            Type::Named {
                module,
                declaration,
                ..
            },
            Type::Named {
                module: other_module,
                declaration: other_declaration,
                ..
            },
        ) => (module, declaration) == (other_module, other_declaration),
        _ => false,
    }
}

/// True when `node` is the integer literal 2^63, which is one more
/// than the greatest `i64`.
fn is_least_magnitude(node: &SyntaxNode) -> bool {
    node.kind() == SyntaxKind::Literal
        && tokens(node).next().is_some_and(|token| {
            token.kind() == SyntaxKind::Int
                && decode_integer(token.text()) == Ok(i64::MIN.unsigned_abs())
        })
}

/// The names of a delta's properties, for a message: `` `added` and
/// `removed` ``.
fn listed(properties: &[(String, Type)]) -> String {
    let names: Vec<String> = properties
        .iter()
        .map(|(name, _)| format!("`{name}`"))
        .collect();
    match names.as_slice() {
        [] => "no property".to_string(),
        [only] => only.clone(),
        [first @ .., last] => format!("{} and {last}", first.join(", ")),
    }
}

/// Adds to `indices` each index of the place `place`, from the
/// outermost in: the `i` and the `j` of `rows[i].cells[j]`.
fn place_indices<'e>(place: &'e Expr, indices: &mut Vec<&'e Expr>) {
    match &place.kind {
        ExprKind::Index { base, index } => {
            indices.push(index);
            place_indices(base, indices);
        }
        ExprKind::Field { base, .. } => place_indices(base, indices),
        _ => {}
    }
}

/// True for a type that is a temporal shape and no ordinary value
/// type, under a `?` or not: a window, an atomic, a reference, a
/// signal, an endpoint. What a default of one is held to is not
/// settled.
fn shaped(ty: &Type) -> bool {
    matches!(
        not_null(Some(ty)),
        Some(
            Type::RollingWindow { .. }
                | Type::Atomic(_)
                | Type::Ref(_)
                | Type::Signal
                | Type::Endpoint(_)
        )
    )
}

/// The type a value that is not `null` is expected to have: of `T?`,
/// `T` (TYPE-14: "either a T or null").
fn not_null(expected: Option<&Type>) -> Option<&Type> {
    match expected {
        Some(Type::Nullable(inner)) => not_null(Some(inner)),
        other => other,
    }
}

/// The operator a token is, between two operands.
fn binary_operator(kind: SyntaxKind) -> Option<BinaryOperator> {
    Some(match kind {
        SyntaxKind::Plus => BinaryOperator::Add,
        SyntaxKind::Minus => BinaryOperator::Subtract,
        SyntaxKind::Star => BinaryOperator::Multiply,
        SyntaxKind::Slash => BinaryOperator::Divide,
        SyntaxKind::SlashSlash => BinaryOperator::FloorDivide,
        SyntaxKind::Percent => BinaryOperator::Modulo,
        SyntaxKind::EqEq => BinaryOperator::Equal,
        SyntaxKind::BangEq => BinaryOperator::NotEqual,
        SyntaxKind::Lt => BinaryOperator::Less,
        SyntaxKind::LtEq => BinaryOperator::LessEqual,
        SyntaxKind::Gt => BinaryOperator::Greater,
        SyntaxKind::GtEq => BinaryOperator::GreaterEqual,
        SyntaxKind::Spaceship => BinaryOperator::Compare,
        SyntaxKind::AmpAmp => BinaryOperator::And,
        SyntaxKind::PipePipe => BinaryOperator::Or,
        _ => return None,
    })
}

/// The operator of a compound assignment: `+` for `+=`. `None` for `=`
/// and for a token that is no assignment.
fn compound(kind: SyntaxKind) -> Option<BinaryOperator> {
    Some(match kind {
        SyntaxKind::PlusEq => BinaryOperator::Add,
        SyntaxKind::MinusEq => BinaryOperator::Subtract,
        SyntaxKind::StarEq => BinaryOperator::Multiply,
        SyntaxKind::SlashEq => BinaryOperator::Divide,
        SyntaxKind::SlashSlashEq => BinaryOperator::FloorDivide,
        SyntaxKind::PercentEq => BinaryOperator::Modulo,
        _ => return None,
    })
}
