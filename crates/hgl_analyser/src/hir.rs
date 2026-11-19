//! The typed form of a body: what a function does, said without its
//! syntax.
//!
//! The syntax tree says how a body is written. The stages after the
//! analyser need what it means: which local each name is, the type of
//! each expression, where a number is widened. This module is that
//! description. It names no target: a transpiler reads it and writes
//! its own target's code.
//!
//! The analyser fills it in pass by pass. What a pass does not read yet
//! is [`ExprKind::Unread`] or [`Stmt::Unread`]: it is there, its type is
//! not known, and nothing is reported of it (ERR-9).

use hgl_diagnostics::Location;
// A tenor's value is the lexer's: a later stage reads it from here.
pub use hgl_lexer::literal::Tenor;

use crate::program::LocalKind;
use crate::types::{Argument as GenericArgument, Type};

/// An application's run statement, typed (RUN-1): "its run statement
/// is `run(call, config)` or `eval(call, config)`, where the call is of
/// an entry ... and the configuration is a value of core's `RunConfig`,
/// or `RunConfig()` where none is written."
#[derive(Debug, Clone, PartialEq)]
pub struct Run {
    /// True for `run`, a run in real time; false for `eval`, a
    /// simulation. RUN-3: "The word of a run statement is its mode."
    pub real_time: bool,
    /// The call of the entry, with the entry's const arguments.
    pub call: Expr,
    /// The configuration, where one is written.
    pub config: Option<Expr>,
    /// Where the statement is written.
    pub at: Location,
}

/// The body of one function: a value function's, a node's, or a
/// graph's.
#[derive(Debug, Clone, PartialEq)]
pub struct Body {
    /// Its parameters and locals, parameters first, each in the order
    /// it is declared. A [`LocalId`] is a position in this list.
    pub locals: Vec<Local>,
    /// Its statements, in order. An arrow body, `=> value`, is one
    /// [`Stmt::Return`]. A node's are its declarations, [`Stmt::Declare`]
    /// for a `state` or a `cache` and [`Stmt::Inject`], and then its
    /// hooks, each a [`Stmt::Hook`] (FN-2).
    pub statements: Vec<Stmt>,
}

/// A hook of a node (TIME-3, TIME-5).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hook {
    /// `start { ... }`: runs once, when the node starts.
    Start,
    /// `when condition { ... }`: a handler, run when the node is
    /// evaluated and its condition admits it.
    When,
    /// `stop { ... }`: runs once, when the node stops.
    Stop,
}

impl Hook {
    /// The keyword the hook is written with.
    pub fn as_str(self) -> &'static str {
        match self {
            Hook::Start => "start",
            Hook::When => "when",
            Hook::Stop => "stop",
        }
    }
}

/// An operation on an endpoint (TIME-9). Core defines the observations
/// and mutations, and the language defines the handler selectors.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EndpointOperation {
    /// `valid(input)`: whether the endpoint holds a value.
    Valid,
    /// `all_valid(input)`: whether every element, field or entry of a
    /// list, tuple, struct or dictionary is valid, one level deep
    /// (TIME-9, END-7).
    AllValid,
    /// `modified(input)`: whether it published in this cycle.
    Modified,
    /// `delta_value(input)`: what it published in this cycle.
    DeltaValue,
    /// `last_modified(input)`: when it last published.
    LastModified,
    /// `invalidate(out)`: clears the output.
    Invalidate,
    /// `scheduled()`: the scheduler's handler selector.
    Scheduled,
    /// `activate(input)`: subscribes to the input again.
    Activate,
    /// `passivate(input)`: stops subscribing to it.
    Passivate,
    /// `is_active(input)`: reads whether the direct input is active.
    IsActive,
}

impl EndpointOperation {
    /// Every operation, with the name it is written by.
    pub const ALL: [(&'static str, EndpointOperation); 10] = [
        ("valid", EndpointOperation::Valid),
        ("all_valid", EndpointOperation::AllValid),
        ("modified", EndpointOperation::Modified),
        ("delta_value", EndpointOperation::DeltaValue),
        ("last_modified", EndpointOperation::LastModified),
        ("invalidate", EndpointOperation::Invalidate),
        ("scheduled", EndpointOperation::Scheduled),
        ("activate", EndpointOperation::Activate),
        ("passivate", EndpointOperation::Passivate),
        ("is_active", EndpointOperation::IsActive),
    ];

    /// The operation written `name`, when it is one.
    pub fn named(name: &str) -> Option<EndpointOperation> {
        Self::ALL
            .iter()
            .find(|(written, _)| *written == name)
            .map(|(_, operation)| *operation)
    }

    /// Operations the language recognises directly in a hook. Core's
    /// `is_active` uses ordinary function resolution and argument binding.
    pub(crate) fn language_form(name: &str) -> Option<EndpointOperation> {
        Self::named(name).filter(|operation| *operation != Self::IsActive)
    }

    /// The name the operation is written by.
    pub fn name(self) -> &'static str {
        Self::ALL
            .iter()
            .find(|(_, operation)| *operation == self)
            .map_or("", |(written, _)| written)
    }
}

/// The position of a local in [`Body::locals`].
pub type LocalId = usize;

/// A parameter or a local of a body.
#[derive(Debug, Clone, PartialEq)]
pub struct Local {
    /// Its name.
    pub name: String,
    /// Where the name is declared.
    pub at: Location,
    /// What declares it.
    pub kind: LocalKind,
    /// Its type; [`Type::Unknown`] when nothing says.
    pub ty: Type,
    /// Whether it is a connection of a graph or an ordinary value
    /// (EXPR-3: "Initialization fixes a binding's value/connection
    /// category").
    pub category: ValueCategory,
}

