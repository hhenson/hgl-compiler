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

use crate::program::{LocalKind, OperatorRef};
use crate::types::{Argument as GenericArgument, CallArguments, Type};

/// An application's run statement, typed (RUN-1): "its run statement
/// is `run call` or `eval call`, where the call is of an entry, written
/// as any call is and giving the entry's const arguments. The statement
/// gives no configuration: the run's is supplied to it (RUN-6)." The
/// defaults the application gives it are its `inject run_config`'s
/// ([`Stmt::Inject`], among the store's statements).
#[derive(Debug, Clone, PartialEq)]
pub struct Run {
    /// True for `run`, a run in real time; false for `eval`, a
    /// simulation. RUN-3: "The word of a run statement is its mode."
    pub real_time: bool,
    /// The injected `run_config`, if in scope, and locals declared
    /// inside the entry arguments (RUN-6, UNIT-14).
    pub locals: Vec<Local>,
    /// The call of the entry, with the entry's const arguments.
    pub call: Expr,
    /// Where the statement is written.
    pub at: Location,
}

/// One entry of a run's configuration, `length: 3s`, written in braces
/// after `eval` or `run` in a test or after `run_config` in an `inject`
/// (RUN-6): "Each entry names a field of the configuration, and its value
/// is value code of that field's type".
#[derive(Debug, Clone, PartialEq)]
pub struct ConfigEntry {
    /// The field it names, a property of core's `RunConfig` (core's
    /// CAP-7): `start`, `end` or `length`.
    pub field: String,
    /// What it gives the field, of the field's type.
    pub value: Expr,
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

/// A checked default and the local bindings declared inside it (UNIT-7).
#[derive(Debug, Clone, PartialEq)]
pub struct DefaultExpr {
    /// The value computed in the declaration's enclosing scope.
    pub value: Expr,
    /// Locals introduced by blocks in the expression, in local ID order.
    pub locals: Vec<Local>,
}

impl DefaultExpr {
    /// Treat the expression as a body for specialization and analysis of its locals.
    pub fn into_body(self) -> Body {
        Body {
            locals: self.locals,
            statements: vec![Stmt::Expr(self.value)],
        }
    }

    /// Recover a default after processing the body made by `into_body`.
    pub(crate) fn from_body(mut body: Body) -> Self {
        let Some(Stmt::Expr(value)) = body.statements.pop() else {
            unreachable!("the default is a single expression");
        };
        Self {
            value,
            locals: body.locals,
        }
    }
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
    /// (EXPR-3). An expression is one while the analyser reads its
    /// body, until it records what it is ([`Expr::category`]).
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
    /// name (TIME-8). `out` is the node's result (TIME-7). An
    /// application's or an integration test's `inject run_config{...}`
    /// gives the run's configuration its defaults (RUN-6).
    Inject {
        /// The local that names the capability.
        local: LocalId,
        /// The entries of the configuration in braces after
        /// `run_config`, in the order written; empty for any other name,
        /// and where none is written.
        config: Vec<ConfigEntry>,
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
        /// What a `when`'s header asks of its node, as the analyser
        /// reads it; `None` for a `start` and a `stop`.
        header: Option<Header>,
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

/// What a `when` header asks of its node (TIME-3, TIME-4), read by the
/// analyser from the typed condition and given to the stages after it,
/// which work none of it out again. TIME-3: "A selector sets subscription or
/// admission when it stands alone in the header or appears as a
/// top-level conjunct. Selectors under negation or disjunction leave
/// the default in place."
#[derive(Debug, Clone, PartialEq)]
pub struct Header {
    /// The inputs whose tick wakes the handler: those its `modified`
    /// selectors name, `modified()` every temporal input and
    /// `modified([])` none; every temporal input where it writes none.
    pub subscribed: Vec<LocalId>,
    /// True where the header writes a `modified` selector.
    pub selects: bool,
    /// What the handler requires to be valid: what its `valid`
    /// selectors name, the temporal inputs and `out`, `valid()` every
    /// temporal input and `valid([])` none. With no `valid` selector,
    /// every temporal input, but for a header that is `scheduled()`
    /// alone, which requires none. Of these, the temporal inputs are
    /// what the handler admits.
    pub required: Vec<LocalId>,
    /// True where `scheduled()` stands alone or as a top-level
    /// conjunct: a wake by the node's scheduler or alarm runs it.
    pub scheduled: bool,
    /// Where `modified([])` stands beside a `modified` selector that
    /// names inputs, each a top-level conjunct: the specification does
    /// not say what the two ask together.
    pub beside: Option<Location>,
    /// What is checked when the handler runs, as positions among the
    /// condition's conjuncts ([`Expr::conjuncts`]): each that is no
    /// selector, and each `modified` selector but `modified([])`, a
    /// guard on a publication of what it names.
    pub conditions: Vec<usize>,
}

/// One `case label: statements` of a `switch`.
#[derive(Debug, Clone, PartialEq)]
pub struct Case {
    /// The label.
    pub label: Expr,
    /// The value the label comes to, where the analyser resolved it
    /// (CTRL-6: "compared by resolved value"): `None` for one it cannot
    /// fold, a generic value parameter among them, whose value only an
    /// instance has (GEN-13).
    pub value: Option<LabelValue>,
    /// Its statements. There is no fallthrough (CTRL-6).
    pub body: Vec<Stmt>,
}

/// The value of a `switch` label, for telling two alike (CTRL-6).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LabelValue {
    /// An integer.
    Int(i64),
    /// `true` or `false`.
    Bool(bool),
    /// A string.
    Str(String),
    /// A member of the selector's enum, by its position.
    Member(usize),
    /// A float, by its bits, with one zero: `-0.0` is `0.0`. A NaN is
    /// equal to nothing (NUM-5), and has no value here.
    Float(u64),
}

impl LabelValue {
    /// The value of the float `value`; `None` for a NaN, which is equal
    /// to nothing, itself among them (NUM-5).
    pub fn float(value: f64) -> Option<LabelValue> {
        if value.is_nan() {
            return None;
        }
        let one_zero = if value == 0.0 { 0.0 } else { value };
        Some(LabelValue::Float(one_zero.to_bits()))
    }
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
    /// Whether it is a connection or an ordinary value where it stands
    /// (FN-3: "There a temporal parameter, and the result of a temporal
    /// or lifted call, is a connection"). The analyser records it on
    /// every expression of the program once the body it stands in is
    /// read, and on an instance's body with the instance's callees in
    /// place ([`crate::Program::instance`]); a stage after the analyser
    /// reads it here and works none of it out again.
    pub category: ValueCategory,
}

impl Expr {
    /// True where the expression is a connection ([`Expr::category`]).
    pub fn is_connection(&self) -> bool {
        self.category == ValueCategory::Connection
    }

