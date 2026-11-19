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

use crate::copies::{self, Copies};
use crate::instances::Instances;
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
        bound: Vec::new(),
        in_case: 0,
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
    /// A function of the program, or a candidate with a body, with
    /// what the call gives its generic parameters: one written by
    /// instance is called by its instance for those.
    Function(&'a [GenericArgument]),
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

/// A capability a node injects (TIME-8), as hgraph gives one to a node
/// and to its `start` and `stop`: the parameter the target writes for
/// it, and that parameter's annotation. `None` for a name that is no
/// capability's.
///
/// Core's CAP-5: "An alarm is a cheaper scheduler". hgraph has one
/// scheduler for a node and no cheaper one, so a node that injects
/// `alarm` is given it too; the provider's functions keep the alarm to
/// what an alarm does.
///
/// hgraph gives a capability for one call and no longer, so each of
/// the three functions names the ones the node injects. That is the
/// language's rule too: a capability "is used within the node that
/// injects it and is not retained".
fn capability(injected: &str) -> Option<(&'static str, &'static str)> {
    match injected {
        "clock" => Some(("_clock", "_hg.EvaluationClock")),
        "logger" => Some(("_logger", "_hg.LOGGER")),
        "global_state" => Some(("_store", "_hg.GlobalState")),
        "scheduler" | "alarm" => Some(("_scheduler", "_hg.SCHEDULER")),
        _ => None,
    }
}

/// What wakes a node beside its inputs (TIME-9): the scheduler or the
/// alarm it injects.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Timer {
    /// `inject scheduler`: `scheduled()` is its `is_scheduled_now`.
    Scheduler,
    /// `inject alarm`: `scheduled()` "holds where the node is evaluated
    /// and no temporal input has ticked".
    Alarm,
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

/// What a candidate gives, for the name of its provider's function: its
/// result, or the T of a `result<T, error>`. A candidate that can fail
/// is declared once for each value of its policy (NUM-9, OPS-8), and
/// the provider has one function for the two, which takes the policy.
fn given(signature: &Signature) -> Option<&Type> {
    match signature.result.as_ref()? {
        Type::Result { value, .. } => Some(value),
        other => Some(other),
    }
}

/// True when core's native candidates of `operator` are told apart by
/// what they give: two of them take the same types and give different
/// ones, as `convert(value: i64) -> f64` and `convert(value: i64) ->
/// bool` do (OPS-4). The provider's function of each such candidate is
/// named with what it gives.
fn told_apart_by_result(program: &Program, operator: OperatorRef) -> bool {
    let Some(core) = program.core() else {
        return false;
    };
    let module = &program.modules[core];
    // What each candidate read so far takes and gives.
    let mut read: Vec<(Vec<&Type>, Option<&Type>)> = Vec::new();
    for (position, declared) in module.declarations.iter().enumerate() {
        if declared.kind != DeclarationKind::NativeCandidate(Role::Value, Some(operator))
            && declared.kind != DeclarationKind::NativeCandidate(Role::Temporal, Some(operator))
        {
            continue;
        }
        let Some(Detail::Callable(signature)) = module.details.get(position) else {
            continue;
        };
        let takes: Vec<&Type> = signature
            .parameters
            .iter()
            .map(|parameter| &parameter.ty)
            .collect();
        let gives = given(signature);
        if read
            .iter()
            .any(|(other, result)| *other == takes && *result != gives)
        {
            return true;
        }
        read.push((takes, gives));
    }
    false
}

/// What `Emitter::branched` made of a statement of a graph.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Branched {
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

/// One `when` of a node: what its header asks, and its body.
struct Handler<'a> {
    /// The inputs whose tick wakes it (TIME-3), as local positions.
    subscribed: Vec<usize>,
    /// The inputs it requires to be valid.
    required: Vec<usize>,
    /// Predicates checked when the node runs, including explicit modification guards.
    conditions: Vec<&'a Expr>,
    /// True where its header has `scheduled()`, alone or as a top-level
    /// conjunct: a wake by the node's scheduler or alarm runs it
    /// (TIME-3).
    scheduled: bool,
    /// True where its header writes a `modified` selector. One that
    /// writes none "subscribes to every temporal input".
    selects: bool,
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
        let mut scheduled = false;
        // TIME-3: "A header that is `scheduled()` alone is the
        // exception: it requires no input to be valid".
        let alone = matches!(
            conjuncts.as_slice(),
            [Expr {
                kind: ExprKind::Endpoint {
                    operation: EndpointOperation::Scheduled,
                    ..
                },
                ..
            }]
        );
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
                } => {
                    add(&mut subscribed, named);
                    // TIME-3: scheduling and modification are distinct. An
                    // explicit guard still checks its publications. The empty
                    // selector listens for a scheduled wake instead.
                    if !named.as_ref().is_some_and(Vec::is_empty) {
                        conditions.push(conjunct);
                    }
                }
                ExprKind::Endpoint {
                    operation: EndpointOperation::Valid,
                    inputs: named,
                } => add(&mut required, named),
                // The node says what `scheduled()` is: it depends on
                // which of the scheduler and the alarm it injects.
                ExprKind::Endpoint {
                    operation: EndpointOperation::Scheduled,
                    ..
                } => scheduled = true,
                _ => conditions.push(conjunct),
            }
        }
        let selects = subscribed.is_some();
        Handler {
            subscribed: subscribed.unwrap_or_else(|| inputs.to_vec()),
            required: required.unwrap_or_else(|| if alone { Vec::new() } else { inputs.to_vec() }),
            conditions,
            scheduled,
            selects,
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
    /// How many `if`s and `switch`es over a connection have been
    /// written: each names its branches' graphs with its number.
    switched: usize,
    /// The locals of the graph being written that have a value so far:
    /// its parameters, a `let` or `var` declared with one, a `var` once
    /// assigned. A branch's graph is given only these (CTRL-14).
    bound: Vec<usize>,
    /// How many branch graphs of a `switch_` enclose what is being
    /// written: a `return` of an ordinary value there is a constant.
    in_case: usize,
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
                // Two candidates that differ only in what they require
                // would be one Python function.
                let python = self.definition_name(self.module, index);
                if self.candidates.insert(python.clone()) {
                    self.value_function(&python, signature, body);
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
            // SOURCE-1: "A temporal `fn` whose body has a `yield` is a
            // source".
            Some(Phase::Node) if yields(&body.statements) => {
                self.source(index, python, signature, body)
            }
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
        if !self.instances.instanced(program, module, declaration) {
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
        self.piece
            .uses
            .insert((module, declaration, Some(instance.suffix.clone())));
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
            copies: Copies::default(),
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
        self.piece.uses.insert((module, declaration, None));
        if module == self.module {
            return python;
        }
        self.imports.insert(declared.path.clone());
        format!("{}.{python}", alias(&declared.path))
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
        let function = Function {
            locals: &body.locals,
            signature: Some(signature),
            mode: Mode::Value,
            copies: Copies::of(self.program, body),
        };
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
            let body: String = self.text[start..]
                .lines()
                .map(|line| format!("    {line}\n"))
                .collect();
            self.text.truncate(start);
            self.line(1, "try:");
            self.text.push_str(&body);
            self.line(1, "except _core.Propagated as _left:");
            // What `?` passes on may hold what the function was given.
            // A result is the caller's to keep, so it is a copy, as
            // any result that is not the function's own is
            // (`given_out`).
            let failure = if signature
                .result
                .as_ref()
                .is_some_and(|result| holds_collection(self.program, result, 0))
            {
                self.natives = true;
                "_core.copied(_left.failure)"
            } else {
                "_left.failure"
            };
            let given = self.returned(failure);
            self.line(2, &format!("return {given}"));
        }
        self.given_back.clear();
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

    /// The `var` parameters of what `value` calls, by their positions,
    /// where it is a call of a function that declares any (FN-15).
    fn places_of(&self, value: &Expr) -> Vec<usize> {
        let Some((module, declaration, _)) = called(value) else {
            return Vec::new();
        };
        match self.program.modules[module].details.get(declaration) {
            Some(Detail::Callable(signature)) => signature
                .parameters
                .iter()
                .enumerate()
                .filter(|(_, parameter)| parameter.var)
                .map(|(index, _)| index)
                .collect(),
            _ => Vec::new(),
        }
    }

    /// A call that assigns places, which is the whole of a statement
    /// (FN-15): the call, and then each place given what its `var`
    /// parameter held when the function returned, "in the order of the
    /// parameters". Returns what the call's own result is read as, or
    /// `None` where `value` is no such call.
    ///
    /// A function with `var` parameters returns its result and then
    /// what each holds, so the call is bound to `_given`: `_given[0]`
    /// is the result, and `_given[1]` the first place's new value. A
    /// function that fails raises, and the target leaves each place as
    /// it was (see where the call is written).
    fn place_call(&mut self, function: &Function, value: &Expr, depth: usize) -> Option<String> {
        let places = self.places_of(value);
        if places.is_empty() {
            return None;
        }
        // A function of the program, or the candidate of an operator
        // the call resolved to: the candidate says which of its
        // parameters is `var` (FN-15, GEN-6).
        let (module, declaration, arguments) = called(value)?;
        let (module, declaration) = (&module, &declaration);
        let Some(Detail::Callable(signature)) =
            self.program.modules[*module].details.get(*declaration)
        else {
            return None;
        };
        if let Some(result) = self.native_at(
            function,
            (*module, *declaration),
            signature,
            arguments,
            depth,
        ) {
            return Some(result);
        }
        // FN-15: the place is the one the call's arguments name when
        // they are evaluated. An index or a key of it is evaluated
        // once, there, and held: an earlier place's assignment may
        // change what it would come to, `change(i, items[i])`.
        //
        // EXPR-2: "Evaluate ordinary call/constructor arguments once in
        // written order". Holding an index evaluates it before the
        // call, so where one is held every argument is: each in the
        // order written, an index just before the argument it is of.
        // What an index is of is read before the index is evaluated,
        // as it is in any read: in `rows[key][index]` a `rows` with no
        // such key fails before `index` is evaluated.

        let is_place = |argument: &Argument| {
            argument
                .parameter
                .is_some_and(|parameter| places.contains(&parameter))
        };
        let holds = arguments
            .iter()
            .any(|argument| is_place(argument) && !indices_of(&argument.value).is_empty());
        if holds {
            let (mut indices, mut reads) = (0, 0);
            for (position, argument) in arguments.iter().enumerate() {
                // What this argument's indices are of, by where each is
                // written.
                let mut read_here = Vec::new();
                if is_place(argument) {
                    for (of, index) in indices_of(&argument.value) {
                        if let Some(of) = of {
                            reads += 1;
                            let held = format!("_in_{reads}");
                            // Read only to be read through: nothing of
                            // it is kept.
                            let written = self.expression(function, of);
                            self.line(depth, &format!("{held} = {written}"));
                            self.held.push((of.at, held));
                            read_here.push(of.at);
                        }
                        indices += 1;
                        let held = format!("_at_{indices}");
                        let written = self.expression(function, index);
                        self.line(depth, &format!("{held} = {written}"));
                        self.held.push((index.at, held));
                    }
                }
                let held = format!("_arg_{}", position + 1);
                let written = if is_place(argument) {
                    self.given_place(function, &argument.value)
                } else {
                    self.expression(function, &argument.value)
                };
                self.line(depth, &format!("{held} = {written}"));
                // An index is held until the place is assigned. What it
                // is of is not: an earlier place's assignment may have
                // given the root another value, and the assignment
                // reads what is there then.
                self.held.retain(|(at, _)| !read_here.contains(at));
                self.held.push((argument.value.at, held));
            }
        }
        // FN-15: "A function that fails leaves the place holding what
        // its parameter held when it failed: no change it had made is
        // undone." A list or a dictionary is changed where it is, so
        // what the function did to one stays where it fails. A value
        // of any other type is given back when the function returns,
        // and is as it was where it does not.
        self.place_call = true;
        let called = self.expression(function, value);
        self.place_call = false;
        self.line(depth, &format!("_given = {called}"));
        for (position, parameter) in places.iter().enumerate() {
            let Some(argument) = arguments
                .iter()
                .find(|argument| argument.parameter == Some(*parameter))
            else {
                continue;
            };
            // What the function gives back is its own: it has copied
            // what it kept (see `kept`).
            let given = format!("_given[{}]", position + 1);
            if let Some((target, assigned)) = self.assigned(function, &argument.value, given) {
                self.hoist(depth);
                self.line(depth, &format!("{target} = {assigned}"));
            }
        }
        self.held.clear();
        Some("_given[0]".to_string())
    }

    /// A call of a native function of core that is given a place inside
    /// a value as its one `var` parameter, `append(book.orders, order)`
    /// (FN-15, LIST-1): the provider goes in to the place, and the
    /// function changes what is there where it is (`_core.given_at`).
    /// Returns what the call's result is read as, or `None` for any
    /// other call, which is given a value and assigns the place after.
    ///
    /// The place is the first argument written, so that the rest are
    /// evaluated after its way in has been read, as they are written
    /// (EXPR-2): they are given as they are asked for.
    fn native_at(
        &mut self,
        function: &Function,
        (module, declaration): (usize, usize),
        signature: &Signature,
        arguments: &[Argument],
        depth: usize,
    ) -> Option<String> {
        let program = self.program;
        let declared = &program.modules[module].declarations[declaration];
        let native = format!("fn_{}", declared.name);
        if declared.kind != DeclarationKind::NativeFunction(Role::Value)
            || Some(module) != program.core()
            || !provided().any(|provided| provided == native)
        {
            return None;
        }
        let mut places = signature
            .parameters
            .iter()
            .enumerate()
            .filter(|(_, parameter)| parameter.var);
        let (place, None) = (places.next()?, places.next()) else {
            return None;
        };
        let (first, rest) = arguments.split_first()?;
        // Every parameter is given: a native function of core's that
        // takes a place has no default.
        if first.parameter != Some(place.0) || arguments.len() != signature.parameters.len() {
            return None;
        }
        let root = self.way_in(function, &first.value, 1)?;
        self.natives = true;
        let target = self.place(function, root);
        let steps = self.steps(function, &first.value);
        let mut given = Vec::new();
        for argument in rest {
            let parameter = signature.parameters.get(argument.parameter?)?;
            let value = self.expression(function, &argument.value);
            // The function puts what it is given into the place: a
            // copy, where something else holds it and the place may
            // hold it.
            let value = if may_hold(self.program, &first.value.ty, &argument.value.ty) {
                self.copied(function, &argument.value, value)
            } else {
                value
            };
            given.push(format!("{}={value}", name(&parameter.name)));
        }
        self.hoist(depth);
        self.line(
            depth,
            &format!(
                "_given = _core.given_at({target}, ({steps}), _core.{native}, {}, lambda: _py.dict({}))",
                string(&name(&place.1.name)),
                given.join(", ")
            ),
        );
        self.line(depth, &format!("{target} = _given[1]"));
        Some("_given[0]".to_string())
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
        if temporal && let Type::List { element, size } = ty {
            return self.list_annotation(ty, element, size.as_ref(), at);
        }
        // TYPE-9: temporalization maps "dictionaries to keyed
        // children": hgraph's `TSD`, a child for each key, each child
        // temporalized in turn. Its key is a value, a scalar or an enum.
        if temporal && let Type::Dict { key, value } = ty {
            let key = self.annotation(key, false, at)?;
            if key == "_py.object" {
                self.unsupported(at, &format!("the type `{ty}`"));
                return None;
            }
            let child = self.child_annotation(ty, value, at)?;
            self.hgraph = true;
            return Some(format!("_hg.TSD[{key}, {child}]"));
        }
        // A list, a tuple or a dictionary that is a node's configuration
        // is given to hgraph inside an object it keeps as it is
        // (`held`): to hgraph it is an object.
        if !temporal && held(ty) {
            return Some("_py.object".to_string());
        }
        // TYPE-14: "`T?` denotes an ordinary value that is either a T
        // or null". hgraph has a value that holds one `T` or is unset
        // where the type under `TS[...]` is `T | None`, and a tick of
        // it may be the unset one: a published null.
        if let Type::Nullable(inner) = ty {
            // As a temporal value only a scalar or an enum may be
            // unset this way: it publishes whole. A set publishes its
            // changes, which hgraph's `TS` of a set that may be unset
            // would not gather, so a nullable set is not written.
            let whole = scalar(inner).is_some()
                || matches!(
                    &**inner,
                    Type::Named {
                        kind: NamedKind::Enum,
                        ..
                    }
                );
            if temporal && !whole {
                self.unsupported(at, &format!("the type `{ty}`"));
                return None;
            }
            let inner = self.annotation(inner, false, at)?;
            return Some(if temporal {
                self.hgraph = true;
                format!("_hg.TS[{inner} | None]")
            } else {
                format!("{inner} | None")
            });
        }
        // TYPE-9: "`atomic<V>` stops expansion at a complete ordinary
        // V": hgraph's `TS[V]`, whose tick is the whole value. Its V is
        // written as a value is; a held list, tuple or dictionary is
        // not written as a temporal payload yet.
        if let Type::Atomic(payload) = ty {
            let payload_written = self.annotation(payload, false, at)?;
            if !temporal {
                return Some(payload_written);
            }
            if payload_written == "_py.object" {
                self.unsupported(at, &format!("the type `{ty}`"));
                return None;
            }
            self.hgraph = true;
            return Some(format!("_hg.TS[{payload_written}]"));
        }
        if let Type::Named {
            module,
            declaration,
            arguments,
            kind: NamedKind::Struct,
            ..
        } = ty
        {
            return self.struct_annotation(ty, (*module, *declaration), arguments, temporal, at);
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
            // ERR-2 and ERR-4: a `result` and an `error` are no
            // composite, and publish whole (TYPE-9): the provider's
            // classes, `Ok` and `Err` both a `Result`.
            (None, Type::Error) => {
                self.natives = true;
                "_core.Error".to_string()
            }
            (None, Type::Result { .. }) => {
                self.natives = true;
                "_core.Result".to_string()
            }
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

    /// The temporal annotation of `child`, a field, a position or a
    /// keyed child of the temporal composite `ty` (TYPE-9). An unsized
    /// list ticks whole here (`list_annotation`), and its parent's
    /// update would give it a position's change, so as a child it is
    /// reported.
    fn child_annotation(&mut self, ty: &Type, child: &Type, at: Location) -> Option<String> {
        if growing_list(child) {
            self.unsupported(at, &format!("the type `{ty}`"));
            return None;
        }
        self.annotation(child, true, at)
    }

    /// The annotation of a temporal list `ty` of `element`, of `size`.
    ///
    /// TYPE-9: temporalization maps "lists to child positions". A list
    /// of a fixed size N (TYPE-10: "fixed sizes are nonnegative integer
    /// constants") is hgraph's `TSL` of N children, each temporalized
    /// in turn, whose tick is a `dict` of each position that changed:
    /// TYPE-16's `dict<i64, delta<V>>`.
    ///
    /// TYPE-10: "`list<T>` denotes a growing shape", and hgraph has no
    /// `TSL` that grows. Such a list is a `TS` of a whole tuple of its
    /// elements, each a value that publishes whole, so a tick is the
    /// complete list. Its delta, a position's own validity and a child
    /// of it are not written: they are reported where they are met.
    fn list_annotation(
        &mut self,
        ty: &Type,
        element: &Type,
        size: Option<&Constant>,
        at: Location,
    ) -> Option<String> {
        match size.map(|size| &size.value) {
            Some(ConstantValue::Int(count)) => {
                let child = self.child_annotation(ty, element, at)?;
                self.hgraph = true;
                Some(format!("_hg.TSL[{child}, _hg.Size[{count}]]"))
            }
            None if publishes_whole(element) => {
                let element = self.annotation(element, false, at)?;
                self.hgraph = true;
                Some(format!("_hg.TS[_py.tuple[{element}, ...]]"))
            }
            _ => {
                self.unsupported(at, &format!("the type `{ty}`"));
                None
            }
        }
    }

    /// The annotation of the struct `ty`, declared at `(module,
    /// declaration)` with `arguments`: its class as a value, and as a
    /// temporal value hgraph's bundle, `TSB`.
    ///
    /// TYPE-9: "Temporalization recursively maps tuples/structs to
    /// child fields". The bundle's schema has a field for each field
    /// the struct has, a parent's first, each temporalized in turn:
    /// `bid: f64` is a `TS[float]` of it. The provider makes the schema
    /// once for the struct's class, beside it (`_core.schema`), so that
    /// every annotation of the struct names one schema.
    ///
    /// A generic struct's application and an abstract struct, whose
    /// value may be of any struct below it, are not written yet.
    fn struct_annotation(
        &mut self,
        ty: &Type,
        (module, declaration): (usize, usize),
        arguments: &[GenericArgument],
        temporal: bool,
        at: Location,
    ) -> Option<String> {
        let program = self.program;
        if !arguments.is_empty() || inherited(program, module, declaration) {
            self.unsupported(at, &format!("the type `{ty}`"));
            return None;
        }
        let class = self.type_name(module, declaration);
        if !temporal {
            return Some(class);
        }
        let members = match program.modules[module].details.get(declaration) {
            Some(Detail::Struct(read)) => &read.members,
            _ => {
                self.unsupported(at, &format!("the type `{ty}`"));
                return None;
            }
        };
        let mut fields = Vec::new();
        for member in members {
            let field = self.child_annotation(ty, &member.ty, at)?;
            fields.push(format!("{}: {field}", string(&name(&member.name))));
        }
        self.hgraph = true;
        self.natives = true;
        Some(format!(
            "_hg.TSB[_core.schema({class}, {{{}}})]",
            fields.join(", ")
        ))
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
    /// A selector sets subscription or admission
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
            copies: Copies::of(self.program, body),
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
        // TIME-8: the capabilities the node injects, each as hgraph
        // gives it.
        let mut capabilities: Vec<(&str, &str)> = Vec::new();
        // TIME-9: the scheduler or the alarm, where it injects one.
        let mut timer: Option<Timer> = None;
        for statement in &body.statements {
            match statement {
                // TIME-6: "Cache is reconstructible node-local value
                // data, initialized on every start". hgraph's `STATE`
                // is the node's own and is recorded nowhere, so a
                // `state` and a `cache` are both fields of it, given
                // their values in `start`, in the order written.
                Stmt::Declare { local, value }
                    if matches!(
                        body.locals[*local].kind,
                        LocalKind::State | LocalKind::Cache
                    ) =>
                {
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
                    self.unsupported(body.locals[*local].at, "this declaration of a node");
                }
                Stmt::Inject { local, at } => {
                    let injected = &body.locals[*local].name;
                    if injected == "out" {
                        has_out = true;
                    } else if let Some(written) = capability(injected) {
                        let injects = match injected.as_str() {
                            "scheduler" => Some(Timer::Scheduler),
                            "alarm" => Some(Timer::Alarm),
                            _ => None,
                        };
                        if injects.is_some() && timer.is_some() {
                            // Core's CAP-5: "A node injects it in place
                            // of `scheduler`".
                            self.unsupported(*at, "a node that injects a scheduler and an alarm");
                        } else {
                            timer = timer.or(injects);
                            capabilities.push(written);
                        }
                    } else {
                        self.unsupported(*at, &format!("an `inject` of `{injected}`"));
                    }
                }
                other => self.unsupported(at_of(other, &body.locals), "this statement of a node"),
            }
        }
        // TIME-7: "`inject out` exposes that same result": hgraph's own
        // `_output`, which it gives the node and its `stop`.
        // An unsized list's delta is applied to what the node last
        // published (see `bundled`), which it reads from `_output`.
        let has_out = has_out || signature.result.as_ref().is_some_and(growing_list);
        let output = has_out.then(|| {
            // hgraph names the output's type after its input's:
            // `TS_OUT[int]` for `TS[int]`, `TSS_OUT[int]` for a set,
            // `TSB_OUT[...]` for a struct's bundle, `TSL_OUT` for a
            // list's and `TSD_OUT` for a dictionary's. Only the
            // outermost is renamed: a child stays as it is.
            let annotation = match &result {
                Some(result) => {
                    let kind = ["TSS", "TSB", "TSL", "TSD"]
                        .into_iter()
                        .find(|kind| result.starts_with(&format!("_hg.{kind}[")))
                        .unwrap_or("TS");
                    result.replacen(&format!("{kind}["), &format!("{kind}_OUT["), 1)
                }
                None => "_py.object".to_string(),
            };
            format!("_output: {annotation} = None")
        });
        if let Some(output) = &output {
            self.hgraph = true;
            parameters.push(output.clone());
        }
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
        // TIME-4: only validity requirements shared by all handlers may
        // prevent the whole node from evaluating.
        let valid: Vec<usize> = inputs
            .iter()
            .copied()
            .filter(|input| {
                handlers
                    .iter()
                    .all(|handler| handler.required.contains(input))
            })
            .collect();
        let has_state = !states.is_empty();
        if has_state {
            parameters.push("_state: _hg.STATE = None".to_string());
            lifecycle.push("_state: _hg.STATE = None".to_string());
        }
        // TIME-5: "`stop` reads a temporal input under manual
        // admission". hgraph gives a `stop` each input it names, as the
        // node has it: the configuration, then the inputs. Its injected
        // output and state are available at stop too.
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
        // TIME-7: "`out` is read in a `when` and in `stop`".
        if let Some(output) = output {
            stopping.push(output);
        }
        // A capability is given to the node, to its `start` and to its
        // `stop`, each for that one call.
        for (parameter, annotation) in &capabilities {
            let written = format!("{parameter}: {annotation} = None");
            parameters.push(written.clone());
            lifecycle.push(written.clone());
            stopping.push(written);
        }
        let stopping = stopping.join(", ");
        let lifecycle = lifecycle.join(", ");
        if !handlers.is_empty() && active.is_empty() && timer.is_none() {
            // TIME-3: with no scheduler and no alarm "nothing wakes it:
            // the node is dormant".
            self.unsupported(declaration.at, "a node that no input wakes");
        }
        // hgraph takes no node without an input. One with none, which
        // its scheduler or its alarm wakes, is given one it is never
        // woken by: the node is written under another name, and a graph
        // of the function's own name gives it a constant.
        let hidden = inputs.is_empty() && timer.is_some();
        let exposed = python;
        let node_name = if hidden {
            format!("_node_{python}")
        } else {
            python.to_string()
        };
        let python = node_name.as_str();
        // The function's own parameters, as the graph that stands for
        // a node with a hidden input takes them.
        let declared: Vec<String> = parameters[..signature.parameters.len()].to_vec();
        // TIME-9: what says that the scheduler or the alarm woke the
        // node. A scheduler's wake is due "whether or not a temporal
        // input has ticked". An alarm's is told from the inputs: it
        // "holds where the node is evaluated and no temporal input has
        // ticked". hgraph's scheduler is asked too, since it evaluates
        // a node at a wake that was since withdrawn.
        //
        // An input has ticked where it published or was invalidated:
        // hgraph's `modified` is of a publication alone, and its
        // `invalidated` of the cycle the input lost its value (END-4:
        // "The transition to invalid notifies active subscribers").
        let ticked: Vec<String> = inputs
            .iter()
            .map(|&local| {
                let input = name(&body.locals[local].name);
                format!("{input}.modified or {input}.invalidated")
            })
            .collect();
        let timer_woke = timer.map(|timer| match timer {
            Timer::Alarm if !ticked.is_empty() => format!(
                "_scheduler.is_scheduled_now and not ({})",
                ticked.join(" or ")
            ),
            _ => "_scheduler.is_scheduled_now".to_string(),
        });
        // That an input woke the node: one that is active published or
        // became invalid. Where the node passivates none, each input a
        // handler subscribes to is active.
        let mut passivates = false;
        hgl_analyser::hir::walk_statements(&body.statements, &mut |part| {
            passivates |= matches!(
                part.kind,
                ExprKind::Endpoint {
                    operation: EndpointOperation::Passivate,
                    ..
                }
            );
        });
        let input_woke = {
            let woke: Vec<String> = inputs
                .iter()
                .map(|&local| {
                    let input = name(&body.locals[local].name);
                    if passivates {
                        format!("(({input}.modified or {input}.invalidated) and {input}.active)")
                    } else {
                        format!("{input}.modified or {input}.invalidated")
                    }
                })
                .collect();
            match woke.as_slice() {
                // One term that is in parentheses of its own.
                [one] if passivates => one.clone(),
                _ => format!("({})", woke.join(" or ")),
            }
        };
        // What woke the node is read where the node is entered, and
        // not again where a later handler asks: a handler before it
        // may passivate the input that woke the node, or ask its
        // scheduler for another wake under the tag that is due, and
        // neither changes what woke this evaluation. A node with one
        // handler has none before it, and reads it in place.
        let timed = |handler: &Handler| handler.scheduled || handler.subscribed.is_empty();
        let several = handlers.len() > 1;
        let holds_timer = several && timer.is_some() && handlers.iter().any(timed);
        let holds_input = several
            && timer.is_some()
            && handlers
                .iter()
                .any(|handler| !timed(handler) && !handler.selects);
        let timer_guard = match &timer_woke {
            Some(_) if holds_timer => Some("_timer_woke".to_string()),
            other => other.clone(),
        };
        let input_guard = if holds_input {
            "_input_woke".to_string()
        } else {
            input_woke.clone()
        };
        let names = |locals: &[usize]| -> String {
            let named: Vec<String> = locals
                .iter()
                .map(|&local| name(&body.locals[local].name))
                .collect();
            tuple(&named)
        };
        let mut options = Vec::new();
        if hidden {
            // The hidden input wakes the node at no time, and need not
            // be valid.
            options.push("active=()".to_string());
            options.push("valid=()".to_string());
        } else {
            if active.len() != inputs.len() && !handlers.is_empty() {
                options.push(format!("active={}", names(&active)));
            }
            if valid.len() != inputs.len() && !handlers.is_empty() {
                options.push(format!("valid={}", names(&valid)));
            }
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
        let publishes = result.is_some();
        let returns = result.map_or(String::new(), |result| format!(" -> {result}"));
        if hidden {
            parameters.insert(0, "_begin: _hg.TS[_py.bool]".to_string());
        }
        self.line(
            0,
            &format!("def {python}({}){returns}:", parameters.join(", ")),
        );
        let before = self.text.len();
        self.unheld(signature);
        if let (true, Some(woke)) = (holds_timer, &timer_woke) {
            self.line(1, &format!("_timer_woke = {woke}"));
        }
        if holds_input {
            self.line(1, &format!("_input_woke = {input_woke}"));
        }
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
            let mut guards: Vec<String> = Vec::new();
            // TIME-3: "A wake by the scheduler or alarm wakes the
            // handlers that subscribe to no input and those whose
            // header has `scheduled()` ... Such a wake alone runs no
            // other handler: one with no `modified` selector
            // subscribes to every temporal input, and is run by an
            // input's wake."
            if timed(handler) {
                match &timer_guard {
                    Some(woke) => guards.push(woke.clone()),
                    None => {
                        if !active.is_empty() {
                            self.unsupported(declaration.at, "a handler that no input wakes");
                        }
                        continue;
                    }
                }
            } else if timer.is_some() && !handler.selects {
                guards.push(input_guard.clone());
            }
            for &local in &handler.required {
                if !valid.contains(&local) {
                    guards.push(format!("{}.valid", endpoint(local)));
                }
            }
            for condition in &handler.conditions {
                if let ExprKind::Endpoint {
                    operation: EndpointOperation::Modified,
                    inputs: selected,
                } = &condition.kind
                {
                    // One selector matches a publication on any named input.
                    // Separate selectors remain separate conjuncts. END-4:
                    // invalidation can wake the node but is no publication.
                    let mut publications = Vec::new();
                    for &local in selected.as_deref().unwrap_or(&inputs) {
                        let input = endpoint(local);
                        publications.push(if handler.required.contains(&local) {
                            format!("{input}.modified")
                        } else {
                            format!("({input}.modified and {input}.valid)")
                        });
                    }
                    guards.push(match publications.as_slice() {
                        [] => "False".to_string(),
                        [one] => one.clone(),
                        several => format!("({})", several.join(" or ")),
                    });
                } else {
                    guards.push(self.expression(&function, condition));
                }
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
            copies: Copies::of(self.program, body),
        };
        let stop_hook = Function {
            locals: &body.locals,
            signature: Some(signature),
            mode: Mode::Stop,
            copies: Copies::of(self.program, body),
        };
        // `start`: the state's first values, then the hook's own
        // statements.
        if has_state || start.is_some() {
            self.line(0, "");
            self.line(0, "");
            self.line(0, &format!("@{python}.start"));
            self.line(0, &format!("def _start_{python}({lifecycle}):"));
            let before = self.text.len();
            self.unheld(signature);
            for (local, value) in &states {
                let field = name(&body.locals[*local].name);
                let value = match value {
                    Some(value) => {
                        let written = self.expression(&lifecycle_hook, value);
                        self.kept(&lifecycle_hook, *local, value, written)
                    }
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
            self.unheld(signature);
            self.block(&stop_hook, stop, 1);
        }
        if hidden {
            // The graph a call names: the node, given the constant its
            // hidden input is never woken by.
            let mut given = vec!["_begin=_hg.const(True)".to_string()];
            given.extend(signature.parameters.iter().map(|parameter| {
                let python = name(&parameter.name);
                format!("{python}={python}")
            }));
            self.line(0, "");
            self.line(0, "");
            self.line(0, "@_hg.graph");
            self.line(
                0,
                &format!("def {exposed}({}){returns}:", declared.join(", ")),
            );
            let gives = if publishes { "return " } else { "" };
            self.line(1, &format!("{gives}{python}({})", given.join(", ")));
        }
    }

    /// A source (SOURCE-1): "a node that produces its output and has no
    /// temporal parameters". It is written as three functions and a
    /// graph that wires them, `python`:
    ///
    /// - `_yields_python`, a Python generator that holds the source's
    ///   statements, each `yield time: value` a `yield (time, value)`;
    /// - `_source_python`, a node that resumes the generator and
    ///   publishes what it is due. hgraph takes no node without an
    ///   input, so it has one, `_begin`, which the graph gives a
    ///   constant: that wakes it once, when the graph starts
    ///   (SOURCE-2: "The first evaluation begins the body"), and its
    ///   own scheduler wakes it after;
    /// - its `start`, which gives each `cache` and `state` its value,
    ///   makes the generator and runs the source's `start`, and its
    ///   `stop`, the source's `stop`: "`stop`, where present, always
    ///   runs when the node ends, whatever state the generator is in".
    ///
    /// hgraph's own generator node is not used: it has no `start` and
    /// no `stop`, and publishes a target that has passed where
    /// SOURCE-3 skips it. The provider's `resume` holds a target to
    /// SOURCE-2 and SOURCE-3.
    fn source(&mut self, index: usize, python: &str, signature: &Signature, body: &Body) {
        let program = self.program;
        let declaration = &program.modules[self.module].declarations[index];
        let (parameters, result) = self.temporal_signature(declaration, signature);
        // What the body is made of: the declarations, the hooks, and
        // the statements that yield.
        let mut states: Vec<(usize, &Option<Expr>)> = Vec::new();
        let mut start: Option<&[Stmt]> = None;
        let mut stop: Option<&[Stmt]> = None;
        let mut capabilities: Vec<(&str, &str)> = Vec::new();
        let mut block: Vec<Stmt> = Vec::new();
        for statement in &body.statements {
            match statement {
                Stmt::Declare { local, value }
                    if matches!(
                        body.locals[*local].kind,
                        LocalKind::State | LocalKind::Cache
                    ) =>
                {
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
                Stmt::Inject { local, at } => {
                    let injected = &body.locals[*local].name;
                    match capability(injected) {
                        // A source is resumed by hgraph's scheduler,
                        // which is the one a node is given: a source
                        // that injects one of its own is not written.
                        Some(_) if matches!(injected.as_str(), "scheduler" | "alarm") => {
                            self.unsupported(
                                *at,
                                &format!("an `inject` of `{injected}` in a source"),
                            );
                        }
                        Some(written) => capabilities.push(written),
                        None => self.unsupported(*at, &format!("an `inject` of `{injected}`")),
                    }
                }
                other => block.push(other.clone()),
            }
        }
        // The configuration by name, as the generator and the node are
        // given it.
        let names: Vec<String> = signature
            .parameters
            .iter()
            .map(|parameter| name(&parameter.name))
            .collect();
        let generator = format!("_yields_{python}");
        let node = format!("_source_{python}");
        self.hgraph = true;
        self.natives = true;

        // The statements that yield: a generator, given the
        // configuration and the node's state.
        let mut given = names.clone();
        given.push("_state".to_string());
        self.line(0, "");
        self.line(0, "");
        self.line(0, &format!("def {generator}({}):", given.join(", ")));
        let yielding = Function {
            locals: &body.locals,
            signature: Some(signature),
            mode: Mode::Source,
            copies: Copies::of(self.program, body),
        };
        self.block(&yielding, &block, 1);

        // The node. The clock and the scheduler are the target's own
        // here: the clock says what the current evaluation time is,
        // which a duration is counted from, and the scheduler wakes
        // the node at the next target.
        let mut hooks: Vec<String> = parameters.clone();
        hooks.push("_state: _hg.STATE = None".to_string());
        for (parameter, annotation) in &capabilities {
            hooks.push(format!("{parameter}: {annotation} = None"));
        }
        let mut node_parameters = vec!["_begin: _hg.TS[_py.bool]".to_string()];
        node_parameters.extend(hooks.iter().cloned());
        if !capabilities
            .iter()
            .any(|(parameter, _)| *parameter == "_clock")
        {
            node_parameters.push("_clock: _hg.EvaluationClock = None".to_string());
        }
        node_parameters.push("_scheduler: _hg.SCHEDULER = None".to_string());
        let returns = result
            .as_ref()
            .map_or(String::new(), |result| format!(" -> {result}"));
        self.line(0, "");
        self.line(0, "");
        self.line(0, "@_hg.compute_node");
        self.line(
            0,
            &format!("def {node}({}){returns}:", node_parameters.join(", ")),
        );
        // A capability is good for one call: the generator reads the
        // ones of the evaluation it is resumed in.
        for (parameter, _) in &capabilities {
            self.line(1, &format!("_state.{parameter} = {parameter}"));
        }
        self.line(
            1,
            "_published = _core.resume(_state._source, _clock.evaluation_time, _scheduler)",
        );
        self.line(1, "if _published is not None:");
        self.line(2, "return _published[0]");

        // `start`: the first values, the hook, and the generator.
        let hooks = hooks.join(", ");
        let lifecycle_hook = Function {
            locals: &body.locals,
            signature: Some(signature),
            mode: Mode::Lifecycle,
            copies: Copies::of(self.program, body),
        };
        self.line(0, "");
        self.line(0, "");
        self.line(0, &format!("@{node}.start"));
        self.line(0, &format!("def _start_{node}({hooks}):"));
        self.unheld(signature);
        for (local, value) in &states {
            let field = name(&body.locals[*local].name);
            let value = match value {
                Some(value) => {
                    let written = self.expression(&lifecycle_hook, value);
                    self.kept(&lifecycle_hook, *local, value, written)
                }
                None => "None".to_string(),
            };
            self.line(1, &format!("_state.{field} = {value}"));
        }
        // The generator is made before the source's `start` runs: a
        // `return` there ends the hook, and must not leave the node
        // with nothing to resume. Making it runs none of the source's
        // statements: Python begins a generator where it is first
        // resumed.
        self.line(
            1,
            &format!(
                "_state._source = _core.Source({generator}({}))",
                given.join(", ")
            ),
        );
        if let Some(start) = start {
            self.statements(&lifecycle_hook, start, 1);
        }
        if let Some(stop) = stop {
            let stop_hook = Function {
                locals: &body.locals,
                signature: Some(signature),
                mode: Mode::Stop,
                copies: Copies::of(self.program, body),
            };
            self.line(0, "");
            self.line(0, "");
            self.line(0, &format!("@{node}.stop"));
            self.line(0, &format!("def _stop_{node}({hooks}):"));
            self.unheld(signature);
            self.block(&stop_hook, stop, 1);
        }

        // The graph a call names: the node, woken once as it starts.
        // Configuration the node keeps is given to it inside an object
        // hgraph keeps as it is.
        let mut wired = vec!["_hg.const(True)".to_string()];
        wired.extend(signature.parameters.iter().map(|parameter| {
            let python = name(&parameter.name);
            if held(&parameter.ty) {
                format!("_core.Held({python})")
            } else {
                python
            }
        }));
        self.line(0, "");
        self.line(0, "");
        self.line(0, "@_hg.graph");
        self.line(
            0,
            &format!("def {python}({}){returns}:", parameters.join(", ")),
        );
        self.line(1, &format!("return {node}({})", wired.join(", ")));
    }

    /// A graph: `@graph`, whose body wires the nodes it calls.
    fn graph(&mut self, index: usize, python: &str, signature: &Signature, body: &Body) {
        let program = self.program;
        let declaration = &program.modules[self.module].declarations[index];
        let function = Function {
            locals: &body.locals,
            signature: Some(signature),
            mode: Mode::Graph,
            copies: Copies::of(self.program, body),
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
        self.bound = (0..body.locals.len())
            .filter(|&local| body.locals[local].kind == LocalKind::Parameter)
            .collect();
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
                        copies: Copies::of(self.program, body),
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
                let setting = Function {
                    locals: &body.locals,
                    signature: None,
                    mode: Mode::Lifecycle,
                    copies: Copies::of(self.program, body),
                };
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
                let function = Function {
                    locals: &[],
                    signature: None,
                    mode: Mode::Test,
                    copies: Copies::default(),
                };
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
            self.statement(function, statement, depth);
        }
        self.text[before..].lines().count()
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
    fn branched(
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
                    // A path that does not return goes on into the rest
                    // of the block: each branch's graph has it after
                    // its own statements.
                    let on = |statements: &[Stmt]| continued(statements, rest);
                    let Some(result) = function
                        .signature
                        .and_then(|signature| signature.result.clone())
                    else {
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
                    let call = self.switched(function, condition, &branches, &result, *at, depth);
                    self.line(depth, &format!("return {call}"));
                    return Branched::Ended;
                }
                let branches = [("True", then_statements), ("False", other_statements)];
                self.assigned_in_branches(function, condition, &branches, *at, depth);
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
                    // `var` is given leaves the graph, not the branch's
                    // graph: not written here.
                    if declared.is_some()
                        && branch_statements
                            .iter()
                            .flatten()
                            .any(|statements| may_return(statements))
                    {
                        self.unsupported(
                            *at,
                            "a `return` in an `if` over a connection that gives a `let` or a `var` its value",
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
                    let call = self.switched(function, condition, &branches, &whole.ty, *at, depth);
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
                    } else {
                        // A branch with no value: its statements, the
                        // absent value, and then what follows.
                        if ends && nested_return(statements) {
                            self.unsupported(
                                *at,
                                "a returning `if` or `switch` in a branch with no value of a `let`'s `if`",
                            );
                        }
                        self.statements(function, statements, depth + 1);
                        if !always_returns(statements) {
                            self.line(
                                depth + 1,
                                &(match &target {
                                    Some(target) => format!("{target} = {absent}"),
                                    None => format!("return {absent}"),
                                }),
                            );
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
                let mut branches: Vec<(String, &[Stmt])> = Vec::new();
                for case in cases {
                    let label = self.expression(function, &case.label);
                    branches.push((label, &case.body));
                }
                if let Some(default) = default {
                    // CTRL-6: "An unmatched selector uses default".
                    branches.push(("_hg.DEFAULT".to_string(), default));
                }
                if branches
                    .iter()
                    .any(|(_, statements)| may_return(statements))
                {
                    let Some(result) = function
                        .signature
                        .and_then(|signature| signature.result.clone())
                    else {
                        self.unsupported(*at, "a `return` in a `switch` of a graph with no result");
                        return Branched::Ended;
                    };
                    // A case that can reach its end goes on into the rest
                    // of the block, as an `if`'s branch does. With no
                    // `default`, an unmatched selector fails (CTRL-6).
                    let branches: Vec<Branch> = branches
                        .into_iter()
                        .map(|(key, statements)| Branch {
                            key,
                            statements: continued(statements, rest),
                            tail: Tail::Returns,
                        })
                        .collect();
                    let call = self.switched(function, selector, &branches, &result, *at, depth);
                    self.line(depth, &format!("return {call}"));
                    return Branched::Ended;
                }
                let branches: Vec<(&str, &[Stmt])> = branches
                    .iter()
                    .map(|(key, statements)| (key.as_str(), *statements))
                    .collect();
                self.assigned_in_branches(function, selector, &branches, *at, depth);
                Branched::Written
            }
            _ => Branched::No,
        }
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
        branches: &[(&str, &[Stmt])],
        at: Location,
        depth: usize,
    ) {
        // The `var`s declared outside the branches that one assigns.
        let mut assigned: Vec<usize> = Vec::new();
        let mut declared: Vec<usize> = Vec::new();
        for (_, statements) in branches {
            hgl_analyser::hir::walk_statement_nodes(statements, &mut |node| match node {
                hgl_analyser::hir::Node::Statement(Stmt::Declare { local, .. }) => {
                    declared.push(*local)
                }
                hgl_analyser::hir::Node::Statement(Stmt::Assign { local, .. })
                    if !assigned.contains(local) =>
                {
                    assigned.push(*local);
                }
                _ => {}
            });
        }
        assigned.retain(|local| !declared.contains(local));
        let [var] = assigned.as_slice() else {
            self.unsupported(
                at,
                "an `if` or a `switch` over a connection whose branches do not assign one `var`",
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
            .collect();
        let call = self.switched(function, key, &branches, &result, at, depth);
        let target = self.place(function, *var);
        self.line(depth, &format!("{target} = {call}"));
        self.bind(function, *var);
    }

    /// hgraph's `switch_` over the connection `key`, with a graph for
    /// each branch, written before the statement at `depth`. Gives the
    /// call.
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
        result: &Type,
        at: Location,
        depth: usize,
    ) -> String {
        use hgl_analyser::hir::{Node, walk_statement_nodes};
        self.hgraph = true;
        let number = self.switched;
        self.switched += 1;
        // What each branch's graph takes: every connection any branch
        // names, read or assigned, at any depth, that has a value where
        // the `switch_` is written (`Emitter::bound`), less the names a
        // branch declares itself. One a branch does not need is
        // harmless; one with no value yet is never given.
        let mut read: Vec<usize> = Vec::new();
        let mut declared: Vec<usize> = Vec::new();
        for branch in branches {
            let mut visit = |node: Node| match node {
                Node::Statement(Stmt::Declare { local, .. }) => declared.push(*local),
                Node::Statement(Stmt::For { names, .. }) => declared.extend(names),
                Node::Statement(Stmt::Assign { local, .. }) => read.push(*local),
                Node::Expression(Expr {
                    kind: ExprKind::Local(local),
                    ..
                }) => read.push(*local),
                _ => {}
            };
            walk_statement_nodes(&branch.statements, &mut visit);
            match &branch.tail {
                Tail::Value(value) => value.walk_nodes(&mut visit),
                // A branch that does not assign the `var` gives the
                // binding it had before.
                Tail::Local(var) => read.push(*var),
                Tail::Nothing | Tail::Returns => {}
            }
        }
        read.retain(|local| {
            !declared.contains(local)
                && self.bound.contains(local)
                && function.locals[*local].category == ValueCategory::Connection
        });
        read.sort_unstable();
        read.dedup();
        let annotation = self
            .annotation(result, true, at)
            .unwrap_or_else(|| "_py.object".to_string());
        let parameters: Vec<String> = read
            .iter()
            .map(|&local| {
                let declared = &function.locals[local];
                let of = self
                    .annotation(&declared.ty, true, at)
                    .unwrap_or_else(|| "_py.object".to_string());
                format!("{}: {of}", name(&declared.name))
            })
            .collect();
        let mut cases = Vec::new();
        let outside = std::mem::replace(&mut self.bound, read.clone());
        for (position, branch) in branches.iter().enumerate() {
            // A child graph has its parameters bound, and what its own
            // statements bind.
            self.bound = read.clone();
            let graph = format!("_case_{number}_{position}");
            self.line(depth, "@_hg.graph");
            self.line(
                depth,
                &format!("def {graph}({}) -> {annotation}:", parameters.join(", ")),
            );
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
                Tail::Nothing | Tail::Returns => {}
            }
            self.in_case += 1;
            self.statements(function, &all, depth + 1);
            self.in_case -= 1;
            if matches!(branch.tail, Tail::Nothing) && !always_returns(&all) {
                self.line(depth + 1, &format!("return _hg.nothing({annotation})"));
            }
            cases.push(format!("{}: {graph}", branch.key));
        }
        let key = self.expression(function, key);
        self.bound = outside;
        let mut given = vec![key, format!("{{{}}}", cases.join(", "))];
        given.extend(read.iter().map(|&local| {
            let python = name(&function.locals[local].name);
            format!("{python}={python}")
        }));
        format!("_hg.switch_({})", given.join(", "))
    }

    /// True where nothing else holds what `value` gives
    /// ([`copies::fresh`]), and in a graph, where a value is a
    /// connection and nothing is changed where it is.
    fn fresh(&self, function: &Function, value: &Expr) -> bool {
        function.mode == Mode::Graph || copies::fresh(self.program, value)
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
    /// `state` or a `cache` it is read from goes on changing.
    fn given_out(&mut self, function: &Function, value: &Expr, written: String) -> String {
        let locals = function.locals;
        let own = match function.mode {
            Mode::Graph | Mode::Test => true,
            Mode::Value => {
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
            && function.mode != Mode::Graph
            && holds_collection(self.program, &value.ty, 0)
        {
            self.natives = true;
            format!("_core.copied({written})")
        } else {
            written
        }
    }

    /// True where `base` is a list or a dictionary that a `var`, a
    /// `state` or a `cache` of this function holds itself ([`owns`]): an
    /// element or an entry of it is assigned where the collection is,
    /// and no entry of it is copied for the change. A `var` parameter
    /// is such a place: what it is given is the caller's own
    /// collection, which the function changes where it is (FN-15).
    fn changed_where_it_is(&self, function: &Function, base: &Expr) -> bool {
        let ExprKind::Local(local) = &base.kind else {
            return false;
        };
        let declared = &function.locals[*local];
        // In a graph an assignment into a `var` makes a new connection
        // (EXPR-15), and is written as one.
        function.mode != Mode::Graph
            && owns(self.program, function, *local)
            && matches!(declared.ty, Type::List { .. } | Type::Dict { .. })
    }

    /// What a `var` parameter is given for the place `value` (FN-15):
    /// what the place holds, as it is. The function changes a list or
    /// a dictionary where it is, and gives back what the place is to
    /// hold.
    fn given_place(&mut self, function: &Function, value: &Expr) -> String {
        if let Some((_, held)) = self.held.iter().find(|(at, _)| *at == value.at) {
            return held.clone();
        }
        self.expression(function, value)
    }

    /// What a local is written as where it is assigned or read as it
    /// is: a field of the node's state for a `state`, its name
    /// otherwise.
    fn place(&mut self, function: &Function, local: usize) -> String {
        let declared = &function.locals[local];
        match declared.kind {
            LocalKind::State | LocalKind::Cache => format!("_state.{}", name(&declared.name)),
            // TIME-7: "`inject out` exposes that same result for reads,
            // assignment".
            LocalKind::Inject if declared.name == "out" => {
                // "`start` neither reads nor writes it": the analyser
                // rejects that (`out.lifecycle`), and `start` is given
                // no output.
                if function.mode == Mode::Lifecycle {
                    self.unsupported(declared.at, "`out` in `start`");
                }
                "_output.value".to_string()
            }
            // TIME-8: a capability is the parameter hgraph gives it
            // as.
            LocalKind::Inject => match capability(&declared.name) {
                // A source's statements are resumed in one evaluation
                // after another, each with capabilities of its own:
                // the node puts them in its state for the statements
                // to read.
                Some((parameter, _)) if function.mode == Mode::Source => {
                    format!("_state.{parameter}")
                }
                Some((parameter, _)) => parameter.to_string(),
                None => {
                    self.unsupported(declared.at, "this injected capability");
                    "None".to_string()
                }
            },
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
        match statement {
            Stmt::Declare { local, value } => {
                if let Some(value) = value {
                    let written = match self.place_call(function, value, depth) {
                        Some(result) => result,
                        None => self.expression(function, value),
                    };
                    let value = self.kept(function, *local, value, written);
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
                    let value = self.copied(function, assigned, value);
                    let value = self.bundled(function, assigned, value);
                    if Self::publishes_null(function, assigned) {
                        self.published(value)
                    } else {
                        value
                    }
                } else {
                    self.kept(function, *local, assigned, value)
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
                // A field of a temporal struct's `out` is a port of its
                // own (TYPE-9), which is not assigned yet.
                let mut root = place;
                while let ExprKind::Field { base, .. } | ExprKind::Index { base, .. } = &root.kind {
                    root = base;
                }
                if let ExprKind::Local(local) = root.kind
                    && Self::is_out(function, local)
                    && matches!(
                        function.locals[local].ty,
                        Type::Named {
                            kind: NamedKind::Struct,
                            ..
                        }
                    )
                {
                    self.unsupported(place.at, "an assignment to a field of a temporal struct");
                }
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
                        let written = self.expression(function, index);
                        self.line(depth, &format!("{held} = {written}"));
                        self.held.push((index.at, held));
                        held_here.push(index.at);
                    }
                }
                // The place keeps what it is given: a copy, where
                // something else holds it.
                let written = self.expression(function, value);
                let value = self.copied(function, value, written);
                if let Some((target, value)) = self.assigned(function, place, value) {
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
                // FN-3: in a branch's graph "an ordinary value in a
                // branch is then wired as a constant".
                let value = if function.mode == Mode::Graph
                    && self.in_case > 0
                    && !self.connects(function, returned)
                {
                    // A struct's connection is a bundle (TYPE-9), and a
                    // constant of the class is no bundle.
                    if matches!(
                        returned.ty,
                        Type::Named {
                            kind: NamedKind::Struct,
                            ..
                        }
                    ) {
                        self.unsupported(
                            returned.at,
                            "an ordinary struct as a branch's connection",
                        );
                    }
                    format!("_hg.const({value})")
                } else {
                    value
                };
                // A node that publishes a nullable publishes its null.
                let value =
                    if function.mode == Mode::Node && Self::publishes_null(function, returned) {
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
                let value = self.copied(function, yielded, value);
                let value = self.bundled(function, yielded, value);
                let value = if Self::publishes_null(function, yielded) {
                    self.published(value)
                } else {
                    value
                };
                self.line(depth, &format!("yield ({time}, {value})"));
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
        // A loop runs over the value its sequence had where the loop
        // began. Its names are references to the entries, and the loop
        // takes a copy only where its own body changes the place the
        // sequence is read from ([`Copies`]).
        let over = if names
            .first()
            .is_some_and(|&first| function.copies.copied(first))
            && function.mode != Mode::Graph
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
    fn bind(&mut self, function: &Function, local: usize) {
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

    /// True when what the function being written publishes of `value`
    /// may be a null that is Python's `None`: its result's type is
    /// written `T?`, and `value` is a value of it. A value that is the
    /// nullable's delta is hgraph's own already, `Null` or the value
    /// (TYPE-16).
    fn publishes_null(function: &Function, value: &Expr) -> bool {
        function
            .signature
            .and_then(|signature| signature.result.as_ref())
            .is_some_and(publishes_whole_null)
            && !matches!(&value.ty, Type::Delta(of) if is_nullable(of))
    }

    /// True when `local` is the node's `inject out` (TIME-7).
    fn is_out(function: &Function, local: usize) -> bool {
        let declared = &function.locals[local];
        declared.kind == LocalKind::Inject && declared.name == "out"
    }

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
                    | ExprKind::Capture(_)
                    | ExprKind::Captured { .. }
            )
            && !self.connected_field(function, value)
            && !self.connected_element(function, value)
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
                let read = self.place(function, *local);
                // In a hook a temporal input is read through its
                // payload: in a `when`, and in `stop` (TIME-5).
                if matches!(function.mode, Mode::Node | Mode::Stop)
                    && Self::is_input(function, *local)
                {
                    self.port_value(&function.locals[*local].ty, &read)
                } else if matches!(function.mode, Mode::Node | Mode::Stop)
                    && Self::is_out(function, *local)
                {
                    // `out` read whole: a struct's value is made from
                    // its bundle's fields (TYPE-9), as an input's is.
                    self.port_value(&function.locals[*local].ty, "_output")
                } else if function.mode == Mode::Lifecycle && Self::is_input(function, *local) {
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
                // The node's own output is hgraph's `_output`.
                let input = if endpoint.kind == LocalKind::Inject {
                    "_output".to_string()
                } else {
                    name(&endpoint.name)
                };
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
                    // TYPE-16: a list's delta has `updated`, `removed` and
                    // `size_modified`. An unsized list here ticks whole
                    // (`list_annotation`), and does not say which
                    // positions changed.
                    EndpointOperation::DeltaValue if growing_list(&endpoint.ty) => {
                        self.unsupported(value.at, "the delta of an unsized list");
                        "None".to_string()
                    }
                    // END-3: what the input published this cycle. A
                    // fixed list's is hgraph's `dict` of each position
                    // that changed, its `updated` (see `list_delta`), and a dictionary's
                    // hgraph's `dict` of each key changed, with `REMOVE`
                    // for each removed (see `dict_delta`).
                    EndpointOperation::DeltaValue => format!("{input}.delta_value"),
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
                    let read = property.map(|property| format!("{native}_{}", property.name));
                    let core = Some(*module) == program.core();
                    return match (property, read) {
                        (Some(property), Some(read))
                            if core && provided().any(|provided| provided == read) =>
                        {
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
                // TYPE-16: a nullable's delta has one property, `value`,
                // "null for a published null". hgraph's delta of that
                // is `Null`.
                if matches!(&base.ty, Type::Delta(of) if is_nullable(of)) && *member == 0 {
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
                if let Type::Delta(of) = &base.ty
                    && let Type::List { size, .. } = &**of
                {
                    let read = match (*member, size) {
                        (0, _) => "updated",
                        (1, None) => "removed",
                        (2, None) => "size_modified",
                        (2, Some(_)) => return "None".to_string(),
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
                if matches!(&base.ty, Type::Delta(of) if matches!(**of, Type::Dict { .. })) {
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
                if let Type::Delta(of) = &base.ty
                    && let Some(field) = field_of(self.program, of, *member)
                {
                    self.natives = true;
                    let base = self.expression(function, base);
                    return format!("_core.changed({base}, {})", string(&name(&field.name)));
                }
                // TYPE-9: an `atomic<V>` of a struct holds the whole
                // struct, whose fields are read as the struct's.
                let whole = match &base.ty {
                    Type::Atomic(payload) => &**payload,
                    other => other,
                };
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
                    return self.port_value(&value.ty, &port);
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
                let base = self.expression(function, base);
                let index = self.expression(function, index);
                format!("{base}[{index}]")
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
    /// The call is a node or a graph of the program. A lifted value
    /// function, and a graph given a wiring-time value, which hgraph's
    /// `try_except` does not take, are not written yet.
    fn capture(&mut self, function: &Function, call: &Expr) -> String {
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
        if declared.kind.role() != Some(Role::Temporal) || !bodied {
            self.unsupported(at, "a capture of a lifted call");
            return "None".to_string();
        }
        if !self.is_node((module, declaration))
            && signature
                .parameters
                .iter()
                .any(|parameter| parameter.constant)
        {
            self.unsupported(at, "a capture of a graph given a wiring-time value");
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
        self.hgraph = true;
        let attempted = format!("_hg.try_except({given})");
        if signature.result.is_none() {
            format!("_hg.combine(exception={attempted})")
        } else {
            attempted
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
            copies: Copies::default(),
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
        // A place more than one step inside the value a `var`, a
        // `state` or a `cache` holds, with a collection on the way: the
        // provider goes in, and changes each collection where it is.
        // The value is given as it is asked for, after the way in has
        // been read, as it is evaluated in any assignment.
        if let Some(root) = self
            .way_in(function, place, 2)
            .filter(|_| goes_through_a_collection(place))
        {
            self.natives = true;
            let target = self.place(function, root);
            let steps = self.steps(function, place);
            return Some((
                target.clone(),
                format!("_core.set_at({target}, ({steps}), lambda: {value})"),
            ));
        }
        self.assigned_by_copy(function, place, value)
    }

    /// The local a place is rooted in, where the provider may go in to
    /// the place and change what is there where it is (`_core.set_at`,
    /// `_core.given_at`); `None` for any other place.
    ///
    /// The local is a `var`, a `state` or a `cache` of this function
    /// that holds a collection ([`owns`]), and no `var` parameter,
    /// whose value is the caller's until the call returns (FN-15). The
    /// way in has at least `least` steps, and a list or a dictionary on
    /// it or at its end: a way of struct fields alone to a scalar
    /// changes nothing where it is. The provider is given every index
    /// before it reads the first step, so an index that is an
    /// expression is taken only where nothing that may fail is read
    /// before it, the element of a list or the entry of a dictionary:
    /// `groups[i % 2]`, and not `rows[key][f()]`, whose `f()` is
    /// evaluated only once `rows[key]` has been read (EXPR-7).
    fn way_in(&self, function: &Function, place: &Expr, least: usize) -> Option<usize> {
        // A place that is itself a list or a dictionary is one a
        // function may change where it is.
        let mut collection = matches!(place.ty, Type::List { .. } | Type::Dict { .. });
        let mut steps = 0;
        let mut at = place;
        loop {
            match &at.kind {
                ExprKind::Local(local) => {
                    let root = function.mode != Mode::Graph && owns(self.program, function, *local);
                    return (root && collection && steps >= least).then_some(*local);
                }
                ExprKind::Field { base, member } => {
                    field_of(self.program, &base.ty, *member)?;
                    at = base;
                }
                ExprKind::Index { base, index } => {
                    if !self.simple_index(index) && goes_through_a_collection(base) {
                        return None;
                    }
                    element_read(&base.ty)?;
                    collection |= matches!(base.ty, Type::List { .. } | Type::Dict { .. });
                    at = base;
                }
                _ => return None,
            }
            steps += 1;
        }
    }

    /// True for an index that is a literal, a name or a member of an
    /// enum, or one a call has evaluated already and holds: reading it
    /// cannot fail.
    fn simple_index(&self, index: &Expr) -> bool {
        matches!(
            index.kind,
            ExprKind::Literal(_) | ExprKind::Local(_) | ExprKind::EnumMember { .. }
        ) || self.held.iter().any(|(held, _)| *held == index.at)
    }

    /// Writes the lines a place's way in asks for before its statement
    /// (`steps`).
    fn hoist(&mut self, depth: usize) {
        for line in std::mem::take(&mut self.hoisted) {
            self.line(depth, &line);
        }
    }

    /// The way in to `place` from the local it is rooted in, as the
    /// provider is given it: each step its kind and what it names, a
    /// struct's field `("f", "bids")`, a dictionary's key `("k", price)`
    /// and a list's index or a tuple's position `("i", 0)`.
    fn steps(&mut self, function: &Function, place: &Expr) -> String {
        let mut written = Vec::new();
        let mut at = place;
        while let ExprKind::Field { base, .. } | ExprKind::Index { base, .. } = &at.kind {
            let step = match &at.kind {
                ExprKind::Field { member, .. } => {
                    let field = field_of(self.program, &base.ty, *member)
                        .map_or_else(String::new, |field| name(&field.name));
                    format!("(\"f\", {})", string(&field))
                }
                ExprKind::Index { index, .. } => {
                    let kind = if matches!(base.ty, Type::Dict { .. }) {
                        "k"
                    } else {
                        "i"
                    };
                    let mut key = self.expression(function, index);
                    if !self.simple_index(index) {
                        // Evaluated once, before the statement.
                        let held = format!("_key_{}", self.hoisted.len() + 1);
                        self.hoisted.push(format!("{held} = {key}"));
                        key = held;
                    }
                    format!("(\"{kind}\", {key})")
                }
                _ => unreachable!("the loop is over fields and indices"),
            };
            written.push(step);
            at = base;
        }
        // From the root inward, with the comma a tuple of one needs.
        written.reverse();
        written
            .iter()
            .map(|step| format!("{step},"))
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// The copying form of an assignment to a place (`assigned`): each
    /// value on the way is given a new one, but for the collection a
    /// `var` of this function holds itself, which is changed where it
    /// is.
    fn assigned_by_copy(
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
                // The struct is read to make the new one of: nothing
                // of it is kept that it does not hold already.
                let held = self.expression(function, base);
                let value = format!("_core.with_field({held}, {}={value})", name(&field.name));
                self.assigned_by_copy(function, base, value)
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
                // The collection a `var`, a `state` or a `cache` holds
                // itself is changed where it is; one inside another
                // value is given a new one.
                let with = match with {
                    _ if !self.changed_where_it_is(function, base) => with,
                    "with_item" => "set_item",
                    _ => "set_entry",
                };
                let value = format!("_core.{with}({held}, {index}, {value})");
                self.assigned_by_copy(function, base, value)
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
        generics: &[GenericArgument],
        signature: &Signature,
        arguments: &[Argument],
    ) -> String {
        // A node keeps its configuration: a list, a tuple or a
        // dictionary is given to it inside an object hgraph keeps as
        // it is. What a parameter is of is read from the instance the
        // call names, where the node is written by instance: a
        // parameter declared a `T` is a list where `T` is one (GEN-2),
        // and the node written for it takes the list out.
        let node = self.is_node(callee);
        let kept: Vec<bool> = {
            let instances = self.instances;
            let instance = instances
                .instanced(self.program, callee.0, callee.1)
                .then(|| instances.find(self.program, callee, generics))
                .flatten();
            let taken = instance.map_or(signature, |instance| &instance.signature);
            signature
                .parameters
                .iter()
                .enumerate()
                .map(|(index, parameter)| {
                    let ty = taken
                        .parameters
                        .get(index)
                        .map_or(&parameter.ty, |parameter| &parameter.ty);
                    node && parameter.constant && held(ty)
                })
                .collect()
        };
        if kept.contains(&true) {
            self.natives = true;
        }
        let kept = |index: usize, value: String| {
            if kept[index] {
                format!("_core.Held({value})")
            } else {
                value
            }
        };
        // The owner's rule: an argument is a reference to what it
        // names, and is copied in two cases alone.
        //
        // "the value is passed to a function and the function also
        // recieves a var reference to the item or items it contains":
        // an argument read from a place that the same call gives to a
        // `var` parameter, itself or a part of it or what holds it.
        //
        // And one a native function keeps: a native function with a
        // `var` parameter puts what it is given into that place, as
        // `append` does its item, and what is kept is copied where
        // something else holds it. A function of the program copies
        // what it keeps itself, where it keeps it.
        let places: Vec<usize> = arguments
            .iter()
            .filter(|argument| {
                argument
                    .parameter
                    .and_then(|index| signature.parameters.get(index))
                    .is_some_and(|parameter| parameter.var)
            })
            .filter_map(|argument| copies::place_of(function.locals, &argument.value))
            .collect();
        let native = self.program.modules[callee.0]
            .bodies
            .get(callee.1)
            .is_none_or(Option::is_none);
        // The types of the places such a function is given: what it
        // keeps of another argument, it keeps in one of them.
        let holders: Vec<&Type> = arguments
            .iter()
            .filter(|argument| {
                argument
                    .parameter
                    .and_then(|index| signature.parameters.get(index))
                    .is_some_and(|parameter| native && parameter.var)
            })
            .map(|argument| &argument.value.ty)
            .collect();
        let mut given_places: Vec<usize> = Vec::new();
        let mut written: Vec<String> = arguments
            .iter()
            .filter_map(|argument| {
                let index = argument.parameter?;
                let parameter = signature.parameters.get(index)?;
                let collection = function.mode != Mode::Graph
                    && holds_collection(self.program, &argument.value.ty, 0);
                let value = if parameter.var {
                    // FN-15: the place itself. A second `var`
                    // parameter given the same place takes a copy:
                    // each has a value of its own, and the place is
                    // given them "in the order of the parameters".
                    let written = self.given_place(function, &argument.value);
                    let place = copies::place_of(function.locals, &argument.value);
                    let again = place.is_some_and(|place| given_places.contains(&place));
                    given_places.extend(place);
                    if again && collection {
                        self.natives = true;
                        format!("_core.copied({written})")
                    } else {
                        written
                    }
                } else {
                    let written = self.expression(function, &argument.value);
                    let read_from =
                        function
                            .copies
                            .places(self.program, function.locals, &argument.value);
                    let shared = read_from.iter().any(|place| places.contains(place));
                    if collection && shared {
                        self.natives = true;
                        format!("_core.copied({written})")
                    } else if collection
                        && holders
                            .iter()
                            .any(|holder| may_hold(self.program, holder, &argument.value.ty))
                    {
                        self.copied(function, &argument.value, written)
                    } else {
                        written
                    }
                };
                Some(format!("{}={}", name(&parameter.name), kept(index, value)))
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
                written.push(format!("{}={}", name(&parameter.name), kept(index, value)));
            }
        }
        written.join(", ")
    }

    /// True when `callee`, a module and a declaration of it, is written
    /// as an hgraph node that a call names: a temporal function with a
    /// runtime body that is no source. A source's name is the graph
    /// that wires its node.
    fn is_node(&self, (module, declaration): (usize, usize)) -> bool {
        let declared_in = &self.program.modules[module];
        declared_in.declarations[declaration].kind.role() == Some(Role::Temporal)
            && declared_in.phases.get(declaration).copied().flatten() == Some(Phase::Node)
            && declared_in
                .bodies
                .get(declaration)
                .and_then(Option::as_ref)
                .is_some_and(|body| !yields(&body.statements))
    }

    /// The lines that take a node's kept configuration out of the
    /// objects it was given in, at the head of the node, its `start`
    /// or its `stop`: `items = items.value`.
    fn unheld(&mut self, signature: &Signature) {
        for parameter in &signature.parameters {
            if parameter.constant && held(&parameter.ty) {
                let python = name(&parameter.name);
                self.line(1, &format!("{python} = {python}.value"));
            }
        }
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
                copies: Copies::default(),
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

    /// What a node publishes for `value`, written `written`: a whole
    /// struct published to a temporal struct is given as each of its
    /// fields, as hgraph's bundle takes it (TYPE-9), and anything else,
    /// a struct's delta among them, as it is. A struct with a field
    /// that is a temporal struct too is not written whole yet.
    fn bundled(&mut self, function: &Function, value: &Expr, written: String) -> String {
        let result = function
            .signature
            .and_then(|signature| signature.result.as_ref());
        if !matches!(function.mode, Mode::Node | Mode::Source) {
            return written;
        }
        // TYPE-16: a list's delta has `updated`, `removed` and
        // `size_modified`. An unsized list is a whole tuple here
        // (`list_annotation`): the provider applies the delta to the
        // list the node last published, its `_output` (see
        // `list_applied`), and publishes the whole list made.
        if let Some(result) = result
            && growing_list(result)
            && result != &value.ty
        {
            if matches!(&value.ty, Type::Delta(of) if growing_list(of)) {
                self.natives = true;
                return format!("_core.list_applied(_output, {written})");
            }
            self.unsupported(value.at, "a delta published to an unsized list");
            return written;
        }
        if result != Some(&value.ty) {
            return written;
        }
        self.bundle_of(&value.ty, value.at, written)
    }

    /// `written`, a whole value of `ty`, as a temporal value of `ty`
    /// publishes it: a struct's fields as hgraph's bundle takes them
    /// (TYPE-9), and any other value as it is.
    fn bundle_of(&mut self, ty: &Type, at: Location, written: String) -> String {
        match ty {
            // An unsized list's tick is the whole list, as a tuple that
            // is made of it: hgraph keeps no list the program goes on
            // changing.
            Type::List { size: None, .. } => return format!("_py.tuple({written})"),
            // A fixed list's `TSL` takes a tuple of its elements, each
            // one's update as that position's own (TYPE-9): an element
            // that publishes whole and is not null is set by it.
            Type::List { element, .. } => {
                if publishes_whole(element) && !is_nullable(element) {
                    return format!("_py.tuple({written})");
                }
                self.unsupported(
                    at,
                    "a whole list published where an element is temporal composite or nullable",
                );
                return written;
            }
            // A dictionary's `TSD` takes its keys' changes. A whole
            // dictionary also removes each key it has not, which hgraph
            // is not told here.
            Type::Dict { .. } => {
                self.unsupported(at, "a whole dictionary published to a temporal dictionary");
                return written;
            }
            _ => {}
        }
        let Type::Named {
            module,
            declaration,
            kind: NamedKind::Struct,
            ..
        } = ty
        else {
            return written;
        };
        let nested = match self.program.modules[*module].details.get(*declaration) {
            // A struct's bundle takes a field's update as that field's
            // own: a struct field's as its fields, a set field's, a
            // `TSS`, as members added, and a list's or a dictionary's
            // by position or key, so none is replaced whole.
            Some(Detail::Struct(read)) => read.members.iter().any(|member| {
                matches!(
                    member.ty,
                    Type::Named {
                        kind: NamedKind::Struct,
                        ..
                    } | Type::Set(_)
                        | Type::List { .. }
                        | Type::Dict { .. }
                )
            }),
            _ => true,
        };
        if nested {
            self.unsupported(
                at,
                "a whole struct published where a field is a temporal struct, set, list or dictionary",
            );
            return written;
        }
        self.natives = true;
        format!("_core.bundled({written})")
    }

    /// The port of a node's temporal input that `value` reads, where it
    /// is the input or a field of a temporal struct port, as hgraph
    /// names it: `quote`, `quote["bid"]`. `None` for anything else, an
    /// `atomic<V>`'s field among them, which is read from its value.
    fn input_port(&self, function: &Function, value: &Expr) -> Option<String> {
        match &value.kind {
            ExprKind::Local(local) if Self::is_input(function, *local) => {
                Some(name(&function.locals[*local].name))
            }
            // The node's own output, hgraph's `_output`.
            ExprKind::Local(local) if Self::is_out(function, *local) => Some("_output".to_string()),
            ExprKind::Field { base, member } => {
                let field = field_of(self.program, &base.ty, *member)?;
                let base = self.input_port(function, base)?;
                Some(format!("{base}[{}]", string(&name(&field.name))))
            }
            _ => None,
        }
    }

    /// What the port `port`, of a temporal value of `ty`, holds, read in
    /// a hook. A struct's bundle holds a value of the struct, made from
    /// each field's port in turn (TYPE-9); a list's and a dictionary's
    /// children hold its elements; any other port holds its `value`.
    fn port_value(&mut self, ty: &Type, port: &str) -> String {
        self.port_value_at(ty, port, 0)
    }

    /// `port_value` of a port read inside `depth` comprehensions, whose
    /// names `_child{depth}` and `_key{depth}` are its own.
    fn port_value_at(&mut self, ty: &Type, port: &str, depth: usize) -> String {
        match ty {
            // TYPE-9: "lists to child positions". An unsized list is a
            // whole tuple (`list_annotation`), and a list of the
            // program is a Python `list`, made of it.
            Type::List { size: None, .. } => return format!("_py.list({port}.value)"),
            // A fixed list's value is each child's, in order: hgraph's
            // own tuple of them where each child publishes whole.
            Type::List { element, .. } if publishes_whole(element) => {
                return format!("_py.list({port}.value)");
            }
            Type::List { element, .. } => {
                let child = format!("_child{depth}");
                let read = self.port_value_at(element, &child, depth + 1);
                return format!("[{read} for {child} in {port}]");
            }
            // TYPE-9: "dictionaries to keyed children". A dictionary's
            // value has each key the `TSD` has, with its child's value.
            Type::Dict { value, .. } if publishes_whole(value) => {
                return format!("_py.dict({port}.value)");
            }
            Type::Dict { value, .. } => {
                let (key, child) = (format!("_key{depth}"), format!("_child{depth}"));
                let read = self.port_value_at(value, &child, depth + 1);
                return format!("{{{key}: {read} for {key}, {child} in {port}.items()}}");
            }
            _ => {}
        }
        let fields = match ty {
            Type::Named {
                module,
                declaration,
                kind: NamedKind::Struct,
                ..
            } => match self.program.modules[*module].details.get(*declaration) {
                Some(Detail::Struct(read)) => Some((*module, *declaration, &read.members)),
                _ => None,
            },
            _ => None,
        };
        let Some((module, declaration, members)) = fields else {
            return format!("{port}.value");
        };
        let class = self.type_name(module, declaration);
        let fields: Vec<String> = members
            .iter()
            .map(|member| {
                let field = name(&member.name);
                let read =
                    self.port_value_at(&member.ty, &format!("{port}[{}]", string(&field)), depth);
                format!("{field}={read}")
            })
            .collect();
        format!("{class}({})", fields.join(", "))
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
        let whole = match &base.ty {
            Type::Atomic(payload) => &**payload,
            other => other,
        };
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
            Type::List { size: Some(_), .. } | Type::Dict { .. }
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
            // ERR-7: a capture is "read through two connections".
            ExprKind::Capture(_) | ExprKind::Captured { .. } => true,
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
                    // What the input holds, a struct's value made
                    // from its bundle's fields (TYPE-9).
                    let read = self.port_value(ty, &python);
                    // FN-12: "A fixed-size list is valid as any list is
                    // (TIME-9)": hgraph's `valid` of it.
                    match self.left_out(target, index, parameter) {
                        Some(fallback) => passed.push(format!(
                            "{python}=({read} if {python}.valid else {fallback})"
                        )),
                        None => {
                            passed.push(format!("{python}={read}"));
                            required.push(python);
                        }
                    }
                }
                Some(Bound::Configuration(ty)) => {
                    let annotation = self
                        .annotation(ty, false, at)
                        .unwrap_or_else(|| "_py.object".to_string());
                    parameters.push(format!("{python}: {annotation}"));
                    // Configuration the node keeps comes inside an
                    // object hgraph keeps as it is.
                    if held(ty) {
                        passed.push(format!("{python}={python}.value"));
                    } else {
                        passed.push(format!("{python}={python}"));
                    }
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
            Lifts::Function(generics) => {
                let declared = &self.program.modules[target.0].declarations[target.1];
                (self.called_name(target, generics, at), name(&declared.name))
            }
        };
        let returns = result.and_then(|result| self.annotation(result, true, at));
        // hgraph requires every input to be valid unless it is told
        // which ones.
        let mut options = Vec::new();
        if required.len() != inputs {
            options.push(format!("valid={}", tuple(&required)));
        }
        let options = if options.is_empty() {
            String::new()
        } else {
            format!("({})", options.join(", "))
        };
        let call = format!("{called}({})", passed.join(", "));
        let call = match lifts {
            Lifts::Native(..) => self.native_result(signature.result.as_ref(), call),
            Lifts::Function(_) => call,
        };
        let call = match result {
            Some(result) if publishes_whole_null(result) => self.published(call),
            Some(result) => self.bundle_of(result, at, call),
            _ => call,
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
            let taken = match input(self, &argument.value) {
                Some(ty) => Bound::Input(ty),
                None => Bound::Configuration(argument.value.ty.clone()),
            };
            let value = self.expression(function, &argument.value);
            // A lifted node keeps its configuration, as any node does.
            let value = match &taken {
                Bound::Configuration(ty) if held(ty) => {
                    self.natives = true;
                    format!("_core.Held({value})")
                }
                _ => value,
            };
            bound[index] = Some(taken);
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
        self.piece.uses.insert((module, declaration, None));
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
        // True for the call that is the whole of a statement, and not
        // for what its arguments call.
        let whole_statement = std::mem::take(&mut self.place_call);
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
            // GEN-16: in an instance this is the call of the candidate
            // that answers the requirement. It is left where nothing
            // answers, and in a value function with generic parameters,
            // which is written once and not by instance.
            Callee::Required { .. } => {
                self.unsupported(
                    at,
                    "an operation a generic declaration requires, with no candidate selected for it",
                );
                return "None".to_string();
            }
            Callee::Unread => {
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
        // FN-15: a call that assigns places is written with the
        // assignments after it, which only a statement has room for.
        if !whole_statement && signature.parameters.iter().any(|parameter| parameter.var) {
            self.unsupported(at, "a call that assigns a place, inside an expression");
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
            let mut native = format!("{}_{}", declared.name, types.join("_"));
            let mut shown = format!("{}({})", declared.name, types.join(", "));
            // OPS-4: "`convert<T>(value)` converts a value to the type
            // T": core's candidates of `convert` take the same type and
            // give different ones, so the provider's function is named
            // with what it gives too, `convert_i64_to_f64`.
            if let DeclarationKind::NativeCandidate(_, Some(operator)) = declared.kind
                && told_apart_by_result(program, operator)
                && let Some(given) = given(signature)
            {
                native = format!("{native}_to_{}", mangled(given));
                shown = format!("{shown} -> {given}");
            }
            let core = Some(module) == program.core();
            if !core || !provided().any(|provided| provided == native) {
                self.unsupported(at, &format!("the candidate `{shown}`"));
                return "None".to_string();
            }
            Some(native)
        } else {
            None
        };
        // NATIVE-7: a native function of core is the provider's too,
        // `_core.fn_log`. It binds its own name (NATIVE-3), which the
        // provider's own helpers are kept apart from by the `fn_`.
        if let DeclarationKind::NativeFunction(Role::Value) = declared.kind {
            let native = format!("fn_{}", declared.name);
            let core = Some(module) == program.core();
            let lifted = function.mode == Mode::Graph
                && arguments
                    .iter()
                    .any(|argument| self.connects(function, &argument.value));
            if !core || lifted || !provided().any(|provided| provided == native) {
                self.unsupported(at, &format!("the native function `{}`", declared.name));
                return "None".to_string();
            }
            self.natives = true;
            let arguments = self.arguments(
                function,
                (module, declaration),
                generics,
                signature,
                arguments,
            );
            let called = format!("_core.{native}({arguments})");
            return self.native_result(signature.result.as_ref(), called);
        }
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
                None => Lifts::Function(generics),
            };
            let node = self.lifted((module, declaration), lifts, signature, &bound, result, at);
            return format!("{node}({})", written.join(", "));
        }
        if let Some(native) = native {
            // NUM-2: an `i64` result outside the signed 64-bit range is
            // an error, which Python's own `+` would not make it: the
            // provider's function does.
            self.natives = true;
            let arguments = self.arguments(
                function,
                (module, declaration),
                generics,
                signature,
                arguments,
            );
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
        let arguments = self.arguments(
            function,
            (module, declaration),
            generics,
            signature,
            arguments,
        );
        format!("{python}({arguments})")
    }

    /// `eval(target, ...)`: the harness's `evaluate`, given the target,
    /// what its run is wired with ([`Emitter::run_wired`]), and each
    /// argument by its parameter's name.
    ///
    /// The harness does the rest, as core has an eval: each dense trace
    /// becomes a replay list (CEVAL-2), which a replay publishes
    /// (CEVAL-3); a record keeps each publication of the result
    /// (CEVAL-4) and puts them in the run's store (CRUN-7) under the
    /// run's key (CRUN-8); and the eval's value is that recording
    /// written dense, of a run of its own (CEVAL-5).
    ///
    /// RUN-1, core's CRUN-2: a run's configuration is given last, as
    /// `_config`, a name no parameter has, so that it is evaluated
    /// after the call's arguments, as EXPR-13 has them. The harness
    /// reads its `start`, `end` and `length`.
    ///
    /// TST-17: "An integration test ... beside `eval` it writes
    /// `run(call, config)` (RUN-3), whose value is the run's output as
    /// an eval's is." `real_time` is true for that form, and the
    /// harness is told so with `_real_time=True`.
    fn eval(
        &mut self,
        function: &Function,
        call: &Expr,
        ty: &Type,
        config: Option<&Expr>,
        real_time: bool,
    ) -> String {
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
            // GEN-2: a function written by instance is lifted as its
            // instance for the eval's generic arguments, whose result
            // names no generic parameter.
            let instances = self.instances;
            let signature = instances
                .find(self.program, (*module, *declaration), generics)
                .map_or(signature, |instance| &instance.signature);
            let (written, bound) = self.bound(function, signature, arguments, |_, value| {
                crate::instances::trace_payload(&value.ty)
            });
            let node = self.lifted(
                (*module, *declaration),
                Lifts::Function(generics),
                signature,
                &bound,
                signature.result.as_ref(),
                call.at,
            );
            let run = self.run_wired(call, ty);
            let mut given = vec![node, run];
            given.extend(written);
            if let Some(config) = config {
                given.push(format!("_config={}", self.expression(function, config)));
            }
            if real_time {
                given.push("_real_time=True".to_string());
            }
            return format!("_harness.evaluate({})", given.join(", "));
        }
        let run = self.run_wired(call, ty);
        let target = self.called_name((*module, *declaration), generics, call.at);
        let arguments = self.arguments(
            function,
            (*module, *declaration),
            generics,
            signature,
            arguments,
        );
        let mut given = vec![target, run];
        if !arguments.is_empty() {
            given.push(arguments);
        }
        if let Some(config) = config {
            given.push(format!("_config={}", self.expression(function, config)));
        }
        if real_time {
            given.push("_real_time=True".to_string());
        }
        format!("_harness.evaluate({})", given.join(", "))
    }

    /// What the harness wires the run of the eval that takes `call`
    /// with; `ty` is the eval's own type. Core's CEVAL-1: "core's
    /// `replay` wired to each temporal input of the target, and the
    /// target's result as the entry's", which the run records (CRUN-8).
    ///
    /// Core has `replay` and `record` as HGL, each for any type, so
    /// each is written for the type it is of here (GEN-2): `replay`
    /// for each input the eval gives a trace, by the input's name,
    /// `{"value": _inst_replay__i64}`, and then `record` for the
    /// target's result, or `None` where it has none.
    fn run_wired(&mut self, call: &Expr, ty: &Type) -> String {
        let at = call.at;
        let program = self.program;
        let Some(wiring) = crate::instances::run_wiring(program, call, ty) else {
            self.unsupported(at, "this eval");
            return "{}, None".to_string();
        };
        let instance =
            |emitter: &mut Self, function: &str, of: &Type| match crate::instances::core_function(
                program, function,
            ) {
                Some(found) => emitter.called_name(found, &[GenericArgument::Type(of.clone())], at),
                None => {
                    emitter.unsupported(at, &format!("an eval, with no `{function}` in core"));
                    "None".to_string()
                }
            };
        let inputs: Vec<String> = wiring
            .inputs
            .iter()
            .map(|(input, of)| {
                let replay = instance(self, "replay", of);
                format!("{}: {replay}", string(&name(input)))
            })
            .collect();
        let result = match &wiring.result {
            Some(of) => instance(self, "record", of),
            None => "None".to_string(),
        };
        format!("{{{}}}, {result}", inputs.join(", "))
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
        if matches!(&value.ty, Type::Delta(of) if is_nullable(of)) {
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
        if let Type::Delta(of) = &value.ty
            && let Type::Named {
                module,
                declaration,
                kind: NamedKind::Struct,
                ..
            } = &**of
        {
            let program = self.program;
            let members = match program.modules[*module].details.get(*declaration) {
                Some(Detail::Struct(read)) => &read.members[..],
                _ => &[],
            };
            let mut written = Vec::new();
            for argument in arguments {
                let given = self.expression(function, &argument.value);
                if let Some(field) = argument.parameter.and_then(|index| members.get(index)) {
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
        if matches!(&value.ty, Type::Delta(of) if matches!(**of, Type::Dict { .. })) {
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
        if let Type::Delta(of) = &value.ty
            && let Type::List { size, .. } = &**of
        {
            const PROPERTIES: [&str; 3] = ["updated", "removed", "size_modified"];
            let mut written = Vec::new();
            for argument in arguments {
                let given = self.expression(function, &argument.value);
                let Some(index) = argument.parameter.filter(|index| *index < PROPERTIES.len())
                else {
                    continue;
                };
                if size.is_some() && index > 0 {
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
        let element = match &value.ty {
            Type::Delta(of) => match &**of {
                Type::Set(element) => Some(&**element),
                _ => None,
            },
            _ => None,
        };
        let Some(element) = element else {
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

    /// What publishes `value`, of a nullable type: the value, or
    /// hgraph's `Null` where it is null. To hgraph a node that returns
    /// `None` publishes nothing, and one that returns `Null` publishes
    /// a value that is unset (TIME-7: "`return null` for an `i64?`
    /// result [publishes] a null").
    fn published(&mut self, value: String) -> String {
        self.natives = true;
        format!("_core.published({value})")
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
        Type::RollingWindow {
            value,
            maximum,
            minimum,
        } => match minimum {
            Some(minimum) => format!(
                "rolling_window<{}, {}, {}>",
                one(value)?,
                canonical_constant(maximum)?,
                canonical_constant(minimum)?
            ),
            None => format!(
                "rolling_window<{}, {}>",
                one(value)?,
                canonical_constant(maximum)?
            ),
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

/// True for a type of configuration a node is given inside an object
/// hgraph keeps as it is: a list, a tuple or a dictionary, nullable or
/// not. hgraph makes a value of its own of a node's configuration, and
/// has none for some of what one of these may hold, a set's delta say.
fn held(ty: &Type) -> bool {
    match ty {
        Type::Nullable(inner) => held(inner),
        Type::List { .. } | Type::Tuple(_) | Type::Dict { .. } => true,
        _ => false,
    }
}

/// What a call calls, and its arguments: a function of the program, or
/// the candidate of an operator the call resolved to. `None` for any
/// other expression, and for a call of what is not yet known.
fn called(value: &Expr) -> Option<(usize, usize, &[Argument])> {
    let ExprKind::Call {
        callee, arguments, ..
    } = &value.kind
    else {
        return None;
    };
    match callee {
        Callee::Function {
            module,
            declaration,
            ..
        }
        | Callee::Candidate {
            module,
            declaration,
            ..
        } => Some((*module, *declaration, arguments)),
        _ => None,
    }
}

/// Whether reading `place` reads an element of a list or an entry of
/// a dictionary, which may fail.
fn reads_an_element(mut place: &Expr) -> bool {
    while let ExprKind::Field { base, .. } | ExprKind::Index { base, .. } = &place.kind {
        if matches!(place.kind, ExprKind::Index { .. }) {
            return true;
        }
        place = base;
    }
    false
}

/// The indices of a place that are evaluated, from its root outward as
/// they are written. Each is with what it is an index of, where reading
/// that may fail.
fn indices_of(mut place: &Expr) -> Vec<(Option<&Expr>, &Expr)> {
    let mut indices = Vec::new();
    while let ExprKind::Field { base, .. } | ExprKind::Index { base, .. } = &place.kind {
        if let ExprKind::Index { index, .. } = &place.kind
            && !matches!(index.kind, ExprKind::Literal(_))
        {
            let of = reads_an_element(base).then_some(&**base);
            indices.push((of, &**index));
        }
        place = base;
    }
    indices.reverse();
    indices
}

/// True where the way to `place` from its root reads an element of a
/// list or an entry of a dictionary: `rows[key][0]`, `book.bids[price]`,
/// and not `line.end.x`.
fn goes_through_a_collection(place: &Expr) -> bool {
    let mut at = place;
    while let ExprKind::Field { base, .. } | ExprKind::Index { base, .. } = &at.kind {
        if matches!(at.kind, ExprKind::Index { .. })
            && matches!(base.ty, Type::List { .. } | Type::Dict { .. })
        {
            return true;
        }
        at = base;
    }
    false
}

/// True for a local that holds a list or a dictionary of its own: a
/// `var`, a `var` parameter, a `state` or a `cache` of such a type,
/// which is no connection.
///
/// LIST-3: "a value taken from the place before the call is as it
/// was". Python's list is one object that every name of it sees
/// change. So the collection such a local holds is changed where it
/// is, and is the local's alone: what is kept of it elsewhere, and a
/// name that outlives a change of it, is a copy (`Emitter::kept`).
fn owns(program: &Program, function: &Function, local: usize) -> bool {
    let declared = &function.locals[local];
    matches!(
        declared.kind,
        LocalKind::Var | LocalKind::State | LocalKind::Cache
    ) && holds_collection(program, &declared.ty, 0)
        && declared.category != ValueCategory::Connection
}

/// True for a type whose value may be, or may hold, a list or a
/// dictionary that a place changes where it is: one of those, and a
/// nullable, a tuple, a set, a result or a struct with one inside it.
/// A generic parameter, `any` and a struct that another inherits from
/// may hold anything. A scalar, an enum and a struct of those are
/// simple: they are copied, and nothing in them is changed.
pub(crate) fn holds_collection(program: &Program, ty: &Type, depth: usize) -> bool {
    // A struct may hold itself inside a list, which is a collection.
    if depth > 8 {
        return true;
    }
    let any = |types: &[&Type]| {
        types
            .iter()
            .any(|ty| holds_collection(program, ty, depth + 1))
    };
    match ty {
        Type::List { .. } | Type::Dict { .. } => true,
        Type::Scalar(scalar) => *scalar == Scalar::Any,
        Type::Named {
            kind: NamedKind::Enum | NamedKind::Native,
            ..
        }
        | Type::Error => false,
        Type::Named {
            module,
            declaration,
            kind: NamedKind::Struct,
            ..
        } => match program.modules[*module].details.get(*declaration) {
            Some(Detail::Struct(read)) => {
                inherited(program, *module, *declaration)
                    || read
                        .members
                        .iter()
                        .any(|member| holds_collection(program, &member.ty, depth + 1))
            }
            _ => true,
        },
        Type::Nullable(inner) | Type::Set(inner) => any(&[inner]),
        Type::Tuple(elements) => elements
            .iter()
            .any(|element| holds_collection(program, element, depth + 1)),
        Type::Result { value, error } => any(&[value, error]),
        _ => true,
    }
}

/// The types of what a value of `ty` holds directly: a list's element,
/// a dictionary's key and value, a tuple's and a struct's parts. `None`
/// for a type that does not say: a generic parameter, `any`, a native
/// type, a struct with generic parameters or one that another inherits
/// from.
fn held_types<'a>(program: &'a Program, ty: &'a Type) -> Option<Vec<&'a Type>> {
    match ty {
        Type::Scalar(scalar) => (*scalar != Scalar::Any).then(Vec::new),
        Type::Named {
            kind: NamedKind::Enum,
            ..
        }
        | Type::Error => Some(Vec::new()),
        Type::Named {
            module,
            declaration,
            kind: NamedKind::Struct,
            ..
        } => match program.modules[*module].details.get(*declaration) {
            Some(Detail::Struct(read))
                if read.generics.is_empty() && !inherited(program, *module, *declaration) =>
            {
                Some(read.members.iter().map(|member| &member.ty).collect())
            }
            _ => None,
        },
        Type::Nullable(inner) | Type::Set(inner) => Some(vec![inner]),
        Type::List { element, .. } => Some(vec![element]),
        Type::Dict { key, value } => Some(vec![key, value]),
        Type::Tuple(elements) => Some(elements.iter().collect()),
        Type::Result { value, error } => Some(vec![value, error]),
        _ => None,
    }
}

/// True where a value of type `holder` may come to hold a value of type
/// `given`, or a part of one, that is or holds a list or a dictionary:
/// a `list<list<i64>>` may hold a `list<i64>`, and a `list<i64>` holds
/// nothing of another `list<i64>` but its numbers. What a native
/// function puts into the place it is given is copied only where this
/// holds: `append(rows, row)` keeps `row`, and `extend(values, more)`
/// keeps the numbers `more` holds.
fn may_hold(program: &Program, holder: &Type, given: &Type) -> bool {
    // Every type inside `ty`, itself left out; `None` where a part of
    // it does not say what it holds. A struct may hold itself inside a
    // list: a type this deep is not followed further.
    fn inside<'a>(
        program: &'a Program,
        ty: &'a Type,
        depth: usize,
        into: &mut Vec<&'a Type>,
    ) -> Option<()> {
        if depth > 8 {
            return None;
        }
        for part in held_types(program, ty)? {
            if !into.contains(&part) {
                into.push(part);
                inside(program, part, depth + 1, into)?;
            }
        }
        Some(())
    }
    let mut holds = Vec::new();
    if inside(program, holder, 0, &mut holds).is_none() {
        return true;
    }
    let mut parts = vec![given];
    if inside(program, given, 0, &mut parts).is_none() {
        return true;
    }
    parts
        .iter()
        .any(|part| holds_collection(program, part, 0) && holds.contains(part))
}

/// True where another struct of the program inherits from the struct
/// `declaration` of `module`: a value of that type may be of the other
/// struct, with fields this one has not.
fn inherited(program: &Program, module: usize, declaration: usize) -> bool {
    program.modules.iter().any(|module_of| {
        module_of.details.iter().any(|detail| {
            matches!(detail, Detail::Struct(other) if other.parents.iter().any(|parent| {
                matches!(parent, Type::Named { module: parent_module, declaration: parent_declaration, .. }
                    if *parent_module == module && *parent_declaration == declaration)
            }))
        })
    })
}

/// True for a type written `T?` (TYPE-14).
fn is_nullable(ty: &Type) -> bool {
    matches!(ty, Type::Nullable(_))
}

/// True for a type hgraph holds as one value under `TS[...]`, so that a
/// tick of it is the whole value: a scalar, an enum, and a nullable one.
fn publishes_whole(ty: &Type) -> bool {
    match ty {
        Type::Nullable(inner) => publishes_whole(inner),
        Type::Named {
            kind: NamedKind::Enum,
            ..
        } => true,
        other => scalar(other).is_some(),
    }
}

/// True for an unsized list, `list<T>` (TYPE-10: "a growing shape"),
/// which this target writes as a `TS` of a whole tuple.
fn growing_list(ty: &Type) -> bool {
    matches!(ty, Type::List { size: None, .. })
}

/// True for a temporal type whose null is published whole, as hgraph's
/// `Null`: a `T?`, and an `atomic<T?>` (TYPE-9: "`atomic<V>` stops
/// expansion at a complete ordinary V").
fn publishes_whole_null(ty: &Type) -> bool {
    match ty {
        Type::Atomic(payload) => is_nullable(payload),
        other => is_nullable(other),
    }
}

/// True when `statements` hold a `yield`, at any depth: in a loop, a
/// `switch`, an `if` or a block. SOURCE-1: "A temporal `fn` whose body
/// has a `yield` is a source".
fn yields(statements: &[Stmt]) -> bool {
    fn within(value: &Expr) -> bool {
        match &value.kind {
            // A block ends with an expression where its last statement
            // is one: an `if` that yields, say.
            ExprKind::Block { statements, value } => {
                yields(statements) || value.as_deref().is_some_and(within)
            }
            ExprKind::If {
                then, otherwise, ..
            } => within(then) || otherwise.as_deref().is_some_and(within),
            _ => false,
        }
    }
    statements.iter().any(|statement| match statement {
        Stmt::Yield { .. } => true,
        Stmt::For { body, .. } | Stmt::While { body, .. } => yields(body),
        Stmt::Switch { cases, default, .. } => {
            cases.iter().any(|case| yields(&case.body)) || default.as_deref().is_some_and(yields)
        }
        Stmt::Expr(value) => within(value),
        _ => false,
    })
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
