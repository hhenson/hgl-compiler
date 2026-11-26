//! Writes HGL's composite values beside the list and the tuple as C++:
//! structs and abstract structs (TYPE-3 to TYPE-6), sets and dictionaries
//! (TYPE-14, EXPR-3, ERR-6, CTRL-12), deltas as values (TYPE-16), an
//! enum's member made from a value (TYPE-7), and an equality of a `T?`
//! with a `T` (OPS-2, TYPE-14). Part of the transpiler, the last stage.
//!
//! | HGL | C++ |
//! | --- | --- |
//! | `struct Point { x: f64 }` | An aggregate `struct Point { ::hgraph::Float x; }` with `==` of each field (OPS-2), in the module's types header; `hglrt::fields<Point>` naming its members, `std::hash<Point>`, and its delta, `d_::Point` |
//! | `struct Box<T> { value: T }` | A class template, `template <class T_> struct Box`; `Box<i64>` is `Box<::hgraph::Int>` |
//! | `abstract struct Shape { id: i64 }` | A class holding any struct that inherits from it, with a function of each field's name, `id()`; `hglrt::parents<Round>` lists what `Round` inherits from |
//! | `Point(y: 2.0, x: 1.0)` | `Point{1.0, 2.0}`, its fields in their order; each held first, in the order written, where that order shows (TYPE-5, EXPR-2) |
//! | `point.x`, `shape.id`, `change.added` | `(point).x`, `(shape).id()`, `(change).added` |
//! | `set<K>`, `dict<K, V>` | `::hglrt::set<K>`, `::hglrt::dict<K, V>`: hash containers, in no order (CTRL-12) |
//! | `[1, 2]` as a set, `["a": 1]` | `::hglrt::set<K>{1, 2}`, `::hglrt::dict_of<K, V>({{"a", 1}})` |
//! | `book[key]`, `book[key] = value`, `book[key] += value` | `::hglrt::entry_at(book, key)`, which fails with `value.missing_key` (ERR-6); `::hglrt::entry(book, key) = value`, which adds the key (EXPR-3); `::hglrt::entry_at(book, key) = ...` |
//! | `for x in members`, `for k, v in book` | `for (const K& x : members)`, `for (const auto& [k, v] : book)` |
//! | `delta<set<i64>>(added: [1])`, of a dictionary, a list, a nullable, a struct | `::hglrt::set_delta<::hgraph::Int>{...}`, `::hglrt::dict_delta`, `::hglrt::list_delta_of<L>(...)`, `::hglrt::nullable_delta<T, D>{...}`, `d_::Point{...}` |
//! | `Side(1)`, `Side("buy")` | `::hglrt::enum_member<Side, ::hgraph::Int>(1, {{0, Side::buy}, ...})` |
//! | `maybe == 1` of an `i64?` | `(maybe == 1)`: an optional and a value are equal where the optional holds that value |
//!
//! A struct that holds itself (TYPE-6), a field of an abstract struct
//! assigned, a dictionary's entry given to a `var` parameter, and a pack
//! are not written yet.

use hgl_analyser::hir::Expr;
use hgl_analyser::{Detail, NamedKind, Program, Type};

use super::Emitter;
use super::calls::changes;
use super::expressions::integer_literal;
use super::names::{self, identifier};
use super::statements::Scope;
use super::types::{cpp_type_in, enum_name};
use crate::runtime;

pub(crate) use self::layouts::Layout;
use self::layouts::struct_delta_type;
pub(super) use self::layouts::struct_type;

mod collections;
mod construction;
mod layouts;