    /// The local this expression names directly, including a read
    /// whose proof of presence waits for instance selection (ERR-12).
    pub(crate) fn named_local(&self) -> Option<LocalId> {
        match self.kind {
            ExprKind::Local(local) | ExprKind::DeferredPresent(local) => Some(local),
            _ => None,
        }
    }

    /// True for a call that is lifted (FN-6): see the `lifted` of
    /// [`ExprKind::Call`].
    pub fn is_lifted(&self) -> bool {
        matches!(self.kind, ExprKind::Call { lifted: true, .. })
    }
}

/// Which branches can supply the deferred type of an `if` with an
/// `else` (CTRL-10, CTRL-13). A branch that returns supplies no value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IfInference {
    /// No inference across two branches. The result may come from its
    /// context, or may have no value or no `else`.
    None,
    /// The value takes the common type of both branches.
    Both,
    /// Only the `then` branch supplies a value. The other one returns.
    Then,
    /// Only the `else` branch supplies a value. The other one returns.
    Otherwise,
}

/// What an expression is.
#[derive(Debug, Clone, PartialEq)]
pub enum ExprKind {
    /// A literal.
    Literal(Value),
    /// A compile-time callable argument (FN-17). It is erased before emission.
    StaticCallable(Box<Callee>),
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
    /// One concrete entry of an argument pack (FN-19). Lowering maps
    /// it to the runtime input created for that key.
    PackEntry {
        /// The local for the pack parameter in the unexpanded signature.
        pack: LocalId,
        /// The entry's original name or position.
        key: PackKey,
    },
    /// An immutable graph binding read where a null test proved its
    /// presence, before the instance has determined its ordinary type (ERR-12).
    DeferredPresent(LocalId),
    /// The endpoint of a temporal input, or of `out`, where a native
    /// value function takes the endpoint and not its payload: the
    /// argument of an `endpoint<T>` parameter (TIME-9). The payload is
    /// not read.
    EndpointOf(LocalId),
    /// An operation on a child of a temporal input, an element, a field
    /// or an entry (TIME-1, TIME-9, END-8): `valid(parts[1])`,
    /// `modified(quote.bid)`, `delta_value(parts[i])`. The child is an
    /// endpoint whether or not it is valid, and is named, not read:
    /// `child` is the [`ExprKind::Index`] or [`ExprKind::Field`] a read
    /// of it is written as, rooted at the input's [`ExprKind::Local`].
    ChildEndpoint {
        /// The operation: `valid`, `all_valid`, `modified`,
        /// `delta_value` or `last_modified`.
        operation: EndpointOperation,
        /// The child.
        child: Box<Expr>,
    },
    /// The endpoint of a child of a temporal input, given to an
    /// `endpoint<T>` parameter or retained by an inferred `let`
    /// (TIME-9, ERR-14). It is named as in [`ExprKind::ChildEndpoint`].
    ChildEndpointOf(Box<Expr>),
    /// The child endpoint held by an inferred `let` (ERR-14). This
    /// names the endpoint selected when the binding was evaluated,
    /// with its payload type. An endpoint operation uses it directly.
    /// It also names a captured parent's endpoint when a value helper
    /// binds a child of that parent (FN-14).
    BoundChild(LocalId),
    /// A read of a collection's child that is checked when it runs
    /// (TIME-1, ERR-14): `parts[i]` in a hook, whose child is known only
    /// then. A child that is not valid fails the read with
    /// `value.unset_read`. Where `result` is true the place expects a
    /// `result<T, error>`: the read gives `ok` of the child's value, or
    /// `err` of an `error` with that code, and the expression is of that
    /// result; otherwise it is of the child's T.
    CheckedChild {
        /// The read: an [`ExprKind::Index`] or [`ExprKind::Field`]
        /// rooted at a temporal input, or a [`ExprKind::BoundChild`].
        read: Box<Expr>,
        /// True where the place expects `result<T, error>`.
        result: bool,
    },
    /// A read of a window's value where its place expects
    /// `result<list<V>, error>` (TYPE-11, ERR-14): `ok` of the `list<V>`
    /// it holds, oldest first, or, where the window is not all valid,
    /// `err` of an `error` whose code is `value.unset_read`. The read is
    /// of a temporal input, of `out`, or of a child of either named when
    /// the program is compiled, typed as the `list<V>` it holds; one at
    /// an index known only when it runs is a
    /// [`ExprKind::CheckedChild`]. The expression is of the result.
    CheckedWindow(Box<Expr>),
    /// An admitted read of a whole dictionary or unsized list into a
    /// `result<T, error>` place (TIME-1, ERR-14). An incomplete valid
    /// entry or a gap without a value gives `err(value.unset_read)`.
    CheckedWhole(Box<Expr>),
    /// A read of what an endpoint published in the cycle, its
    /// `delta_value`, or of what a window lost in it, its
    /// `removed_value`, where its place expects `result<T, error>`, T
    /// what the read gives (ERR-14, TIME-9): `ok` of that, or `err` of an
    /// `error` whose code is `value.unset_read` where the endpoint has
    /// none in the cycle. Only a window of a duration that a value leaves
    /// with none arriving has no delta at a tick (TYPE-11). The read is
    /// an [`ExprKind::Endpoint`], an [`ExprKind::ChildEndpoint`] or the
    /// call of core's `removed_value`; the expression is of the result.
    CheckedDelta(Box<Expr>),
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
        /// Which branches supply the deferred type, as source flow
        /// determined (CTRL-10, CTRL-13).
        inference: IfInference,
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
    /// A unary minus awaiting its operand's type in a graph.
    DeferredNegation {
        /// The operator this symbol binds.
        identity: OperatorRef,
        /// The source location of the minus itself.
        at: Location,
        /// What the original place expects of the operator's result.
        expected: Option<Box<Type>>,
        /// Where the operand was written.
        operand_at: Location,
        /// Configuration entries in written order (EXPR-13).
        configuration: Vec<DeferredOption>,
        /// The operand, already read.
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
    /// A binary symbol awaiting an operand's type in a graph.
    DeferredBinary {
        /// The binary form written in source.
        operator: BinaryOperator,
        /// The operator this symbol binds.
        identity: OperatorRef,
        /// The source location of the symbol itself.
        at: Location,
        /// What the original place expects of the operator's result.
        expected: Option<Box<Type>>,
        /// The source locations of the two written operands.
        operands: [Location; 2],
        /// Configuration entries in written order (EXPR-13).
        configuration: Vec<DeferredOption>,
        /// The left operand, already read in source order.
        left: Box<Expr>,
        /// The right operand, already read in source order.
        right: Box<Expr>,
    },
    /// An `i64` taken as the `f64` of the same number, where an `f64`
    /// is wanted (EXPR-5, NUM-1). The source writes nothing for it.
    Widen(Box<Expr>),
    /// `convert<list<B>>(values)`: a list of structs taken as a list of
    /// an abstract struct `B` they inherit from. Core's OPS-6: "it
    /// yields the same elements". The expression's type is the list
    /// asked for; core declares no candidate for it.
    ParentList(Box<Expr>),
    /// A call: `scale(value, factor: 2.0)`.
    Call {
        /// What is called.
        callee: Callee,
        /// The arguments, as they are written.
        arguments: Vec<Argument>,
        /// How the call is written.
        form: Form,
        /// Values explicitly supplied in braces (EXPR-10), by source
        /// location, for the instance's wiring-time check (EXPR-14).
        braced: Vec<Location>,
        /// True where the call is lifted (FN-6: "A selected value
        /// helper, a `const fn` or a value candidate of an operator,
        /// with temporal arguments in composition is lifted"): it is
        /// written in a graph's body, it calls a value function or a
        /// value candidate that marks no parameter `var` (FN-15: "A
        /// function or a candidate that declares one is not lifted"),
        /// and an argument is a connection. The arguments that are
        /// connections are the node's inputs, and the rest its
        /// configuration. Recorded with [`Expr::category`]. An
        /// operation the declaration requires (GEN-7) is not, as its
        /// candidate is not known: an instance's body has the
        /// candidate in its place, and says. An eval's call is not
        /// either: the eval wires its target (TST-8).
        lifted: bool,
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
        /// Whether GEN-2 infers the type arguments from these fields.
        /// Explicit arguments and an expected struct type stay fixed.
        infer_types: bool,
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
    /// An enum construction whose argument type awaits graph settlement.
    /// The expression's type is already the enum it constructs (TYPE-7).
    DeferredEnumFrom(Box<Expr>),
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
    /// A graph member read awaiting the base type of a selected instance.
    DeferredMember {
        /// What the member is read from.
        base: Box<Expr>,
        /// The member's spelling before its position is known.
        name: String,
        /// Where the member name is written.
        at: Location,
        /// Whether the matching ERR-3 guard holds here if the base
        /// settles to an ordinary result binding.
        variant_proven: bool,
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
    /// (ERR-2). Its type is the result its place expects. It can keep
    /// its checked payload while an inferred constructor field settles.
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
    /// A result given where its T is expected (ERR-15: "Where it expects
    /// T, it gives a success's value, and a failure's own error is
    /// raised, where raising is admitted (ERR-5)"): what a call gives
    /// whose function, or whose operator's selected candidate, gives a
    /// `result<T, error>`, or what a binding holds; or, in a value
    /// function that gives a `result<U, E>`, a `result<T, E>` of that
    /// same E. The expression is of the T; `raise` says where a failure
    /// goes. In composition, where the result is a connection, the
    /// expression is a connection of T.
    Raised {
        /// The result.
        result: Box<Expr>,
        /// Where a failure is raised.
        raise: Raise,
    },
    /// A result given where its `T?` is expected, `T` not nullable
    /// (ERR-15: "Where it expects `T?`, it gives a success's value, or
    /// null for a failure, whose error is not kept"), a comparison with
    /// `null` among them. The expression is of the `T?`. In composition,
    /// where the result is a connection, `reader` says which node turns
    /// it into a connection of `T?`: false for the node of the call that
    /// gives it ("of `T?`, where a failure publishes null"), true for the
    /// node that reads it, a result a binding holds ("takes that form as
    /// a call's does, in the node that reads it").
    OrNull {
        /// The result.
        result: Box<Expr>,
        /// True where the node that reads a connection of the result
        /// takes its `T?`; false otherwise.
        reader: bool,
    },
    /// A `T?` given where a T is expected (ERR-12: "A `T?` value given
    /// where a T is expected is read as its value: the place tries it,
    /// and a null fails the read with the code `value.null`, an
    /// execution failure raised as a result's failure is (ERR-5)"). The
    /// expression is of the T. A null written there is no such read: it
    /// rejects (TYPE-14). A binding a null test has proved not null is
    /// read as its T, and is not one either.
    NullRead {
        /// The nullable value read.
        value: Box<Expr>,
        /// Where a null's failure is raised.
        raise: Raise,
        /// The code of the failure a null gives: [`NULL_READ_CODE`].
        code: &'static str,
    },
    /// A nullable bound child's payload on a path that proves this
    /// alias is present (ERR-12). The inner read retains its endpoint
    /// admission and failure checks (ERR-14, TIME-1).
    PresentChild(Box<Expr>),
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
    /// `eval target(...)` (TST-5): the call of the target it stands
    /// for, with a trace given to each temporal parameter and
    /// configuration to each `const` one. Its type is a trace of the
    /// target's result, or none for a target with no result (TST-8).
    Eval {
        /// The call the eval takes, with a trace where the function
        /// called takes a temporal input.
        call: Box<Expr>,
        /// The entries of the run's configuration written in braces
        /// after the word, `eval{length: 4s} target(...)`, in the order
        /// written (RUN-6). A field none names is left out.
        config: Vec<ConfigEntry>,
        /// The local an integration test binds `run_config` to, where it
        /// injects it before this eval (TST-17). RUN-6: "a field the
        /// statement does not name is the injected configuration's."
        inherited: Option<LocalId>,
        /// True for `run target(...)` in an integration test (TST-17):
        /// the run is in real time, where an eval's is a simulation
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

/// The code of the failure a `T?` read where a T is expected gives
/// where it holds null (ERR-12; the catalogue's `value.null`).
pub const NULL_READ_CODE: &str = "value.null";

/// Where a failure raised by a place is raised (ERR-5: "This rule says
/// what raised means"), for a result given where its T is expected
/// ([`ExprKind::Raised`]) and a null read where a T is expected
/// ([`ExprKind::NullRead`]). The analyser decides it from where the
/// expression stands; a target writes what it says.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Raise {
    /// In a value function that gives a `result<U, E>`: "the function
    /// gives that E as its failure", and nothing after it is evaluated.
    Return,
    /// In a value function whose result is no result, one that gives an
    /// `i64` or gives nothing: "its failure is an execution failure
    /// (ERR-1), as an index outside a list is (EXPR-7)". It fails what
    /// evaluates the function. A constant's value and a default are
    /// read so too.
    Execution,
    /// In a `when`: "the node fails".
    Node,
    /// In a source's body: "its failure fails the source, as it fails a
    /// node in a `when`".
    Source,
    /// In a lifecycle hook, `start` or `stop`, and in a node's `state`
    /// or `cache` initializer, which its start evaluates: "a failure of
    /// that phase (RUN-4)".
    Lifecycle(Hook),
    /// In value code evaluated where a graph is built (ERR-15: "A call
    /// that is evaluated where the graph is built and gives no
    /// connection (FN-6, FN-9) takes its form as value code does"): a
    /// failure of the graph's building.
    Build,
    /// Among a test's statements: a failure of the test.
    Test,
    /// Among an application's statements: a failure of the run.
    Application,
    /// In composition, a connection a call gives, of a temporal function
    /// or one that is lifted (FN-6): "a success publishes its value and
    /// a failure fails the call's node in that cycle, as a failure
    /// raised in a `when` does, within whatever `capture` bounds it
    /// (ERR-7)".
    CallNode,
    /// In composition, any other connection, one a binding holds among
    /// them, given where a connection of T is taken: the node that reads
    /// it fails in a cycle that publishes a failure, or a null (ERR-12:
    /// "the node that reads it failing in a cycle that publishes a
    /// null").
    Reader,
}

/// Where a required operator call reads its configuration if the selected
/// parameter takes a result's or a nullable value's payload (EXPR-5).
#[derive(Debug, Clone, PartialEq)]
pub struct ConfigurationRead {
    /// Where a read raises an error.
    pub raise: Raise,
    /// The enclosing function's result, for propagation (ERR-5).
    pub result: Option<Type>,
    /// Whether the result belongs to an anonymous function.
    pub anonymous: bool,
}

/// A configuration entry of a graph symbol awaiting candidate selection.
#[derive(Debug, Clone, PartialEq)]
pub struct DeferredOption {
    /// The written option name (EXPR-10).
    pub name: String,
    /// The source expression that gives the option.
    pub at: Location,
    /// The existing reading, absent when the candidate must supply its place.
    pub value: Option<Expr>,
}

/// An operator call whose arguments need the concrete graph instance.
#[derive(Debug, Clone, PartialEq)]
pub struct DeferredOperator {
    /// The bound operator identity (GEN-4).
    pub identity: OperatorRef,
    /// The written argument list or Boolean symbol expression.
    pub list: Location,
    /// What the original place expects of the call's result.
    pub expected: Option<Type>,
    /// The explicit arguments written after the operator name (GEN-8).
    pub generics: Option<CallArguments>,
    /// Which arguments take their type from a candidate, in written order.
    pub contextual: Vec<bool>,
    /// Whether this call shares the enclosing `capture`'s argument list.
    pub captured: bool,
}

/// A function argument waiting for a type its graph instance can infer.
#[derive(Debug, Clone, PartialEq)]
pub struct ContextualArgument {
    /// Its index in the call's arguments, in written order.
    pub position: usize,
    /// The argument's source expression.
    pub at: Location,
    /// The local proofs in scope at this read (ERR-3, ERR-12, TIME-4).
    pub facts: Vec<ArgumentFact>,
}

/// A fact about a binding, recorded by declaration location so it can be
/// recovered if an instance inserts hidden parameters before its locals.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArgumentFact {
    /// A nullable local is present (ERR-12).
    Present(Location),
    /// An input is valid (TIME-4).
    Valid(Location),
    /// The local is a success (ERR-3).
    Success(Location),
    /// The local is a failure (ERR-3).
    Failure(Location),
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
        /// Resolved type packs (GEN-10), in the order of their generic
        /// parameters. A pack has no single type in `generics`.
        type_packs: Vec<TypePackArgument>,
        /// What answers each requirement the function states, with
        /// `generics` in place: see [`Answer`]. Empty where `generics`
        /// is.
        requirements: Vec<Answer>,
        /// Written arguments whose places still depend on graph instance
        /// inference, by index and source location (GEN-2, EXPR-9, ERR-2).
        contextual: Vec<ContextualArgument>,
    },
    /// The value candidate selected for a call of an operator (GEN-5).
    Candidate {
        /// The operator called: the position of its module in
        /// `Program::modules`, and its position in that module's
        /// `declarations`.
        operator: (usize, usize),
        /// The operator's own generic arguments, used to read inherited
        /// configuration defaults in its defining scope (GEN-6, UNIT-7).
        operator_generics: Vec<GenericArgument>,
        /// The position of the candidate's module in
        /// `Program::modules`.
        module: usize,
        /// The candidate's position in that module's `declarations`.
        declaration: usize,
        /// What each generic parameter of the candidate stands for at
        /// this call (GEN-2), in the order declared. Empty for a
        /// candidate with none.
        generics: Vec<GenericArgument>,
        /// Resolved type packs (GEN-10), in the order of their generic
        /// parameters. A pack has no single type in `generics`.
        type_packs: Vec<TypePackArgument>,
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
        /// The operator's own generic arguments, for inherited defaults.
        operator_generics: Vec<GenericArgument>,
        /// The requirement of the enclosing declaration the call is:
        /// its position among the constraint calls that hold wherever
        /// the declaration's `requires` does, in the order written.
        requirement: usize,
        /// The context of supplied configuration, fitted to the selected
        /// candidate's parameters once the requirement is answered.
        configuration: Option<Box<ConfigurationRead>>,
    },
    /// A call of a function parameter, or that parameter forwarded as
    /// a compile-time argument (FN-17).
    Parameter {
        /// The position of the callable in the enclosing signature.
        parameter: usize,
        /// Whether its template composes a temporal function.
        temporal: bool,
    },
    /// A statically selected anonymous function, with explicit lexical captures.
    Anonymous(Box<AnonymousFunction>),
    /// A named operator awaiting an argument type before selection (GEN-7).
    DeferredOperator(Box<DeferredOperator>),
    /// A call waiting for the entries of its pack arguments (FN-19).
    DeferredPackCall,
    /// A deferred operator call whose candidates are all value functions.
    DeferredValue,
    /// No callee: a name left for the prelude, a function that is
    /// itself an expression, or an operator of which no candidate is
    /// selected, by its name or by a symbol.
    Unread,
}

impl Callee {
    /// The function or the candidate the call selected: the position of
    /// its module in `Program::modules` and its position in that
    /// module's `declarations`. `None` for an operation the enclosing
    /// declaration requires, whose candidate is selected where it is
    /// called, and for no callee.
    pub fn declaration(&self) -> Option<(usize, usize)> {
        match self {
            Callee::Function {
                module,
                declaration,
                ..
            }
            | Callee::Candidate {
                module,
                declaration,
                ..
            } => Some((*module, *declaration)),
            Callee::Required { .. }
            | Callee::Parameter { .. }
            | Callee::Anonymous(_)
            | Callee::DeferredOperator(_)
            | Callee::DeferredPackCall
            | Callee::DeferredValue
            | Callee::Unread => None,
        }
    }

    /// What each generic parameter of the function or the candidate
    /// selected stands for at this call, in the order declared. Empty
    /// for a callee that is neither.
    pub fn generics(&self) -> &[GenericArgument] {
        match self {
            Callee::Function { generics, .. } | Callee::Candidate { generics, .. } => generics,
            Callee::Required { .. }
            | Callee::Parameter { .. }
            | Callee::Anonymous(_)
            | Callee::DeferredOperator(_)
            | Callee::DeferredPackCall
            | Callee::DeferredValue
            | Callee::Unread => &[],
        }
    }

    /// The concrete type sequences this call binds to generic packs (GEN-10).
    pub fn type_packs(&self) -> &[TypePackArgument] {
        match self {
            Callee::Function { type_packs, .. } | Callee::Candidate { type_packs, .. } => {
                type_packs
            }
            Callee::Required { .. }
            | Callee::Parameter { .. }
            | Callee::Anonymous(_)
            | Callee::DeferredOperator(_)
            | Callee::DeferredPackCall
            | Callee::DeferredValue
            | Callee::Unread => &[],
        }
    }
}

/// The checked body of an anonymous argument and its lexical environment (FN-14, FN-17).
#[derive(Debug, Clone, PartialEq)]
pub struct AnonymousFunction {
    /// Where it is written, for identity and diagnostics.
    pub at: Location,
    /// Its concrete parameter and result template.
    pub template: crate::types::FunctionType,
    /// The inputs come first in the local table, then its captures, then its other locals.
    pub body: Body,
    /// The bindings in the expression's enclosing body that supply those captures.
    pub captures: Vec<LocalId>,
    /// Type and value arguments of the enclosing specialization, if any.
    pub specialization: Vec<GenericArgument>,
}

impl AnonymousFunction {
    /// The type of the captured input in this function's local table.
    pub fn capture_type(&self, index: usize) -> &Type {
        &self.body.locals[self.template.parameters.len() + index].ty
    }

    /// What a caller passes for this lexical capture (FN-14). A child
    /// selected in the helper still needs its parent's endpoint (ERR-14).
    pub fn capture_value(&self, index: usize, binding: &Local, at: Location) -> Expr {
        let capture = self.captures[index];
        let ty = self.capture_type(index);
        let (kind, ty) =
            if matches!(ty, Type::Endpoint(_)) && !matches!(binding.ty, Type::Endpoint(_)) {
                (ExprKind::EndpointOf(capture), ty.clone())
            } else {
                (ExprKind::Local(capture), binding.ty.clone())
            };
        Expr {
            kind,
            ty,
            at,
            category: binding.category,
        }
    }
}

/// The ordered list of types that a call gives one generic type pack.
#[derive(Debug, Clone, PartialEq)]
pub struct TypePackArgument {
    /// Its position among the function's generic parameters.
    pub parameter: usize,
    /// Each type of the pack in binding order, possibly none.
    pub types: Vec<Type>,
    /// The graph's arguments still have to complete this inferred pack.
    /// A pack with written types is already fixed (GEN-8).
    pub infer_from_arguments: bool,
    /// Each type's name or position in the same order (GEN-17).
    pub keys: Vec<PackKey>,
}

impl TypePackArgument {
    /// A pack of types given in positional order (GEN-10).
    pub(crate) fn positional(parameter: usize, types: Vec<Type>) -> Self {
        let keys = (0..types.len()).map(PackKey::Position).collect();
        Self {
            parameter,
            types,
            infer_from_arguments: false,
            keys,
        }
    }

    /// A pack whose types are known with their names or positions (GEN-17).
    pub(crate) fn keyed(parameter: usize, entries: Vec<(PackKey, Type)>) -> Self {
        let (keys, types) = entries.into_iter().unzip();
        Self {
            parameter,
            types,
            infer_from_arguments: false,
            keys,
        }
    }

    /// The type named by a compile-time key (GEN-17).
    pub(crate) fn member(&self, key: &crate::types::ConstantValue) -> Option<&Type> {
        use crate::types::ConstantValue;

        self.keys
            .iter()
            .zip(&self.types)
            .find_map(|(entry, ty)| match (entry, key) {
                (PackKey::Name(name), ConstantValue::Str(given)) if name == given => Some(ty),
                (PackKey::Position(pos), ConstantValue::Int(given))
                    if usize::try_from(*given) == Ok(*pos) =>
                {
                    Some(ty)
                }
                _ => None,
            })
    }
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
    /// first. `None` when it is given to nothing.
    pub parameter: Option<usize>,
    /// The position or name of an argument captured by that parameter's
    /// pack (FN-8, GEN-10).
    pub pack: Option<PackKey>,
    /// Its value.
    pub value: Expr,
}

/// What identifies a captured argument within a pack (FN-8, GEN-10).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PackKey {
    /// Zero-based position in a positional pack in written order.
    Position(usize),
    /// The supplied source name in a named pack.
    Name(String),
}

/// The type and key of one input of an instantiated pack parameter (FN-19).
#[derive(Debug, Clone, PartialEq)]
pub struct PackInput {
    /// The parameter slot before expansion.
    pub parameter: usize,
    /// The key in that parameter's pack.
    pub key: PackKey,
    /// The type of the entry after ordinary argument fitting.
    pub ty: Type,
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
            | ExprKind::PackEntry { .. }
            | ExprKind::BoundChild(_)
            | ExprKind::DeferredPresent(_)
            | ExprKind::EndpointOf(_)
            | ExprKind::GenericValue { .. }
            | ExprKind::EnumMember { .. }
            | ExprKind::StaticCallable(_)
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
                ..
            } => {
                condition.walk_nodes(visit);
                then.walk_nodes(visit);
                if let Some(otherwise) = otherwise {
                    otherwise.walk_nodes(visit);
                }
            }
            ExprKind::DeferredNegation {
                configuration,
                operand,
                ..
            } => {
                for value in configuration
                    .iter()
                    .filter_map(|entry| entry.value.as_ref())
                {
                    value.walk_nodes(visit);
                }
                operand.walk_nodes(visit);
            }
            ExprKind::DeferredBinary {
                left,
                configuration,
                right,
                ..
            } => {
                left.walk_nodes(visit);
                for value in configuration
                    .iter()
                    .filter_map(|entry| entry.value.as_ref())
                {
                    value.walk_nodes(visit);
                }
                right.walk_nodes(visit);
            }
            ExprKind::Unary { operand: inner, .. }
            | ExprKind::Widen(inner)
            | ExprKind::ParentList(inner)
            | ExprKind::EnumFrom { value: inner, .. }
            | ExprKind::DeferredEnumFrom(inner)
            | ExprKind::Field { base: inner, .. }
            | ExprKind::DeferredMember { base: inner, .. }
            | ExprKind::Tagged { payload: inner, .. }
            | ExprKind::TagTest { value: inner, .. }
            | ExprKind::Payload { result: inner, .. }
            | ExprKind::ErrorPart { base: inner, .. }
            | ExprKind::Raised { result: inner, .. }
            | ExprKind::OrNull { result: inner, .. }
            | ExprKind::NullRead { value: inner, .. }
            | ExprKind::PresentChild(inner)
            | ExprKind::Capture(inner)
            | ExprKind::Captured { capture: inner, .. }
            | ExprKind::ChildEndpoint { child: inner, .. }
            | ExprKind::ChildEndpointOf(inner)
            | ExprKind::CheckedChild { read: inner, .. }
            | ExprKind::CheckedWindow(inner)
            | ExprKind::CheckedWhole(inner)
            | ExprKind::CheckedDelta(inner)
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
                for entry in config {
                    entry.value.walk_nodes(visit);
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
        self.walk_nodes_mut(&mut |node| {
            if let NodeMut::Expression(expression) = node {
                visit(expression);
            }
        });
    }