/// What a value of a body is (FN-3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValueCategory {
    /// An ordinary value, known while the graph is built. Every local
    /// of a value function and of a hook is one.
    Value,
    /// A connection of a graph: a temporal parameter, or what a
    /// temporal or lifted call gives.
    Connection,
    /// One or the other, which is not known here. What an operation
    /// the graph requires gives when it is given no connection (GEN-7):
    /// a temporal candidate would make it a connection, and a value
    /// candidate a value (GEN-5), and which it is is selected where the
    /// graph's generic parameters are known. And a `var` of a graph
    /// declared with no value, until an assignment gives it one
    /// (EXPR-3).
    Unsettled,
}

/// One statement.
#[derive(Debug, Clone, PartialEq)]
pub enum Stmt {
    /// `let name = value`, `var name: T = value`, `var name: T`: the
    /// local begins, with its first value when one is written.
    Declare {
        /// The local.
        local: LocalId,
        /// Its first value.
        value: Option<Expr>,
    },
    /// `name = value` or `name += value`.
    Assign {
        /// The local assigned.
        local: LocalId,
        /// The operator of a compound assignment: `+` for `+=`.
        operator: Option<BinaryOperator>,
        /// What the local becomes. For a compound assignment, the
        /// symbol with the local's value as its left operand: `name +=
        /// value` assigns what `name + value` is.
        value: Expr,
        /// Where the statement is written.
        at: Location,
    },
    /// `point.x = value`, `row[0] += value`: an assignment to a place
    /// inside a value (EXPR-3).
    AssignPlace {
        /// The place: a chain of fields and elements from a local, read
        /// as the expression it is. Its type is what the place takes.
        place: Expr,
        /// The operator of a compound assignment: `+` for `+=`.
        operator: Option<BinaryOperator>,
        /// What the place becomes. For a compound assignment, the
        /// symbol with the place's value as its left operand.
        value: Expr,
        /// Where the statement is written.
        at: Location,
    },
    /// `inject name`: a capability of a node, one statement for each
    /// name (TIME-8). `out` is the node's result (TIME-7).
    Inject {
        /// The local that names the capability.
        local: LocalId,
        /// Where the name is written.
        at: Location,
    },
    /// `start { ... }`, `when condition { ... }` or `stop { ... }`: a
    /// hook of a node.
    Hook {
        /// Which hook it is.
        hook: Hook,
        /// The condition of a `when`, when it has one.
        condition: Option<Expr>,
        /// Its statements. A temporal parameter is read there as a
        /// value of its declared type (FN-3).
        body: Vec<Stmt>,
        /// Where the hook is written.
        at: Location,
    },
    /// `return`, `return value`, or the value of an arrow body.
    Return {
        /// The value returned.
        value: Option<Expr>,
        /// Where the statement is written.
        at: Location,
    },
    /// `assert condition`, in a test: the condition is a `bool` known
    /// when the test's graph is built (TST-4).
    Assert {
        /// What is asserted.
        condition: Expr,
        /// Where the statement is written.
        at: Location,
    },
    /// `assert raises("code") { ... }`, in a test: the block is expected
    /// to fail with that execution code (TST-10).
    AssertRaises {
        /// The execution code the block is to fail with. `None` where
        /// what is written is no string literal, which is reported.
        code: Option<String>,
        /// The block that is run.
        body: Vec<Stmt>,
        /// Where the statement is written.
        at: Location,
    },
    /// `yield time: value`, in a source: it "writes its output and sets
    /// its next evaluation" (SOURCE-1).
    Yield {
        /// When the value is published.
        time: Expr,
        /// The value published, of the source's result type.
        value: Expr,
        /// Where the statement is written.
        at: Location,
    },
    /// An expression evaluated for what it does.
    Expr(Expr),
    /// `for item in items { ... }`, or `for key, value in entries { ... }`
    /// (CTRL-12).
    For {
        /// The names the pattern binds: one, or a key and a value.
        names: Vec<LocalId>,
        /// What is looped over.
        sequence: Expr,
        /// The body.
        body: Vec<Stmt>,
        /// Where the statement is written.
        at: Location,
    },
    /// `while condition { ... }` (CTRL-12).
    While {
        /// The condition; `None` for the form with none, which a value
        /// function rejects.
        condition: Option<Expr>,
        /// The body.
        body: Vec<Stmt>,
        /// Where the statement is written.
        at: Location,
    },
    /// `switch selector { case label: ... default: ... }` (CTRL-6).
    Switch {
        /// What is selected on.
        selector: Expr,
        /// The cases, in order.
        cases: Vec<Case>,
        /// The statements of `default:`, when there is one.
        default: Option<Vec<Stmt>>,
        /// Where the statement is written.
        at: Location,
    },
    /// A statement no pass reads yet.
    Unread {
        /// Where it is written.
        at: Location,
    },
}

/// One `case label: statements` of a `switch`.
#[derive(Debug, Clone, PartialEq)]
pub struct Case {
    /// The label.
    pub label: Expr,
    /// Its statements. There is no fallthrough (CTRL-6).
    pub body: Vec<Stmt>,
}

/// One expression, with its type.
#[derive(Debug, Clone, PartialEq)]
pub struct Expr {
    /// What it is.
    pub kind: ExprKind,
    /// Its type; [`Type::Unknown`] when it is not known.
    pub ty: Type,
    /// Where it is written.
    pub at: Location,
}