/// The C++ type of a set, a dictionary, a delta or an atomic value (the
/// arms of `types::cpp_type_in` this module writes). `None` for one not
/// written yet.
pub(super) fn composite_type(
    program: &Program,
    ty: &Type,
    queries: Option<&[Type]>,
) -> Option<String> {
    Some(match ty {
        // TYPE-14: "A set's element and a dictionary's key are never
        // null".
        Type::Set(element) => format!(
            "::{}::set<{}>",
            runtime::NAMESPACE,
            cpp_type_in(program, element, queries)?
        ),
        Type::Dict { key, value } => format!(
            "::{}::dict<{}, {}>",
            runtime::NAMESPACE,
            cpp_type_in(program, key, queries)?,
            cpp_type_in(program, value, queries)?
        ),
        Type::Delta(of) => delta_type(program, of, queries)?,
        // TYPE-9: "`atomic<V>` stops expansion at a complete ordinary V":
        // as a value, a struct's field of it holds the V.
        Type::Atomic(payload) => cpp_type_in(program, payload, queries)?,
        _ => return None,
    })
}

/// The C++ type of `delta<of>` where it is a type of its own (TYPE-16):
/// a set's, a dictionary's, a list's and a nullable's of the library, a
/// struct's the struct's delta.
/// "In a generic declaration it stays as written until T is known": in a
/// generic struct, `hglrt::delta_t` of the parameter.
fn delta_type(program: &Program, of: &Type, queries: Option<&[Type]>) -> Option<String> {
    Some(match of {
        Type::Set(element) => format!(
            "::{}::set_delta<{}>",
            runtime::NAMESPACE,
            cpp_type_in(program, element, queries)?
        ),
        Type::Dict { key, value } if !changes_whole(value) => format!(
            "::{}::dict_delta<{}, {}>",
            runtime::NAMESPACE,
            cpp_type_in(program, key, queries)?,
            cpp_type_in(program, value, queries)?
        ),
        Type::List { element, .. } if !changes_whole(element) => format!(
            "::{}::list_delta<{}>",
            runtime::NAMESPACE,
            cpp_type_in(program, of, queries)?
        ),
        // "A nullable's delta is an optional: it holds what was
        // published, a null among them", and a type of its own, which is
        // no nullable: the library's, of the payload and its delta.
        Type::Nullable(payload) => format!(
            "::{}::nullable_delta<{}, {}>",
            runtime::NAMESPACE,
            cpp_type_in(program, payload, queries)?,
            cpp_type_in(program, &Type::delta((**payload).clone()), queries)?
        ),
        Type::Named {
            module,
            declaration,
            arguments,
            kind: NamedKind::Struct,
            ..
        } => struct_delta_type(program, (*module, *declaration), arguments, queries)?,
        Type::Parameter { name, .. } if queries.is_some() => format!(
            "::{}::delta_t<{}>",
            runtime::NAMESPACE,
            names::template_parameter(name)
        ),
        // A delta that is no type of its own is the type it is.
        other if Type::delta(other.clone()) != Type::Delta(Box::new(other.clone())) => {
            cpp_type_in(program, &Type::delta(other.clone()), queries)?
        }
        _ => return None,
    })
}

/// The collection an empty `[]` is where `ty` is expected: `ty`, or the
/// collection a nullable or an atomic of it holds.
fn collection_of(ty: &Type) -> &Type {
    match ty {
        Type::Nullable(inner) | Type::Atomic(inner) => collection_of(inner),
        other => other,
    }
}

/// True where `ty`, a dictionary's value or a list's element, is an
/// atomic or holds one as a dictionary's value or a list's element, at
/// any depth. An atomic's delta is the whole value (TYPE-16); the
/// library's delta of a dictionary or a list takes `hglrt::delta_t` of
/// its value's C++ type, which for the value an atomic holds is that
/// value's delta. A struct's delta is the target's own, which keeps it.
fn changes_whole(ty: &Type) -> bool {
    match ty {
        Type::Atomic(_) => true,
        Type::List { element, .. } => changes_whole(element),
        Type::Dict { value, .. } => changes_whole(value),
        _ => false,
    }
}