    fn walk_nodes_mut(&mut self, visit: &mut dyn FnMut(NodeMut<'_>)) {
        visit(NodeMut::Expression(self));
        match &mut self.kind {
            ExprKind::Literal(_)
            | ExprKind::Constant { .. }
            | ExprKind::Endpoint { .. }
            | ExprKind::Local(_)
            | ExprKind::PackEntry { .. }
            | ExprKind::BoundChild(_)
            | ExprKind::DeferredPresent(_)
            | ExprKind::EndpointOf(_)
            | ExprKind::GenericValue { .. }
            | ExprKind::EnumMember { .. }
            | ExprKind::StaticCallable(_)
            | ExprKind::Unread => {}
            ExprKind::Block { statements, value } => {
                walk_statement_nodes_mut(statements, visit);
                if let Some(value) = value {
                    value.walk_nodes_mut(visit);
                }
            }
            ExprKind::If {
                condition,
                then,
                otherwise,
                ..
            } => {
                condition.walk_nodes_mut(visit);
                then.walk_nodes_mut(visit);
                if let Some(otherwise) = otherwise {
                    otherwise.walk_nodes_mut(visit);
                }
            }
            ExprKind::DeferredNegation {
                configuration,
                operand,
                ..
            } => {
                for value in configuration
                    .iter_mut()
                    .filter_map(|entry| entry.value.as_mut())
                {
                    value.walk_nodes_mut(visit);
                }
                operand.walk_nodes_mut(visit);
            }
            ExprKind::DeferredBinary {
                left,
                configuration,
                right,
                ..
            } => {
                left.walk_nodes_mut(visit);
                for value in configuration
                    .iter_mut()
                    .filter_map(|entry| entry.value.as_mut())
                {
                    value.walk_nodes_mut(visit);
                }
                right.walk_nodes_mut(visit);
            }
            ExprKind::Unary { operand: inner, .. }
            | ExprKind::Widen(inner)
            | ExprKind::ParentList(inner)
            | ExprKind::EnumFrom { value: inner, .. }
            | ExprKind::DeferredEnumFrom(inner)
            | ExprKind::Field { base: inner, .. }
            | ExprKind::DeferredMember { base: inner, .. }
            | ExprKind::Tagged { payload: inner, .. }
            | ExprKind::TagTest { value: inner, .. }
            | ExprKind::Payload { result: inner, .. }
            | ExprKind::ErrorPart { base: inner, .. }
            | ExprKind::Raised { result: inner, .. }
            | ExprKind::OrNull { result: inner, .. }
            | ExprKind::NullRead { value: inner, .. }
            | ExprKind::PresentChild(inner)
            | ExprKind::Capture(inner)
            | ExprKind::Captured { capture: inner, .. }
            | ExprKind::ChildEndpoint { child: inner, .. }
            | ExprKind::ChildEndpointOf(inner)
            | ExprKind::CheckedChild { read: inner, .. }
            | ExprKind::CheckedWindow(inner)
            | ExprKind::CheckedWhole(inner)
            | ExprKind::CheckedDelta(inner)
            | ExprKind::Publications(inner) => inner.walk_nodes_mut(visit),
            ExprKind::Binary { left, right, .. }
            | ExprKind::Index {
                base: left,
                index: right,
            }
            | ExprKind::TraceCompare { left, right, .. } => {
                left.walk_nodes_mut(visit);
                right.walk_nodes_mut(visit);
            }
            ExprKind::Call { arguments, .. }
            | ExprKind::Construct { arguments, .. }
            | ExprKind::Delta { arguments } => {
                for argument in arguments {
                    argument.value.walk_nodes_mut(visit);
                }
            }
            ExprKind::Error { message, code } => {
                message.walk_nodes_mut(visit);
                if let Some(code) = code {
                    code.walk_nodes_mut(visit);
                }
            }
            ExprKind::Tuple(entries) | ExprKind::Sequence(entries) => {
                for entry in entries {
                    entry.walk_nodes_mut(visit);
                }
            }
            ExprKind::Pairs(entries) => {
                for (key, value) in entries {
                    key.walk_nodes_mut(visit);
                    value.walk_nodes_mut(visit);
                }
            }
            ExprKind::Eval { call, config, .. } => {
                call.walk_nodes_mut(visit);
                for entry in config {
                    entry.value.walk_nodes_mut(visit);
                }
            }
            ExprKind::Trace(cells) => {
                for cell in cells {
                    for part in [&mut cell.time, &mut cell.value].into_iter().flatten() {
                        part.walk_nodes_mut(visit);
                    }
                }
            }
        }
    }
}

/// A statement or an expression, as a mutable walk visits it.
pub enum NodeMut<'a> {
    /// A statement.
    Statement(&'a mut Stmt),
    /// An expression.
    Expression(&'a mut Expr),
}

/// [`walk_statements`], for a `visit` that may change each expression.
pub(crate) fn walk_statements_mut(statements: &mut [Stmt], visit: &mut dyn FnMut(&mut Expr)) {
    walk_statement_nodes_mut(statements, &mut |node| {
        if let NodeMut::Expression(expression) = node {
            visit(expression);
        }
    });
}

/// [`walk_statement_nodes`], for a `visit` that may change each statement
/// and expression. A replacement is walked as what it was replaced with.
pub fn walk_statement_nodes_mut(statements: &mut [Stmt], visit: &mut dyn FnMut(NodeMut<'_>)) {
    for statement in statements {
        visit(NodeMut::Statement(statement));
        match statement {
            Stmt::Inject { config, .. } => {
                for entry in config {
                    entry.value.walk_nodes_mut(visit);
                }
            }
            Stmt::Unread { .. } => {}
            Stmt::Declare { value, .. } | Stmt::Return { value, .. } => {
                if let Some(value) = value {
                    value.walk_nodes_mut(visit);
                }
            }
            Stmt::Assign { value, .. } | Stmt::Expr(value) => value.walk_nodes_mut(visit),
            Stmt::Assert { condition, .. } => condition.walk_nodes_mut(visit),
            Stmt::AssignPlace { place, value, .. } => {
                place.walk_nodes_mut(visit);
                value.walk_nodes_mut(visit);
            }
            Stmt::Hook {
                condition, body, ..
            }
            | Stmt::While {
                condition, body, ..
            } => {
                if let Some(condition) = condition {
                    condition.walk_nodes_mut(visit);
                }
                walk_statement_nodes_mut(body, visit);
            }
            Stmt::AssertRaises { body, .. } => walk_statement_nodes_mut(body, visit),
            Stmt::Yield { time, value, .. } => {
                time.walk_nodes_mut(visit);
                value.walk_nodes_mut(visit);
            }
            Stmt::For { sequence, body, .. } => {
                sequence.walk_nodes_mut(visit);
                walk_statement_nodes_mut(body, visit);
            }
            Stmt::Switch {
                selector,
                cases,
                default,
                ..
            } => {
                selector.walk_nodes_mut(visit);
                for case in cases {
                    case.label.walk_nodes_mut(visit);
                    walk_statement_nodes_mut(&mut case.body, visit);
                }
                if let Some(default) = default {
                    walk_statement_nodes_mut(default, visit);
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
            Stmt::Inject { config, .. } => {
                for entry in config {
                    entry.value.walk_nodes(visit);
                }
            }
            Stmt::Unread { .. } => {}
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

impl Expr {
    /// The top-level conjuncts of a condition, left to right: each side
    /// of an `&&` in turn, at any depth, and the condition itself where
    /// it is no `&&`.
    pub fn conjuncts(&self) -> Vec<&Expr> {
        let mut conjuncts = Vec::new();
        // A list of what is still to split stands in for recursion; the
        // right side is pushed first so that the left is split first.
        let mut to_do = vec![self];
        while let Some(part) = to_do.pop() {
            match &part.kind {
                ExprKind::Binary {
                    operator: BinaryOperator::And,
                    left,
                    right,
                } => {
                    to_do.push(right);
                    to_do.push(left);
                }
                _ => conjuncts.push(part),
            }
        }
        conjuncts
    }

    /// The root of a place: the expression its fields and indices are
    /// of, `book` of `book.orders[0]`. An expression that is no field
    /// and no index is its own root.
    pub fn place_root(&self) -> &Expr {
        let mut root = self;
        while let ExprKind::Field { base, .. } | ExprKind::Index { base, .. } = &root.kind {
            root = base;
        }
        root
    }
}

impl ExprKind {
    /// The values a block or an `if` may come to (CTRL-10): a block's
    /// value, which it may not have, and each branch of an `if`, its
    /// `then` first; `None` for any other expression. A branch is
    /// itself a block, or the `if` of an `else if`.
    pub fn branch_values(&self) -> Option<Vec<&Expr>> {
        match self {
            ExprKind::Block { value, .. } => Some(value.as_deref().into_iter().collect()),
            ExprKind::If {
                then, otherwise, ..
            } => Some(
                std::iter::once(&**then)
                    .chain(otherwise.as_deref())
                    .collect(),
            ),
            _ => None,
        }
    }

    /// The operator and operands of a symbol between two operands,
    /// however it is recorded: an [`ExprKind::Binary`], or the call the
    /// symbol stands for.
    pub fn binary(&self) -> Option<(BinaryOperator, &Expr, &Expr)> {
        match self {
            ExprKind::Binary {
                operator,
                left,
                right,
            }
            | ExprKind::DeferredBinary {
                operator,
                left,
                right,
                ..
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