/// What an expression is.
#[derive(Debug, Clone, PartialEq)]
pub enum ExprKind {
    /// A literal.
    Literal(Value),
    /// A constant of a module read by its name (UNIT-13): "The constant
    /// has that one value wherever it is read".
    Constant {
        /// The position of its module in `Program::modules`.
        module: usize,
        /// Its position in that module's declarations.
        declaration: usize,
    },
    /// An operation on an endpoint (TIME-9): `valid(value)`,
    /// `modified()`, `invalidate(out)`.
    Endpoint {
        /// The operation.
        operation: EndpointOperation,
        /// The endpoints it is given: temporal parameters of the node,
        /// or the injected `out`. `None` for one written with no
        /// argument, which in a `when` header stands for every temporal
        /// input (TIME-3); an empty list for `valid([])`.
        inputs: Option<Vec<LocalId>>,
    },
    /// A parameter or a local. Its type is the local's, or the payload's
    /// where the local is a `T?` that a test against null has proved is
    /// not null there (ERR-12).
    Local(LocalId),
    /// The endpoint of a temporal input, or of `out`, where a native
    /// value function takes the endpoint and not its payload: the
    /// argument of an `endpoint<T>` parameter (TIME-9). The payload is
    /// not read.
    EndpointOf(LocalId),
    /// `{ statements }`: a block. Its value is that of its last
    /// statement, when that is an expression (CTRL-10).
    Block {
        /// The statements before the value, or all of them when the
        /// block has no value.
        statements: Vec<Stmt>,
        /// The value.
        value: Option<Box<Expr>>,
    },
    /// `if condition { ... } else { ... }` (CTRL-10). A branch is a
    /// [`ExprKind::Block`], or the `If` of an `else if`.
    If {
        /// The condition, a `bool`.
        condition: Box<Expr>,
        /// What is done when it holds.
        then: Box<Expr>,
        /// What is done when it does not.
        otherwise: Option<Box<Expr>>,
    },
    /// A generic value parameter of the declaration: the `N` of
    /// `const fn width<const N: i64>() -> i64 => N`.
    GenericValue {
        /// Its name.
        name: String,
        /// Where it is declared.
        at: Location,
    },
    /// `!value`, and a `-value` not read as a call: see [`Form`].
    Unary {
        /// The operator.
        operator: UnaryOperator,
        /// The operand.
        operand: Box<Expr>,
    },
    /// `left && right`, `left || right`, and each other symbol with
    /// two operands that is not read as a call: see [`Form`].
    Binary {
        /// The operator.
        operator: BinaryOperator,
        /// The left operand.
        left: Box<Expr>,
        /// The right operand.
        right: Box<Expr>,
    },
    /// An `i64` taken as the `f64` of the same number, where an `f64`
    /// is wanted (EXPR-5, NUM-1). The source writes nothing for it.
    Widen(Box<Expr>),
    /// A call: `scale(value, factor: 2.0)`.
    Call {
        /// What is called.
        callee: Callee,
        /// The arguments, as they are written.
        arguments: Vec<Argument>,
        /// How the call is written.
        form: Form,
    },
    /// A struct made from its fields: `Quote(bid: 1.0, ask: 2.0)`. The
    /// expression's type is the struct, with its generic arguments.
    Construct {
        /// The position of the struct's module in `Program::modules`.
        module: usize,
        /// The position of the struct in that module's `declarations`.
        declaration: usize,
        /// The fields given, as they are written. A field's position
        /// is its place among the struct's `members`.
        arguments: Vec<Argument>,
    },
    /// A member of an enum: `Side.buy`, or `.buy` where a `Side` is
    /// expected (TYPE-7).
    EnumMember {
        /// The position of the enum's module in `Program::modules`.
        module: usize,
        /// The position of the enum in that module's `declarations`.
        declaration: usize,
        /// The member's position among the enum's members.
        member: usize,
    },
    /// `Side(1)` or `Side("buy")`: the member of an enum that has the
    /// number, or the name, given (TYPE-7).
    EnumFrom {
        /// The position of the enum's module in `Program::modules`.
        module: usize,
        /// The position of the enum in that module's `declarations`.
        declaration: usize,
        /// The number, an `i64`, or the name, a `str`.
        value: Box<Expr>,
    },
    /// A field of a struct value, `quote.bid`; a property of a native
    /// value, `clock.now`; or a property of a delta that is a type of
    /// its own, `change.added` (TYPE-16).
    Field {
        /// The value.
        base: Box<Expr>,
        /// The field's position among the struct's `members`, or the
        /// property's among the native type's. For a set's delta, 0 is
        /// `added` and 1 is `removed`; for a dictionary's, 0 is
        /// `updated` and 1 is `removed`; for a nullable's, 0 is
        /// `value`; for a struct's, the position of the field it is
        /// the delta of.
        member: usize,
    },
    /// A delta that is a type of its own made from its properties:
    /// `delta<set<i64>>(added: [1, 2], removed: [3])`, `delta<i64?>()`,
    /// `delta<Quote>(bid: 1.0)` (TYPE-16). The expression's type is
    /// the delta.
    Delta {
        /// The properties given, as they are written. A property's
        /// position is as [`ExprKind::Field`] has it; one left out is
        /// empty, or null for a nullable's `value` and a struct's
        /// field.
        arguments: Vec<Argument>,
    },
    /// `base[index]`.
    Index {
        /// What is indexed.
        base: Box<Expr>,
        /// The index or key.
        index: Box<Expr>,
    },
    /// `ok(value)` or `err(value)`: a result that owns the payload
    /// (ERR-2). Its type is the result its place expects.
    Tagged {
        /// True for `ok`, a success; false for `err`, a failure.
        success: bool,
        /// The payload: the T of a success, the E of a failure.
        payload: Box<Expr>,
    },
    /// `is_ok(value)` or `is_err(value)`: a bool (ERR-3).
    TagTest {
        /// True for `is_ok`; false for `is_err`.
        success: bool,
        /// The result tested.
        value: Box<Expr>,
    },
    /// `binding.value` or `binding.error`: the payload of a result that
    /// a guard has proved a success, or a failure (ERR-3).
    Payload {
        /// True for `.value`, the T; false for `.error`, the E.
        success: bool,
        /// The result read.
        result: Box<Expr>,
    },
    /// `error(message: text, code: optional_code)` (ERR-4).
    Error {
        /// The message, a `str`.
        message: Box<Expr>,
        /// The code, a `str?`; `None` when it is not written, which is
        /// null.
        code: Option<Box<Expr>>,
    },
    /// `.message`, `.code` or `.context` of an error, or `.node`,
    /// `.phase` or `.evaluation_time` of its context (ERR-4).
    ErrorPart {
        /// The error, or the context, read.
        base: Box<Expr>,
        /// Which of its parts.
        part: ErrorPart,
    },
    /// `value?`: the T of a success; a failure leaves the function or
    /// fails the node (ERR-5).
    Propagate(Box<Expr>),
    /// `capture(callable, args...)`: a temporal error boundary around
    /// the call it holds (ERR-7).
    Capture(Box<Expr>),
    /// `seen.value` or `seen.error`: one of the two connections a
    /// capture is read through (ERR-7).
    Captured {
        /// The capture read: the local it is bound to, or the capture
        /// itself.
        capture: Box<Expr>,
        /// True for `.error`, a connection of `error`; false for
        /// `.value`, the call's own output.
        error: bool,
    },
    /// `(a, b)`.
    Tuple(Vec<Expr>),
    /// `[a, b]`: a sequence of values. Its type says what it makes.
    Sequence(Vec<Expr>),
    /// `[key: value, ...]`: a sequence of pairs, which the type its
    /// place expects makes a dictionary (EXPR-9). Each pair is its key
    /// and its value.
    Pairs(Vec<(Expr, Expr)>),
    /// `eval(target, ...)` (TST-5): the call of the target it stands
    /// for, with a trace given to each temporal parameter and
    /// configuration to each `const` one. Its type is a trace of the
    /// target's result, or none for a target with no result (TST-8).
    Eval {
        /// The call the eval takes, with a trace where the function
        /// called takes a temporal input.
        call: Box<Expr>,
        /// The run's configuration, a `RunConfig` of core's, where the
        /// eval writes one (RUN-1).
        config: Option<Box<Expr>>,
        /// True for `run(...)` in an integration test (TST-17): the
        /// run is in real time, where an eval's is a simulation
        /// (RUN-3).
        real_time: bool,
    },
    /// A trace as the value a `const fn` is given (TST-16): "for a
    /// result type T a `list<tuple<datetime, delta<T>>>`, one tuple for
    /// each publication, in time order". The expression inside is the
    /// trace.
    Publications(Box<Expr>),
    /// A trace written out (TST-5, TST-7): `[7, _, 9]`, dense, or
    /// `[0s: 7, 2s: 9]`, timed.
    Trace(Vec<Cell>),
    /// `left == right` or `left != right` of two traces (TST-4):
    /// "Harness comparison is intrinsic: compare presence, exact
    /// present data, and shape/length; overload candidates cannot
    /// change it."
    TraceCompare {
        /// The left trace.
        left: Box<Expr>,
        /// The right trace.
        right: Box<Expr>,
        /// True for `!=`.
        negated: bool,
    },
    /// An expression no pass reads yet, or one that has been reported.
    Unread,
}

