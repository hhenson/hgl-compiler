//! Writes set and dictionary literals, indexing, assignment and iteration.

use hgl_analyser::Type;
use hgl_analyser::hir::{Expr, ExprKind, Stmt, Value};
use hgl_diagnostics::Location;

use crate::emit::Emitter;
use crate::emit::calls::changes;
use crate::emit::expressions::reads_an_element;
use crate::emit::names::{self, identifier};
use crate::emit::statements::Scope;
use crate::emit::values::INDENT;
use crate::runtime;

impl Emitter<'_> {
    /// `[a, b]` where a set is expected (EXPR-9), and `[]` where a set or
    /// a dictionary is: each entry evaluated in the order written.
    pub(in crate::emit) fn set_literal(
        &mut self,
        scope: &mut Scope,
        value: &Expr,
        entries: &[Expr],
    ) -> String {
        let Some(ty) = self.written_type(&value.ty, value.at) else {
            return "0".to_string();
        };
        match &value.ty {
            Type::Set(_) => self.constructed(scope, &ty, entries),
            Type::Dict { .. } if entries.is_empty() => format!("{ty}{{}}"),
            _ => self.missing(value.at, &format!("the type `{}`", value.ty)),
        }
    }

    /// `[key: value, ...]` where a dictionary is expected (EXPR-9): each
    /// key and each value evaluated in the order written. Where an entry
    /// after the first runs statements, which may change what an earlier
    /// one read, each is first held as a value of its own. The pinned text
    /// does not say what a dictionary written with one key twice holds, so
    /// one of more than one entry is written only where its keys are
    /// constants that are not `==` ([`distinct_keys`]), and reported
    /// otherwise.
    pub(in crate::emit) fn dict_literal(
        &mut self,
        scope: &mut Scope,
        value: &Expr,
        entries: &[(Expr, Expr)],
    ) -> String {
        let Type::Dict { key, value: held } = &value.ty else {
            return self.missing(value.at, &format!("the type `{}`", value.ty));
        };
        let (Some(key_type), Some(value_type)) = (
            self.written_type(key, value.at),
            self.written_type(held, value.at),
        ) else {
            return "0".to_string();
        };
        if entries.is_empty() {
            return format!(
                "::{}::dict<{key_type}, {value_type}>{{}}",
                runtime::NAMESPACE
            );
        }
        let made = format!(
            "::{}::dict_of<{key_type}, {value_type}>",
            runtime::NAMESPACE
        );
        if !distinct_keys(entries) {
            return self.missing(value.at, "a dictionary written with keys that may be one");
        }
        let flattened: Vec<&Expr> = entries
            .iter()
            .flat_map(|(key, value)| [key, value])
            .collect();
        let ordered = flattened.iter().skip(1).any(|entry| changes(entry));
        if !ordered {
            let pairs: Vec<String> = entries
                .iter()
                .map(|(key, value)| {
                    let key = self.expression(scope, key);
                    let value = self.expression(scope, value);
                    format!("{{{key}, {value}}}")
                })
                .collect();
            return format!("{made}({{{}}})", pairs.join(", "));
        }
        let mut steps = String::new();
        let mut pairs = Vec::new();
        for (key, held) in entries {
            let key = self.expression(scope, key);
            let held = self.expression(scope, held);
            let key_name = names::temporary("key", self.temporary());
            let value_name = names::temporary("value", self.temporary());
            steps.push_str(&format!(
                "const {key_type} {key_name} = {key}; const {value_type} {value_name} = {held}; "
            ));
            pairs.push(format!("{{{key_name}, {value_name}}}"));
        }
        scope.acted = true;
        let Some(ty) = self.written_type(&value.ty, value.at) else {
            return "0".to_string();
        };
        format!(
            "[&]() -> {ty} {{ {steps}return {made}({{{}}}); }}()",
            pairs.join(", ")
        )
    }

    /// `book[key]` of a dictionary (ERR-6, EXPR-7): "brackets and `at`
    /// are strict point-in-time reads; absence fails", with
    /// `value.missing_key`. The dictionary, then the key (EXPR-2): where
    /// they are held (`expressions::held_read`), the dictionary is held
    /// first and the value read is a value of its own.
    pub(in crate::emit) fn dict_read(
        &mut self,
        scope: &mut Scope,
        value: &Expr,
        base: &Expr,
        index: &Expr,
    ) -> String {
        if self.held_read(base, index) {
            let Some(ty) = self.written_type(&value.ty, value.at) else {
                return "0".to_string();
            };
            let (Some(book_type), Some(key_type)) = (
                self.written_type(&base.ty, base.at),
                self.written_type(&index.ty, index.at),
            ) else {
                return "0".to_string();
            };
            let held = if self.refers(base) && !changes(index) {
                format!("const {book_type}&")
            } else {
                format!("const {book_type}")
            };
            let base = self.expression(scope, base);
            let index = self.expression(scope, index);
            scope.acted = true;
            let book = names::temporary("book", self.temporary());
            let key = names::temporary("key", self.temporary());
            return format!(
                "[&]() -> {ty} {{ {held} {book} = {base}; const {key_type} {key} = {index}; return ::{}::entry_at({book}, {key}); }}()",
                runtime::NAMESPACE
            );
        }
        let base = self.expression(scope, base);
        let index = self.expression(scope, index);
        scope.acted = true;
        format!("::{}::entry_at({base}, {index})", runtime::NAMESPACE)
    }

    /// `book[key]` as a place (EXPR-3): "A key the dictionary has not is
    /// added" where `adds`, the place the assignment writes whole;
    /// otherwise the key must be there, as a compound assignment reads
    /// the place first. `base` is the dictionary's place, `listed` it
    /// written. EXPR-2: the dictionary's place, then the key. Where the
    /// key is held before the statement (`holds`), a dictionary's place
    /// that is read checked (an element or an entry) is checked there
    /// first; where it is not, and both that place and the key may fail,
    /// the entry is made by a lambda that takes them in that order, as a
    /// list's element is (`statements::place_held`).
    pub(in crate::emit) fn dict_place(
        &mut self,
        scope: &mut Scope,
        (base, listed): (&Expr, String),
        index: &Expr,
        holds: bool,
        adds: bool,
    ) -> Option<String> {
        let function = format!(
            "::{}::{}",
            runtime::NAMESPACE,
            if adds { "entry" } else { "entry_at" }
        );
        let held = holds
            && !matches!(
                index.kind,
                ExprKind::Literal(_) | ExprKind::EnumMember { .. }
            );
        if held && reads_an_element(base) {
            scope.before.push(format!("static_cast<void>({listed});"));
        }
        let written = self.expression(scope, index);
        let key_type = self.named_type(&index.ty)?;
        if held {
            let key = names::temporary("key", self.temporary());
            scope
                .before
                .push(format!("const {key_type} {key} = {written};"));
            return Some(format!("{function}({listed}, {key})"));
        }
        if !holds && reads_an_element(base) && self.may_fail_here(index) {
            // A reference to the entry, or the place that adds it, which
            // holds its dictionary by reference and its key as a value.
            let result = match &base.ty {
                _ if adds => String::new(),
                Type::Dict { value, .. } => format!(" -> {}&", self.named_type(value)?),
                _ => return None,
            };
            let book = names::temporary("book", self.temporary());
            let key = names::temporary("key", self.temporary());
            return Some(format!(
                "[&](){result} {{ auto& {book} = {listed}; const {key_type} {key} = {written}; return {function}({book}, {key}); }}()"
            ));
        }
        Some(format!("{function}({listed}, {written})"))
    }

    /// `for item in members` over a set, and `for key, value in entries`
    /// over a dictionary (CTRL-12), in no order. Each name is bound to a
    /// member, or a key and its value, of what the set or the dictionary
    /// held when the loop began: what is looped over is held whole where
    /// it could change while the loop runs (the copy rule) and where it
    /// is a value made here, which a range over a part of it would not
    /// keep.
    #[allow(clippy::too_many_arguments)]
    pub(in crate::emit) fn composite_for(
        &mut self,
        scope: &mut Scope,
        bound: &[usize],
        sequence: &Expr,
        statements: &[Stmt],
        at: Location,
        depth: usize,
        text: &mut String,
    ) {
        let indent = INDENT.repeat(depth);
        let body = scope.body;
        let Some(whole) = self.written_type(&sequence.ty, at) else {
            return;
        };
        let names: Vec<String> = bound
            .iter()
            .map(|local| identifier(&body.locals[*local].name))
            .collect();
        let binding = match (&sequence.ty, names.as_slice()) {
            (Type::Set(element), [name]) => {
                let Some(element) = self.written_type(element, at) else {
                    return;
                };
                format!("[[maybe_unused]] const {element}& {name}")
            }
            (Type::Dict { .. }, [key, value]) => {
                format!("[[maybe_unused]] const auto& [{key}, {value}]")
            }
            _ => {
                self.unsupported(at, "a `for` loop over this");
                return;
            }
        };
        let items = self.expression(scope, sequence);
        let copied = bound.iter().any(|local| scope.copies.copied(*local));
        let over = if copied || !self.refers(sequence) {
            let held = names::temporary("items", self.temporary());
            text.push_str(&format!("{indent}const {whole} {held} = {items};\n"));
            held
        } else {
            items
        };
        text.push_str(&format!("{indent}for ({binding} : {over})\n{indent}{{\n"));
        self.statements(scope, statements, depth + 1, text);
        text.push_str(&format!("{indent}}}\n"));
    }
}

