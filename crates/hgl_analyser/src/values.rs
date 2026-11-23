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
//! type and no report: configuration given to `!`, `&&` or `||`, and a
//! conversion written with a type's name, `str(value)`. A type that is
//! not known is reported once, where
//! it was found, and never again where it is used (ERR-9).

use std::cell::RefCell;
use std::collections::HashMap;

use hgl_diagnostics::{Diagnostics, Location};
use hgl_parser::{HasSpan, SyntaxNode, SyntaxToken};
use hgl_source::FileId;

use crate::hir::{self, Expr, Hook, Local, LocalId, ValueCategory};
use crate::program::{
    Binding, Declaration, DeclarationKind, LocalKind, Module, OperatorRef, Phase, Program, Role,
};
use crate::types::{CallArguments, Detail, Scalar, Signature, Type};
use anonymous::Anonymous;
use control::label;
use endpoints::endpoint_argument;

pub(crate) use candidates::{Selecting, select_in_instance};
pub(crate) use constants::generic_default;
pub(crate) use lifting::record;

// The bodies of anonymous functions and their direct calls (FN-7, FN-14).
mod anonymous;
pub(crate) use anonymous::remap_locals;
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
mod dependent_packs;
pub(crate) use dependent_packs::PackTypes;
// The endpoint operations and an `endpoint<T>` parameter's argument (TIME-9).
mod endpoints;
pub(crate) use endpoints::node_endpoint_reads;
// A test's block, `assert`, `eval` and traces; a run statement and the store (TST-3, RUN-1).
mod evals;
// An expression, typed: literals, names, tuples and sequences (EXPR-3, EXPR-9).
mod expressions;
// What a place expects of a value, and the values that fit it (EXPR-5, TYPE-14).
mod fit;
// The form a result takes from its place, and a `T?` read where a T is expected (ERR-12, ERR-15).
mod forms;
mod forwarded_packs;
pub(crate) use forwarded_packs::{ForwardingInstance, has_forwarded_calls};
mod function_arguments;
// Fits destinations whose graph values become known only in an instance (GEN-7).
mod graph_instance;
pub(crate) use graph_instance::{GraphDestinations, GraphReturnCheck};
// What a call's generic parameters stand for (GEN-2, GEN-8, GEN-12).
mod inference;
// What a value is in a graph: a connection or an ordinary value (FN-3, FN-6).
mod law_calls;
mod lifting;
// `base.member`, `.member` and `base[index]` (TYPE-7, EXPR-7, ERR-6).
mod members;
// A node's body and a source's: hooks, headers, admission (FN-2, TIME-3).
mod node;
// Omitted configuration defaults in selected operator calls (GEN-6, UNIT-8).
mod operator_defaults;
pub(crate) use operator_defaults::{
    fit_operator_default, fit_required_configuration, operator_default_answers,
};
// A call of an operator: its requested output, arguments and candidates (GEN-5, GEN-6).
mod operators;
// `requires`, and what a generic body may call by it (GEN-3, GEN-7, GEN-16).
mod quantifiers;
mod requirements;
// The result forms, the parts of a result and of an error, and `capture` (ERR-2 to ERR-7).
mod results;
// The sizes a struct application or a call gives a list or a window (TYPE-10, TYPE-11).
mod sizes;
// A function's body and its statements: `let`, `var`, assignment, `return` (FN-7, EXPR-3).
mod statements;
// `-`, `!` and the operators with two operands, as calls of core's (NUM-1, EXPR-6).
mod symbols;