/// A part of an error, or of an error's context, that source reads
/// (ERR-4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorPart {
    /// `.message`, a `str`.
    Message,
    /// `.code`, a `str?`.
    Code,
    /// `.context`, the error's provenance: null for a constructed
    /// error, and otherwise a record of the three parts below.
    Context,
    /// `.node` of a context, a `str`: the node that first failed.
    Node,
    /// `.phase` of a context, a `str`: `"evaluate"`, `"start"` or
    /// `"stop"`.
    Phase,
    /// `.evaluation_time` of a context, a `datetime`.
    EvaluationTime,
}

/// What a call calls.
#[derive(Debug, Clone, PartialEq)]
pub enum Callee {
    /// A value function.
    Function {
        /// The position of its module in `Program::modules`.
        module: usize,
        /// Its position in that module's `declarations`.
        declaration: usize,
        /// What each generic parameter of the function stands for at
        /// this call (GEN-2), in the order declared; `Type::Unknown`
        /// for one nothing resolved. Empty for a function with none,
        /// and for one whose generic parameters this pass does not
        /// resolve.
        generics: Vec<GenericArgument>,
        /// What answers each requirement the function states, with
        /// `generics` in place: see [`Answer`]. Empty where `generics`
        /// is.
        requirements: Vec<Answer>,
    },
    /// The value candidate selected for a call of an operator (GEN-5).
    Candidate {
        /// The operator called: the position of its module in
        /// `Program::modules`, and its position in that module's
        /// `declarations`.
        operator: (usize, usize),
        /// The position of the candidate's module in
        /// `Program::modules`.
        module: usize,
        /// The candidate's position in that module's `declarations`.
        declaration: usize,
        /// What each generic parameter of the candidate stands for at
        /// this call (GEN-2), in the order declared. Empty for a
        /// candidate with none.
        generics: Vec<GenericArgument>,
        /// What answers each requirement the candidate states, with
        /// `generics` in place: see [`Answer`].
        requirements: Vec<Answer>,
    },
    /// An operation the enclosing declaration requires of its generic
    /// parameters (GEN-7, GEN-16): the `add_(lhs, rhs)` of a body under
    /// `requires add_(T, T) -> T`. Which candidate it is is selected
    /// where the parameters are known, not here: a call of the
    /// declaration says what answers the requirement there
    /// ([`Answer`]).
    Required {
        /// The operator called: the position of its module in
        /// `Program::modules`, and its position in that module's
        /// `declarations`.
        operator: (usize, usize),
        /// The requirement of the enclosing declaration the call is:
        /// its position among the constraint calls that hold wherever
        /// the declaration's `requires` does, in the order written.
        requirement: usize,
    },
    /// No callee: a name left for the prelude, a function that is
    /// itself an expression, or an operator of which no candidate is
    /// selected, by its name or by a symbol.
    Unread,
}

