//! Writes the statements of value code as C++: a value function's, an
//! instance's, and a named test's. Part of the transpiler, the last
//! stage.
//!
//! | HGL | C++ |
//! | --- | --- |
//! | `let name = value` | `const T name = value;`, or `const T& name = value;` where it reads a place or a reference the copy rule lets it refer to (`hgl_lower::copies`) |
//! | `var name: T = value`, `var name: T` | `T name = value;`, `T name{};` (EXPR-3: it is assigned before it is read) |
//! | `name = value`, `name += value` | `name = value;`, the compound one's value the symbol's call |
//! | `items[i] = value`, `pair[0] = value` | `::hglrt::item(items, i) = value;`, `std::get<0>(pair) = value;` |
//! | `return`, `return value` | `return;`, `return value;` |
//! | `if`, `while`, `for item in items` | `if`, `while`, `for (const T& item : items)` (CTRL-10, CTRL-12); over a set or a dictionary, `composites`' |
//! | `point.x = value`, `book[key] = value` | `point.x = value;`, `::hglrt::entry(book, key) = value;` (`composites`) |
//! | `switch selector { case a: ... default: ... }` | the selector held once, and an `if` for each label, compared by value (CTRL-6) |
//! | A call that assigns places, a value dropped | `call(...);`, `static_cast<void>(value);` |
//!
//! A `?` inside a statement (ERR-5) is written before it: the result is
//! held under a name of its own, and a failure leaves the function with
//! it (`Scope::before`).

use hgl_analyser::hir::{
    Body, Case, Expr, ExprKind, LabelValue, LocalId, Node, Stmt, walk_statement_nodes,
};
use hgl_analyser::{LocalKind, Program, Signature, Type};
use hgl_lower::copies::Copies;
use hgl_lower::graphs::may_return;

use super::Emitter;
use super::calls::{changes, may_fail};
use super::expressions::{held_read, reads_an_element};
use super::names::{self, identifier};
use super::types::{by_value, cpp_type};
use super::values::INDENT;

/// What the code of one body is written with.
pub(super) struct Scope<'b> {
    /// The body.
    pub(super) body: &'b Body,
    /// What its `let`s and loops are bound to (the copy rule).
    pub(super) copies: Copies,
    /// What the function gives, where it gives anything: a `?` leaves
    /// the function with a failure of it (ERR-5).
    pub(super) result: Option<Type>,
    /// True in a named test, where a statement is a test's (TST-3).
    pub(super) testing: bool,
    /// What the statement being written needs run before it, each a
    /// line: a `?`'s result held, and the function left where it is a
    /// failure (ERR-5).
    pub(super) before: Vec<String>,
    /// How many levels in what is written now is run only where
    /// something else decides it: the right of `&&` or `||`, a branch
    /// of an `if` written as an expression. A `?` there cannot be
    /// written before its statement.
    pub(super) conditional: usize,
    /// True once something that may fail or do anything has been
    /// written of the statement being written: a `?` after it would be
    /// run before it if it were written before the statement (EXPR-13).
    pub(super) acted: bool,
    /// True once a local, a constant or a place has been read of the
    /// statement being written: a `?` after it would be run before the
    /// read if it were written before the statement.
    pub(super) read: bool,
    /// True while the call that is the whole of a statement is written,
    /// until it starts: only it may assign places (FN-15).
    pub(super) statement_call: bool,
    /// While a compound assignment's value is written, its place and the
    /// C++ place it is: the value reads the place as its left operand,
    /// and reads it where the statement's held indices name it.
    pub(super) read_as: Option<(Expr, String)>,
    /// While an assignment's place is written, whether the part of it
    /// still to be written is the whole place of an assignment that is
    /// no compound one: a dictionary's entry there is added where the
    /// dictionary has not its key (EXPR-3). `None` outside an
    /// assignment's place: a `var` parameter's.
    pub(super) assigning: Option<bool>,
}