/// Types the body of every value function of the program, filling in
/// [`Module::bodies`].
pub(crate) fn check(program: &mut Program, diagnostics: &mut Diagnostics) {
    // The signature of what has none: a test, a run statement, a
    // constant.
    let empty = Signature {
        generics: Vec::new(),
        parameters: Vec::new(),
        result: None,
        reference: false,
    };
    // UNIT-13: the value each constant is written with, read as a value
    // of the constant's type with no parameter in scope. Every module's
    // are read before any body, which may read one as a `switch` label
    // does (CTRL-6: "compared by resolved value").
    for index in 0..program.modules.len() {
        let module = &program.modules[index];
        let (bindings, written, calls) = tables(module);
        let selecting = RefCell::new(Vec::new());
        let found = Tables {
            bindings: &bindings,
            written: &written,
            calls: &calls,
            selecting: &selecting,
        };
        let constants = module
            .declarations
            .iter()
            .enumerate()
            .map(|(position, declaration)| {
                let Some(Detail::Constant(ty)) = module.details.get(position) else {
                    return None;
                };
                Typer::new(program, module, declaration, &empty, found, diagnostics)
                    .constant(ty)
                    .map(|constant| {
                        let mut body = constant.into_body();
                        lifting::record(program, &mut body, false);
                        hir::DefaultExpr::from_body(body)
                    })
            })
            .collect();
        program.modules[index].constants = constants;
    }
    for index in 0..program.modules.len() {
        let module = &program.modules[index];
        let (bindings, written, calls) = tables(module);
        let selecting = RefCell::new(Vec::new());
        let found = Tables {
            bindings: &bindings,
            written: &written,
            calls: &calls,
            selecting: &selecting,
        };
        let mut bodies = Vec::with_capacity(module.declarations.len());
        // The calls each body selects for each instance, by the
        // declaration whose body it is.
        let mut selected = Vec::with_capacity(module.declarations.len());
        let mut graph_return_checks = Vec::with_capacity(module.declarations.len());
        for (position, declaration) in module.declarations.iter().enumerate() {
            let mut check_return = None;
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
                    let (body, pending) =
                        Typer::new(program, module, declaration, signature, found, diagnostics)
                            .graph();
                    check_return = pending;
                    body
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
                    let typer =
                        Typer::new(program, module, declaration, signature, found, diagnostics);
                    if declaration.kind == DeclarationKind::Operator {
                        typer.property_identities();
                    }
                    None
                }
            };
            let mut calls = selecting.take();
            if let Some(body) = &body {
                candidates::record_tag_tests(body, &mut calls);
            }
            bodies.push(body);
            selected.push(calls);
            graph_return_checks.push(check_return);
        }
        // TST-3: "A test is a composition-phase block with assertions."
        // It is no declaration: it has a name, and neither a parameter
        // nor a result, so it is read as a graph of that name with an
        // empty signature.
        let mut test_bodies = Vec::with_capacity(module.tests.len());
        for (owner, test) in module.tests.iter().enumerate() {
            let mut stand_in =
                Declaration::stand_in(&test.name, test.at, test.file, &test.syntax, true);
            stand_in.test_owner = Some(owner);
            let body = Typer::new(program, module, &stand_in, &empty, found, diagnostics)
                .test(test.integration);
            test_bodies.push(body);
        }
        // UNIT-14: the statements an application sets its run's store
        // with are no declaration either. They are read together, as
        // the statements of one block are.
        let mut store_body = module.store.first().and_then(|first| {
            let stand_in =
                Declaration::stand_in("store", first.at, first.file, &first.syntax, false);
            Typer::new(program, module, &stand_in, &empty, found, diagnostics).store(&module.store)
        });
        // RUN-1, RUN-6: an application's run statement reads the
        // run configuration injected in the statements before it.
        let mut run_body = module.run.as_ref().and_then(|run| {
            let stand_in = Declaration::stand_in("run", run.at, run.file, &run.syntax, false);
            Typer::new(program, module, &stand_in, &empty, found, diagnostics)
                .run_statement(store_body.as_ref())
        });
        // UNIT-8: "Defaults remain required-constant/cold expressions
        // checked against the expected type. Any parameter but a pack
        // may have one: a value function's, const configuration, or a
        // temporal input." Each is read as a value of its parameter's
        // type, whatever its declaration is: a function, a candidate,
        // an operator, or the contract of a native one.
        let mut defaults = Vec::with_capacity(module.declarations.len());
        selecting.borrow_mut().clear();
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
            let calls = selecting.take();
            if declaration.kind == DeclarationKind::Operator {
                // The operator has no body. Its inherited defaults use
                // this table when specialized in that scope (UNIT-7).
                selected[position].extend(calls);
            }
        }
        // What each expression is and which calls are lifted (FN-3,
        // FN-6), written on what was read for the stages after this
        // one. Only a graph's body composes.
        for (position, body) in bodies.iter_mut().enumerate() {
            if let Some(body) = body {
                let graph = module.phases.get(position) == Some(&Some(Phase::Graph));
                lifting::record(program, body, graph);
            }
        }
        for body in test_bodies
            .iter_mut()
            .chain(std::iter::once(&mut store_body))
            .flatten()
        {
            lifting::record(program, body, false);
        }
        if let Some(run) = &mut run_body {
            lifting::record_run(program, run);
        }
        for slot in defaults.iter_mut().flatten() {
            if let Some(default) = slot.take() {
                let mut body = default.into_body();
                lifting::record(program, &mut body, false);
                *slot = Some(hir::DefaultExpr::from_body(body));
            }
        }
        program.modules[index].bodies = bodies;
        program.modules[index].test_bodies = test_bodies;
        program.modules[index].run_body = run_body;
        program.modules[index].store_body = store_body;
        program.modules[index].defaults = defaults;
        program.modules[index].selecting = selected;
        program.modules[index].graph_return_checks = graph_return_checks;
    }
    // GEN-3: a `requires` not told where a call passes on a generic
    // parameter of its own is told where each call gives that one.
    operators::inherit_configuration_defaults(program);
    let default_calls = operator_defaults::check(program, diagnostics);
    requirements::instances_hold(program, default_calls, diagnostics);
    forwarded_packs::check_instances(program, diagnostics);
    // TYPE-10, TYPE-11: the sizes each struct application written in a
    // type gives the struct's lists and windows.
    sizes::applied_sizes(program, diagnostics);
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