/// What answers a requirement a call's callee states (GEN-16).
///
/// A declaration's requirements are the constraint calls that hold
/// wherever its `requires` does: the condition itself, and each side of
/// an `&&`. A call of the declaration has one `Answer` for each, in the
/// order written. With them, what a [`Callee::Required`] of the
/// declaration's body calls is known for that call: under
/// `fn sum<T>(a: T, b: T) -> T requires add_(T, T) -> T`, the call
/// `sum(1.5, 2.5)` answers the requirement with the candidate
/// `add_(f64, f64)`.
#[derive(Debug, Clone, PartialEq)]
pub enum Answer {
    /// The one most-specific value candidate that takes the types the
    /// requirement is supplied with at this call (GEN-5, GEN-16).
    Candidate {
        /// The operator required: the position of its module in
        /// `Program::modules`, and its position in that module's
        /// `declarations`.
        operator: (usize, usize),
        /// The position of the candidate's module in
        /// `Program::modules`.
        module: usize,
        /// The candidate's position in that module's `declarations`.
        declaration: usize,
        /// What each generic parameter of the candidate stands for
        /// (GEN-2), in the order declared.
        generics: Vec<GenericArgument>,
        /// What answers each requirement the candidate states itself.
        requirements: Vec<Answer>,
    },
    /// A requirement the declaration the call is written in states
    /// itself (GEN-7), which is evidence for the same call: its
    /// position among that declaration's requirements. What answers it
    /// is known where that declaration is called.
    Stated(usize),
    /// Nothing read: a constraint call that is no operator requirement,
    /// one supplied with what is no type, or one for which no one
    /// candidate is told to be selected.
    Unread,
}

/// How a call is written in the source.
///
/// A symbol stands for the operator it binds (the generics chapter's
/// table), so `a + b` is recorded as the call `add_(a, b)` is, marked
/// as written with a symbol. Its arguments are in the order EXPR-13
/// evaluates them: the left operand, the entries of its configuration
/// that are parameters, the right operand; for a symbol before one
/// operand, the entries and then the operand.
///
/// A symbol that is not read as a call is an [`ExprKind::Binary`] or an
/// [`ExprKind::Unary`]: `&&`, `||` and `!`, and a symbol with an operand
/// whose operators are not judged yet. [`ExprKind::binary`] and
/// [`ExprKind::unary`] give the operands of either.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Form {
    /// `name(arguments)`.
    Named,
    /// `left + right`: a symbol between two operands.
    Binary(BinaryOperator),
    /// `-value`: a symbol before one operand.
    Unary(UnaryOperator),
}

/// One cell of a trace written out (TST-5): "Each cell is one cycle's
/// publication".
#[derive(Debug, Clone, PartialEq)]
pub struct Cell {
    /// When it is published, in a timed trace: a `duration` from the
    /// run's start or a `datetime` (TST-7). None in a dense trace, whose
    /// cell stands at its place.
    pub time: Option<Expr>,
    /// What is published. None for `_`, no publication.
    pub value: Option<Expr>,
}

/// One argument of a call or a constructor.
#[derive(Debug, Clone, PartialEq)]
pub struct Argument {
    /// The parameter or field it is given to: its index among the
    /// function's parameters, or among the struct's fields, a parent's
    /// first. `None` when it is given to a pack, or to nothing.
    pub parameter: Option<usize>,
    /// Its value.
    pub value: Expr,
}

/// The value of a literal.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    /// An integer.
    Int(i64),
    /// A float.
    Float(f64),
    /// `true` or `false`.
    Bool(bool),
    /// A string: its contents, the escapes read.
    Str(String),
    /// `null`.
    Null,
    /// A duration, in microseconds.
    Duration(i64),
    /// A tenor (LEX-28, TYPE-17): its count of months and of days, or
    /// its count of business days.
    Tenor(Tenor),
    /// A date, a time or another temporal literal: the parts it writes.
    /// Its type says which it is.
    Temporal(Temporal),
}