/// True where the keys of `entries`, a dictionary written out, are known
/// to be distinct: there is at most one, or each is a literal or an enum's
/// member and no two are `==` (OPS-2: the two zeros are one).
fn distinct_keys(entries: &[(Expr, Expr)]) -> bool {
    /// A key as a constant, where it is one that is told apart here.
    #[derive(PartialEq)]
    enum Key<'e> {
        Value(&'e Value),
        Float(f64),
        Member(usize, usize, usize),
    }
    fn key(expr: &Expr) -> Option<Key<'_>> {
        match &expr.kind {
            ExprKind::Literal(Value::Float(number)) => Some(Key::Float(*number)),
            ExprKind::Literal(
                value @ (Value::Int(_) | Value::Str(_) | Value::Bool(_) | Value::Duration(_)),
            ) => Some(Key::Value(value)),
            // An integer literal where an `f64` is expected (EXPR-5).
            ExprKind::Widen(inner) => match inner.kind {
                ExprKind::Literal(Value::Int(number)) => Some(Key::Float(number as f64)),
                _ => None,
            },
            ExprKind::EnumMember {
                module,
                declaration,
                member,
            } => Some(Key::Member(*module, *declaration, *member)),
            _ => None,
        }
    }
    if entries.len() <= 1 {
        return true;
    }
    let keys: Option<Vec<Key>> = entries.iter().map(|(written, _)| key(written)).collect();
    let Some(keys) = keys else {
        return false;
    };
    keys.iter()
        .enumerate()
        .all(|(at, one)| keys[at + 1..].iter().all(|other| other != one))
}