impl<'b> Scope<'b> {
    /// The scope of a function of `signature` whose body is `body`.
    pub(super) fn of(program: &Program, body: &'b Body, signature: &Signature) -> Scope<'b> {
        Scope {
            body,
            copies: Copies::of(program, body),
            result: signature.result.clone(),
            testing: false,
            before: Vec::new(),
            conditional: 0,
            acted: false,
            read: false,
            statement_call: false,
            read_as: None,
            assigning: None,
        }
    }

    /// The scope of what no function holds: a named test's block, a
    /// default or a constant's value (UNIT-7, UNIT-13), read with no
    /// local but its own in scope.
    pub(super) fn cold(program: &Program, body: &'b Body) -> Scope<'b> {
        Scope {
            body,
            copies: Copies::of(program, body),
            result: None,
            testing: false,
            before: Vec::new(),
            conditional: 0,
            acted: false,
            read: false,
            statement_call: false,
            read_as: None,
            assigning: None,
        }
    }
}

impl Emitter<'_> {
    /// Writes each of `statements` into `text`, `depth` levels in.
    pub(super) fn statements(
        &mut self,
        scope: &mut Scope,
        statements: &[Stmt],
        depth: usize,
        text: &mut String,
    ) {
        for statement in statements {
            self.statement(scope, statement, depth, text);
        }
    }

    /// Writes `statement` into `text`, `depth` levels in, after what it
    /// needs run before it (`Scope::before`).
    pub(super) fn statement(
        &mut self,
        scope: &mut Scope,
        statement: &Stmt,
        depth: usize,
        text: &mut String,
    ) {
        let indent = INDENT.repeat(depth);
        let mut written = String::new();
        let outer_before = std::mem::take(&mut scope.before);
        let outer_acted = std::mem::replace(&mut scope.acted, false);
        let outer_read = std::mem::replace(&mut scope.read, false);
        self.statement_text(scope, statement, depth, &mut written);
        for written in std::mem::replace(&mut scope.before, outer_before) {
            for line in written.lines() {
                text.push_str(&format!("{indent}{line}\n"));
            }
        }
        scope.acted = outer_acted;
        scope.read = outer_read;
        text.push_str(&written);
    }

    /// The text of `statement`, `depth` levels in, without what it needs
    /// run before it.
    fn statement_text(
        &mut self,
        scope: &mut Scope,
        statement: &Stmt,
        depth: usize,
        text: &mut String,
    ) {
        let indent = INDENT.repeat(depth);
        let body = scope.body;
        match statement {
            Stmt::Return { value: None, .. } => text.push_str(&format!("{indent}return;\n")),
            Stmt::Return {
                value: Some(value), ..
            } => {
                let value = self.expression(scope, value);
                text.push_str(&format!("{indent}return {value};\n"));
            }
            // EXPR-3: `let` is immutable and `var` may be assigned.
            Stmt::Declare {
                local,
                value: Some(value),
            } => {
                let declared = &body.locals[*local];
                // The value first, which is what a reader reaches first.
                let written = self.expression(scope, value);
                let Some(ty) = self.written_type(&declared.ty, declared.at) else {
                    return;
                };
                let name = identifier(&declared.name);
                let line = if declared.kind == LocalKind::Var {
                    // A place: it holds a value of its own, changed where
                    // it is.
                    format!("{ty} {name} = {written};")
                } else if by_value(self.program, &declared.ty)
                    || scope.copies.copied(*local)
                    || !refers(value)
                {
                    // A copy, where what it reads could change while it
                    // is in scope; a value of its own, where it is made
                    // here.
                    format!("const {ty} {name} = {written};")
                } else {
                    // A reference to what something else holds, which
                    // nothing changes while it is in scope.
                    format!("const {ty}& {name} = {written};")
                };
                text.push_str(&format!("{indent}[[maybe_unused]] {line}\n"));
            }
            // EXPR-3: "A binding must be definitely assigned before it is
            // read", so the value it starts with is never read.
            Stmt::Declare { local, value: None } => {
                let declared = &body.locals[*local];
                let Some(ty) = self.written_type(&declared.ty, declared.at) else {
                    return;
                };
                text.push_str(&format!(
                    "{indent}[[maybe_unused]] {ty} {}{{}};\n",
                    identifier(&declared.name)
                ));
            }
            // A compound assignment's value is the symbol with the
            // local as its left operand.
            Stmt::Assign { local, value, .. } => {
                let value = self.expression(scope, value);
                text.push_str(&format!(
                    "{indent}{} = {value};\n",
                    identifier(&body.locals[*local].name)
                ));
            }
            // The place is the one its indices name before the value is
            // evaluated (EXPR-2, EXPR-3), and C++ evaluates the value
            // first: where the value or the place runs statements, or the
            // place is an element and the value or one of its indices may
            // fail, each index is held, and the place checked, first. A compound assignment
            // reads the place it writes, as its value's left operand: it
            // is read where the held indices name it, so that each index
            // is evaluated once.
            Stmt::AssignPlace {
                place,
                operator,
                value,
                at,
            } => {
                let holds = changes(value)
                    || changes(place)
                    || (reads_an_element(place) && (may_fail(value) || indexed_may_fail(place)));
                scope.assigning = Some(operator.is_none());
                let written = self.place_held(scope, place, holds);
                scope.assigning = None;
                let Some(written) = written else {
                    self.unsupported(*at, "an assignment to this place");
                    return;
                };
                // Where the indices are held, the place is also checked
                // there, before the value runs (EXPR-3, EXPR-7): the value
                // could replace the list it is in.
                if holds && reads_an_element(place) {
                    scope.before.push(format!("static_cast<void>({written});"));
                }
                let outer = operator
                    .is_some()
                    .then(|| scope.read_as.replace((place.clone(), written.clone())));
                let value = self.expression(scope, value);
                if let Some(outer) = outer {
                    scope.read_as = outer;
                }
                text.push_str(&format!("{indent}{written} = {value};\n"));
            }
            // CTRL-10: an `if` that stands as a statement, whose
            // branches' values are dropped.
            Stmt::Expr(Expr {
                kind:
                    ExprKind::If {
                        condition,
                        then,
                        otherwise,
                    },
                ..
            }) => {
                let condition = self.expression(scope, condition);
                text.push_str(&format!("{indent}if ({condition})\n{indent}{{\n"));
                self.branch(scope, then, depth + 1, text);
                text.push_str(&format!("{indent}}}\n"));
                // An `else if` is written as one, where what its
                // condition needs is nothing to run before it.
                let mut otherwise = otherwise.as_deref();
                while let Some(Expr {
                    kind:
                        ExprKind::If {
                            condition,
                            then,
                            otherwise: next,
                        },
                    ..
                }) = otherwise
                {
                    let before = std::mem::take(&mut scope.before);
                    let written = self.expression(scope, condition);
                    if !scope.before.is_empty() {
                        scope.before = before;
                        break;
                    }
                    scope.before = before;
                    text.push_str(&format!("{indent}else if ({written})\n{indent}{{\n"));
                    self.branch(scope, then, depth + 1, text);
                    text.push_str(&format!("{indent}}}\n"));
                    otherwise = next.as_deref();
                }
                if let Some(otherwise) = otherwise {
                    text.push_str(&format!("{indent}else\n{indent}{{\n"));
                    self.branch(scope, otherwise, depth + 1, text);
                    text.push_str(&format!("{indent}}}\n"));
                }
            }
            // A block that stands as a statement: its statements, and its
            // value dropped.
            Stmt::Expr(
                block @ Expr {
                    kind: ExprKind::Block { .. },
                    ..
                },
            ) => {
                text.push_str(&format!("{indent}{{\n"));
                self.branch(scope, block, depth + 1, text);
                text.push_str(&format!("{indent}}}\n"));
            }
            // A call stands as a statement as it is; any other value, and
            // a call's that gives one, is dropped.
            Stmt::Expr(value) => {
                scope.statement_call = matches!(value.kind, ExprKind::Call { .. });
                let written = self.expression(scope, value);
                scope.statement_call = false;
                if self.gives_nothing(value) {
                    text.push_str(&format!("{indent}{written};\n"));
                } else {
                    text.push_str(&format!("{indent}static_cast<void>({written});\n"));
                }
            }
            // CTRL-12: `while condition { ... }`.
            Stmt::While {
                condition: Some(condition),
                body: statements,
                ..
            } => {
                let condition = self.expression(scope, condition);
                if !scope.before.is_empty() {
                    // What the condition needs run before it would be
                    // run once, and not again for each pass.
                    self.unsupported(
                        statement_at(statement, &body.locals),
                        "a `?` in a loop's condition",
                    );
                    return;
                }
                text.push_str(&format!("{indent}while ({condition})\n{indent}{{\n"));
                self.statements(scope, statements, depth + 1, text);
                text.push_str(&format!("{indent}}}\n"));
            }
            // CTRL-12: `for item in items { ... }` over a list. The name
            // is bound to each element in turn: a reference to it, or a
            // copy where the list could change while it is in scope (the
            // copy rule), and then the loop runs over what the list held
            // when it began.
            Stmt::For {
                names: bound,
                sequence,
                body: statements,
                at,
            } => {
                // CTRL-12: over a set or a dictionary (`composites`).
                if matches!(sequence.ty, Type::Set(_) | Type::Dict { .. }) {
                    self.composite_for(scope, bound, sequence, statements, *at, depth, text);
                    return;
                }
                let (Type::List { element, .. }, [name]) = (&sequence.ty, bound.as_slice()) else {
                    self.unsupported(*at, "a `for` loop over this");
                    return;
                };
                let Some(element) = self.written_type(element, *at) else {
                    return;
                };
                let Some(list) = self.written_type(&sequence.ty, *at) else {
                    return;
                };
                let items = self.expression(scope, sequence);
                let local = identifier(&body.locals[*name].name);
                let copied = scope.copies.copied(*name);
                // A list that is not what a local or a constant holds may
                // be part of a value that ends with the loop's first line,
                // `rows()[0]`: it is held as a value of its own, as is one
                // the loop could change.
                let source = if copied || !refers(sequence) {
                    let held = names::temporary("items", self.temporary());
                    text.push_str(&format!("{indent}const {list} {held} = {items};\n"));
                    held
                } else {
                    items
                };
                let binding = if copied {
                    format!("const {element} {local}")
                } else {
                    format!("const {element}& {local}")
                };
                text.push_str(&format!("{indent}for ({binding} : {source})\n{indent}{{\n"));
                self.statements(scope, statements, depth + 1, text);
                text.push_str(&format!("{indent}}}\n"));
            }
            // CTRL-6: the selector is evaluated once, and each label is
            // compared with it by value, in order; there is no
            // fallthrough.
            Stmt::Switch {
                selector,
                cases,
                default,
                at,
            } => {
                // UNIT-12: "A `switch` case has no scope of its own: its
                // statements bind in the block around the `switch`". Each
                // case is a C++ block of its own, so a binding of one that
                // is read after the `switch` is not written yet.
                let outlives = cases
                    .iter()
                    .map(|case| case.body.as_slice())
                    .chain(default.as_deref())
                    .flatten()
                    .filter_map(|bound| match bound {
                        Stmt::Declare { local, .. } => Some(*local),
                        _ => None,
                    })
                    .any(|local| {
                        mentions(&scope.body.statements, local)
                            > mentions(std::slice::from_ref(statement), local)
                    });
                if outlives {
                    self.unsupported(*at, "a binding of a `switch` case, read after it");
                    return;
                }
                let Some(ty) = self.written_type(&selector.ty, *at) else {
                    return;
                };
                let selected = self.expression(scope, selector);
                let held = names::temporary("selected", self.temporary());
                text.push_str(&format!("{indent}{{\n"));
                let inner = INDENT.repeat(depth + 1);
                text.push_str(&format!("{inner}const {ty} {held} = {selected};\n"));
                let mut first = true;
                for Case {
                    label,
                    body: statements,
                    ..
                } in cases
                {
                    let label = self.expression(scope, label);
                    let word = if first { "if" } else { "else if" };
                    first = false;
                    text.push_str(&format!("{inner}{word} ({held} == {label})\n{inner}{{\n"));
                    self.statements(scope, statements, depth + 2, text);
                    text.push_str(&format!("{inner}}}\n"));
                }
                if let Some(statements) = default {
                    if first {
                        text.push_str(&format!("{inner}{{\n"));
                    } else {
                        text.push_str(&format!("{inner}else\n{inner}{{\n"));
                    }
                    self.statements(scope, statements, depth + 2, text);
                    text.push_str(&format!("{inner}}}\n"));
                } else if !first && self.every_value(&selector.ty, cases) {
                    // The labels name every value the selector can hold,
                    // so no other is reached: C++ is told so.
                    text.push_str(&format!("{inner}else\n{inner}{{\n"));
                    text.push_str(&format!("{inner}{INDENT}std::unreachable();\n"));
                    text.push_str(&format!("{inner}}}\n"));
                } else {
                    // CTRL-6: "An unmatched selector uses default or
                    // fails", with no code, as the hgraph target fails.
                    let fail = format!(
                        "::{}::fail_uncoded(\"no case of the `switch` has the selector's value\");",
                        crate::runtime::NAMESPACE
                    );
                    if first {
                        text.push_str(&format!("{inner}{fail}\n"));
                    } else {
                        text.push_str(&format!(
                            "{inner}else\n{inner}{{\n{inner}{INDENT}{fail}\n{inner}}}\n"
                        ));
                    }
                }
                text.push_str(&format!("{indent}}}\n"));
            }
            Stmt::Assert { condition, .. } if scope.testing => {
                let condition = self.expression(scope, condition);
                let check = names::testing("check");
                text.push_str(&format!("{indent}{check}({condition});\n"));
            }
            // TST-10: the block is run, and the test fails unless it
            // fails with that code; it goes on after the assertion where
            // it does.
            Stmt::AssertRaises {
                code: Some(code),
                body: statements,
                ..
            } if scope.testing => {
                let raises = names::testing("raises");
                text.push_str(&format!(
                    "{indent}{raises}({}, [&] {{\n",
                    names::string(code)
                ));
                self.statements(scope, statements, depth + 1, text);
                text.push_str(&format!("{indent}}});\n"));
            }
            Stmt::Assert { at, .. } => self.unsupported(*at, "`assert` here"),
            Stmt::AssertRaises { at, .. } => self.unsupported(*at, "this `assert raises`"),
            Stmt::While { at, .. } => self.unsupported(*at, "a `while` with no condition"),
            Stmt::Yield { at, .. } => self.unsupported(*at, "a `yield`"),
            Stmt::Hook { at, .. } => self.unsupported(*at, "a hook"),
            Stmt::Inject { at, .. } => self.unsupported(*at, "an injected capability"),
            Stmt::Unread { at } => self.unsupported(*at, "this statement"),
        }
    }

    /// Writes the statements of `branch`, a block or the `if` of an
    /// `else if`, `depth` levels in, its value dropped.
    fn branch(&mut self, scope: &mut Scope, branch: &Expr, depth: usize, text: &mut String) {
        match &branch.kind {
            ExprKind::Block { statements, value } => {
                self.statements(scope, statements, depth, text);
                if let Some(value) = value {
                    self.statement(scope, &Stmt::Expr((**value).clone()), depth, text);
                }
            }
            _ => self.statement(scope, &Stmt::Expr(branch.clone()), depth, text),
        }
    }

    /// True where the labels of `cases` name every value of `ty`: each
    /// member of an enum, or both of `bool`'s (CTRL-6).
    fn every_value(&self, ty: &Type, cases: &[Case]) -> bool {
        let values: Vec<&LabelValue> = cases
            .iter()
            .filter_map(|case| case.value.as_ref())
            .collect();
        match ty {
            Type::Scalar(hgl_analyser::Scalar::Bool) => {
                values.contains(&&LabelValue::Bool(true))
                    && values.contains(&&LabelValue::Bool(false))
            }
            Type::Named {
                module,
                declaration,
                ..
            } => match self.program.modules[*module].details.get(*declaration) {
                Some(hgl_analyser::Detail::Enum(read)) => (0..read.members.len())
                    .all(|member| values.contains(&&LabelValue::Member(member))),
                _ => false,
            },
            _ => false,
        }
    }

    /// A new number for a name the piece makes (`names::temporary`).
    pub(super) fn temporary(&mut self) -> usize {
        self.temporaries += 1;
        self.temporaries - 1
    }

    /// The C++ place `place` is, which an assignment or a `var`
    /// parameter writes (EXPR-3, FN-15): a local, a list's element, a
    /// tuple's position, a struct's field, and in an assignment a
    /// dictionary's entry, at any depth. `None` for any other.
    pub(super) fn place(&mut self, scope: &mut Scope, place: &Expr) -> Option<String> {
        self.place_held(scope, place, false)
    }

    /// [`Emitter::place`], each list's index held under a name of its
    /// own before the statement where `holds` is true.
    fn place_held(&mut self, scope: &mut Scope, place: &Expr, holds: bool) -> Option<String> {
        // Whether this is the whole place of an assignment that is no
        // compound one; what it is part of is not (EXPR-3).
        let adds = scope.assigning == Some(true);
        if scope.assigning.is_some() {
            scope.assigning = Some(false);
        }
        match &place.kind {
            ExprKind::Local(local) => {
                scope.read = true;
                Some(identifier(&scope.body.locals[*local].name))
            }
            ExprKind::Index { base, index } => match &base.ty {
                Type::Tuple(_) => {
                    let ExprKind::Literal(hgl_analyser::hir::Value::Int(position)) = index.kind
                    else {
                        return None;
                    };
                    let base = self.place_held(scope, base, holds)?;
                    Some(format!("std::get<{position}>({base})"))
                }
                // EXPR-2: the list, then the index. Where the indices are
                // held before the statement, the list's own place is
                // checked there before its index is held; where they are
                // not, and both the list's place and the index may fail,
                // the element is a reference made by a lambda that takes
                // them in that order.
                Type::List { .. } => {
                    let item = format!("::{}::item", crate::runtime::NAMESPACE);
                    let listed = self.place_held(scope, base, holds)?;
                    let held = holds && !matches!(index.kind, ExprKind::Literal(_));
                    if held && reads_an_element(base) {
                        scope.before.push(format!("static_cast<void>({listed});"));
                    }
                    let written = self.expression(scope, index);
                    if held {
                        let name = names::temporary("index", self.temporary());
                        scope
                            .before
                            .push(format!("const ::hgraph::Int {name} = {written};"));
                        return Some(format!("{item}({listed}, {name})"));
                    }
                    if !holds && reads_an_element(base) && may_fail(index) {
                        let ty = cpp_type(self.program, &place.ty)?;
                        let list = names::temporary("list", self.temporary());
                        let at = names::temporary("index", self.temporary());
                        return Some(format!(
                            "[&]() -> {ty}& {{ auto& {list} = {listed}; const auto {at} = {written}; return {item}({list}, {at}); }}()"
                        ));
                    }
                    Some(format!("{item}({listed}, {written})"))
                }
                // EXPR-3: "A key the dictionary has not is added"
                // (`composites`).
                Type::Dict { .. } if scope.assigning.is_some() => {
                    let listed = self.place_held(scope, base, holds)?;
                    self.dict_place(scope, (base, listed), index, holds, adds)
                }
                _ => None,
            },
            // EXPR-3: "a field, `point.x = 1`" (`composites`).
            ExprKind::Field { base, member } => {
                let written = self.place_held(scope, base, holds)?;
                self.field_place(&base.ty, written, *member)
            }
            _ => None,
        }
    }
}

/// True where an index of the place `place`, at any depth, may fail.
fn indexed_may_fail(place: &Expr) -> bool {
    match &place.kind {
        ExprKind::Index { base, index } => may_fail(index) || indexed_may_fail(base),
        ExprKind::Field { base, .. } => indexed_may_fail(base),
        _ => false,
    }
}

/// How many times `statements` name `local`, at any depth: each read of
/// it, and each assignment to it.
fn mentions(statements: &[Stmt], local: LocalId) -> usize {
    let mut found = 0;
    walk_statement_nodes(statements, &mut |node| match node {
        Node::Expression(Expr {
            kind: ExprKind::Local(read),
            ..
        })
        | Node::Statement(Stmt::Assign { local: read, .. })
            if *read == local =>
        {
            found += 1
        }
        _ => {}
    });
    found
}

/// True where `value` reads what something else holds, as it is: a
/// local, an element, an entry, a position or a field of one, a constant.
/// A `let` of it may be a reference; any other value is made where it is
/// read, an element whose list and index are held before it is read
/// among them (`expressions::held_read`), and a list's delta's `updated`
/// and `removed` (`composites::field_refers`).
pub(super) fn refers(value: &Expr) -> bool {
    match &value.kind {
        ExprKind::Local(_) | ExprKind::Constant { .. } => true,
        ExprKind::Index { base, index } => !held_read(base, index) && refers(base),
        ExprKind::Field { base, member } => {
            super::composites::field_refers(&base.ty, *member) && refers(base)
        }
        _ => false,
    }
}

/// True where some path through `statements` returns from the function:
/// an expression that holds them cannot be written as one.
pub(super) fn returns(statements: &[Stmt]) -> bool {
    may_return(statements)
}

/// Where `statement` is written: a declaration where the name it
/// declares is, `locals` being the body's.
pub(super) fn statement_at(
    statement: &Stmt,
    locals: &[hgl_analyser::hir::Local],
) -> hgl_diagnostics::Location {
    match statement {
        Stmt::Declare { local, .. } => locals[*local].at,
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
    }
}