/// The parts a temporal literal writes (LEX-11): `@2026-09-03T09:30Z` has
/// a date, a time and an offset. Which are there is what the literal's
/// type goes by, and each part's value has been checked where the literal
/// was read: the date exists, and the time is a time of day.
///
/// | Source | date | time | offset | zone | Type |
/// | --- | --- | --- | --- | --- | --- |
/// | `@2026-09-03` | yes | | | | `date` |
/// | `@2026-09-03T09:30` | yes | yes | | | `civil_datetime` |
/// | `@2026-09-03T09:30Z` | yes | yes | yes | | `datetime` |
/// | `@2026-09-03T09:30+01:00[Europe/London]` | yes | yes | yes | yes | `zoned_datetime` |
/// | `@09:30` | | yes | | | `time` |
/// | `@09:30[Europe/London]` | | yes | | yes | `zoned_time` |
/// | `@[Europe/London]` | | | | yes | `timezone` |
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Temporal {
    /// The calendar date, where one is written.
    pub date: Option<CalendarDate>,
    /// The time of day, where one is written.
    pub time: Option<ClockTime>,
    /// The offset from UTC, where one is written: minutes east of UTC,
    /// negative for west, and zero for `Z`.
    pub offset: Option<i16>,
    /// The zone's name without its brackets, where one is written.
    pub zone: Option<String>,
}

/// A date of the Gregorian calendar (TYPE-2: years 1 to 9999).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CalendarDate {
    /// The year, 1 to 9999.
    pub year: u16,
    /// The month, 1 to 12.
    pub month: u8,
    /// The day of the month, 1 to the month's length.
    pub day: u8,
}

/// A time of day, to the microsecond (TYPE-2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClockTime {
    /// The hour, 0 to 23.
    pub hour: u8,
    /// The minute, 0 to 59.
    pub minute: u8,
    /// The second, 0 to 59: zero where the literal writes none (LEX-11).
    pub second: u8,
    /// The microsecond within the second, 0 to 999,999.
    pub microsecond: u32,
}

/// An operator with one operand.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnaryOperator {
    /// `-`.
    Negate,
    /// `!`.
    Not,
}

/// An operator with two operands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinaryOperator {
    /// `+`.
    Add,
    /// `-`.
    Subtract,
    /// `*`.
    Multiply,
    /// `/`: true division.
    Divide,
    /// `//`: floor division.
    FloorDivide,
    /// `%`.
    Modulo,
    /// `==`.
    Equal,
    /// `!=`.
    NotEqual,
    /// `<`.
    Less,
    /// `<=`.
    LessEqual,
    /// `>`.
    Greater,
    /// `>=`.
    GreaterEqual,
    /// `<=>`: full comparison.
    Compare,
    /// `&&`.
    And,
    /// `||`.
    Or,
}

impl Expr {
    /// Calls `visit` with this expression and then with every expression
    /// inside it, a statement's among them: each once, an outer one
    /// before those it holds.
    ///
    /// `&mut dyn FnMut` is a reference to any closure that may change
    /// what it has captured, a list it adds to say.
    pub fn walk(&self, visit: &mut dyn FnMut(&Expr)) {
        self.walk_nodes(&mut |node| {
            if let Node::Expression(expression) = node {
                visit(expression);
            }
        });
    }

    /// [`Expr::walk`], which also calls `visit` with every statement
    /// inside the expression, a block's and a loop's among them: each
    /// before what it holds.
    pub fn walk_nodes<'a>(&'a self, visit: &mut dyn FnMut(Node<'a>)) {
        visit(Node::Expression(self));
        match &self.kind {
            ExprKind::Literal(_)
            | ExprKind::Constant { .. }
            | ExprKind::Endpoint { .. }
            | ExprKind::Local(_)
            | ExprKind::EndpointOf(_)
            | ExprKind::GenericValue { .. }
            | ExprKind::EnumMember { .. }
            | ExprKind::Unread => {}
            ExprKind::Block { statements, value } => {
                walk_statement_nodes(statements, visit);
                if let Some(value) = value {
                    value.walk_nodes(visit);
                }
            }
            ExprKind::If {
                condition,
                then,
                otherwise,
            } => {
                condition.walk_nodes(visit);
                then.walk_nodes(visit);
                if let Some(otherwise) = otherwise {
                    otherwise.walk_nodes(visit);
                }
            }
            ExprKind::Unary { operand: inner, .. }
            | ExprKind::Widen(inner)
            | ExprKind::EnumFrom { value: inner, .. }
            | ExprKind::Field { base: inner, .. }
            | ExprKind::Tagged { payload: inner, .. }
            | ExprKind::TagTest { value: inner, .. }
            | ExprKind::Payload { result: inner, .. }
            | ExprKind::ErrorPart { base: inner, .. }
            | ExprKind::Propagate(inner)
            | ExprKind::Capture(inner)
            | ExprKind::Captured { capture: inner, .. }
            | ExprKind::Publications(inner) => inner.walk_nodes(visit),
            ExprKind::Binary { left, right, .. }
            | ExprKind::Index {
                base: left,
                index: right,
            }
            | ExprKind::TraceCompare { left, right, .. } => {
                left.walk_nodes(visit);
                right.walk_nodes(visit);
            }
            ExprKind::Call { arguments, .. }
            | ExprKind::Construct { arguments, .. }
            | ExprKind::Delta { arguments } => {
                for argument in arguments {
                    argument.value.walk_nodes(visit);
                }
            }
            ExprKind::Error { message, code } => {
                message.walk_nodes(visit);
                if let Some(code) = code {
                    code.walk_nodes(visit);
                }
            }
            ExprKind::Tuple(entries) | ExprKind::Sequence(entries) => {
                for entry in entries {
                    entry.walk_nodes(visit);
                }
            }
            ExprKind::Pairs(entries) => {
                for (key, value) in entries {
                    key.walk_nodes(visit);
                    value.walk_nodes(visit);
                }
            }
            ExprKind::Eval { call, config, .. } => {
                call.walk_nodes(visit);
                if let Some(config) = config {
                    config.walk_nodes(visit);
                }
            }
            ExprKind::Trace(cells) => {
                for cell in cells {
                    for part in [&cell.time, &cell.value].into_iter().flatten() {
                        part.walk_nodes(visit);
                    }
                }
            }
        }
    }
}

