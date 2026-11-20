//! Control flow over connections in a graph: an `if` or a `switch` whose
//! condition or selector is a connection (CTRL-2, CTRL-3, CTRL-5,
//! CTRL-6, CTRL-13 to CTRL-15), a `for` over a connection (CTRL-7,
//! CTRL-15), and a `var` that holds a connection.
//! Part of the transpiler, the last stage; a family of the dispatch point
//! (`forms`), which writes a graph's runs of statements so that it sees
//! what follows each.
//!
//! | HGL | C++ |
//! | --- | --- |
//! | `if open { price }`, `switch route { case "left": ... }` over a connection | a struct `switch_00_` in the graph's struct, a child graph `branch_00_` in it for each branch, and its static `wire_`, which compiles each with hgraph's `compile_subgraph` and wires them with hgraph's `compiled::switch_` on the condition or the selector: `switch_00_::wire_(wiring_, open, price)` |
//! | A connection a branch reads of the graph around it (CTRL-7: "Captured temporal connections are bound whole") | an input of every branch, in the order of the locals, given to `switch_` |
//! | An ordinary value a branch reads | an `hgraph::Scalar` of every branch, given to `compile_subgraph` |
//! | An `if` with no `else` whose value is used (CTRL-3, CTRL-13) | the branch of `false` gives `::hglrt::never<S>(wiring_)`, a connection that never ticks |
//! | An `if` or a `switch` that stands as a statement and assigns a `var` (CTRL-14) | each branch gives the `var` as it leaves it, and the `var` is what `switch_` gives |
//! | One that assigns nothing (CTRL-3: "An outputless if has no synthetic result") | branches with no result, `compiled::switch_sink_` |
//! | A `return` in a branch (CTRL-5, CTRL-14) | each branch is its statements and then the statements after the `if` or the `switch`, where it reaches them, and the graph returns what `switch_` gives |
//! | A `switch` with no `default` (CTRL-6: "An unmatched selector uses default or fails") | a default branch given the selector, which fails as it starts (`::hglrt::unmatched`) |
//! | `var x = c`, `x = d`, with `c`, `d` connections (CTRL-14) | `::hgraph::Port<S> x = c;`, `x = d;` |
//! | `for name, price in prices`, `for item in items` over a connection of `dict<K, V>` or `list<T>` (CTRL-7, CTRL-15) | a struct `each_00_` in the graph's struct, its child graph `child_`, given the key where the body reads it, the member and what it takes, and its `wire_`, which wires it with hgraph's `compiled::map_sink_`: a child for each key or position |
//! | `for item in items` over a connection of `list<T, N>` (CTRL-15) | the body written `N` times, each in a block where `item` is the list's child at that position |
//!
//! hgraph's `switch_` starts the branch a key selects, stops the one
//! before, and keeps it while the key is published again as it was: a
//! branch's state is fresh each time it is selected (CTRL-2). It binds the
//! started branch's inputs in that cycle, and an input bound to a valid
//! output is then modified: the branch's nodes run once with the values
//! held. What the result gives while a branch is selected is hgraph's:
//! a branch that returns a connection it takes publishes a reference to it.
//!
//! Which connections a branch takes, which paths return, and what follows
//! a branch are `hgl_lower::graphs`'s, which both targets read.
//!
//! An `if` over an ordinary value whose value is a connection is C++'s
//! `if`, where the graph is built (CTRL-1).
//!
//! Not written yet, and reported: a `for` over a connection of a set
//! (hgraph's `compiled::map_` makes children for the keys of a dictionary
//! or the positions of a list, and takes no set of keys alone), a loop's
//! key read other than as the argument of a temporal parameter, the
//! store read in a branch or a loop's body (hgraph builds a child graph
//! on a wiring of its own, which neither sees nor sets the root's store),
//! an ordinary value that may fail computed there, or in a graph called
//! there at any depth (hgraph compiles every
//! child as the graph around it is built, where FN-9 has its calls
//! evaluated as the child is built), a
//! `return` under a branch selected at runtime in a loop unrolled while
//! the graph is built, branches that assign more than one `var`, and an
//! `if` whose value is used and whose branches return or assign a `var`
//! declared before it.

use hgl_analyser::hir::{Case, Expr, ExprKind, Node, Stmt, ValueCategory, walk_statement_nodes};
use hgl_analyser::{LocalKind, Phase, Program, Type};
use hgl_diagnostics::Location;
use hgl_lower::graphs::{
    alternative_statements, always_returns, assigned_outside, continued, key_read_as_value,
    label_value, may_return, named_in, nested_return, taken,
};

use super::Emitter;
use super::calls::may_fail;
use super::forms::Family;
use super::graphs::WIRING;
use super::names::{self, identifier};
use super::nodes::{Context, Kind, Role};
use super::statements::Scope;
use super::values::INDENT;
use crate::runtime;

/// What the family keeps while it writes a graph.
#[derive(Default)]
pub(super) struct State {
    /// The locals of the graph being written that have a value where the
    /// statement being written stands: what a child graph written there
    /// may take of it (`hgl_lower::graphs::taken`).
    bound: Vec<usize>,
    /// How many loops unrolled while the graph is built enclose what is
    /// written.
    unrolling: usize,
}

/// The control family.
pub(super) struct Control;

/// What writing a statement of a run came to.
enum Branched {
    /// It is not this family's.
    No,
    /// It is written, and the run goes on.
    Written,
    /// It is written with the statements after it, which are not written
    /// again.
    Ended,
}

/// One branch of an `if` or a `switch` over a connection: a child graph.
struct Branch {
    /// What selects it.
    key: Key,
    /// Its statements.
    statements: Vec<Stmt>,
    /// What it gives after them.
    tail: Tail,
}

/// What selects a branch.
enum Key {
    /// A label: its C++, and where the `case` is written, for a `switch`.
    Label(String, Option<Location>),
    /// `default`: any value no label has.
    Default,
    /// A value no label has, in a `switch` with no `default`: the branch
    /// fails (CTRL-6).
    Unmatched,
}

/// What a branch gives after its statements.
enum Tail {
    /// The branch's value, the value of the `if` (CTRL-13).
    Value(Box<Expr>),
    /// A `var` declared before the `if` or the `switch`, as the branch
    /// leaves it (CTRL-14).
    Local(usize),
    /// Nothing that ticks: an `if` with no `else` (CTRL-3).
    Nothing,
    /// The branch returns from the graph on every path (CTRL-5).
    Returns,
    /// The branches give nothing (CTRL-3).
    Outputless,
}

impl Family for Control {
    // A graph's runs are this family's: a `return` in a branch selected
    // at runtime needs what follows it (CTRL-14), and a branch takes what
    // has a value where it is written (CTRL-7).
    fn statements(
        &self,
        emitter: &mut Emitter<'_>,
        scope: &mut Scope<'_>,
        statements: &[Stmt],
        depth: usize,
        text: &mut String,
    ) -> bool {
        if emitter.kind() != Some(Kind::Graph) {
            return false;
        }
        // The graph's own body: its parameters have values.
        if std::ptr::eq(statements, scope.body.statements.as_slice()) {
            emitter.control.bound = (0..scope.body.locals.len())
                .filter(|&local| scope.body.locals[local].kind == LocalKind::Parameter)
                .collect();
            emitter.control.unrolling = 0;
        }
        for (index, statement) in statements.iter().enumerate() {
            let rest = &statements[index + 1..];
            match emitter.branched(scope, statement, rest, depth, text) {
                Branched::Ended => break,
                Branched::Written => {}
                Branched::No => emitter.plain_statement(scope, statement, depth, text),
            }
            emitter.bind_statement(statement);
        }
        true
    }