/// Check the lexical body with the concrete types of an instance
/// (GEN-7). Endpoint admission and forwarded calls use the same local
/// flow and written annotations as ordinary source checking.
pub(crate) fn reread_instance_body(
    program: &Program,
    (module, declaration): (usize, usize),
    (signature, map): (&Signature, &[(Location, &crate::Argument)]),
    (functions, pack_inputs): (&[(usize, hir::Callee)], Option<&[hir::PackInput]>),
    project: &dyn Fn(&Type) -> Type,
    diagnostics: &mut Diagnostics,
) -> Option<hir::Body> {
    let owner = &program.modules[module];
    let declared = &owner.declarations[declaration];
    let (bindings, mut written, mut calls) = tables(owner);
    let span = declared.syntax.span();
    let in_body =
        |(file, start)| file == declared.at.file && (span.start..span.end).contains(&start);
    let projected: Vec<_> = written
        .iter()
        .filter(|&(&at, _)| in_body(at))
        .map(|(&at, ty)| (at, project(ty)))
        .collect();
    written.extend(projected.iter().map(|(at, ty)| (*at, ty)));
    let projected_calls: Vec<_> = calls
        .iter()
        .filter(|&(&at, _)| in_body(at))
        .map(|(&at, call)| {
            let arguments = call
                .arguments
                .iter()
                .map(|(at, argument)| {
                    let argument = match argument {
                        crate::Argument::Type(ty) => crate::Argument::Type(project(ty)),
                        crate::Argument::Constant(_) => crate::instance::placed(argument, map),
                    };
                    (*at, argument)
                })
                .collect();
            (
                at,
                CallArguments {
                    at: call.at,
                    arguments,
                },
            )
        })
        .collect();
    calls.extend(projected_calls.iter().map(|(at, call)| (*at, call)));
    let selecting = RefCell::new(Vec::new());
    let reading = Signature {
        generics: match &owner.details[declaration] {
            Detail::Callable(declared) => candidates::in_instance(declared, map).generics,
            _ => Vec::new(),
        },
        ..signature.clone()
    };
    let mut typer = Typer::new(
        program,
        owner,
        declared,
        &reading,
        Tables {
            bindings: &bindings,
            written: &written,
            calls: &calls,
            selecting: &selecting,
        },
        diagnostics,
    );
    typer.instance = map
        .iter()
        .map(|(at, argument)| (*at, (*argument).clone()))
        .collect();
    typer.pack_inputs = pack_inputs.map(|inputs| inputs.to_vec());
    typer.pack_functions = functions.to_vec();
    match owner.phases[declaration] {
        Some(Phase::Graph) => typer.graph().0,
        Some(Phase::Node) if crate::phases::is_source(declared) => typer.source(),
        Some(Phase::Node) => typer.node(),
        _ => typer.body(),
    }
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
    /// Where each call is put whose candidate a generic parameter of the
    /// declaration being read selects, for each instance to select
    /// ([`candidates::Selecting`]). A `RefCell` lets a reader that holds
    /// the tables by a shared reference add to it.
    selecting: &'a RefCell<Vec<Selecting>>,
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
    /// Absent while reading the enclosing declaration itself.
    anonymous: Option<Anonymous>,
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
    /// How many more expressions of the construct being read may be
    /// read a second time: see [`REREADS`].
    rereads: u32,
    /// How many expressions are being read around what is read now. A
    /// statement read where there is none is read once, and is a
    /// construct of its own ([`Typer::begin_construct`]).
    expressing: u32,
    /// Where the construct being read is written, for the report of a
    /// spent allowance.
    construct: Location,
    /// True once the construct's allowance has been spent and reported.
    spent: bool,
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
    /// For a switch or a conditional whose tail would return on every
    /// path, the switches whose enum coverage must still be proved
    /// (CTRL-6, CTRL-14).
    returning_tails: HashMap<Location, Vec<Location>>,
    /// Inside a composition loop that makes one child for each member,
    /// how many locals there were where the outermost such loop began:
    /// a `var` before that is declared outside it (CTRL-15).
    dynamic: Option<usize>,
    /// Names of active graph range passes with constant bounds (CTRL-16).
    unrolled_range_bindings: Vec<LocalId>,
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
    /// Where each inferred `let` holding a child endpoint comes from
    /// (ERR-14). Known paths still require the same guards at a read.
    bound_children: HashMap<LocalId, children::ChildOrigin>,
    /// The inferred initializer where a child is being held, without a
    /// payload read (ERR-14).
    binding_child: Option<Location>,
    /// True while a named test's block is read (TST-3): there `assert`
    /// is a statement, and what the test may not hold is reported by the
    /// phase pass and not read.
    testing: bool,
    /// True while value code is read that selects among all of an
    /// operator's candidates (GEN-5): a test's statements outside an
    /// `eval` or a `run` (TST-3) and an application's statements
    /// (UNIT-14).
    value_code: bool,
    /// True while the block read is an integration test's (TST-17):
    /// there `run` is written beside `eval`, and `run_config` may be
    /// injected (RUN-6).
    integration: bool,
    /// True while an application's statements before its run statement
    /// are read (UNIT-14): there `run_config` and `global_state` are
    /// injected (FN-9, RUN-6).
    applying: bool,
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
    /// The candidates of each operator called so far, by the operator's
    /// module and declaration and whether the temporal ones are among
    /// them. A chain of symbols asks for the same operator's at every
    /// link.
    candidates: HashMap<(usize, usize, bool), Vec<Candidate<'a>>>,
    /// The values of the module constants this function's constants
    /// have named (UNIT-13): a constant named twice is read once.
    constants: crate::constant::Constants,
    /// See [`Tables::selecting`].
    selecting: &'a RefCell<Vec<Selecting>>,
    /// What each generic parameter of the declaration being read stands
    /// for, where a call in its body is selected for one instance of it
    /// ([`candidates::Selecting`]): where each is declared, with its
    /// argument. Empty otherwise.
    instance: Vec<(Location, crate::types::Argument)>,
    /// Concrete entries of the enclosing instance's pack parameters.
    pack_inputs: Option<Vec<hir::PackInput>>,
    /// Compile-time callbacks available to forwarded calls.
    pack_functions: Vec<(usize, hir::Callee)>,
    /// The generic value parameters of the declaration being read that
    /// its `requires` binds, each where it is declared with the constant
    /// it is bound to (GEN-3: "A positive conjunctive equality may bind
    /// one unresolved generic from known data"): `M` of `requires M ==
    /// .slow` is `.slow` where the body gives it to an option.
    bound: Vec<(Location, crate::types::Argument)>,
    diagnostics: &'a mut Diagnostics,
}

