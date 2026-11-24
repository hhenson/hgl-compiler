//! Writes the statements of value code as C++: a value function's, an
//! instance's, and a named test's. Part of the transpiler, the last
//! stage.
//!
//! | HGL | C++ |
//! | --- | --- |
//! | `let name = value` | `const T name = value;`, or `const T& name = value;` where it reads a place or a reference the copy rule lets it refer to (`hgl_lower::copies`) |
//! | `var name: T = value`, `var name: T` | `T name = value;`, `T name{};` (EXPR-3: it is assigned before it is read) |
//! | `name = value`, `name += value` | `name = value;`, or `name.append(value);` where the string append plan applies (TYPE-1, EXPR-3) |
//! | `items[i] = value`, `pair[0] = value` | `::hglrt::item(items, i) = value;`, `std::get<0>(pair) = value;` |
//! | `return`, `return value` | `return;`, `return value;` |
//! | `if`, `while`, `for item in items` | `if`, `while`, `for (const T& item : items)` (CTRL-10, CTRL-12); over a set or a dictionary, `composites`' |
//! | `point.x = value`, `book[key] = value` | `point.x = value;`, `::hglrt::entry(book, key) = value;` (`composites`) |
//! | `switch selector { case a: ... default: ... }` | the selector held once, and an `if` for each label, compared by value (CTRL-6) |
//! | A call that assigns places, a value dropped | `call(...);`, `static_cast<void>(value);` |
//!
//! Contextual reads are written at their expressions (`expressions`).

use hgl_analyser::hir::{
    Body, Case, Expr, ExprKind, LabelValue, LocalId, Node, Stmt, walk_statement_nodes,
};
use hgl_analyser::{LocalKind, Program, Type};
use hgl_lower::copies::Copies;
use hgl_lower::graphs::may_return;
use hgl_lower::optimise::Optimisations;

use super::Emitter;
use super::calls::changes;
use super::expressions::reads_an_element;
use super::names::{self, identifier};
use super::types::{by_value, cpp_type};
use super::values::INDENT;