impl Expr {
    /// [`Expr::walk`], for a `visit` that may change each expression:
    /// one it replaces is then walked as what it was replaced with.
    pub(crate) fn walk_mut(&mut self, visit: &mut dyn FnMut(&mut Expr)) {
        visit(self);
        match &mut self.kind {
            ExprKind::Literal(_)
            | ExprKind::Constant { .. }
            | ExprKind::Endpoint { .. }
            | ExprKind::Local(_)
            | ExprKind::EndpointOf(_)
            | ExprKind::GenericValue { .. }
            | ExprKind::EnumMember { .. }
            | ExprKind::Unread => {}
            ExprKind::Block { statements, value } => {
                walk_statements_mut(statements, visit);
                if let Some(value) = value {
                    value.walk_mut(visit);
                }
            }
            ExprKind::If {
                condition,
                then,
                otherwise,
            } => {
                condition.walk_mut(visit);
                then.walk_mut(visit);
                if let Some(otherwise) = otherwise {
                    otherwise.walk_mut(visit);
                }
            }
            ExprKind::Unary { operand: inner, .. }
            | ExprKind::Widen(inner)
            | ExprKind::EnumFrom { value: inner, .. }
            | ExprKind::Field { base: inner, .. }
            | ExprKind::Tagged { payload: inner, .. }
            | ExprKind::TagTest { value: inner, .. }
            | ExprKind::Payload { result: inner, .. }
            | ExprKind::ErrorPart { base: inner, .. }
            | ExprKind::Propagate(inner)
            | ExprKind::Capture(inner)
            | ExprKind::Captured { capture: inner, .. }
            | ExprKind::Publications(inner) => inner.walk_mut(visit),
            ExprKind::Binary { left, right, .. }
            | ExprKind::Index {
                base: left,
                index: right,
            }
            | ExprKind::TraceCompare { left, right, .. } => {
                left.walk_mut(visit);
                right.walk_mut(visit);
            }
            ExprKind::Call { arguments, .. }
            | ExprKind::Construct { arguments, .. }
            | ExprKind::Delta { arguments } => {
                for argument in arguments {
                    argument.value.walk_mut(visit);
                }
            }
            ExprKind::Error { message, code } => {
                message.walk_mut(visit);
                if let Some(code) = code {
                    code.walk_mut(visit);
                }
            }
            ExprKind::Tuple(entries) | ExprKind::Sequence(entries) => {
                for entry in entries {
                    entry.walk_mut(visit);
                }
            }
            ExprKind::Pairs(entries) => {
                for (key, value) in entries {
                    key.walk_mut(visit);
                    value.walk_mut(visit);
                }
            }
            ExprKind::Eval { call, config, .. } => {
                call.walk_mut(visit);
                if let Some(config) = config {
                    config.walk_mut(visit);
                }
            }
            ExprKind::Trace(cells) => {
                for cell in cells {
                    for part in [&mut cell.time, &mut cell.value].into_iter().flatten() {
                        part.walk_mut(visit);
                    }
                }
            }
        }
    }
}

/// [`walk_statements`], for a `visit` that may change each expression.
pub(crate) fn walk_statements_mut(statements: &mut [Stmt], visit: &mut dyn FnMut(&mut Expr)) {
    for statement in statements {
        match statement {
            Stmt::Inject { .. } | Stmt::Unread { .. } => {}
            Stmt::Declare { value, .. } | Stmt::Return { value, .. } => {
                if let Some(value) = value {
                    value.walk_mut(visit);
                }
            }
            Stmt::Assign { value, .. } | Stmt::Expr(value) => value.walk_mut(visit),
            Stmt::Assert { condition, .. } => condition.walk_mut(visit),
            Stmt::AssignPlace { place, value, .. } => {
                place.walk_mut(visit);
                value.walk_mut(visit);
            }
            Stmt::Hook {
                condition, body, ..
            }
            | Stmt::While {
                condition, body, ..
            } => {
                if let Some(condition) = condition {
                    condition.walk_mut(visit);
                }
                walk_statements_mut(body, visit);
            }
            Stmt::AssertRaises { body, .. } => walk_statements_mut(body, visit),
            Stmt::Yield { time, value, .. } => {
                time.walk_mut(visit);
                value.walk_mut(visit);
            }
            Stmt::For { sequence, body, .. } => {
                sequence.walk_mut(visit);
                walk_statements_mut(body, visit);
            }
            Stmt::Switch {
                selector,
                cases,
                default,
                ..
            } => {
                selector.walk_mut(visit);
                for case in cases {
                    case.label.walk_mut(visit);
                    walk_statements_mut(&mut case.body, visit);
                }
                if let Some(default) = default {
                    walk_statements_mut(default, visit);
                }
            }
        }
    }
}