impl Emitter<'_> {
    /// `Kind(value)` (TYPE-7): "`Side(1)` is the member whose number is 1,
    /// and `Side("buy")` the member of that exact name ... a value that is
    /// none fails where it is evaluated". The catalogue has no code for
    /// that failure.
    pub(super) fn enum_from(
        &mut self,
        scope: &mut Scope,
        value: &Expr,
        (module, declaration): (usize, usize),
        from: &Expr,
    ) -> String {
        let Some(Detail::Enum(read)) = self.program.modules[module].details.get(declaration) else {
            return self.missing(value.at, "an enum's member made from a value");
        };
        self.note_enum(module, declaration);
        let kind = enum_name(self.program, module, declaration);
        let by_name = from.ty == Type::Scalar(hgl_analyser::Scalar::Str);
        let mut table = Vec::new();
        for member in &read.members {
            let member_name = format!("{kind}::{}", identifier(&member.name));
            if by_name {
                table.push(format!(
                    "{{{}, {member_name}}}",
                    names::string(&member.name)
                ));
            } else {
                let Some(number) = member.value else {
                    return self.missing(value.at, "an enum's member made from a value");
                };
                table.push(format!("{{{}, {member_name}}}", integer_literal(number)));
            }
        }
        let given = if by_name {
            "::hgraph::Str"
        } else {
            "::hgraph::Int"
        };
        let written = self.expression(scope, from);
        scope.acted = true;
        format!(
            "::{}::enum_member<{kind}, {given}>({written}, {{{}}})",
            runtime::NAMESPACE,
            table.join(", ")
        )
    }

    /// `left == right` or `left != right` where one is a `T?` and the
    /// other a `T` or a `T?` (TYPE-14: "A T is a value where a `T?` is
    /// expected"; OPS-2: "`==` and `!=` compare two values of one type"):
    /// two nullables are equal where both are null or both hold equal
    /// values. C++'s comparison of optionals is that. The two are
    /// evaluated in the order written (EXPR-2), each held as a value of its
    /// own where either runs statements or both may fail. `None` for
    /// operands of any other types.
    pub(super) fn nullable_equality(
        &mut self,
        scope: &mut Scope,
        equal: bool,
        left: &Expr,
        right: &Expr,
    ) -> Option<String> {
        let nullable_of = |nullable: &Type, other: &Type| matches!(nullable, Type::Nullable(payload) if **payload == *other || *nullable == *other);
        if !nullable_of(&left.ty, &right.ty) && !nullable_of(&right.ty, &left.ty) {
            return None;
        }
        let symbol = if equal { "==" } else { "!=" };
        // Either side running statements may change what the other reads,
        // a field of an abstract struct read by reference among it: both
        // are then held as values, in the order written.
        let ordered = changes(left)
            || changes(right)
            || (self.may_fail_here(left) && self.may_fail_here(right));
        let left_text = self.expression(scope, left);
        let right_text = self.expression(scope, right);
        if !ordered {
            return Some(format!("({left_text} {symbol} {right_text})"));
        }
        let left_type = self.written_type(&left.ty, left.at)?;
        let right_type = self.written_type(&right.ty, right.at)?;
        let first = names::temporary("left", self.temporary());
        let second = names::temporary("right", self.temporary());
        scope.acted = true;
        Some(format!(
            "[&] {{ const {left_type} {first} = {left_text}; const {right_type} {second} = {right_text}; return {first} {symbol} {second}; }}()"
        ))
    }
}

/// True where `ty` is a struct, or the delta of a set, a dictionary, a
/// nullable or a struct, whose field or property a read names in place,
/// so that a `let` of the read may refer to it where its base is one a
/// `let` may refer to (`statements::refers`). A list's delta's
/// `updated` and `removed` are made where they are read.
pub(super) fn field_refers(base: &Type, member: usize) -> bool {
    match base {
        Type::Named {
            kind: NamedKind::Struct,
            ..
        } => true,
        Type::Delta(of) => match &**of {
            Type::List { .. } => member == 2,
            Type::Set(_) | Type::Dict { .. } | Type::Nullable(_) => true,
            Type::Named {
                kind: NamedKind::Struct,
                ..
            } => true,
            _ => false,
        },
        _ => false,
    }
}