    fn statement(
        &self,
        emitter: &mut Emitter<'_>,
        scope: &mut Scope<'_>,
        statement: &Stmt,
        depth: usize,
        text: &mut String,
    ) -> bool {
        if emitter.kind() != Some(Kind::Graph) {
            return false;
        }
        let indent = INDENT.repeat(depth);
        match statement {
            // CTRL-14: "A `var` keeps the category of its first value".
            Stmt::Declare {
                local,
                value: Some(value),
            } if scope.body.locals[*local].kind == LocalKind::Var
                && emitter.role(*local) == Role::Connection =>
            {
                let declared = &scope.body.locals[*local];
                let (ty, at, name) = (declared.ty.clone(), declared.at, identifier(&declared.name));
                let port = emitter.connection(scope, value, &ty);
                if let Some(schema) = emitter.temporal_schema(&ty, at) {
                    text.push_str(&format!(
                        "{indent}[[maybe_unused]] ::hgraph::Port<{schema}> {name} = {port};\n"
                    ));
                }
                true
            }
            // A `var` declared with no value is given one on each path
            // before it is read (CTRL-14: "With none before, every branch
            // that does not return assigns it").
            Stmt::Declare { local, value: None } if emitter.role(*local) == Role::Connection => {
                let declared = &scope.body.locals[*local];
                let (ty, at, name) = (declared.ty.clone(), declared.at, identifier(&declared.name));
                if let Some(schema) = emitter.temporal_schema(&ty, at) {
                    text.push_str(&format!(
                        "{indent}[[maybe_unused]] ::hgraph::Port<{schema}> {name}{{}};\n"
                    ));
                }
                true
            }
            Stmt::Assign {
                local, value, at, ..
            } if emitter.role(*local) == Role::Connection => {
                let declared = &scope.body.locals[*local];
                if declared.kind != LocalKind::Var {
                    emitter.unsupported(*at, "this assignment to a connection");
                    return true;
                }
                let (ty, name) = (declared.ty.clone(), identifier(&declared.name));
                let port = emitter.connection(scope, value, &ty);
                text.push_str(&format!("{indent}{name} = {port};\n"));
                true
            }
            _ => false,
        }
    }

    fn wiring(
        &self,
        emitter: &mut Emitter<'_>,
        scope: &mut Scope<'_>,
        value: &Expr,
    ) -> Option<String> {
        let ExprKind::If {
            condition,
            then,
            otherwise,
        } = &value.kind
        else {
            return None;
        };
        if !condition.is_connection() {
            return Some(emitter.composed_if(scope, value, condition, then, otherwise.as_deref()));
        }
        Some(emitter.valued_if(scope, value, condition, then, otherwise.as_deref()))
    }
}

/// The statements and the value of a branch of an `if` whose value is
/// used: a block's, or an `else if`, which is the value of its `else`.
fn parts(branch: &Expr) -> (&[Stmt], Option<&Expr>) {
    match &branch.kind {
        ExprKind::Block { statements, value } => (statements, value.as_deref()),
        _ => (&[], Some(branch)),
    }
}

/// True where `statements` read or assign `local`, at any depth.
fn reads(statements: &[Stmt], local: usize) -> bool {
    let mut found = false;
    walk_statement_nodes(statements, &mut |node| {
        let (mut read, mut declared) = (Vec::new(), Vec::new());
        named_in(node, &mut read, &mut declared);
        found |= read.contains(&local);
    });
    found
}

/// True where `value`, a branch's value, returns from the graph inside.
fn returning(value: Option<&Expr>) -> bool {
    value.is_some_and(|value| may_return(&[Stmt::Expr(value.clone())]))
}