/// What the code of one body is written with.
pub(super) struct Scope<'b> {
    /// The body.
    pub(super) body: &'b Body,
    /// What its `let`s and loops are bound to (the copy rule).
    pub(super) copies: Copies,
    /// The body plans for target-independent optimisations.
    optimisations: Optimisations,
    /// True in a named test, where a statement is a test's (TST-3).
    pub(super) testing: bool,
    /// Declarations and assignments that prepare the statement's places.
    pub(super) before: Vec<String>,
    /// How many levels in what is written now is run only where
    /// something else decides it: the right of `&&` or `||`, a branch
    /// of an `if` written as an expression.
    pub(super) conditional: usize,
    /// True once something that may fail or do anything has been
    /// written of the statement being written (EXPR-13).
    pub(super) acted: bool,
    /// True once a local, a constant or a place has been read of the
    /// statement being written.
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
    /// The scope for a body and its locally bound places.
    pub(super) fn cold(program: &Program, body: &'b Body) -> Scope<'b> {
        Scope {
            body,
            copies: Copies::of(program, body),
            optimisations: Optimisations::of(body),
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
        // A run a family writes whole is the dispatch point's (`forms`).
        if self.form_statements(scope, statements, depth, text) {
            return;
        }
        for statement in statements {
            self.statement(scope, statement, depth, text);
        }
    }

    /// Writes `statement` into `text`, `depth` levels in, after what it
    /// needs run before it (`Scope::before`). Each line it writes is given
    /// the HGL file and line of the statement outside a test, and in a test
    /// written for a debugger (`stamped`). The debugger also maps the
    /// test host's marker to the statement (`tests`).
    pub(super) fn statement(
        &mut self,
        scope: &mut Scope,
        statement: &Stmt,
        depth: usize,
        text: &mut String,
    ) {
        let indent = INDENT.repeat(depth);
        let mut whole = String::new();
        let mut written = String::new();
        let outer_before = std::mem::take(&mut scope.before);
        let outer_acted = std::mem::replace(&mut scope.acted, false);
        let outer_read = std::mem::replace(&mut scope.read, false);
        self.statement_text(scope, statement, depth, &mut written);
        for written in std::mem::replace(&mut scope.before, outer_before) {
            for line in written.lines() {
                whole.push_str(&format!("{indent}{line}\n"));
            }
        }
        scope.acted = outer_acted;
        scope.read = outer_read;
        whole.push_str(&written);
        self.push_stamped(scope, statement, &whole, text);
    }

    /// Write the loop header after holding both bounds in source order
    /// (EXPR-16). Advancing the last included value cannot overflow: the
    /// cursor needs only reach the exclusive bound.
    pub(super) fn range_header(
        &mut self,
        scope: &mut Scope,
        lower: &Expr,
        upper: &Expr,
        binding: &str,
    ) -> String {
        let start = names::temporary("range_start", self.temporary());
        let limit = names::temporary("range_limit", self.temporary());
        let cursor = names::temporary("range_value", self.temporary());
        let value = self.expression(scope, lower);
        scope
            .before
            .push(format!("const ::hgraph::Int {start} = {value};"));
        let value = self.expression(scope, upper);
        scope
            .before
            .push(format!("const ::hgraph::Int {limit} = {value};"));
        format!(
            "for (auto {cursor} = {start}; {cursor} < {limit}; ++{cursor}) {{ [[maybe_unused]] const auto {binding} = {cursor};"
        )
    }

    /// Pushes `written`, the code of `statement`, onto `text`, each of its
    /// lines given the statement's HGL file and line (`stamped`).
    pub(super) fn push_stamped(
        &mut self,
        scope: &Scope,
        statement: &Stmt,
        written: &str,
        text: &mut String,
    ) {
        let directive = if scope.testing && !self.debugger {
            None
        } else {
            self.line_directive(statement_at(statement, &scope.body.locals))
        };
        match directive {
            Some(directive) => text.push_str(&super::stamped(&directive, written)),
            None => text.push_str(written),
        }
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
        // The statements of a hook, a source and a graph that are no value
        // code's are the dispatch point's (`forms`).
        if self.form_statement(scope, statement, depth, text) {
            return;
        }
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
                    || !self.refers(value)
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
            // A local str can take the lowered in-place append (TYPE-1).
            // Other compound assignments write the selected symbol's value.
            Stmt::Assign { local, value, .. } => {
                let name = identifier(&body.locals[*local].name);
                if let Some(parts) = scope.optimisations.text_append(self.program, statement) {
                    for part in parts {
                        let value = self.expression(scope, part);
                        text.push_str(&format!("{indent}{name}.append({value});\n"));
                    }
                } else {
                    let value = self.expression(scope, value);
                    text.push_str(&format!("{indent}{name} = {value};\n"));
                }
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
                    || (reads_an_element(place)
                        && (self.may_fail_here(value) || self.indexed_may_fail(place)));
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
                        ..
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
                            ..
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
            // CTRL-8: a hook iterates the collection's current values.
            Stmt::For {
                names: bound,
                sequence,
                body: statements,
                at,
            } => {
                if let (ExprKind::Range { lower, upper }, [name]) =
                    (&sequence.kind, bound.as_slice())
                {
                    let name = identifier(&body.locals[*name].name);
                    let header = self.range_header(scope, lower, upper, &name);
                    text.push_str(&format!("{indent}{header}\n"));
                    self.statements(scope, statements, depth + 1, text);
                    text.push_str(&format!("{indent}}}\n"));
                    return;
                }
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
                let source = if copied || !self.refers(sequence) {
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
                    text.push_str(&format!("{inner}{INDENT}::std::unreachable();\n"));
                    text.push_str(&format!("{inner}}}\n"));
                } else {
                    // CTRL-6: "An unmatched selector uses default or
                    // fails", with no code.
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
                // TST-11: the harness tests the escaping failure rather
                // than treating a failed inner assertion as one.
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
                    Some(format!("::std::get<{position}>({base})"))
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
                    if !holds && reads_an_element(base) && self.may_fail_here(index) {
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

    /// True where an index of the place `place`, at any depth, may fail.
    fn indexed_may_fail(&self, place: &Expr) -> bool {
        match &place.kind {
            ExprKind::Index { base, index } => {
                self.may_fail_here(index) || self.indexed_may_fail(base)
            }
            ExprKind::Field { base, .. } => self.indexed_may_fail(base),
            _ => false,
        }
    }

    /// True where `value` refers to a local, constant, or part of one.
    /// A contextual read or an ordered index expression yields a value.
    pub(super) fn refers(&self, value: &Expr) -> bool {
        match &value.kind {
            ExprKind::Local(local) => self.input_reader(*local).is_none(),
            ExprKind::Constant { .. } => true,
            ExprKind::Index { base, index } => !self.held_read(base, index) && self.refers(base),
            ExprKind::Field { base, member } => {
                super::composites::field_refers(&base.ty, *member) && self.refers(base)
            }
            _ => false,
        }
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
