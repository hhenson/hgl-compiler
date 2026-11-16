//! Types the bodies of value functions.
//!
//! `read` says what each written type means and `declared` checks the
//! declarations against one another. This pass reads what a value
//! function, `const fn`, does: it gives every expression of the body a
//! type, checks each against the type its place requires, and writes the
//! body down as [`hir::Body`] for the stages after the analyser.
//!
//! It is built up form by form. This part reads:
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
//!   lists and dictionaries (TYPE-7, EXPR-7, ERR-6).
//!
//! A form it does not read yet is left [`ExprKind::Unread`], with no
//! type and no report: control flow, operators given configuration, a
//! call of what is no value function or struct. A type that is not
//! known is reported once, where it was found, and never again where it
//! is used (ERR-9).

use std::collections::HashMap;

use hgl_diagnostics::{Category, Diagnostics, Location};
use hgl_lexer::literal::{
    decode_duration, decode_float, decode_integer, decode_string, decode_temporal,
};
use hgl_parser::{HasSpan, SyntaxKind, SyntaxNode, SyntaxToken};
use hgl_source::FileId;

use crate::declared::{agree, names_one_of, names_parameter, substitute_within};
use crate::hir::{
    self, BinaryOperator, Body, Callee, EndpointOperation, Expr, ExprKind, Form, Hook, Local,
    LocalId, Stmt, UnaryOperator, Value, ValueCategory,
};
use crate::program::{
    Binding, Declaration, DeclarationKind, LocalKind, Module, OperatorRef, Phase, Program, Role,
};
use crate::read::{child, default_of, has_token, is_type, parameters, tokens};
use crate::types::{
    Argument, CallArguments, Constant, ConstantValue, Detail, GenericKind, GenericParameter, Pack,
    Scalar, Signature, Type,
};
use crate::{codes, error};

/// Types the body of every value function of the program, filling in
/// [`Module::bodies`].
pub(crate) fn check(program: &mut Program, diagnostics: &mut Diagnostics) {
    for index in 0..program.modules.len() {
        let module = &program.modules[index];
        // What each name resolved to, by where it is used (UNIT-10).
        let bindings: HashMap<(FileId, u32), &Binding> = module
            .uses
            .iter()
            .map(|used| ((used.at.file, used.at.span.start), &used.binding))
            .collect();
        // What each written type means, by where it starts.
        let written: HashMap<(FileId, u32), &Type> = module
            .written
            .iter()
            .map(|written| ((written.at.file, written.at.span.start), &written.ty))
            .collect();
        // The generic arguments each call writes, by where its callee
        // starts (GEN-8).
        let calls: HashMap<(FileId, u32), &CallArguments> = module
            .call_arguments
            .iter()
            .map(|call| ((call.at.file, call.at.span.start), call))
            .collect();
        let mut bodies = Vec::with_capacity(module.declarations.len());
        for (position, declaration) in module.declarations.iter().enumerate() {
            let body = match (&declaration.kind, module.details.get(position)) {
                // FN-1: a `const fn`. GEN-4: "an `impl const fn` a value
                // candidate", whose body is a value function's.
                (
                    DeclarationKind::Function(Role::Value)
                    | DeclarationKind::Implementation(Role::Value, _),
                    Some(Detail::Callable(signature)),
                ) => Typer::new(
                    program,
                    module,
                    declaration,
                    signature,
                    Tables {
                        bindings: &bindings,
                        written: &written,
                        calls: &calls,
                    },
                    diagnostics,
                )
                .body(),
                // FN-2: a node, whose hooks are read as value code
                // (FN-3: "In a hook a temporal parameter is a value of
                // its declared type, read as in a value function"). A
                // source's contract is proposed, and its body is not
                // read.
                (
                    DeclarationKind::Function(Role::Temporal)
                    | DeclarationKind::Implementation(Role::Temporal, _),
                    Some(Detail::Callable(signature)),
                ) if module.phases.get(position) == Some(&Some(Phase::Node))
                    && !crate::phases::is_source(declaration) =>
                {
                    Typer::new(
                        program,
                        module,
                        declaration,
                        signature,
                        Tables {
                            bindings: &bindings,
                            written: &written,
                            calls: &calls,
                        },
                        diagnostics,
                    )
                    .node()
                }
                // FN-2: a graph, whose body describes composition.
                (
                    DeclarationKind::Function(Role::Temporal)
                    | DeclarationKind::Implementation(Role::Temporal, _),
                    Some(Detail::Callable(signature)),
                ) if module.phases.get(position) == Some(&Some(Phase::Graph)) => Typer::new(
                    program,
                    module,
                    declaration,
                    signature,
                    Tables {
                        bindings: &bindings,
                        written: &written,
                        calls: &calls,
                    },
                    diagnostics,
                )
                .graph(),
                _ => None,
            };
            bodies.push(body);
        }
        program.modules[index].bodies = bodies;
    }
}