/// How many expressions of one construct are read a second time, beside
/// [`REREADS_PER_NODE`] for each node of its syntax: as the type their
/// place turns out to have (EXPR-5), to try them as a candidate's
/// parameter (GEN-5), or to learn what a generic parameter or a
/// sequence's entries are (GEN-2, EXPR-9). An expression read again may
/// hold others that are, so the readings of a construct nested that way
/// grow with each level, and a bound is kept so that the pass ends
/// whatever it is given.
///
/// A construct is a statement read where no expression is being read,
/// which is read once: a statement of a body, of a hook, of a block a
/// statement holds; a node's declaration; a body written as one
/// expression. Each has an allowance of its own, so what one is judged
/// to be does not depend on what is read before it. One that needs more
/// is reported (local `reading.limit`), and is never left unjudged.
const REREADS: u32 = 10_000;

/// How many more expressions of a construct may be read a second time
/// for each node of its syntax: see [`REREADS`]. A construct that tries
/// each of its operands once as each candidate's parameter needs about
/// that many, so one that is long and not nested is judged whatever its
/// length.
const REREADS_PER_NODE: u32 = 16;

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
        let mut typer = Typer::bare(program, module, declaration, signature, tables, diagnostics);
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
        typer.bound = typer.bindings();
        typer.applications();
        typer
    }

    /// A reader that has declared nothing and checked nothing: for what
    /// is asked of the typed program as a whole, the `requires` of each
    /// instance a call makes ([`requirements::instances_hold`]).
    fn bare(
        program: &'a Program,
        module: &'a Module,
        declaration: &'a Declaration,
        signature: &'a Signature,
        tables: Tables<'a>,
        diagnostics: &'a mut Diagnostics,
    ) -> Typer<'a> {
        Typer {
            program,
            module,
            file: module.files[declaration.file].file,
            declaration,
            signature,
            anonymous: None,
            bindings: tables.bindings,
            written: tables.written,
            calls: tables.calls,
            locals: Vec::new(),
            declared: HashMap::new(),
            assigned: Vec::new(),
            rereads: allowance(&declaration.syntax),
            expressing: 0,
            construct: declaration.at,
            spent: false,
            returned: false,
            unsure: false,
            graph: false,
            temporal: None,
            must_return: false,
            returning_tails: HashMap::new(),
            dynamic: None,
            unrolled_range_bindings: Vec::new(),
            hook: None,
            out: None,
            present: Vec::new(),
            aliases: HashMap::new(),
            bound_children: HashMap::new(),
            binding_child: None,
            testing: false,
            value_code: false,
            integration: false,
            applying: false,
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
            selecting: tables.selecting,
            instance: Vec::new(),
            pack_inputs: None,
            pack_functions: Vec::new(),
            bound: Vec::new(),
            diagnostics,
        }
    }

    /// The current function's result, which is the anonymous function's
    /// while reading its body (FN-7, CTRL-5).
    fn result_type(&self) -> Option<&Type> {
        self.anonymous
            .as_ref()
            .map(|scope| &scope.result)
            .or(self.signature.result.as_ref())
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

    /// Installs the lexical scope used for operator selection in an instance.
    /// Contextual operands can read these bindings by name (EXPR-5, GEN-7).
    fn use_instance_locals(&mut self, locals: Vec<Local>) {
        self.locals = locals;
        self.index_instance_locals();
    }

    /// Rebuilds lookup tables for the concrete locals of the current graph.
    fn index_instance_locals(&mut self) {
        self.declared = self
            .locals
            .iter()
            .enumerate()
            .map(|(id, local)| (local.at, id))
            .collect();
        self.assigned = vec![true; self.locals.len()];
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
            selecting: self.selecting.borrow().len(),
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
        // A construct's spent allowance is reported once, and the report
        // stands though the reading it was spent in is taken back.
        let spent: Vec<_> = self.diagnostics.as_slice()[mark.diagnostics..]
            .iter()
            .filter(|diagnostic| {
                diagnostic.code == hgl_diagnostics::Code::Local(crate::codes::local::READING_LIMIT)
            })
            .cloned()
            .collect();
        self.diagnostics.truncate(mark.diagnostics);
        for diagnostic in spent {
            self.diagnostics.emit(diagnostic);
        }
        self.restore_read(mark, keep_flow);
        clean
    }

    /// Restores the surrounding scope after an anonymous function's
    /// body or a tentative read, keeping the current diagnostics.
    fn restore_read(&mut self, mark: Mark, keep_flow: bool) {
        // A call the reading put to be selected for each instance goes
        // with it: the reading that stands puts its own.
        self.selecting.borrow_mut().truncate(mark.selecting);
        // A reading that declared nothing has nothing of its own in the
        // tables of locals, and they are not walked: a reading taken
        // back costs what was read, not what the function holds.
        if self.locals.len() > mark.locals {
            self.locals.truncate(mark.locals);
            self.declared.retain(|_, local| *local < mark.locals);
            // What was noted of a local that is taken back goes with it:
            // its number is given to the next local declared.
            self.aliases.retain(|local, _| *local < mark.locals);
            self.bound_children.retain(|local, _| *local < mark.locals);
            self.captures.retain(|local, _| *local < mark.locals);
        }
        self.present.truncate(mark.present);
        if keep_flow {
            self.assigned.truncate(mark.locals);
        } else {
            self.assigned = mark.assigned;
            self.returned = mark.returned;
            self.unsure = mark.unsure;
        }
    }

    /// True when one more expression of the construct being read may be
    /// read a second time, which this counts.
    ///
    /// Where the construct's allowance is spent, it is reported once
    /// (local `reading.limit`) and this is false. The last of the
    /// allowance is not given, so `rereads` is 0 only once that report
    /// is made: a call a site then leaves unjudged for want of a reading
    /// is in a construct that is rejected (ERR-9).
    fn may_reread(&mut self) -> bool {
        if self.rereads > 1 {
            self.rereads -= 1;
            return true;
        }
        self.rereads = 0;
        if !self.spent {
            self.spent = true;
            self.diagnostics.emit(
                crate::local_error(
                    hgl_diagnostics::Category::Type,
                    crate::codes::local::READING_LIMIT,
                    self.construct,
                    "judging this reads its expressions again more often than this compiler \
                     does for one statement: an expression that takes its type from its place \
                     is tried for each candidate, and these nest"
                        .to_string(),
                    "EXPR-5",
                )
                .with_note("written as smaller statements, each `let` its own, it is judged"),
            );
        }
        false
    }

    /// Starts a construct at `node`, a statement or a node's declaration,
    /// where no expression is being read: it has its own allowance of
    /// rereads ([`REREADS`]). Inside an expression, which may be read
    /// again, `node` is part of the construct around it, and nothing
    /// starts. What is given back is the construct around it, which
    /// [`Typer::end_construct`] takes up again once `node` is read.
    fn begin_construct(&mut self, node: &SyntaxNode) -> Option<Construct> {
        if self.expressing > 0 {
            return None;
        }
        let outer = Construct {
            rereads: self.rereads,
            spent: self.spent,
            at: self.construct,
        };
        self.rereads = allowance(node);
        self.spent = false;
        self.construct = self.at(node);
        Some(outer)
    }

    /// Takes up again the construct `outer` that [`Typer::begin_construct`]
    /// gave back: a statement in a block a statement holds, `if c { ... }`,
    /// spends none of the allowance of the one that holds it.
    fn end_construct(&mut self, outer: Option<Construct>) {
        if let Some(outer) = outer {
            self.rereads = outer.rereads;
            self.spent = outer.spent;
            self.construct = outer.at;
        }
    }
}

/// A construct's allowance of rereads as it stands: see
/// [`Typer::begin_construct`].
struct Construct {
    /// How many more expressions of it may be read a second time.
    rereads: u32,
    /// True once its allowance has been spent and reported.
    spent: bool,
    /// Where it is written.
    at: Location,
}

/// The rereads a construct written at `node` may make: see [`REREADS`].
fn allowance(node: &SyntaxNode) -> u32 {
    let nodes = u32::try_from(node.descendants().count()).unwrap_or(u32::MAX);
    REREADS.saturating_add(nodes.saturating_mul(REREADS_PER_NODE))
}

/// `bool`.
const BOOL: Type = Type::Scalar(Scalar::Bool);

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
    /// How many calls are put to be selected for each instance.
    selecting: usize,
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