impl Emitter<'_> {
    /// Writes `statement` of a graph that is no control flow of this
    /// family's. The names of a loop unrolled while the graph is built
    /// have values in its body.
    fn plain_statement(
        &mut self,
        scope: &mut Scope,
        statement: &Stmt,
        depth: usize,
        text: &mut String,
    ) {
        if let Stmt::For { names, .. } = statement {
            let before = self.control.bound.clone();
            self.control.bound.extend(names);
            self.control.unrolling += 1;
            self.statement(scope, statement, depth, text);
            self.control.unrolling -= 1;
            self.control.bound = before;
            return;
        }
        self.statement(scope, statement, depth, text);
    }

    /// Notes what `statement` gives a value: a local declared with one,
    /// assigned, or a capability injected (FN-9).
    fn bind_statement(&mut self, statement: &Stmt) {
        let bound = match statement {
            Stmt::Declare {
                local,
                value: Some(_),
            }
            | Stmt::Assign { local, .. }
            | Stmt::Inject { local, .. } => *local,
            _ => return,
        };
        if !self.control.bound.contains(&bound) {
            self.control.bound.push(bound);
        }
    }

    /// The result of what is being written: a `return` gives it.
    fn current_result(&self) -> Option<Type> {
        self.temporal
            .as_ref()
            .and_then(|context| context.result.clone())
    }

    /// Writes `statement` of a graph where it is an `if` or a `switch`
    /// of this family's, with `rest`, the statements after it, where a
    /// path through it goes on into them.
    fn branched(
        &mut self,
        scope: &mut Scope,
        statement: &Stmt,
        rest: &[Stmt],
        depth: usize,
        text: &mut String,
    ) -> Branched {
        match statement {
            // An `if` over a connection that stands as a statement.
            Stmt::Expr(Expr {
                kind:
                    ExprKind::If {
                        condition,
                        then,
                        otherwise,
                    },
                at,
                ..
            }) if condition.is_connection() => {
                // Each branch's statements; an `else if` is the one
                // statement of the `else`.
                let branches = vec![
                    (
                        Key::Label("true".to_string(), None),
                        alternative_statements(Some(then)),
                    ),
                    (
                        Key::Label("false".to_string(), None),
                        alternative_statements(otherwise.as_deref()),
                    ),
                ];
                if branches
                    .iter()
                    .any(|(_, statements)| may_return(statements))
                {
                    return self.returned(
                        scope,
                        (condition, "if"),
                        branches,
                        rest,
                        *at,
                        depth,
                        text,
                    );
                }
                self.assigned_in_branches(scope, (condition, "if"), branches, *at, depth, text);
                Branched::Written
            }
            // A `switch` over a connection (CTRL-15: "over a connection
            // each case is a child graph selected at runtime").
            Stmt::Switch {
                selector,
                cases,
                default,
                at,
            } if selector.is_connection() => {
                if self.labels_repeat(cases) {
                    return Branched::Ended;
                }
                let mut branches = Vec::new();
                for case in cases {
                    let label = self.expression(scope, &case.label);
                    branches.push((Key::Label(label, Some(case.label.at)), case.body.clone()));
                }
                match default {
                    Some(default) => branches.push((Key::Default, default.clone())),
                    // CTRL-6: "An unmatched selector uses default or
                    // fails".
                    None => branches.push((Key::Unmatched, Vec::new())),
                }
                if branches
                    .iter()
                    .any(|(_, statements)| may_return(statements))
                {
                    return self.returned(
                        scope,
                        (selector, "switch"),
                        branches,
                        rest,
                        *at,
                        depth,
                        text,
                    );
                }
                // UNIT-12: "A `switch` case has no scope of its own: its
                // statements bind in the block around the `switch`". A
                // case here is a graph of its own, so a binding of one that
                // is read after the `switch` is not written yet.
                let outlives = branches
                    .iter()
                    .flat_map(|(_, statements)| statements)
                    .filter_map(|bound| match bound {
                        Stmt::Declare { local, .. } => Some(*local),
                        _ => None,
                    })
                    .any(|local| reads(rest, local));
                if outlives {
                    self.unsupported(
                        *at,
                        "a binding of a case of a `switch` over a connection, read after it",
                    );
                    return Branched::Ended;
                }
                self.assigned_in_branches(scope, (selector, "switch"), branches, *at, depth, text);
                Branched::Written
            }
            // A `let` or a `var` given the value of an `if` over a
            // connection, a branch of which returns from the graph.
            Stmt::Declare {
                local,
                value:
                    Some(Expr {
                        kind:
                            ExprKind::If {
                                condition,
                                then,
                                otherwise,
                            },
                        at,
                        ..
                    }),
            } if condition.is_connection() && {
                let (statements, value) = parts(then);
                may_return(statements)
                    || returning(value)
                    || otherwise.as_deref().is_some_and(|other| {
                        let (statements, value) = parts(other);
                        may_return(statements) || returning(value)
                    })
            } =>
            {
                self.returned_value(
                    scope,
                    (condition, *local, *at),
                    [Some(parts(then)), otherwise.as_deref().map(parts)],
                    rest,
                    depth,
                    text,
                )
            }
            // An `if` over an ordinary value that stands as a statement,
            // with a returning `if` or `switch` in a branch: what follows
            // it is written in each branch, so that the one in it has its
            // other path (CTRL-5).
            Stmt::Expr(
                whole @ Expr {
                    kind:
                        ExprKind::If {
                            condition,
                            then,
                            otherwise,
                        },
                    ..
                },
            ) if !rest.is_empty()
                && (nested_return(&alternative_statements(Some(then)))
                    || nested_return(&alternative_statements(otherwise.as_deref()))) =>
            {
                let block = |statements: Vec<Stmt>| {
                    Box::new(Expr {
                        kind: ExprKind::Block {
                            statements,
                            value: None,
                        },
                        ty: whole.ty.clone(),
                        at: whole.at,
                        category: ValueCategory::Value,
                    })
                };
                let then = continued(&alternative_statements(Some(then)), rest);
                let other = continued(&alternative_statements(otherwise.as_deref()), rest);
                let again = Stmt::Expr(Expr {
                    kind: ExprKind::If {
                        condition: condition.clone(),
                        then: block(then),
                        otherwise: Some(block(other)),
                    },
                    ..whole.clone()
                });
                self.statement(scope, &again, depth, text);
                Branched::Ended
            }
            // The same of a `switch` over an ordinary value (CTRL-15: it
            // "composes the one case selected").
            Stmt::Switch {
                selector,
                cases,
                default,
                at,
            } if !rest.is_empty()
                && (cases.iter().any(|case| nested_return(&case.body))
                    || default.as_deref().is_some_and(nested_return)) =>
            {
                let again = Stmt::Switch {
                    selector: selector.clone(),
                    cases: cases
                        .iter()
                        .map(|case| Case {
                            label: case.label.clone(),
                            value: case.value.clone(),
                            body: continued(&case.body, rest),
                        })
                        .collect(),
                    // CTRL-6: with no `default`, a selector no case has
                    // fails, and nothing follows.
                    default: default
                        .as_deref()
                        .map(|statements| continued(statements, rest)),
                    at: *at,
                };
                self.statement(scope, &again, depth, text);
                Branched::Ended
            }
            Stmt::For {
                names,
                sequence,
                body,
                at,
            } if sequence.is_connection() => {
                self.each(scope, (names, sequence), body, *at, depth, text);
                Branched::Written
            }
            _ => Branched::No,
        }
    }

    /// A `for` of a graph over a connection (CTRL-15): "Over a connection
    /// of `list<T, N>` it composes its body `N` times, `item` a
    /// connection of `T` ... Over a connection of `list<T>`, `set<T>`, or
    /// `dict<K, V>` it makes one child for each member (CTRL-7): `item` is
    /// a connection of `T` for a list and an ordinary `T`, the member
    /// itself, for a set; `key` is an ordinary `K` and `value` a
    /// connection of `V` for a dictionary."
    ///
    /// The child is a graph of its own, which hgraph's `compiled::map_sink_`
    /// runs for each key of a dictionary or each position of a list: it
    /// keeps a child while its member stays and stops it where the member
    /// is removed, and one added again starts fresh (CTRL-7: "Update
    /// keeps the child; removal stops it; reinsertion creates fresh
    /// state"). The connections the body reads of the graph around it are
    /// given to each child whole ("Captured temporal connections are bound
    /// whole and add no membership"), and each ordinary value as its
    /// configuration. hgraph gives a child its key as a connection, so a
    /// key is read only as the whole argument of a temporal parameter
    /// (`hgl_lower::graphs::key_read_as_value`), where a connection is
    /// what is taken.
    fn each(
        &mut self,
        scope: &mut Scope,
        (names, sequence): (&[usize], &Expr),
        body: &[Stmt],
        at: Location,
        depth: usize,
        text: &mut String,
    ) {
        let program = self.program;
        let locals = &scope.body.locals;
        // The key hgraph gives each child, and the member's own connection.
        let (key, member, element) = match (&sequence.ty, names) {
            (Type::Dict { value, .. }, [key, member]) => (Some(*key), *member, &**value),
            (
                Type::List {
                    element,
                    size: None,
                },
                [member],
            ) => (None, *member, &**element),
            (
                Type::List {
                    element,
                    size: Some(size),
                },
                [item],
            ) => {
                // TYPE-4: a fixed list holds exactly its size.
                let hgl_analyser::ConstantValue::Int(count) = size.value else {
                    self.unsupported(at, "a `for` over a connection of a list of this size");
                    return;
                };
                let count = u64::try_from(count).unwrap_or(0);
                self.unrolled_each(
                    scope,
                    (*item, sequence, element, count),
                    body,
                    at,
                    depth,
                    text,
                );
                return;
            }
            // hgraph's `compiled::map_` makes its children for the keys of
            // a dictionary or the positions of a list it multiplexes, and
            // takes no set of keys alone.
            (Type::Set(_), _) => {
                self.unsupported(at, "a `for` over a connection of a set");
                return;
            }
            _ => {
                self.unsupported(at, "this `for`");
                return;
            }
        };
        if let Some(key) = key
            && let Some(read) = key_read_as_value(program, key, body)
        {
            self.unsupported(
                read,
                "a loop's key read other than as the argument of a temporal parameter",
            );
            return;
        }
        let mut read: Vec<usize> = Vec::new();
        let mut declared: Vec<usize> = names.to_vec();
        walk_statement_nodes(body, &mut |node| named_in(node, &mut read, &mut declared));
        let key = key.filter(|key| read.contains(key));
        let bound = self.control.bound.clone();
        let inputs = taken(locals, &bound, &[], read.clone(), &declared);
        let configured = self.configured(scope, &read, &declared, &bound);
        if self.reads_a_capability(scope, &configured, at) || self.computes_while_built(body, &[]) {
            return;
        }
        let Some(element) = self.temporal_schema(element, at) else {
            return;
        };
        let mut ports = Vec::new();
        for &local in &inputs {
            let Some(schema) = self.temporal_schema(&locals[local].ty, locals[local].at) else {
                return;
            };
            ports.push((identifier(&locals[local].name), schema));
        }
        let mut scalars = Vec::new();
        for &local in &configured {
            let Some(written) = self.written_type(&locals[local].ty, locals[local].at) else {
                return;
            };
            scalars.push((local, identifier(&locals[local].name), written));
        }
        // The child's parameters: the key, where the body reads it, the
        // member's connection, then what it takes.
        let mut parameters = vec![format!("[[maybe_unused]] ::hgraph::Wiring& {WIRING}")];
        if let Some(key) = key {
            let Some(schema) = self.temporal_schema(&locals[key].ty, locals[key].at) else {
                return;
            };
            parameters.push(format!(
                "[[maybe_unused]] ::hgraph::Port<{schema}> {}",
                identifier(&locals[key].name)
            ));
        }
        parameters.push(format!(
            "[[maybe_unused]] ::hgraph::Port<{element}> {}",
            identifier(&locals[member].name)
        ));
        for (port, schema) in &ports {
            parameters.push(format!("[[maybe_unused]] ::hgraph::Port<{schema}> {port}"));
        }
        for (local, scalar, written) in &scalars {
            parameters.push(format!(
                "[[maybe_unused]] ::hgraph::Scalar<{}, {written}> {scalar}",
                names::string(&locals[*local].name)
            ));
        }
        // The child's own context: the key and the member are connections
        // of it, and each value it is configured with its `hgraph::Scalar`.
        let Some(outer) = self.temporal.as_ref() else {
            return;
        };
        let mut context = Context::new(Kind::Graph, locals.len());
        context.roles = outer.roles.clone();
        context.roles[member] = Role::Connection;
        if let Some(key) = key {
            context.roles[key] = Role::Connection;
        }
        for &local in &configured {
            context.roles[local] = Role::Config;
        }
        let mut child_bound: Vec<usize> = inputs.iter().chain(&configured).copied().collect();
        child_bound.push(member);
        child_bound.extend(key);
        let mut statements = String::new();
        let mut child_scope = Scope::cold(program, scope.body);
        let before = std::mem::replace(&mut self.control.bound, child_bound);
        let unrolling = std::mem::take(&mut self.control.unrolling);
        self.in_context(&mut context, |emitter| {
            emitter.statements(&mut child_scope, body, 3, &mut statements);
        });
        self.control.bound = before;
        self.control.unrolling = unrolling;
        if let Some(outer) = self.temporal.as_mut() {
            outer.lifted.append(&mut context.lifted);
        }
        let number = self.temporary();
        let name = names::temporary("each", number);
        let line = self.line_of(at);
        let label = names::string(&format!(
            "{}:for on line {line}",
            program.modules[self.module].path
        ));
        // Its `wire_`, given the collection, the connections the child
        // takes and the values it is configured with.
        let mut wired = vec![
            format!("::hgraph::Wiring& {WIRING}"),
            "::hgraph::WiringPortRef collection_".to_string(),
        ];
        for (port, schema) in &ports {
            wired.push(format!("[[maybe_unused]] ::hgraph::Port<{schema}> {port}"));
        }
        for (local, scalar, written) in &scalars {
            let taken = super::values::parameter_type(program, &locals[*local].ty, false, written);
            wired.push(format!("[[maybe_unused]] {taken} {scalar}"));
        }
        let erased: Vec<String> = ports
            .iter()
            .map(|(port, _)| format!("{port}.erased()"))
            .collect();
        let mut given = vec![WIRING.to_string()];
        given.extend(scalars.iter().map(|(_, scalar, _)| scalar.clone()));
        let keyed = if key.is_some() {
            "::hgraph::MapKeyArg::First"
        } else {
            "::hgraph::MapKeyArg::None"
        };
        let deeper = INDENT.repeat(2);
        let struct_text = format!(
            "// The `for` on line {line} over a connection (CTRL-7, CTRL-15): a child graph for\n\
             // each member, which hgraph's `map_` runs while the member is there.\n\
             struct {name}\n{{\n\
             {INDENT}struct child_\n{INDENT}{{\n\
             {INDENT}{INDENT}static constexpr auto name = {label};\n\
             {INDENT}{INDENT}static void compose({})\n{INDENT}{INDENT}{{\n{statements}{INDENT}{INDENT}}}\n{INDENT}}};\n\n\
             {INDENT}static void wire_({})\n{INDENT}{{\n\
             {deeper}const std::array<::hgraph::WiringPortRef, {}> broadcast_{{{}}};\n\
             {deeper}::hgraph::compiled::map_sink_({WIRING}, collection_, broadcast_, ::hgraph::compile_subgraph<child_>({}), {keyed});\n\
             {INDENT}}}\n}};\n",
            parameters.join(", "),
            wired.join(", "),
            erased.len(),
            erased.join(", "),
            given.join(", "),
        );
        if let Some(context) = self.temporal.as_mut() {
            context.lifted.push(struct_text);
        }
        let collection = self.connection(scope, sequence, &sequence.ty);
        let mut arguments = vec![WIRING.to_string(), format!("{collection}.erased()")];
        arguments.extend(ports.iter().map(|(port, _)| port.clone()));
        for &local in &configured {
            let read = Expr {
                kind: ExprKind::Local(local),
                ty: locals[local].ty.clone(),
                at,
                category: ValueCategory::Value,
            };
            arguments.push(self.expression(scope, &read));
        }
        text.push_str(&format!(
            "{}{name}::wire_({});\n",
            INDENT.repeat(depth),
            arguments.join(", ")
        ));
    }

    /// A `for` over a connection of `list<T, N>`, `size` its `N` (CTRL-15:
    /// "it composes its body `N` times, `item` a connection of `T`"): the
    /// body written once for each position, where the graph is built, in
    /// a block of its own in which `item` is the connection of that
    /// position, the list's child as its temporal form gives it.
    #[allow(clippy::too_many_arguments)]
    fn unrolled_each(
        &mut self,
        scope: &mut Scope,
        (item, sequence, element, size): (usize, &Expr, &Type, u64),
        body: &[Stmt],
        at: Location,
        depth: usize,
        text: &mut String,
    ) {
        // The list is read once for each position: it is read where it is
        // a local, and a list made where it is looped over is not written.
        if !matches!(sequence.kind, ExprKind::Local(_)) {
            self.unsupported(
                at,
                "a `for` over a connection of a fixed list that is no local",
            );
            return;
        }
        let Some(schema) = self.temporal_schema(element, at) else {
            return;
        };
        let indent = INDENT.repeat(depth);
        let name = identifier(&scope.body.locals[item].name);
        let role = self.role(item);
        if let Some(context) = self.temporal.as_mut() {
            context.roles[item] = Role::Connection;
        }
        let before = self.control.bound.clone();
        self.control.unrolling += 1;
        for position in 0..size {
            let child = Expr {
                kind: ExprKind::Index {
                    base: Box::new(sequence.clone()),
                    index: Box::new(Expr {
                        kind: ExprKind::Literal(hgl_analyser::hir::Value::Int(
                            i64::try_from(position).unwrap_or(i64::MAX),
                        )),
                        ty: Type::Scalar(hgl_analyser::Scalar::I64),
                        at,
                        category: ValueCategory::Value,
                    }),
                },
                ty: element.clone(),
                at,
                category: ValueCategory::Connection,
            };
            let port = self.wiring_form(scope, &child);
            text.push_str(&format!(
                "{indent}{{\n{indent}{INDENT}[[maybe_unused]] const ::hgraph::Port<{schema}> {name} = {port};\n"
            ));
            self.control.bound.push(item);
            self.statements(scope, body, depth + 1, text);
            text.push_str(&format!("{indent}}}\n"));
            self.control.bound.clone_from(&before);
        }
        self.control.unrolling -= 1;
        if let Some(context) = self.temporal.as_mut() {
            context.roles[item] = role;
        }
    }

    /// Reports an ordinary value that may fail which a child graph
    /// computes, in `statements` or in `tails`, the values it gives, and
    /// says whether it did. FN-9: a child of a `switch` or of a `for` over
    /// a temporal collection "is built then, and its calls are evaluated
    /// then", as the branch or the member starts; hgraph compiles every
    /// child once, while the graph around it is built, and so would compute
    /// it then, where no branch may be selected, and fail before the run
    /// ([`computed_while_built`]).
    fn computes_while_built(&mut self, statements: &[Stmt], tails: &[&Expr]) -> bool {
        let mut composed = Vec::new();
        let Some(computed) = computed_while_built(self.program, statements, tails, &mut composed)
        else {
            return false;
        };
        self.unsupported(
            computed,
            "an ordinary value that may fail computed in a branch or a loop's body, which hgraph computes while the graph around it is built",
        );
        true
    }

    /// Reports a capability among `configured`, what a child graph at `at`
    /// reads of the graph it is written in, and says whether it did. A
    /// child is compiled on a wiring of its own, which neither sees nor
    /// may set the store of the run's root (FN-9: a child of a `switch` or
    /// of a `for` over a temporal collection "is built then, and its calls
    /// are evaluated then"), and no capability is a value hgraph keeps as
    /// configuration.
    fn reads_a_capability(&mut self, scope: &Scope, configured: &[usize], at: Location) -> bool {
        let program = self.program;
        let capability = configured
            .iter()
            .find_map(|&local| match &scope.body.locals[local].ty {
                Type::Named {
                    module,
                    declaration,
                    ..
                } => super::endpoints::capability_type(program, *module, *declaration).map(|_| {
                    program.modules[*module].declarations[*declaration]
                        .name
                        .clone()
                }),
                _ => None,
            });
        let Some(name) = capability else {
            return false;
        };
        if name == "GlobalState" {
            self.unsupported(
                at,
                "the store in a branch or a loop's body, a child graph that hgraph builds on a wiring of its own, which neither sees nor sets the root's store",
            );
        } else {
            self.unsupported(at, &format!("a `{name}` read in a branch or a loop's body"));
        }
        true
    }

    /// The ordinary values of the graph being written that a child graph
    /// reads, `read` less what it declares, `declared`, each with a value
    /// where the child is written, `bound`: its configuration.
    fn configured(
        &self,
        scope: &Scope,
        read: &[usize],
        declared: &[usize],
        bound: &[usize],
    ) -> Vec<usize> {
        let locals = &scope.body.locals;
        let mut configured: Vec<usize> = read
            .iter()
            .copied()
            .filter(|local| {
                !declared.contains(local)
                    && bound.contains(local)
                    && locals[*local].category == ValueCategory::Value
                    && self.role(*local) != Role::Connection
            })
            .collect();
        configured.sort_unstable();
        configured.dedup();
        configured
    }

    /// Reports a `switch` two of whose labels have one value, and says
    /// whether it did. CTRL-6: "Duplicate values reject". The analyser
    /// rejects those it can tell apart, and gives each case the value it
    /// resolved its label to; a label it could not resolve, a generic
    /// value parameter, has in an instance the value the call gives it
    /// (GEN-13), and two such may be equal.
    fn labels_repeat(&mut self, cases: &[Case]) -> bool {
        let mut seen = Vec::new();
        for case in cases {
            let Some(value) = label_value(case) else {
                continue;
            };
            if seen.contains(&value) {
                self.unsupported(
                    case.label.at,
                    "a `switch` two of whose labels have one value in an instance",
                );
                return true;
            }
            seen.push(value);
        }
        false
    }

    /// Reports a `return` under a branch selected at runtime inside a
    /// loop unrolled while the graph is built, and says whether it did.
    /// CTRL-14 has such a `return` make "the graph's result that value
    /// while the branch is selected and the continuation's otherwise",
    /// and the continuation of a pass is the passes after it and what
    /// follows the loop, which is not written as a branch.
    fn unrolled_return(&mut self, at: Location) -> bool {
        if self.control.unrolling > 0 {
            self.unsupported(
                at,
                "a `return` under a branch selected at runtime in a loop unrolled while the graph is built",
            );
        }
        self.control.unrolling > 0
    }

    /// An `if` or a `switch` over the connection `key` that stands as a
    /// statement and has a `return` in a branch. CTRL-14: "A `return
    /// value` in a branch of a temporal `if` makes the graph's result that
    /// value while the branch is selected and the continuation's
    /// otherwise": each branch is its statements and then `rest`, where it
    /// reaches them, and the graph returns what `switch_` gives.
    #[allow(clippy::too_many_arguments)]
    fn returned(
        &mut self,
        scope: &mut Scope,
        (key, what): (&Expr, &str),
        branches: Vec<(Key, Vec<Stmt>)>,
        rest: &[Stmt],
        at: Location,
        depth: usize,
        text: &mut String,
    ) -> Branched {
        if self.unrolled_return(at) {
            return Branched::Ended;
        }
        let Some(result) = self.current_result() else {
            self.unsupported(
                at,
                "a `return` in an `if` or a `switch` of a graph with no result",
            );
            return Branched::Ended;
        };
        let branches: Vec<Branch> = branches
            .into_iter()
            .map(|(key, statements)| Branch {
                statements: match key {
                    Key::Unmatched => Vec::new(),
                    _ => continued(&statements, rest),
                },
                key,
                tail: Tail::Returns,
            })
            .collect();
        if let Some(call) = self.switched(scope, (key, what), &branches, Some(&result), at) {
            text.push_str(&format!("{}return {call};\n", INDENT.repeat(depth)));
        }
        Branched::Ended
    }

    /// `let name = if condition { ... } else { ... }`, or a `var`, over a
    /// connection, where a branch returns from the graph in its statements
    /// or in its value. CTRL-14: "A `return value` in a branch of a
    /// temporal `if` makes the graph's result that value while the branch
    /// is selected and the continuation's otherwise": each branch's graph
    /// has its statements, the name given the branch's value, and then
    /// `rest`, and the graph returns what `switch_` gives.
    fn returned_value(
        &mut self,
        scope: &mut Scope,
        (condition, local, at): (&Expr, usize, Location),
        branches: [Option<(&[Stmt], Option<&Expr>)>; 2],
        rest: &[Stmt],
        depth: usize,
        text: &mut String,
    ) -> Branched {
        let mut written = Vec::new();
        for (key, branch) in ["true", "false"].into_iter().zip(branches) {
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
            written.push((Key::Label(key.to_string(), None), statements));
        }
        self.returned(scope, (condition, "if"), written, &[], at, depth, text)
    }

    /// An `if` or a `switch` over the connection `key` that stands as a
    /// statement and returns from no branch. CTRL-14: "After the `if` the
    /// `var` is one connection: the branch's binding while that branch is
    /// selected, and the binding it had before for a branch that does not
    /// assign it": each branch gives the `var` as it leaves it, and what
    /// `switch_` gives is assigned to it. CTRL-3: "An outputless if has no
    /// synthetic result; an omitted false branch performs no conditional
    /// work": branches that assign nothing give nothing.
    fn assigned_in_branches(
        &mut self,
        scope: &mut Scope,
        (key, what): (&Expr, &str),
        branches: Vec<(Key, Vec<Stmt>)>,
        at: Location,
        depth: usize,
        text: &mut String,
    ) {
        let indent = INDENT.repeat(depth);
        // The `var`s declared outside the branches that one assigns, by
        // name or by a place inside them.
        let mut assigned: Vec<usize> = Vec::new();
        for (_, statements) in &branches {
            for local in assigned_outside(statements) {
                if !assigned.contains(&local) {
                    assigned.push(local);
                }
            }
        }
        if assigned.is_empty() {
            let branches: Vec<Branch> = branches
                .into_iter()
                .map(|(key, statements)| Branch {
                    key,
                    statements,
                    tail: Tail::Outputless,
                })
                .collect();
            if let Some(call) = self.switched(scope, (key, what), &branches, None, at) {
                text.push_str(&format!("{indent}{call};\n"));
            }
            return;
        }
        let [var] = assigned.as_slice() else {
            self.unsupported(
                at,
                "an `if` or a `switch` over a connection whose branches assign more than one `var`",
            );
            return;
        };
        let declared = &scope.body.locals[*var];
        if self.role(*var) != Role::Connection {
            // CTRL-14: "one that holds an ordinary value would have its
            // value chosen at runtime, and the assignment rejects".
            self.unsupported(at, "an assignment in a branch selected at runtime to this");
            return;
        }
        let (result, name) = (declared.ty.clone(), identifier(&declared.name));
        let branches: Vec<Branch> = branches
            .into_iter()
            .map(|(key, statements)| Branch {
                key,
                statements,
                tail: Tail::Local(*var),
            })
            .collect();
        if let Some(call) = self.switched(scope, (key, what), &branches, Some(&result), at) {
            text.push_str(&format!("{indent}{name} = {call};\n"));
            if !self.control.bound.contains(var) {
                self.control.bound.push(*var);
            }
        }
    }

    /// The `if` `value` over the connection `condition`, whose value is
    /// used (CTRL-13): "It is a connection when its condition is one ...
    /// With no `else`, under a temporal condition it is a connection of
    /// `T` that never ticks while the condition is false (CTRL-3)".
    fn valued_if(
        &mut self,
        scope: &mut Scope,
        value: &Expr,
        condition: &Expr,
        then: &Expr,
        otherwise: Option<&Expr>,
    ) -> String {
        let (then_statements, then_value) = parts(then);
        let other = otherwise.map(parts);
        let returns = may_return(then_statements)
            || returning(then_value)
            || other.is_some_and(|(statements, value)| may_return(statements) || returning(value));
        if returns {
            return self.missing(
                value.at,
                "a `return` in an `if` over a connection whose value is used here",
            );
        }
        // CTRL-14: what `switch_` gives is the `if`'s value, one
        // connection; a `var` of what encloses the `if` that a branch
        // assigns would need another.
        if !assigned_outside(std::slice::from_ref(&Stmt::Expr(value.clone()))).is_empty() {
            return self.missing(
                value.at,
                "an `if` over a connection whose value is used and that assigns a `var` declared before it",
            );
        }
        let tail = |value: Option<&Expr>| match value {
            Some(value) => Tail::Value(Box::new(value.clone())),
            None => Tail::Nothing,
        };
        let branches = [
            Branch {
                key: Key::Label("true".to_string(), None),
                statements: then_statements.to_vec(),
                tail: tail(then_value),
            },
            Branch {
                key: Key::Label("false".to_string(), None),
                statements: other.map_or(Vec::new(), |(statements, _)| statements.to_vec()),
                tail: other.map_or(Tail::Nothing, |(_, value)| tail(value)),
            },
        ];
        match self.switched(
            scope,
            (condition, "if"),
            &branches,
            Some(&value.ty),
            value.at,
        ) {
            Some(call) => call,
            None => "0".to_string(),
        }
    }

    /// The `if` `value` over the ordinary value `condition`, whose value
    /// is a connection (CTRL-1: "A wiring-time condition chooses which
    /// branch to compose"; CTRL-13: "Under a wiring-time condition ... a
    /// branch that is a connection makes it a connection of `T` that
    /// never ticks where the branch is not composed"): C++'s `if` where
    /// the graph is built, in a lambda called where it stands, each
    /// branch its statements and then its value, an ordinary value wired
    /// as a constant (FN-3).
    fn composed_if(
        &mut self,
        scope: &mut Scope,
        value: &Expr,
        condition: &Expr,
        then: &Expr,
        otherwise: Option<&Expr>,
    ) -> String {
        let (then_statements, then_value) = parts(then);
        let other = otherwise.map(parts);
        let returns = may_return(then_statements)
            || returning(then_value)
            || other.is_some_and(|(statements, value)| may_return(statements) || returning(value));
        if returns {
            return self.missing(value.at, "a `return` in an `if` whose value is used here");
        }
        let Some(schema) = self.temporal_schema(&value.ty, value.at) else {
            return "0".to_string();
        };
        let tested = self.expression(scope, condition);
        // Each branch's `return` gives the `if`'s value, from the lambda.
        let result = self
            .temporal
            .as_mut()
            .map(|context| context.result.replace(value.ty.clone()));
        let mut text =
            format!("[&]() -> ::hgraph::Port<{schema}> {{\n{INDENT}if ({tested})\n{INDENT}{{\n");
        let mut branch = |emitter: &mut Self,
                          (statements, given): (&[Stmt], Option<&Expr>),
                          text: &mut String| {
            let mut all = statements.to_vec();
            if let Some(given) = given {
                all.push(Stmt::Return {
                    value: Some(given.clone()),
                    at: given.at,
                });
            }
            emitter.statements(scope, &all, 2, text);
            if given.is_none() && !always_returns(&all) {
                text.push_str(&format!(
                    "{INDENT}{INDENT}return ::{}::never<{schema}>({WIRING});\n",
                    runtime::NAMESPACE
                ));
            }
        };
        branch(self, (then_statements, then_value), &mut text);
        text.push_str(&format!("{INDENT}}}\n{INDENT}else\n{INDENT}{{\n"));
        branch(self, other.unwrap_or((&[], None)), &mut text);
        text.push_str(&format!("{INDENT}}}\n}}()"));
        if let (Some(context), Some(result)) = (self.temporal.as_mut(), result) {
            context.result = result;
        }
        text
    }

    /// hgraph's `switch_` over the connection `key`, with a child graph
    /// for each of `branches`, each giving a connection of `result`, or
    /// nothing where it is `None` (CTRL-3). The struct that wires it is
    /// written into the graph's struct; gives the call of its `wire_`,
    /// or `None` where something is not written yet, which is reported.
    ///
    /// CTRL-7: "Captured temporal connections are bound whole and add no
    /// membership": each branch takes every connection any branch reads
    /// that has a value here (`hgl_lower::graphs::taken`), in the order of
    /// the locals, and `switch_` is given them, whole. Each ordinary value
    /// a branch reads is its configuration, an `hgraph::Scalar`, given
    /// where it is compiled.
    fn switched(
        &mut self,
        scope: &mut Scope,
        (key, what): (&Expr, &str),
        branches: &[Branch],
        result: Option<&Type>,
        at: Location,
    ) -> Option<String> {
        let program = self.program;
        let locals = &scope.body.locals;
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
                Tail::Nothing | Tail::Returns | Tail::Outputless => {}
            }
        }
        let bound = self.control.bound.clone();
        let inputs = taken(locals, &bound, &[], read.clone(), &declared);
        let configured = self.configured(scope, &read, &declared, &bound);
        if self.reads_a_capability(scope, &configured, at) {
            return None;
        }
        for branch in branches {
            let tails: Vec<&Expr> = match &branch.tail {
                Tail::Value(value) => vec![&**value],
                _ => Vec::new(),
            };
            if self.computes_while_built(&branch.statements, &tails) {
                return None;
            }
        }
        // What each is in the C++ written: the key's schema and type, each
        // input's schema, each configuration's type, and the result's
        // schema.
        let key_schema = self.temporal_schema(&key.ty, key.at)?;
        let key_type = self.written_type(&key.ty, key.at)?;
        let mut ports = Vec::new();
        for &local in &inputs {
            let schema = self.temporal_schema(&locals[local].ty, locals[local].at)?;
            ports.push((identifier(&locals[local].name), schema));
        }
        let mut scalars = Vec::new();
        for &local in &configured {
            let written = self.written_type(&locals[local].ty, locals[local].at)?;
            scalars.push((local, identifier(&locals[local].name), written));
        }
        let output = match result {
            Some(result) => Some(self.temporal_schema(result, at)?),
            None => None,
        };
        let returns = match &output {
            Some(schema) => format!("::hgraph::Port<{schema}>"),
            None => "void".to_string(),
        };
        let number = self.temporary();
        let name = names::temporary("switch", number);
        let line = self.line_of(at);
        let label = names::string(&format!(
            "{}:{what} on line {line}",
            program.modules[self.module].path
        ));
        // The branches, each a child graph.
        let mut structs = String::new();
        let mut compiled = Vec::new();
        let mut fallback = None;
        for (position, branch) in branches.iter().enumerate() {
            let graph = names::temporary("branch", position);
            let mut parameters = vec![format!("[[maybe_unused]] ::hgraph::Wiring& {WIRING}")];
            let takes_key = matches!(branch.key, Key::Unmatched);
            if takes_key {
                parameters.push(format!("::hgraph::Port<{key_schema}> key_"));
            }
            for (port, schema) in &ports {
                parameters.push(format!("[[maybe_unused]] ::hgraph::Port<{schema}> {port}"));
            }
            for (local, scalar, written) in &scalars {
                parameters.push(format!(
                    "[[maybe_unused]] ::hgraph::Scalar<{}, {written}> {scalar}",
                    names::string(&locals[*local].name)
                ));
            }
            let body = self.branch_body(
                scope,
                branch,
                &inputs,
                &configured,
                result,
                (&key_type, &output),
                at,
            )?;
            let comment = match &branch.key {
                Key::Label(label, None) => {
                    format!("The branch selected while the condition is `{label}`.")
                }
                Key::Label(_, Some(case)) => {
                    format!("The branch of the `case` on line {}.", self.line_of(*case))
                }
                Key::Default => {
                    "The branch of `default`, which a value no case has selects.".to_string()
                }
                Key::Unmatched => {
                    "The branch a value no case has selects: it fails (CTRL-6).".to_string()
                }
            };
            structs.push_str(&format!(
                "{INDENT}// {comment}\n{INDENT}struct {graph}\n{INDENT}{{\n\
                 {INDENT}{INDENT}static constexpr auto name = {label};\n\
                 {INDENT}{INDENT}static {returns} compose({})\n{INDENT}{INDENT}{{\n{body}{INDENT}{INDENT}}}\n{INDENT}}};\n\n",
                parameters.join(", ")
            ));
            let mut given = vec![WIRING.to_string()];
            given.extend(scalars.iter().map(|(_, scalar, _)| scalar.clone()));
            let graph_call = format!("::hgraph::compile_subgraph<{graph}>({})", given.join(", "));
            match &branch.key {
                // CTRL-6: "Switch labels are typed source constants ...
                // compared by resolved value": hgraph's `switch_` selects
                // the first branch whose key equals the selector's value.
                Key::Label(label, _) => compiled.push(format!(
                    "::hgraph::SwitchBranchGraph{{::hgraph::Value{{static_cast<{key_type}>({label})}}, {graph_call}}}"
                )),
                Key::Default => {
                    fallback = Some(format!("::hgraph::SwitchBranchGraph{{{{}}, {graph_call}}}"))
                }
                Key::Unmatched => {
                    fallback = Some(format!("::hgraph::SwitchBranchGraph{{{{}}, {graph_call}, true}}"))
                }
            }
        }
        // Its `wire_`, given the key, the connections the branches take and
        // the values they are configured with.
        let mut parameters = vec![
            format!("::hgraph::Wiring& {WIRING}"),
            format!("::hgraph::Port<{key_schema}> key_"),
        ];
        for (port, schema) in &ports {
            parameters.push(format!("[[maybe_unused]] ::hgraph::Port<{schema}> {port}"));
        }
        for (local, scalar, written) in &scalars {
            let taken = super::values::parameter_type(program, &locals[*local].ty, false, written);
            parameters.push(format!("[[maybe_unused]] {taken} {scalar}"));
        }
        let deeper = INDENT.repeat(2);
        let mut wiring = format!("{deeper}std::vector<::hgraph::SwitchBranchGraph> branches_;\n");
        for graph in &compiled {
            wiring.push_str(&format!("{deeper}branches_.push_back({graph});\n"));
        }
        let erased: Vec<String> = ports
            .iter()
            .map(|(port, _)| format!("{port}.erased()"))
            .collect();
        wiring.push_str(&format!(
            "{deeper}const std::array<::hgraph::WiringPortRef, {}> inputs_{{{}}};\n",
            erased.len(),
            erased.join(", ")
        ));
        let mut arguments = format!("{WIRING}, key_.erased(), inputs_, std::move(branches_)");
        if let Some(fallback) = &fallback {
            arguments.push_str(&format!(", {fallback}"));
        }
        match &output {
            Some(schema) => wiring.push_str(&format!(
                "{deeper}return ::hgraph::Port<void>{{{WIRING}, ::hgraph::compiled::switch_({arguments})}}.as<{schema}>();\n"
            )),
            None => wiring.push_str(&format!(
                "{deeper}::hgraph::compiled::switch_sink_({arguments});\n"
            )),
        }
        let text = format!(
            "// The `{what}` on line {line} over a connection (CTRL-2, CTRL-13 to CTRL-15): a child\n\
             // graph for each branch, of which hgraph's `switch_` runs the one selected.\n\
             struct {name}\n{{\n{structs}\
             {INDENT}static {returns} wire_({})\n{INDENT}{{\n{wiring}{INDENT}}}\n}};\n",
            parameters.join(", ")
        );
        if let Some(context) = self.temporal.as_mut() {
            context.lifted.push(text);
        }
        // The call: the key, then each input and each configuration as the
        // graph being written has them.
        let key_port = self.connection(scope, key, &key.ty);
        let mut given = vec![WIRING.to_string(), key_port];
        given.extend(ports.iter().map(|(port, _)| port.clone()));
        for &local in &configured {
            let read = Expr {
                kind: ExprKind::Local(local),
                ty: locals[local].ty.clone(),
                at,
                category: ValueCategory::Value,
            };
            given.push(self.expression(scope, &read));
        }
        Some(format!("{name}::wire_({})", given.join(", ")))
    }

    /// The statements of the child graph `branch`'s `compose`, which
    /// takes the connections `inputs` and the configurations `configured`
    /// of the graph it is written in, and gives a connection of `result`
    /// or nothing. `key` is the selector's C++ type and the result's
    /// schema. `None` where something is not written yet, which is
    /// reported.
    #[allow(clippy::too_many_arguments)]
    fn branch_body(
        &mut self,
        scope: &mut Scope,
        branch: &Branch,
        inputs: &[usize],
        configured: &[usize],
        result: Option<&Type>,
        (key_type, output): (&str, &Option<String>),
        at: Location,
    ) -> Option<String> {
        let indent = INDENT.repeat(3);
        let locals = &scope.body.locals;
        if let Key::Unmatched = branch.key {
            return Some(match output {
                Some(schema) => format!(
                    "{indent}return ::{}::unmatched<{key_type}, {schema}>({WIRING}, key_);\n",
                    runtime::NAMESPACE
                ),
                None => format!(
                    "{indent}::{}::unmatched_sink<{key_type}>({WIRING}, key_);\n",
                    runtime::NAMESPACE
                ),
            });
        }
        let mut all = branch.statements.clone();
        let mut text = String::new();
        let mut bound: Vec<usize> = inputs.iter().chain(configured).copied().collect();
        match &branch.tail {
            Tail::Value(value) => all.push(Stmt::Return {
                value: Some((**value).clone()),
                at: value.at,
            }),
            Tail::Local(var) => {
                // A `var` with no value before the branch assigns it
                // (CTRL-14: "With none before, every branch that does not
                // return assigns it").
                if !inputs.contains(var) {
                    let schema = self.temporal_schema(&locals[*var].ty, locals[*var].at)?;
                    text.push_str(&format!(
                        "{indent}[[maybe_unused]] ::hgraph::Port<{schema}> {}{{}};\n",
                        identifier(&locals[*var].name)
                    ));
                    bound.push(*var);
                }
                all.push(Stmt::Return {
                    value: Some(Expr {
                        kind: ExprKind::Local(*var),
                        ty: locals[*var].ty.clone(),
                        at,
                        category: locals[*var].category,
                    }),
                    at,
                });
            }
            Tail::Returns => {
                if !always_returns(&all) {
                    self.unsupported(at, "a branch that can reach its end with no `return`");
                    return None;
                }
            }
            Tail::Nothing | Tail::Outputless => {}
        }
        // The branch's own context: its connections are those of the
        // graph it is written in, and each value it is configured with is
        // its `hgraph::Scalar`, read with `.value()`.
        let outer = self.temporal.as_ref()?;
        let mut context = Context::new(Kind::Graph, locals.len());
        context.roles = outer.roles.clone();
        for &local in configured {
            context.roles[local] = Role::Config;
        }
        context.result = result.cloned();
        let mut branch_scope = Scope::cold(self.program, scope.body);
        let before = std::mem::replace(&mut self.control.bound, bound);
        self.in_context(&mut context, |emitter| {
            emitter.statements(&mut branch_scope, &all, 3, &mut text);
        });
        self.control.bound = before;
        // What the branch wires that is a struct of its own, a lifted
        // call's node or another `switch_`'s, is written in the graph's
        // struct with the rest.
        if let Some(outer) = self.temporal.as_mut() {
            outer.lifted.append(&mut context.lifted);
        }
        if let (Tail::Nothing, Some(schema)) = (&branch.tail, output)
            && !always_returns(&all)
        {
            text.push_str(&format!(
                "{indent}return ::{}::never<{schema}>({WIRING});\n",
                runtime::NAMESPACE
            ));
        }
        Some(text)
    }
}