/// What the earlier passes found of a module, each by the file and
/// offset it is written at.
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
    /// True once `start` has asked for a wake on the node's scheduler
    /// or alarm (TIME-3).
    driven: bool,
    /// Where the node writes `modified([])`, which stands only in a
    /// node that is so woken (TIME-3). The hooks may be written in any
    /// order, so each is judged when the whole node has been read.
    undriven: Vec<Location>,
    /// True while a `state`'s initializer is read (TIME-5).
    initializing: bool,
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
            driven: false,
            undriven: Vec::new(),
            initializing: false,
            requiring: 0,
            identities: HashMap::new(),
            candidates: HashMap::new(),
            diagnostics,
        };
        for parameter in &signature.parameters {
            // What a pack holds is for the pass that reads calls.
            let ty = match parameter.pack {
                Some(_) => Type::Unknown,
                None => parameter.ty.clone(),
            };
            typer.declare(
                &parameter.name,
                parameter.at,
                LocalKind::Parameter,
                ty,
                true,
            );
        }
        typer
    }

    fn at(&self, node: &SyntaxNode) -> Location {
        Location::new(self.file, node.span())
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
                    // temporal inputs."
                    self.initializing = statement.kind() == SyntaxKind::StateDecl;
                    statements.push(self.local(&statement));
                    self.initializing = false;
                }
                SyntaxKind::InjectDecl => statements.extend(self.inject(&statement)),
                SyntaxKind::LifecycleBlock | SyntaxKind::WhenStmt => {
                    statements.push(self.hook(&statement));
                }
                _ => statements.push(self.unread(&statement)),
            }
        }
        // TIME-3: "`modified([])` subscribes to no input: it stands
        // only in a node that injects `scheduler` or `alarm` and
        // schedules a wake on it in `start`, which is then what wakes
        // the handler."
        if !self.driven {
            for selector in std::mem::take(&mut self.undriven) {
                self.diagnostics.emit(
                    error(
                        Category::Phase,
                        codes::MODIFIED_NO_DRIVER,
                        selector,
                        "`modified([])` subscribes to no input, and nothing else wakes this node"
                            .to_string(),
                        "TIME-3",
                    )
                    .with_note(
                        "inject `scheduler` or `alarm` and schedule a wake on it in `start`: \
                         `start { schedule(scheduler, delay) }`",
                    ),
                );
            }
        }
        Some(Body {
            locals: self.locals,
            statements,
        })
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
            let admitted = header
                .as_ref()
                .and_then(|header| self.admission(header))
                .unwrap_or_else(|| self.temporal_inputs());
            self.present.extend(admitted.into_iter().map(Fact::Valid));
        }
        let reported = self.diagnostics.len();
        let condition = header.map(|condition| self.condition(&condition));
        // A header that is itself rejected says nothing sure of what it
        // admits: the reads in its body are then not judged (ERR-9).
        if self.diagnostics.len() != reported {
            let inputs = self.temporal_inputs();
            self.present.extend(inputs.into_iter().map(Fact::Valid));
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
        match header.kind() {
            SyntaxKind::ParenExpr => self.admission(&header.children().next()?),
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
            SyntaxKind::CallExpr => {
                let callee = header.children().next()?;
                let name = match callee.kind() {
                    SyntaxKind::NameExpr => name_token(&callee)?,
                    _ => return None,
                };
                if name.text() != "valid"
                    || self.bindings.contains_key(&(self.file, name.span().start))
                {
                    return None;
                }
                let list = child(header, SyntaxKind::ArgList)?;
                let arguments = arguments_of(&list);
                if arguments.is_empty() {
                    // `valid()`: every temporal input.
                    return Some(self.temporal_inputs());
                }
                // TIME-3: "`valid(a, b)` requires both named inputs,
                // and similarly for more inputs ... `valid([])` requires
                // no inputs to be valid." What names no input, the `[]`
                // among them, admits nothing here, and any other is
                // reported where the header is read.
                Some(
                    arguments
                        .iter()
                        .map(|(_, entry)| entry)
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
    /// TIME-4: "A payload read under manual admission must be dominated
    /// by a successful guard": in a `when`, the input is one its header
    /// admits, or a `valid(input)` holds where the read stands. TIME-5:
    /// "State initializers cannot read current temporal inputs."
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
        if self.hook != Some(Hook::When) || self.present.contains(&Fact::Valid(local)) {
            return;
        }
        self.diagnostics.emit(
            error(
                Category::Phase,
                codes::PAYLOAD_UNGUARDED,
                at,
                format!(
                    "`{name}` may not be valid here: this `when` does not require it to be, \
                     and no `valid({name})` guards the read"
                ),
                "TIME-4",
            )
            .with_note(format!(
                "a guard proves it where the read stands: `if valid({name}) {{ ... }}`"
            )),
        );
        // One report for one input on a path.
        self.present.push(Fact::Valid(local));
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
        let arguments: Vec<SyntaxNode> = arguments_of(list)
            .into_iter()
            .map(|(_, value)| value)
            .collect();
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
            self.undriven.push(selector);
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
            | EndpointOperation::Modified
            | EndpointOperation::Scheduled => BOOL,
            // END-3: "`delta_value(x)` returns the scalar value `x`
            // published this cycle".
            EndpointOperation::DeltaValue => inputs
                .as_ref()
                .and_then(|inputs| inputs.first())
                .map_or(Type::Unknown, |input| self.locals[*input].ty.clone()),
            // END-6: a `datetime?`.
            EndpointOperation::LastModified => nullable(Type::Scalar(Scalar::Datetime)),
            EndpointOperation::Invalidate
            | EndpointOperation::Activate
            | EndpointOperation::Passivate => Type::Unknown,
        };
        (ExprKind::Endpoint { operation, inputs }, ty)
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
        let temporal = |local: LocalId| {
            self.signature
                .parameters
                .get(local)
                .is_some_and(|parameter| !parameter.constant)
        };
        match local {
            Some(local) if temporal(local) || Some(local) == self.out => Some(local),
            _ => {
                self.diagnostics.emit(error(
                    Category::Phase,
                    codes::ENDPOINT_ARGUMENT,
                    self.at(node),
                    format!(
                        "`{}` takes a temporal input of `{}` by its name, or `out`",
                        operation.name(),
                        self.declaration.name
                    ),
                    "TIME-9",
                ));
                // Read for what it reports itself. The argument is
                // rejected already, so what it reads of an input is not
                // judged as well (ERR-9).
                let frame = self.present.len();
                let inputs = self.temporal_inputs();
                self.present.extend(inputs.into_iter().map(Fact::Valid));
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
                let value = self.fit(value, result, "the function's result");
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
            SyntaxKind::AssignStmt => self.assignment(node),
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
        let value = self.expression(node, result.as_ref());
        match &result {
            Some(result) => self.fit(value, result, "the function's result"),
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
        // A place inside a value, `point.x = 1` or `row[0] = 1`, is for
        // the pass that reads members.
        let local = match place.kind() {
            SyntaxKind::NameExpr => self.local_of(&place),
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
            self.assigned_category(local, &value);
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
    fn assigned_category(&mut self, local: LocalId, value: &Expr) {
        use ValueCategory::{Connection, Unsettled, Value};
        let given = self.category(value);
        if self.locals[local].category == Unsettled {
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
            (Connection, Value) => format!(
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
                value.at,
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
            // An anonymous function's body is for a later part. What it
            // captures of a capability is reported here.
            SyntaxKind::FnExpr => {
                self.captured_capabilities(node);
                (ExprKind::Unread, Type::Unknown)
            }
            SyntaxKind::ImpliedMember => self.implied_member(node, expected),
            SyntaxKind::IndexExpr => self.index(node),
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
        Expr { kind, ty, at }
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
            ExprKind::Construct { arguments, .. } => {
                all(&mut arguments.iter().map(|argument| &argument.value))
            }
            ExprKind::Unary { operand, .. } => self.category(operand),
            ExprKind::Binary { left, right, .. } => join(self.category(left), self.category(right)),
            ExprKind::Widen(inner) => self.category(inner),
            ExprKind::Field { base, .. } => self.category(base),
            ExprKind::Index { base, index } => join(self.category(base), self.category(index)),
            ExprKind::Tuple(entries) | ExprKind::Sequence(entries) => all(&mut entries.iter()),
            ExprKind::Block {
                value: Some(value), ..
            } => self.category(value),
            // CTRL-13: "It is a connection when its condition is one or
            // either branch's value is". CTRL-10: "A branch that returns
            // from the function gives the `if` no value and takes no
            // part in its type (CTRL-5)", and so none in what it is. An
            // `if` no branch of which gives a value has none itself.
            ExprKind::If {
                condition,
                then,
                otherwise,
            } => {
                let mut branches = [Some(then), otherwise.as_ref()]
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
    /// while the graph is built.
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
        let text = token.text();
        // A literal the lexer or `forms` has reported has no value
        // here, and no type: it is reported once (ERR-9).
        let read = match token.kind() {
            SyntaxKind::Int => decode_integer(text)
                .ok()
                .and_then(|value| i64::try_from(value).ok())
                .map(|value| (Value::Int(value), Scalar::I64)),
            SyntaxKind::Float => decode_float(text)
                .ok()
                .map(|value| (Value::Float(value), Scalar::F64)),
            SyntaxKind::Str => decode_string(text)
                .ok()
                .map(|value| (Value::Str(value), Scalar::Str)),
            SyntaxKind::KwTrue => Some((Value::Bool(true), Scalar::Bool)),
            SyntaxKind::KwFalse => Some((Value::Bool(false), Scalar::Bool)),
            SyntaxKind::Duration => decode_duration(text)
                .ok()
                .map(|value| (Value::Duration(value), Scalar::Duration)),
            SyntaxKind::Temporal => {
                temporal_type(text).map(|scalar| (Value::Temporal(text.to_string()), scalar))
            }
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
                        self.diagnostics.emit(error(
                            Category::Type,
                            codes::TYPE_MISMATCH,
                            self.at(node),
                            format!("`null` is no value of `{expected}`: write `{expected}?`"),
                            "TYPE-14",
                        ));
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
            _ => None,
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

    /// A name: a parameter, a local, or a generic value parameter. Any
    /// other, a function or a type or a name left for the prelude, is
    /// for the pass that reads calls and members.
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
            ExprKind::Block {
                value: Some(value), ..
            } => self.capability_of(value),
            ExprKind::If {
                then, otherwise, ..
            } => self.capability_of(then).or_else(|| {
                otherwise
                    .as_ref()
                    .and_then(|otherwise| self.capability_of(otherwise))
            }),
            _ => None,
        }
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
            // A sequence with a pair, `[key: value]`, is not read yet,
            // and so neither is what a dictionary holds.
            SyntaxKind::SequenceExpr => Some("an entry of a list or a set"),
            SyntaxKind::ReturnStmt => Some("what is returned"),
            // `out = clock` publishes it; `kept = clock`, of a `state`
            // or a `cache`, keeps it.
            SyntaxKind::AssignStmt => {
                let place = parent.children().next()?;
                if place == used {
                    return None;
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
        if self.is_temporal_input(local) && (self.hook.is_some() || self.initializing) {
            self.payload_read(local, at);
        }
        // TIME-7: "`start` neither reads nor writes it".
        if Some(local) == self.out && self.hook == Some(Hook::Start) {
            self.out_lifecycle(
                at,
                "`out` is neither read nor written in `start`".to_string(),
            );
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
        let refinable = |local: LocalId| {
            matches!(
                self.locals[local].kind,
                LocalKind::Parameter | LocalKind::Let | LocalKind::Iteration
            ) && matches!(self.locals[local].ty, Type::Nullable(_))
        };
        null_tests(condition, &refinable, &|| self.temporal_inputs())
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
        if items
            .iter()
            .any(|item| matches!(item.kind(), SyntaxKind::SequencePair | SyntaxKind::Hole))
        {
            return (ExprKind::Unread, Type::Unknown);
        }
        // A sequence is no `null`: of a `set<i64>?` expected, the set.
        let expected = not_null(expected);
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
        let reread = |this: &mut Self, operand: &Given, other: &Type| {
            if takes_its_type(&operand.node) {
                let expected = beside(&operand.node, other);
                this.expression(&operand.node, Some(&expected))
            } else {
                this.expression(&operand.node, None)
            }
        };
        match (left.retried, right.retried) {
            (false, false) => (left.value, right.value),
            (false, true) => {
                let right = reread(self, &right, &left.value.ty);
                (left.value, right)
            }
            (true, false) => {
                let left = reread(self, &left, &right.value.ty);
                (left, right.value)
            }
            // One that takes its type from the other is read after it.
            (true, true) if takes_its_type(&left.node) && !takes_its_type(&right.node) => {
                let right = reread(self, &right, &Type::Unknown);
                let left = reread(self, &left, &right.ty);
                (left, right)
            }
            (true, true) => {
                let left = reread(self, &left, &Type::Unknown);
                let right = reread(self, &right, &left.ty);
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
            given.constant = Some(self.constant_here(&given.node));
            if !self.is_compile_time(&given.node) {
                self.diagnostics.emit(error(
                    Category::Type,
                    codes::GENERIC_ARGUMENT_CONSTANT,
                    self.at(&given.node),
                    format!(
                        "`{name}` is a generic value parameter, and this is no compile-time \
                         constant: it is known only when the program runs"
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
            return self.called(operator, chosen, given, symbol.form);
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
        let read = self.call_read(node, expected);
        let callee = node
            .children()
            .next()
            .filter(|callee| callee.kind() == SyntaxKind::NameExpr)
            .and_then(|callee| name_token(&callee));
        if let Some(callee) = callee {
            self.note_wake(&callee, &read.0);
        }
        read
    }

    /// Notes a call in `start` that asks for a wake on the node's
    /// scheduler or alarm (TIME-3: the node "schedules a wake on it in
    /// `start`").
    ///
    /// That is `schedule(scheduler, ...)`, written in `start` itself,
    /// the capability given first: by its name, by a local it is bound
    /// to, or as the value of a block (TIME-8, CTRL-10). Core does not
    /// declare `schedule` yet, so the call is known by its name. What a
    /// function given the capability does with it is not followed: a
    /// wake asked for only there is not one this finds.
    fn note_wake(&mut self, callee: &SyntaxToken, kind: &ExprKind) {
        if self.hook != Some(Hook::Start) || self.driven || callee.text() != "schedule" {
            return;
        }
        let ExprKind::Call { arguments, .. } = kind else {
            return;
        };
        // The first argument: the one given to the first parameter, or
        // where the call is not read, the first written.
        let first = arguments
            .iter()
            .find(|argument| argument.parameter == Some(0))
            .or_else(|| arguments.first());
        self.driven = first
            .and_then(|argument| self.capability_of(&argument.value))
            .is_some_and(|capability| {
                let local = &self.locals[capability];
                local.kind == LocalKind::Inject
                    && matches!(local.name.as_str(), "scheduler" | "alarm")
            });
    }

    /// A call, as the form or the function its callee names.
    fn call_read(&mut self, node: &SyntaxNode, expected: Option<&Type>) -> (ExprKind, Type) {
        let (Some(callee), Some(list)) = (node.children().next(), child(node, SyntaxKind::ArgList))
        else {
            return (ExprKind::Unread, Type::Unknown);
        };
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
                && let Some(operation) = EndpointOperation::named(name.text())
            {
                return self.endpoint(operation, &name, &list);
            }
            return self.unread_call(&list);
        };
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
                    } else {
                        "value function"
                    }
                ),
                "FN-3",
            ));
        }
        self.unread_call(list)
    }

    /// A call this pass does not read: its arguments are read for what
    /// they report themselves, with nothing expected of them.
    fn unread_call(&mut self, list: &SyntaxNode) -> (ExprKind, Type) {
        let arguments = arguments_of(list)
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
        let program = self.program;
        let Some(Detail::Callable(signature)) = program.modules[module].details.get(declaration)
        else {
            return self.unread_call(list);
        };
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
            !parameter.has_default && !matches!(parameter.ty, Type::Nullable(_))
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
                    ty: known(&parameter.ty),
                    required: required(parameter),
                    pack: parameter.pack,
                })
                .collect();
            let arguments = self.bind(name, list, &slots, Takes::Parameters);
            let ty = signature.result.as_ref().map_or(Type::Unknown, known);
            let callee = Callee::Function {
                module,
                declaration,
                generics: Vec::new(),
            };
            let kind = ExprKind::Call {
                callee,
                arguments,
                form: Form::Named,
            };
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
            })
            .collect();
        let places = Places {
            slots: &slots,
            patterns: signature
                .parameters
                .iter()
                .map(|parameter| &parameter.ty)
                .collect(),
            generics,
            settled: !expected.is_some_and(open),
            aside: &value_names,
            values: &named,
            defaults: generics
                .iter()
                .map(|generic| {
                    generic_default(&callee.syntax, generic).map(|default| constant_at(&default))
                })
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
        let callee = Callee::Function {
            module,
            declaration,
            generics: resolved,
        };
        let kind = ExprKind::Call {
            callee,
            arguments,
            form: Form::Named,
        };
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
        for (name, value) in arguments_of(list) {
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
            if !self.is_compile_time(&value) {
                self.diagnostics.emit(error(
                    Category::Type,
                    codes::GENERIC_ARGUMENT_CONSTANT,
                    self.at(&value),
                    format!(
                        "`{}` is a generic value parameter, and this is no compile-time \
                         constant: it is known only when the program runs",
                        name.text()
                    ),
                    "GEN-8",
                ));
            }
            found[index] = Some(Argument::Constant(self.constant_here(&value)));
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
            if whole(&expected) {
                let value = self.expression(node, Some(&expected));
                self.fit(value, &expected, &place);
                continue;
            }
            // Its type names a parameter nothing has said anything of:
            // what it is given says.
            let value = self.expression(node, Some(&Type::Unknown));
            infer(pattern, &value.ty, generics, found);
            let expected = type_of(*index, found);
            self.fit(value, &expected, &place);
        }
        let program = self.program;
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
            if constant_fits(program, &default, &expected) == Some(false)
                && constant_fits(program, &default, declared) != Some(false)
                && let Some(kind) = written(&default)
            {
                self.diagnostics.emit(error(
                    Category::Type,
                    codes::TYPE_MISMATCH,
                    Location::new(self.file, callee.span()),
                    format!(
                        "this leaves `{}` to its default, {}, and `{}` is `{expected}` here",
                        generic.name,
                        kind.described(),
                        generic.name
                    ),
                    "GEN-13",
                ));
            }
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
            if ordered {
                if self.may_reread() {
                    let mark = self.mark();
                    let value = self.expression(node, Some(&Type::Unknown));
                    learn(self, &value.ty, &mut found);
                    self.take_back(mark, false);
                }
                continue;
            }
            let reported = self.diagnostics.len();
            let value = self.expression(node, Some(&Type::Unknown));
            learn(self, &value.ty, &mut found);
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
                match values[position].take() {
                    None if placeless => hir::Argument {
                        parameter: Some(index),
                        value: self.expression(node, None),
                    },
                    Some(value) if unfitted[position] => hir::Argument {
                        parameter: Some(index),
                        value,
                    },
                    Some(value) => {
                        let place = format!("the {} `{}`", takes.noun(), slot.name);
                        hir::Argument {
                            parameter: Some(index),
                            value: self.fit(value, &slot.ty, &place),
                        }
                    }
                    None => self.argument(Some(index), node, Some(&slot), takes),
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
        let operator_syntax =
            &program.modules[operator.module].declarations[operator.declaration].syntax;
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
                *found = Some(Argument::Constant(constant_at(&default)));
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
            if constant_fits(program, &default, &resolved) == Some(false)
                && constant_fits(program, &default, declared) != Some(false)
            {
                return Requested {
                    place: None,
                    among: None,
                    unfit: Some(Unfit {
                        generic: index,
                        default,
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
            default,
            resolved,
        }) = unfit
            && let Some(kind) = self::written(&default)
        {
            let generic = &public.generics[generic];
            self.diagnostics.emit(error(
                Category::Type,
                codes::TYPE_MISMATCH,
                Location::new(self.file, name.span()),
                format!(
                    "this leaves `{}` to its default, {}, and `{}` is `{resolved}` here",
                    generic.name,
                    kind.described(),
                    generic.name
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
        self.called(operator, chosen, given, Form::Named)
    }

    /// The declarations the name of `node` is bound to, when it is a
    /// lone name that names an enum.
    fn names_enum(&self, node: &SyntaxNode) -> bool {
        let Some((module, declarations)) = name_token(node)
            .filter(|_| node.kind() == SyntaxKind::NameExpr)
            .and_then(|name| self.declarations(&name))
        else {
            return false;
        };
        declarations.iter().any(|&declaration| {
            self.program.modules[module].declarations[declaration].kind == DeclarationKind::Enum
        })
    }

    /// The constant an option is given, for a candidate's condition to
    /// compare (GEN-6). An enum member is one however it is written
    /// (TYPE-7): `.nan`, `Zero.nan`, `alias::Zero.nan`.
    fn option_value(&self, node: &SyntaxNode) -> Option<OptionValue> {
        // EXPR-1: "Parentheses select grouping": `(Zero.nan)` is
        // `Zero.nan`.
        if node.kind() == SyntaxKind::ParenExpr {
            return self.option_value(&node.children().next()?);
        }
        if node.kind() == SyntaxKind::MemberExpr
            && node
                .children()
                .next()
                .is_some_and(|base| self.names_enum(&base))
        {
            // The member's name is the last token: any word (LEX-9).
            return tokens(node)
                .last()
                .map(|name| OptionValue::Member(name.text().to_string()));
        }
        constant_of(node)
    }

    /// The constant a declaration's default is, for a candidate's
    /// condition to compare. `ty` is the type the option is declared
    /// with.
    ///
    /// A default is written in its declaration's file, which may not be
    /// the one being read, so no name in it is looked up here. An enum
    /// member written in full, `Zero.ieee`, is known for one by the
    /// option's type being an enum: a member of any other would not be
    /// a value of it.
    fn default_value(&self, node: &SyntaxNode, ty: Option<&Type>) -> Option<OptionValue> {
        if node.kind() == SyntaxKind::ParenExpr {
            return self.default_value(&node.children().next()?, ty);
        }
        // TYPE-14: a `Zero?` is "either a T or null", and a member of
        // `Zero` is a value of it.
        let of_enum = match not_null(ty) {
            Some(Type::Named {
                module,
                declaration,
                ..
            }) => {
                self.program.modules[*module].declarations[*declaration].kind
                    == DeclarationKind::Enum
            }
            _ => false,
        };
        if of_enum && node.kind() == SyntaxKind::MemberExpr {
            // The member's name is the last token: any word (LEX-9).
            return tokens(node)
                .last()
                .map(|name| OptionValue::Member(name.text().to_string()));
        }
        constant_of(node)
    }

    /// True when an expression is a compile-time constant (GEN-8): "a
    /// literal, an enum member however it is written (TYPE-7), `Zero.nan`
    /// or `.nan`, a generic value parameter in scope, or a constant
    /// expression of those."
    fn is_compile_time(&self, node: &SyntaxNode) -> bool {
        match node.kind() {
            SyntaxKind::Literal | SyntaxKind::ImpliedMember => true,
            SyntaxKind::ParenExpr | SyntaxKind::UnaryExpr | SyntaxKind::BinaryExpr => node
                .children()
                .filter(|child| child.kind() != SyntaxKind::OperatorConfig)
                .all(|operand| self.is_compile_time(&operand)),
            SyntaxKind::MemberExpr => node
                .children()
                .next()
                .is_some_and(|base| self.names_enum(&base)),
            SyntaxKind::NameExpr => name_token(node).is_some_and(|name| {
                matches!(
                    self.bindings.get(&(self.file, name.span().start)),
                    Some(Binding::Local {
                        kind: LocalKind::GenericParameter,
                        ..
                    })
                )
            }),
            _ => false,
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

    /// The compile-time constant `node` is, read where it stands: a
    /// generic value parameter of the enclosing declaration is kept as
    /// that parameter, so that `keep(value, N: M)` gives `N` the `M` it
    /// names and not a constant of which nothing is known.
    fn constant_here(&self, node: &SyntaxNode) -> Constant {
        let name = |used: &SyntaxNode| {
            let Some(name) = name_token(used) else {
                return ConstantValue::Unevaluated;
            };
            match self.bindings.get(&(self.file, name.span().start)) {
                Some(Binding::Local {
                    at,
                    kind: LocalKind::GenericParameter,
                }) => ConstantValue::Parameter {
                    name: name.text().to_string(),
                    at: *at,
                },
                _ => ConstantValue::Unevaluated,
            }
        };
        Constant {
            text: node.text().to_string().trim().to_string(),
            value: crate::constant::fold(node, &name),
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
                let Some((taken, generics, defaulted)) =
                    instance((candidate, signature), public, given, wanted, &widens)
                else {
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
                    (constant_fits(program, &default, resolved) == Some(false))
                        .then(|| constant_fits(program, &default, declared) == Some(false))
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
        form: Form,
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
                let value = if given.retried {
                    // Read where it stands, as the parameter's type.
                    let value = self.expression(&given.node, Some(&parameter.ty));
                    self.fit(value, &parameter.ty, &place)
                } else {
                    self.fit(given.value, &parameter.ty, &place)
                };
                Some(hir::Argument {
                    parameter: Some(index),
                    value,
                })
            })
            .collect();
        let kind = ExprKind::Call {
            callee: Callee::Candidate {
                operator: (operator.module, operator.declaration),
                module,
                declaration,
                generics,
            },
            arguments,
            form,
        };
        (kind, signature.result.clone().unwrap_or(Type::Unknown))
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
        for (name, value) in arguments_of(list) {
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
        signature: &Signature,
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
        let program = self.program;
        let declared_in = &program.modules[module];
        let file = declared_in.files[candidate.file].file;
        let own: Vec<Location> = signature
            .generics
            .iter()
            .map(|generic| generic.at)
            .collect();
        let map: Vec<(Location, &Argument)> = own.iter().copied().zip(generics).collect();
        // What a type written in the condition means, the candidate's
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
        let mut verdicts = Vec::new();
        for call in calls {
            let verdict = (|| {
                let name = name_token(&call)?;
                // What the name is bound to where the candidate is
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
            })();
            let verdict = match verdict {
                Some((operator, operands, result)) => {
                    self.callable(operator, (file, operands), result.as_ref())
                }
                None => None,
            };
            verdicts.push((call.span().start, verdict));
        }
        verdicts
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
        (file, operands): (FileId, Vec<(SyntaxNode, Type)>),
        result: Option<&Type>,
    ) -> Option<bool> {
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
            return Some(false);
        }
        // The arrow is what the place of the call would expect. Where
        // the operator's defaults make the call's output another type,
        // the call does not give the arrow's: with
        // `operator count<T, const mode: i64 = 0>(value: T) -> i64`,
        // `requires count(bool) -> str` holds for no candidate.
        if let (Some(requested), Some(arrow)) = (&requested, result)
            && !self.admits(arrow, requested)
        {
            return Some(false);
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
            let stated = self.requirements().into_iter().any(|requirement| {
                requirement.operator == operator
                    && requirement.operands.iter().eq(types.iter().copied())
                    && self.asked_by(&requirement) == Some(asked.clone())
            });
            if stated {
                return Some(true);
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
        let (selected, untold) = self.admitted(operator, &candidates, &given, &wanted);
        self.requiring -= 1;
        (!untold).then_some(selected.len() == 1)
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
            .filter_map(|call| {
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
            },
            arguments,
            form,
        };
        (kind, result.unwrap_or(Type::Unknown))
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
        let callable = self.requirements_hold(candidate, signature, (module, generics), &condition);
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
        // no constant this pass reads.
        let this = &*self;
        let value_of = |name: &str| -> Option<OptionValue> {
            // An option that selects: a generic value parameter.
            let generic = signature.generics.iter().position(|generic| {
                matches!(generic.kind, GenericKind::Value(_)) && generic.name == name
            });
            if let Some(generic) = generic {
                let argument = bound
                    .iter()
                    .position(|&place| place == Place::Generic(generic));
                if let Some(position) = argument {
                    return this.option_value(&given[position].node);
                }
                // What was resolved for it: from an argument's type or
                // the result asked for, an integer or a duration, or
                // else its default. The `N` of `value: list<i64, N>`
                // given a list of two is 2 whatever its default says.
                let resolved = match generics.get(generic) {
                    Some(Argument::Constant(constant)) => &constant.value,
                    _ => return None,
                };
                match resolved {
                    ConstantValue::Int(_) | ConstantValue::Duration(_) => {
                        return constant_value(resolved);
                    }
                    // A generic value parameter of the enclosing
                    // function, or `unbounded`: not a value held here.
                    ConstantValue::Parameter { .. } | ConstantValue::Unbounded => return None,
                    // A constant of another kind is held as it is
                    // written, and one not inferred from a type is the
                    // default.
                    ConstantValue::Other(_) | ConstantValue::Unevaluated => {}
                }
                let declared = &signature.generics[generic];
                let default = generic_default(&candidate.syntax, declared)?;
                let ty = match &declared.kind {
                    GenericKind::Value(ty) => Some(ty),
                    _ => None,
                };
                return this.default_value(&default, ty);
            }
            let index = signature
                .parameters
                .iter()
                .position(|parameter| parameter.name == name)?;
            let argument = bound
                .iter()
                .position(|&place| place == Place::Parameter(index));
            if let Some(position) = argument {
                return this.option_value(&given[position].node);
            }
            let default =
                default_at(&candidate.syntax, &signature.parameters[index]).or_else(|| {
                    public
                        .parameters
                        .get(index)
                        .and_then(|parameter| default_at(&operator.syntax, parameter))
                });
            match default {
                Some(default) => {
                    this.default_value(&default, Some(&signature.parameters[index].ty))
                }
                // Written `T?` and left out: it is null.
                None => Some(OptionValue::Literal(Value::Null)),
            }
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
                let read = match (generics, self.declarations(&name)) {
                    (Some(generics), Some(found)) => {
                        self.call_named(&name, &list, found, expected, Some(generics))
                    }
                    _ => self.unread_call(&list),
                };
                // `schedule<duration>(scheduler, delay)` asks for a wake
                // as the call without its generic arguments does.
                self.note_wake(&name, &read.0);
                read
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
                    required: !member.has_default && !is_optional(&ty),
                    ty: placed(ty, &own),
                    pack: None,
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
                        .map(|generic| {
                            generic_default(&declared.syntax, generic)
                                .map(|default| constant_at(&default))
                        })
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
            }
        };
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
        for (name, value) in arguments_of(list) {
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
                let expected = slot.map(|index| &slots[index]);
                self.argument(slot, &value, expected, takes)
            })
            .collect()
    }

    /// The argument written at `node`, given to `slot`. One given to a
    /// pack, or to nothing, is read for what it reports itself.
    fn argument(
        &mut self,
        parameter: Option<usize>,
        node: &SyntaxNode,
        slot: Option<&Slot>,
        takes: Takes,
    ) -> hir::Argument {
        let value = match slot {
            Some(slot) => {
                let value = self.expression(node, Some(&slot.ty));
                let place = format!("the {} `{}`", takes.noun(), slot.name);
                self.fit(value, &slot.ty, &place)
            }
            None => self.expression(node, Some(&Type::Unknown)),
        };
        hir::Argument { parameter, value }
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
        let Type::Named {
            module,
            declaration,
            arguments,
            ..
        } = &base.ty
        else {
            return (ExprKind::Unread, Type::Unknown);
        };
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
                let index = self.expression(&index, Some(&integer));
                // EXPR-7: "by an `i64` index". An index of another type,
                // `pair[0 / 1]` or `pair[0 == 0]`, is reported as that,
                // and is not evaluated.
                let is_integer = self.admits(&integer, &index.ty);
                let index = self.fit(index, &integer, "the index of a tuple");
                let element = match constant_index(&index) {
                    _ if !is_integer => Some(&Type::Unknown),
                    Index::Known(value) => usize::try_from(value)
                        .ok()
                        .and_then(|position| elements.get(position)),
                    // A constant this pass does not evaluate: the
                    // element's type is then not known, and nothing is
                    // reported of it.
                    Index::Unevaluated => Some(&Type::Unknown),
                    Index::NotConstant => None,
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

    /// True when a value of type `ty` is a value of `expected`: the same
    /// type as far as each is known; a `T` where a `T?` is expected
    /// (TYPE-14: "either a T or null"); or a struct where an abstract
    /// struct it inherits from is expected.
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

    fn admits(&self, expected: &Type, ty: &Type) -> bool {
        if open(expected) || open(ty) || agree(expected, ty) {
            return true;
        }
        match expected {
            Type::Nullable(inner) => self.admits(inner, ty),
            Type::Named { .. } => self.inherits(ty, expected),
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

/// What a test has proved of a binding where the pass is reading.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Fact {
    /// It is not null (ERR-12).
    Present(LocalId),
    /// It is valid: a temporal input whose payload may be read
    /// (TIME-4).
    Valid(LocalId),
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
fn null_tests(
    condition: &Expr,
    refinable: &dyn Fn(LocalId) -> bool,
    inputs: &dyn Fn() -> Vec<LocalId>,
) -> (Vec<Fact>, Vec<Fact>) {
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
        ExprKind::Endpoint {
            operation: EndpointOperation::Valid,
            inputs: named,
        } => {
            let valid = match named {
                Some(named) => named.clone(),
                None => inputs(),
            };
            (valid.into_iter().map(Fact::Valid).collect(), Vec::new())
        }
        ExprKind::Unary {
            operator: UnaryOperator::Not,
            operand,
        } => {
            let (when_true, when_false) = null_tests(operand, refinable, inputs);
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
                let (left_true, left_false) = null_tests(left, refinable, inputs);
                let (right_true, right_false) = null_tests(right, refinable, inputs);
                (either(left_true, right_true), both(left_false, right_false))
            }
            BinaryOperator::Or => {
                let (left_true, left_false) = null_tests(left, refinable, inputs);
                let (right_true, right_false) = null_tests(right, refinable, inputs);
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

/// A named operator requirement of a declaration (GEN-16): `add_(T, T)
/// -> T` requires that `add_` is callable with two values of `T`, and
/// that it gives a `T`.
struct Requirement {
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
    /// The default, as written.
    default: SyntaxNode,
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
    match &value.kind {
        ExprKind::Block { value, .. } => value.as_deref().is_some_and(gives_a_value),
        ExprKind::If {
            then, otherwise, ..
        } => gives_a_value(then) || otherwise.as_deref().is_some_and(gives_a_value),
        _ => true,
    }
}

/// The requirement of the enclosing declaration that a call is
/// (GEN-7): see [`Typer::required`].
struct Required {
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
    /// parameter.
    constant: Option<Constant>,
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

/// The constant a generic value parameter is given, as it is written.
fn constant_at(node: &SyntaxNode) -> Constant {
    Constant {
        text: node.text().to_string().trim().to_string(),
        value: crate::constant::fold(node, &|_| ConstantValue::Unevaluated),
    }
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

/// A constant a candidate's condition compares an option with, or the
/// value a call gives an option (GEN-6).
#[derive(Debug, PartialEq)]
enum OptionValue {
    /// `.member`: the member's name. The enum it is of is the option's
    /// type, the same for what the call gives and what it is compared
    /// with.
    Member(String),
    /// A literal, as the value it denotes: `1_000` is `1000`, and a
    /// string is its contents.
    Literal(Value),
}

impl OptionValue {
    /// True when two constants are one value. An integer and a float
    /// are compared as numbers: an option of type `f64` may be given
    /// `1` (EXPR-5).
    fn same(&self, other: &OptionValue) -> bool {
        match (self, other) {
            (OptionValue::Literal(Value::Int(whole)), OptionValue::Literal(Value::Float(real)))
            | (OptionValue::Literal(Value::Float(real)), OptionValue::Literal(Value::Int(whole))) =>
            {
                // `as` here converts an integer to the nearest float.
                *whole as f64 == *real
            }
            _ => self == other,
        }
    }
}

/// The constant an expression is, when it is a literal, a number under
/// a minus, or a `.member`; `None` for any other expression.
fn constant_of(node: &SyntaxNode) -> Option<OptionValue> {
    match node.kind() {
        SyntaxKind::ParenExpr => constant_of(&node.children().next()?),
        // `.member`: the name is the last token, any word (LEX-9).
        SyntaxKind::ImpliedMember => tokens(node)
            .last()
            .map(|name| OptionValue::Member(name.text().to_string())),
        SyntaxKind::UnaryExpr if has_token(node, SyntaxKind::Minus) => {
            let operand = node.children().next()?;
            // LEX-7: the least `i64` is written as a minus over 2^63,
            // which is itself no `i64`.
            if is_least_magnitude(&operand) {
                return Some(OptionValue::Literal(Value::Int(i64::MIN)));
            }
            let value = match constant_of(&operand)? {
                OptionValue::Literal(Value::Int(value)) => Value::Int(value.checked_neg()?),
                OptionValue::Literal(Value::Float(value)) => Value::Float(-value),
                OptionValue::Literal(Value::Duration(value)) => {
                    Value::Duration(value.checked_neg()?)
                }
                _ => return None,
            };
            Some(OptionValue::Literal(value))
        }
        SyntaxKind::Literal => {
            let token = tokens(node).next()?;
            let text = token.text();
            let value = match token.kind() {
                SyntaxKind::Int => Value::Int(i64::try_from(decode_integer(text).ok()?).ok()?),
                SyntaxKind::Float => Value::Float(decode_float(text).ok()?),
                SyntaxKind::Str => Value::Str(decode_string(text).ok()?),
                SyntaxKind::KwTrue => Value::Bool(true),
                SyntaxKind::KwFalse => Value::Bool(false),
                SyntaxKind::KwNull => Value::Null,
                SyntaxKind::Duration => Value::Duration(decode_duration(text).ok()?),
                SyntaxKind::Temporal => Value::Temporal(text.to_string()),
                _ => return None,
            };
            Some(OptionValue::Literal(value))
        }
        // "A constant expression of those": integer and duration
        // arithmetic, as far as the pass that reads constants in types
        // evaluates it. `1 + 1` is `2`.
        _ => folded(node),
    }
}

/// What a compile-time constant is, as far as how it is written says:
/// no name in it is looked up.
pub(crate) enum Written {
    /// An integer literal, under a minus or not. It is an `f64` where
    /// one is expected (EXPR-5).
    Integer,
    /// Integer arithmetic: an `i64`.
    Whole,
    /// A float literal.
    Float,
    /// A string literal.
    Str,
    /// `true` or `false`.
    Bool,
    /// `null`.
    Null,
    /// A duration literal, or arithmetic of them.
    Duration,
    /// `.member`, by its name.
    Member(String),
}

impl Written {
    /// As a message names it.
    pub(crate) fn described(&self) -> String {
        match self {
            Written::Integer | Written::Whole => "an `i64`".to_string(),
            Written::Float => "an `f64`".to_string(),
            Written::Str => "a `str`".to_string(),
            Written::Bool => "a `bool`".to_string(),
            Written::Null => "`null`".to_string(),
            Written::Duration => "a `duration`".to_string(),
            Written::Member(name) => format!("the enum member `.{name}`"),
        }
    }
}

/// What the constant `node` is, when how it is written says.
pub(crate) fn written(node: &SyntaxNode) -> Option<Written> {
    match node.kind() {
        SyntaxKind::ParenExpr => written(&node.children().next()?),
        SyntaxKind::ImpliedMember => tokens(node)
            .last()
            .map(|name| Written::Member(name.text().to_string())),
        SyntaxKind::Literal => match tokens(node).next()?.kind() {
            SyntaxKind::Int => Some(Written::Integer),
            SyntaxKind::Float => Some(Written::Float),
            SyntaxKind::Str => Some(Written::Str),
            SyntaxKind::KwTrue | SyntaxKind::KwFalse => Some(Written::Bool),
            SyntaxKind::KwNull => Some(Written::Null),
            SyntaxKind::Duration => Some(Written::Duration),
            // A date or a time: which, its text says, and is not read
            // here.
            _ => None,
        },
        SyntaxKind::UnaryExpr if has_token(node, SyntaxKind::Minus) => {
            match written(&node.children().last()?)? {
                kind @ (Written::Integer | Written::Whole | Written::Float | Written::Duration) => {
                    Some(kind)
                }
                _ => None,
            }
        }
        _ => match crate::constant::fold(node, &|_| ConstantValue::Unevaluated) {
            ConstantValue::Int(_) => Some(Written::Whole),
            ConstantValue::Duration(_) => Some(Written::Duration),
            _ => None,
        },
    }
}

/// Whether the compile-time constant `node` is a value of `ty`, told
/// from how it is written. `None` where that does not say: a constant
/// that names something, or a type that is not known.
pub(crate) fn constant_fits(program: &Program, node: &SyntaxNode, ty: &Type) -> Option<bool> {
    let kind = written(node)?;
    // TYPE-14: a `T?` is "either a T or null".
    let (ty, optional) = match ty {
        Type::Nullable(inner) => (inner.as_ref(), true),
        other => (other, false),
    };
    match ty {
        // An `any` holds a value of any type (TYPE-8).
        Type::Scalar(Scalar::Any) => None,
        Type::Scalar(scalar) => match kind {
            Written::Integer => Some(matches!(scalar, Scalar::I64 | Scalar::F64)),
            // Whether arithmetic of integer literals is an `f64` where
            // one is expected is not judged here.
            Written::Whole => match scalar {
                Scalar::I64 => Some(true),
                Scalar::F64 => None,
                _ => Some(false),
            },
            Written::Float => Some(*scalar == Scalar::F64),
            Written::Str => Some(*scalar == Scalar::Str),
            Written::Bool => Some(*scalar == Scalar::Bool),
            Written::Duration => Some(*scalar == Scalar::Duration),
            Written::Null => Some(optional),
            Written::Member(_) => Some(false),
        },
        Type::Named {
            module,
            declaration,
            ..
        } => match program.modules[*module].details.get(*declaration) {
            Some(Detail::Enum(read)) => Some(match kind {
                Written::Member(name) => read.members.iter().any(|member| member.name == name),
                Written::Null => optional,
                _ => false,
            }),
            _ => None,
        },
        _ => None,
    }
}

/// The integer or duration a constant expression comes to.
fn folded(node: &SyntaxNode) -> Option<OptionValue> {
    constant_value(&crate::constant::fold(node, &|_| {
        ConstantValue::Unevaluated
    }))
}

/// A constant held as a value, for a candidate's condition to compare:
/// an integer or a duration. `None` for one whose value is not held.
fn constant_value(value: &ConstantValue) -> Option<OptionValue> {
    match value {
        ConstantValue::Int(value) => Some(OptionValue::Literal(Value::Int(*value))),
        ConstantValue::Duration(value) => Some(OptionValue::Literal(Value::Duration(*value))),
        _ => None,
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
    value_of: &'c dyn Fn(&str) -> Option<OptionValue>,
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
/// does not fit the candidate's parameters at all. `candidate` is its
/// declaration, where the defaults of its generic value parameters are
/// written.
///
/// GEN-5: "Match candidates against the requested output before
/// inputs". GEN-12: "A prior binding constrains every later occurrence
/// in declared parameter order." An argument whose type is not known
/// all the way down, `null`, `.member` or `[]`, says nothing.
fn instance<'s>(
    (candidate, signature): (&Declaration, &'s Signature),
    public: &Signature,
    given: &[Given],
    wanted: &Wanted,
    widens: &dyn Fn(&Type, &Type) -> bool,
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
            let constant = given
                .constant
                .clone()
                .unwrap_or_else(|| constant_at(&given.node));
            found[index] = Some(Argument::Constant(constant));
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
        if let Some(default) = generic_default(&candidate.syntax, generic) {
            found[index] = Some(Argument::Constant(constant_at(&default)));
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
        | (Type::Rolling { value: left, .. }, Type::Rolling { value: right, .. }) => {
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
                Some((of.value_of)(&name)?.same(&constant_of(constant)?))
            } else if within {
                let name = option(left)?;
                if right.kind() != SyntaxKind::ConstraintSet {
                    return None;
                }
                let value = (of.value_of)(&name)?;
                let mut found = false;
                for member in right.children() {
                    found |= constant_of(&member)?.same(&value);
                }
                Some(found)
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
        // The value of a block, and of each branch of an `if`.
        ExprKind::Block {
            value: Some(value), ..
        } => widens_an_entry(value),
        ExprKind::If {
            then, otherwise, ..
        } => {
            widens_an_entry(then)
                || otherwise
                    .as_ref()
                    .is_some_and(|otherwise| widens_an_entry(otherwise))
        }
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
    match node.kind() {
        SyntaxKind::ParenExpr => node
            .children()
            .next()
            .is_some_and(|inner| typed_by_place(&inner)),
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
    match node.kind() {
        SyntaxKind::Literal => tokens(node).any(|token| token.kind() == SyntaxKind::Int),
        SyntaxKind::ParenExpr | SyntaxKind::UnaryExpr => node
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
#[derive(Clone, PartialEq)]
enum Label {
    /// An integer.
    Int(i64),
    /// `true` or `false`.
    Bool(bool),
    /// A string.
    Str(String),
    /// A member of the selector's enum, by its position.
    Member(usize),
}

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
        match node.kind() {
            SyntaxKind::Block => self.block(node, expected, used),
            SyntaxKind::IfExpr => self.conditional(node, expected, used),
            SyntaxKind::ParenExpr if node.children().next().is_some() => {
                // "Parentheses select grouping" (EXPR-1).
                let inner = node.children().next().expect("checked above");
                self.flow(&inner, expected, used)
            }
            _ => {
                let value = self.expression(node, expected);
                let ty = value.ty.clone();
                (value, Outcome::Value(ty))
            }
        }
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
                        let value = self.fit(value, expected, "the value of this block");
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
        let mut seen: Vec<(Label, Location)> = Vec::new();
        let mut cases = Vec::new();
        let mut default = None;
        for part in node.children() {
            match part.kind() {
                SyntaxKind::SwitchCase => {
                    let mut inside = part.children();
                    let Some(label) = inside.next() else {
                        continue;
                    };
                    self.assigned = restored(&before, self.locals.len(), false);
                    self.returned = before_returned;
                    let label = self.expression(&label, Some(&of));
                    let label = self.fit(label, &of, "the selector");
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
                    }
                    if let Some(value) = label_of(&label) {
                        match seen.iter().find(|(other, _)| *other == value) {
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
                            None => seen.push((value, label.at)),
                        }
                    }
                    // ERR-12: what a null test proves in one case is
                    // proved of no other, and of nothing after the
                    // `switch`: another case may be the one selected.
                    let frame = self.present.len();
                    let body = inside.map(|statement| self.statement(&statement)).collect();
                    self.present.truncate(frame);
                    ends.push((self.assigned.clone(), self.returned));
                    cases.push(hir::Case { label, body });
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
            .filter(|(label, _)| matches!(label, Label::Member(_)))
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
    match branch.kind() {
        SyntaxKind::Block => {
            value_of_branch(branch).is_some_and(|value| branch_takes_its_type(&value))
        }
        // The condition is first; the branches follow it.
        SyntaxKind::IfExpr => branch
            .children()
            .skip(1)
            .any(|inner| branch_takes_its_type(&inner)),
        SyntaxKind::ParenExpr => branch
            .children()
            .next()
            .is_some_and(|inner| branch_takes_its_type(&inner)),
        _ => takes_its_type(branch),
    }
}

/// True when a branch's value, found as [`branch_takes_its_type`] finds
/// it, is `null`.
fn branch_is_null(branch: &SyntaxNode) -> bool {
    match branch.kind() {
        SyntaxKind::Block => value_of_branch(branch).is_some_and(|value| branch_is_null(&value)),
        SyntaxKind::IfExpr => branch
            .children()
            .skip(1)
            .any(|inner| branch_is_null(&inner)),
        SyntaxKind::ParenExpr => branch
            .children()
            .next()
            .is_some_and(|inner| branch_is_null(&inner)),
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

/// The value of a label that is a constant this pass can tell from
/// another: a literal, or a member of an enum.
fn label_of(label: &Expr) -> Option<Label> {
    match &label.kind {
        ExprKind::Literal(Value::Int(value)) => Some(Label::Int(*value)),
        ExprKind::Literal(Value::Bool(value)) => Some(Label::Bool(*value)),
        ExprKind::Literal(Value::Str(value)) => Some(Label::Str(value.clone())),
        ExprKind::EnumMember { member, .. } => Some(Label::Member(*member)),
        // `-1`: a minus over an integer literal.
        _ => match &negated(label)?.kind {
            ExprKind::Literal(Value::Int(value)) => value.checked_neg().map(Label::Int),
            _ => None,
        },
    }
}

/// True when a type is known all the way down: no part of it is
/// `Type::Unknown`.
fn whole(ty: &Type) -> bool {
    match ty {
        Type::Unknown => false,
        Type::Named { arguments, .. } => arguments.iter().all(|argument| match argument {
            Argument::Type(argument) => whole(argument),
            Argument::Constant(_) => true,
        }),
        Type::Tuple(elements) => elements.iter().all(whole),
        Type::List { element, .. } => whole(element),
        Type::Set(inner)
        | Type::Ref(inner)
        | Type::Endpoint(inner)
        | Type::Atomic(inner)
        | Type::Delta(inner)
        | Type::Nullable(inner) => whole(inner),
        Type::Dict { key, value } => whole(key) && whole(value),
        Type::Rolling { value, .. } => whole(value),
        Type::Result { value, error } => whole(value) && whole(error),
        Type::Scalar(_) | Type::Parameter { .. } | Type::Signal | Type::Error => true,
    }
}

/// What the index of a tuple is, as far as this pass can say.
enum Index {
    /// A constant with this value.
    Known(i64),
    /// A constant whose value another pass finds: one that names a
    /// generic value parameter, divides, or leaves the range of an
    /// `i64`.
    Unevaluated,
    /// No constant: it reads a local, a parameter or a call.
    NotConstant,
}

/// The value of a tuple's index (EXPR-7: "A tuple's index is a constant
/// within its length"): an integer literal, or literals under a minus
/// and joined by `+`, `-` and `*`, as in `pair[1 - 1]`.
fn constant_index(index: &Expr) -> Index {
    match &index.kind {
        ExprKind::Literal(Value::Int(value)) => return Index::Known(*value),
        ExprKind::GenericValue { .. } => return Index::Unevaluated,
        // A call of an operator of which no candidate is selected: it
        // has been reported, or is not judged, and nothing more is said
        // of it (ERR-9).
        ExprKind::Call {
            callee: Callee::Unread,
            form: Form::Binary(_) | Form::Unary(_),
            ..
        } => return Index::Unevaluated,
        _ => {}
    }
    // A symbol, as it is written or as core's own arithmetic.
    if let Some(operand) = negated(index) {
        return match constant_index(operand) {
            // `checked_neg` gives `None` where the result does not fit.
            Index::Known(value) => value.checked_neg().map_or(Index::Unevaluated, Index::Known),
            other => other,
        };
    }
    let Some((operator, left, right)) = arithmetic(index) else {
        return Index::NotConstant;
    };
    match (constant_index(left), constant_index(right)) {
        (Index::NotConstant, _) | (_, Index::NotConstant) => Index::NotConstant,
        (Index::Known(left), Index::Known(right)) => {
            let value = match operator {
                BinaryOperator::Add => left.checked_add(right),
                BinaryOperator::Subtract => left.checked_sub(right),
                BinaryOperator::Multiply => left.checked_mul(right),
                // Division and remainder follow core's numeric
                // contract, which this pass does not evaluate.
                _ => None,
            };
            value.map_or(Index::Unevaluated, Index::Known)
        }
        _ => Index::Unevaluated,
    }
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
        | (Type::Delta(pattern), Type::Delta(actual)) => infer(pattern, actual, generics, found),
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
            Type::Rolling {
                value,
                maximum,
                minimum,
            },
            Type::Rolling {
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
    if let ConstantValue::Parameter { at, .. } = &pattern.value {
        find(*at, Argument::Constant(actual.clone()), generics, found);
    }
}

/// The constant that stands for a generic value nothing has said
/// anything of, in an output requested in part: it agrees with any, and
/// decides nothing.
fn unsaid() -> Constant {
    Constant {
        text: "_".to_string(),
        value: ConstantValue::Unevaluated,
    }
}

/// True for [`unsaid`]. No constant a source writes is `_`.
fn is_unsaid(constant: &Constant) -> bool {
    constant.text == "_" && matches!(constant.value, ConstantValue::Unevaluated)
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

/// TYPE-4: "A field is optional when its type is nullable, `T?`, or an
/// `atomic<...>` of a nullable type, `atomic<T?>`."
fn is_optional(ty: &Type) -> bool {
    match ty {
        Type::Nullable(_) => true,
        Type::Atomic(inner) => matches!(**inner, Type::Nullable(_)),
        _ => false,
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

/// True when a size or a constant argument inside `ty` is a generic
/// value parameter declared at one of `own`: the `N` of `list<i64, N>`.
fn names_value_of(ty: &Type, own: &[Location]) -> bool {
    let named = |constant: &Constant| matches!(&constant.value, ConstantValue::Parameter { at, .. } if own.contains(at));
    let inner = |ty: &Type| names_value_of(ty, own);
    match ty {
        Type::Named { arguments, .. } => arguments.iter().any(|argument| match argument {
            Argument::Type(ty) => inner(ty),
            Argument::Constant(constant) => named(constant),
        }),
        Type::Tuple(elements) => elements.iter().any(inner),
        Type::List { element, size } => size.as_ref().is_some_and(named) || inner(element),
        Type::Set(ty)
        | Type::Ref(ty)
        | Type::Endpoint(ty)
        | Type::Atomic(ty)
        | Type::Delta(ty)
        | Type::Nullable(ty) => inner(ty),
        Type::Dict { key, value } => inner(key) || inner(value),
        Type::Rolling {
            value,
            maximum,
            minimum,
        } => named(maximum) || minimum.as_ref().is_some_and(named) || inner(value),
        Type::Result { value, error } => inner(value) || inner(error),
        Type::Scalar(_) | Type::Parameter { .. } | Type::Signal | Type::Error | Type::Unknown => {
            false
        }
    }
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
fn is_plain(callee: &Callee, arguments: &[hir::Argument], operands: usize) -> bool {
    match callee {
        Callee::Candidate {
            operator, module, ..
        } => operator.0 == *module && arguments.len() == operands,
        _ => false,
    }
}

/// The operand of a minus that is core's negation: `-value` as it is
/// written, or the plain call of `neg_` it stands for (see
/// [`is_plain`]).
fn negated(value: &Expr) -> Option<&Expr> {
    match &value.kind {
        ExprKind::Unary {
            operator: UnaryOperator::Negate,
            operand,
        } => Some(operand),
        ExprKind::Call {
            callee,
            arguments,
            form: Form::Unary(UnaryOperator::Negate),
        } if is_plain(callee, arguments, 1) => Some(&arguments[0].value),
        _ => None,
    }
}

/// The operator and operands of a symbol between two that is core's
/// arithmetic: as it is written, or the plain call it stands for (see
/// [`is_plain`]).
fn arithmetic(value: &Expr) -> Option<(BinaryOperator, &Expr, &Expr)> {
    match &value.kind {
        ExprKind::Binary {
            operator,
            left,
            right,
        } => Some((*operator, left, right)),
        ExprKind::Call {
            callee,
            arguments,
            form: Form::Binary(operator),
        } if is_plain(callee, arguments, 2) => {
            Some((*operator, &arguments[0].value, &arguments[1].value))
        }
        _ => None,
    }
}

/// True when an expression is `.member`, in parentheses or not.
fn is_implied_member(node: &SyntaxNode) -> bool {
    match node.kind() {
        SyntaxKind::ParenExpr => node
            .children()
            .next()
            .is_some_and(|inner| is_implied_member(&inner)),
        SyntaxKind::ImpliedMember => true,
        _ => false,
    }
}

/// True when an expression takes its type from its place and has none
/// of its own: `null`, and `.member`, in parentheses or not.
fn takes_its_type(node: &SyntaxNode) -> bool {
    match node.kind() {
        SyntaxKind::ParenExpr => node
            .children()
            .next()
            .is_some_and(|inner| takes_its_type(&inner)),
        SyntaxKind::ImpliedMember => true,
        SyntaxKind::Literal => tokens(node).any(|token| token.kind() == SyntaxKind::KwNull),
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

/// The type a value that is not `null` is expected to have: of `T?`,
/// `T` (TYPE-14: "either a T or null").
fn not_null(expected: Option<&Type>) -> Option<&Type> {
    match expected {
        Some(Type::Nullable(inner)) => not_null(Some(inner)),
        other => other,
    }
}

/// The identifier a name expression uses: of `module.name`, the name.
fn name_token(node: &SyntaxNode) -> Option<SyntaxToken> {
    let identifiers = |node: &SyntaxNode| {
        tokens(node)
            .filter(|token| token.kind() == SyntaxKind::Ident)
            .collect::<Vec<_>>()
    };
    match child(node, SyntaxKind::QualifiedName) {
        Some(qualified) => identifiers(&qualified).into_iter().nth(1),
        None => identifiers(node).into_iter().next(),
    }
}

/// The type of a temporal literal, by the parts it writes (LEX-11:
/// "Offset-free date/time is civil, not an implicit UTC instant. A
/// zoned datetime requires an explicit offset"). `None` for one the
/// lexer has reported.
fn temporal_type(text: &str) -> Option<Scalar> {
    let literal = decode_temporal(text).ok()?;
    let zone = literal.zone.is_some();
    Some(match (literal.date, literal.time, literal.offset) {
        (Some(_), None, _) => Scalar::Date,
        (Some(_), Some(_), None) => Scalar::CivilDatetime,
        (Some(_), Some(_), Some(_)) if zone => Scalar::ZonedDatetime,
        (Some(_), Some(_), Some(_)) => Scalar::Datetime,
        (None, Some(_), _) if zone => Scalar::ZonedTime,
        (None, Some(_), _) => Scalar::Time,
        (None, None, _) => Scalar::Timezone,
    })
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
