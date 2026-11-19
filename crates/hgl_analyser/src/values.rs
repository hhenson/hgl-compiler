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
use hgl_parser::{HasSpan, SyntaxKind, SyntaxNode, SyntaxToken};
use hgl_source::FileId;

use crate::constant::is_least_magnitude;
use crate::hir::{
    self, BinaryOperator, Body, Callee, EndpointOperation, ErrorPart, Expr, ExprKind, Form, Hook,
    Local, LocalId, Stmt, UnaryOperator, Value, ValueCategory,
};
use crate::program::{
    Binding, Declaration, DeclarationKind, LocalKind, Module, OperatorRef, Phase, Program, Role,
};
use crate::read::is_type;
use crate::syntax::{child, name_token, tokens, ungrouped};
use crate::types::{
    CallArguments, ConstantValue, Detail, GenericKind, NamedKind, Scalar, Signature, Type,
};
use crate::{codes, error};
use candidates::{Given, Key, Wanted, typed_by_place};
use control::{Outcome, branch_is_null, branch_takes_its_type, joined, label};
use endpoints::endpoint_argument;
use fit::{accepts_float, is_integer_literal, not_null, nullable, open, whole};
use operators::{options_of, unreadable};

pub(crate) use constants::generic_default;

// Arguments given to parameters or fields, and a struct's construction (EXPR-2, TYPE-5).
mod arguments;
// What every call is held to once it is built (FN-1, FN-9, FN-15, TIME-8).
mod call_checks;
// A call by name: of a value function, a form, or an enum's member (FN-4, EXPR-2).
mod calls;
// The candidate of an operator a call selects (GEN-5).
mod candidates;
// Capabilities, and the uses of one that reject (TIME-8).
mod capabilities;
// The read of a collection's child, and the endpoint a child is (TIME-1).
mod children;
// The values a body reads as compile-time constants (UNIT-8, UNIT-13, GEN-8).
mod constants;
// Blocks, `if`, `while`, `for` and `switch` (CTRL-5, CTRL-10 to CTRL-15).
mod control;
// A delta as a type of its own: its properties and its construction (TYPE-16).
mod deltas;
// The endpoint operations and an `endpoint<T>` parameter's argument (TIME-9).
mod endpoints;
// A test's block, `assert`, `eval` and traces; a run statement and the store (TST-3, RUN-1).
mod evals;
// What a place expects of a value, and the values that fit it (EXPR-5, TYPE-14).
mod fit;
// What a call's generic parameters stand for (GEN-2, GEN-8, GEN-12).
mod inference;
// `base.member`, `.member` and `base[index]` (TYPE-7, EXPR-7, ERR-6).
mod members;
// A node's body and a source's: hooks, headers, admission (FN-2, TIME-3).
mod node;
// A call of an operator: its requested output, arguments and candidates (GEN-5, GEN-6).
mod operators;
// `requires`, and what a generic body may call by it (GEN-3, GEN-7, GEN-16).
mod requirements;
// The result forms, the parts of a result and of an error, and `value?` (ERR-2 to ERR-7).
mod results;

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
            // Its type is the candidate's to say (GEN-6): it is read as a
            // value of that once the candidate is (`instance`). Whether
            // it is a constant at all is asked here, once.
            let value = self.read_constant(&given.node, None);
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

    /// True when `value` is a connection of a graph's body: what a
    /// temporal input or a graph's result is given in composition.
    fn composes(&self, value: &Expr) -> bool {
        self.graph
            && !self.testing
            && self.hook.is_none()
            && self.category(value) == ValueCategory::Connection
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

/// True when an expression gives a value where it stands. A block gives
/// the one it ends in (CTRL-10), and none when it ends in a `return` or
/// any other statement; an `if` gives one where a branch does.
fn gives_a_value(value: &Expr) -> bool {
    match value.kind.branch_values() {
        Some(values) => values.into_iter().any(gives_a_value),
        None => true,
    }
}

/// True when an expression holds a block, at any depth: reading it may
/// then declare a local, assign one, or return (CTRL-10).
fn holds_block(node: &SyntaxNode) -> bool {
    node.descendants()
        .any(|part| part.kind() == SyntaxKind::Block)
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