/// Where composing `statements` and `tails` computes an ordinary value
/// that may fail, or `None` (`Emitter::computes_while_built`). A value
/// that cannot fail is the same computed either way; what can is a call
/// that is no temporal function's, an element read and a constant
/// (`calls::may_fail`). A graph they call is composed with them, so what
/// its own body computes is computed then too, at any depth: where that
/// may fail, the place is the call. A node's body runs only as the node is
/// evaluated, and is not looked into. `composed` holds the graphs already
/// looked into, each `(module, declaration)`, so that each is looked into
/// once.
fn computed_while_built(
    program: &Program,
    statements: &[Stmt],
    tails: &[&Expr],
    composed: &mut Vec<(usize, usize)>,
) -> Option<Location> {
    let mut found: Option<Location> = None;
    // Each graph called, with the place of its call.
    let mut graphs: Vec<(Location, usize, usize)> = Vec::new();
    let mut visit = |node: Node| {
        let Node::Expression(value) = node else {
            return;
        };
        if found.is_some() {
            return;
        }
        if let ExprKind::Call { callee, .. } = &value.kind
            && let Some((module, declaration)) = callee.declaration()
            && program.modules[module]
                .phases
                .get(declaration)
                .copied()
                .flatten()
                == Some(Phase::Graph)
        {
            graphs.push((value.at, module, declaration));
            return;
        }
        if value.is_connection() {
            return;
        }
        let computed = match &value.kind {
            ExprKind::Call { callee, .. } => {
                !callee.declaration().is_some_and(|(module, declaration)| {
                    program.modules[module].declarations[declaration]
                        .kind
                        .role()
                        == Some(hgl_analyser::Role::Temporal)
                })
            }
            ExprKind::Index { .. } | ExprKind::Constant { .. } => true,
            _ => false,
        };
        if computed && may_fail(value) {
            found = Some(value.at);
        }
    };
    walk_statement_nodes(statements, &mut visit);
    for tail in tails {
        tail.walk_nodes(&mut visit);
    }
    if found.is_some() {
        return found;
    }
    for (at, module, declaration) in graphs {
        if composed.contains(&(module, declaration)) {
            continue;
        }
        composed.push((module, declaration));
        let Some(Some(body)) = program.modules[module].bodies.get(declaration) else {
            continue;
        };
        if computed_while_built(program, &body.statements, &[], composed).is_some() {
            return Some(at);
        }
    }
    None
}