/// A statement or an expression, as a walk over both visits them.
#[derive(Debug, Clone, Copy)]
pub enum Node<'a> {
    /// A statement.
    Statement(&'a Stmt),
    /// An expression.
    Expression(&'a Expr),
}

/// Calls `visit` with every expression of `statements`, as
/// [`Expr::walk`] does for one expression.
pub fn walk_statements(statements: &[Stmt], visit: &mut dyn FnMut(&Expr)) {
    walk_statement_nodes(statements, &mut |node| {
        if let Node::Expression(expression) = node {
            visit(expression);
        }
    });
}

/// Calls `visit` with every statement of `statements` and every
/// expression of each, as [`Expr::walk_nodes`] does for one expression:
/// each statement before what it holds, a nested statement among them.
pub fn walk_statement_nodes<'a>(statements: &'a [Stmt], visit: &mut dyn FnMut(Node<'a>)) {
    for statement in statements {
        visit(Node::Statement(statement));
        match statement {
            Stmt::Inject { .. } | Stmt::Unread { .. } => {}
            Stmt::Declare { value, .. } | Stmt::Return { value, .. } => {
                if let Some(value) = value {
                    value.walk_nodes(visit);
                }
            }
            Stmt::Assign { value, .. } | Stmt::Expr(value) => value.walk_nodes(visit),
            Stmt::Assert { condition, .. } => condition.walk_nodes(visit),
            Stmt::AssignPlace { place, value, .. } => {
                place.walk_nodes(visit);
                value.walk_nodes(visit);
            }
            Stmt::Hook {
                condition, body, ..
            }
            | Stmt::While {
                condition, body, ..
            } => {
                if let Some(condition) = condition {
                    condition.walk_nodes(visit);
                }
                walk_statement_nodes(body, visit);
            }
            Stmt::AssertRaises { body, .. } => walk_statement_nodes(body, visit),
            Stmt::Yield { time, value, .. } => {
                time.walk_nodes(visit);
                value.walk_nodes(visit);
            }
            Stmt::For { sequence, body, .. } => {
                sequence.walk_nodes(visit);
                walk_statement_nodes(body, visit);
            }
            Stmt::Switch {
                selector,
                cases,
                default,
                ..
            } => {
                selector.walk_nodes(visit);
                for case in cases {
                    case.label.walk_nodes(visit);
                    walk_statement_nodes(&case.body, visit);
                }
                if let Some(default) = default {
                    walk_statement_nodes(default, visit);
                }
            }
        }
    }
}

impl ExprKind {
    /// The operator and operands of a symbol between two operands,
    /// however it is recorded: an [`ExprKind::Binary`], or the call the
    /// symbol stands for.
    pub fn binary(&self) -> Option<(BinaryOperator, &Expr, &Expr)> {
        match self {
            ExprKind::Binary {
                operator,
                left,
                right,
            } => Some((*operator, left, right)),
            // The left operand is the first argument and the right the
            // last, with the configuration between (EXPR-13).
            ExprKind::Call {
                form: Form::Binary(operator),
                arguments,
                ..
            } if arguments.len() >= 2 => {
                let (left, right) = (arguments.first()?, arguments.last()?);
                Some((*operator, &left.value, &right.value))
            }
            _ => None,
        }
    }

    /// The operator and operand of a symbol before one operand, however
    /// it is recorded: an [`ExprKind::Unary`], or the call the symbol
    /// stands for.
    pub fn unary(&self) -> Option<(UnaryOperator, &Expr)> {
        match self {
            ExprKind::Unary { operator, operand } => Some((*operator, operand)),
            // "Unary configuration precedes its operand" (EXPR-13).
            ExprKind::Call {
                form: Form::Unary(operator),
                arguments,
                ..
            } => arguments.last().map(|operand| (*operator, &operand.value)),
            _ => None,
        }
    }
}

impl UnaryOperator {
    /// The operator as HGL writes it.
    pub fn symbol(self) -> &'static str {
        match self {
            UnaryOperator::Negate => "-",
            UnaryOperator::Not => "!",
        }
    }

    /// The operator of core's module the symbol stands for (the
    /// generics chapter: "Syntax binds these core identities").
    pub fn identity(self) -> &'static str {
        match self {
            UnaryOperator::Negate => "neg_",
            UnaryOperator::Not => "not_",
        }
    }
}

impl BinaryOperator {
    /// The operator of core's module the symbol stands for (the
    /// generics chapter: "Syntax binds these core identities").
    pub fn identity(self) -> &'static str {
        match self {
            BinaryOperator::Add => "add_",
            BinaryOperator::Subtract => "sub_",
            BinaryOperator::Multiply => "mul_",
            BinaryOperator::Divide => "div_",
            BinaryOperator::FloorDivide => "floordiv_",
            BinaryOperator::Modulo => "mod_",
            BinaryOperator::Equal => "eq_",
            BinaryOperator::NotEqual => "ne_",
            BinaryOperator::Less => "lt_",
            BinaryOperator::LessEqual => "le_",
            BinaryOperator::Greater => "gt_",
            BinaryOperator::GreaterEqual => "ge_",
            BinaryOperator::Compare => "compare_",
            BinaryOperator::And => "and_",
            BinaryOperator::Or => "or_",
        }
    }

    /// The operator as HGL writes it.
    pub fn symbol(self) -> &'static str {
        match self {
            BinaryOperator::Add => "+",
            BinaryOperator::Subtract => "-",
            BinaryOperator::Multiply => "*",
            BinaryOperator::Divide => "/",
            BinaryOperator::FloorDivide => "//",
            BinaryOperator::Modulo => "%",
            BinaryOperator::Equal => "==",
            BinaryOperator::NotEqual => "!=",
            BinaryOperator::Less => "<",
            BinaryOperator::LessEqual => "<=",
            BinaryOperator::Greater => ">",
            BinaryOperator::GreaterEqual => ">=",
            BinaryOperator::Compare => "<=>",
            BinaryOperator::And => "&&",
            BinaryOperator::Or => "||",
        }
    }
}
