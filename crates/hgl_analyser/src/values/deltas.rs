//! The delta of a collection, of a nullable or of a struct as a type of
//! its own: its properties, and a delta constructed from them, held to the
//! bounds of its type and to naming no entry twice (TYPE-16). Part of the
//! typing pass ([`super`]).

use hgl_diagnostics::{Category, Location};
use hgl_parser::{HasSpan, SyntaxKind, SyntaxNode, SyntaxToken};

use super::Typer;
use super::fit::{nullable, open};
use super::inference::{fixed, placed};
use crate::declared::substitute_within;
use crate::hir::{self, Expr, ExprKind};
use crate::program::Program;
use crate::syntax::{child, ungrouped};
use crate::types::{Argument, ConstantValue, Detail, NamedKind, Scalar, Type};
use crate::{codes, error};

impl Program {
    /// The properties of the delta of `of`, each with its type, in
    /// the order TYPE-16 has them. `None` for a type whose delta is no
    /// type of its own, or is not known yet.
    ///
    /// "`set<K>`: ... the properties `added` and `removed`, each a
    /// `set<K>`"; "`dict<K, V>`: ... the properties `updated`, a
    /// `dict<K, delta<V>>` of each key added or changed with its delta,
    /// and `removed`, a `set<K>`"; "`V?`: ... an optional, with the
    /// property `value`, a `delta<V>?`: null for a published null"; "A
    /// struct: ... a property for each field `name: F`, of the same
    /// name and of type `delta<F>?`: null where the field did not
    /// change".
    pub(super) fn delta_properties(&self, of: &Type) -> Option<Vec<(String, Type)>> {
        // A wholly unknown payload does not yet say what shape its
        // delta has. A written generic parameter still has a symbolic delta.
        let property_delta = |ty: &Type| {
            if *ty == Type::Unknown {
                Type::Unknown
            } else {
                Type::delta(ty.clone())
            }
        };
        match of {
            Type::Set(_) => Some(vec![
                ("added".to_string(), of.clone()),
                ("removed".to_string(), of.clone()),
            ]),
            // "`list<V>`, `list<V, N>`: ... the properties `updated`, a
            // `dict<i64, delta<V>>` of each changed position with its
            // delta, `removed`, a `set<i64>` of the positions that lost
            // their value, and `size_modified`, an `i64?`".
            Type::List { element, .. } => {
                let position = || Box::new(Type::Scalar(Scalar::I64));
                Some(vec![
                    (
                        "updated".to_string(),
                        Type::Dict {
                            key: position(),
                            value: Box::new(property_delta(element)),
                        },
                    ),
                    ("removed".to_string(), Type::Set(position())),
                    (
                        "size_modified".to_string(),
                        nullable(Type::Scalar(Scalar::I64)),
                    ),
                ])
            }
            Type::Dict { key, value } => Some(vec![
                (
                    "updated".to_string(),
                    Type::Dict {
                        key: key.clone(),
                        value: Box::new(property_delta(value)),
                    },
                ),
                ("removed".to_string(), Type::Set(key.clone())),
            ]),
            Type::Nullable(payload) => Some(vec![(
                "value".to_string(),
                nullable(property_delta(payload)),
            )]),
            Type::Named {
                module,
                declaration,
                arguments,
                kind: NamedKind::Struct,
                ..
            } => {
                let Some(Detail::Struct(read)) = self.modules[*module].details.get(*declaration)
                else {
                    return None;
                };
                // Each field's type, with the struct's generic
                // arguments in place, as a field read has it.
                let map: Vec<(Location, &Argument)> =
                    fixed(&read.generics).zip(arguments).collect();
                let own: Vec<Location> = read.generics.iter().map(|generic| generic.at).collect();
                Some(
                    read.members
                        .iter()
                        .map(|member| {
                            let ty = placed(substitute_within(&member.ty, &map), &own);
                            // A field whose type is not known has a
                            // delta that is not known either.
                            let delta = if open(&ty) {
                                ty
                            } else {
                                nullable(Type::delta(ty))
                            };
                            (member.name.clone(), delta)
                        })
                        .collect(),
                )
            }
            _ => None,
        }
    }
}

impl<'a> Typer<'a> {
    /// A property of a delta that is a type of its own (TYPE-16):
    /// `change.added`, of a set's delta; `change.updated`, of a
    /// dictionary's; `change.value`, of a nullable's; `change.bid`, of
    /// a struct's.
    pub(super) fn delta_property(
        &mut self,
        base: Expr,
        name: &str,
        at: Location,
        of: &Type,
    ) -> (ExprKind, Type) {
        let Some(properties) = self.program.delta_properties(of) else {
            return (ExprKind::Unread, Type::Unknown);
        };
        let member = properties.iter().position(|(property, _)| property == name);
        let Some(member) = member else {
            self.diagnostics.emit(
                error(
                    Category::Name,
                    codes::MEMBER_UNKNOWN,
                    at,
                    format!("`{}` has no property `{}`", base.ty, name),
                    "TYPE-16",
                )
                .with_note(format!("it has {}", listed(&properties))),
            );
            return (ExprKind::Unread, Type::Unknown);
        };
        let ty = properties[member].1.clone();
        let kind = ExprKind::Field {
            base: Box::new(base),
            member,
        };
        (kind, ty)
    }

    /// `delta<set<i64>>(added: [1, 2], removed: [3])`: a delta made
    /// from its properties.
    ///
    /// TYPE-16: "One that is a type of its own is constructed by its
    /// properties, by name ... A property left out is empty, or null
    /// for a struct's field, a nullable's `value`, and a list's
    /// `size_modified`. A tuple's delta is an ordinary tuple, and is
    /// written as one is."
    ///
    /// A delta that is another type the language has, a scalar's, a
    /// tuple's or a window's, has no constructor: the owner on
    /// hhenson/hgl_spec#163, "`delta<T>(...)` for such a T rejects with
    /// `delta.argument_name`, the empty form included". Each property
    /// it names is reported, or the constructor where it names none. The
    /// constructor of a delta not known yet, a generic parameter's, is
    /// not read.
    pub(super) fn delta_constructor(&mut self, node: &SyntaxNode) -> (ExprKind, Type) {
        let Some(list) = child(node, SyntaxKind::ArgList) else {
            return (ExprKind::Unread, Type::Unknown);
        };
        // The delta is recorded where the constructor starts.
        let written = self.written_type_at(node.span().start);
        let properties = match written {
            Some(Type::Delta(of)) => self.program.delta_properties(of),
            _ => None,
        };
        if let Some(ty) = written
            && !matches!(ty, Type::Delta(_))
            && !open(ty)
        {
            let ty = ty.clone();
            self.no_delta_constructor(node, &list, &ty);
            return self.unread_call(&list);
        }
        let (Some(ty), Some(properties)) = (written, properties) else {
            return self.unread_call(&list);
        };
        let ty = ty.clone();
        let mut given = vec![false; properties.len()];
        let mut arguments = Vec::new();
        // Each property given, with its value as written and its type,
        // for the constants among its entries.
        let mut written = Vec::new();
        for (name, value) in self.call_arguments(&list) {
            let position = name.as_ref().and_then(|name| {
                properties
                    .iter()
                    .position(|(property, _)| property == name.text())
            });
            let Some(position) = position else {
                // A name that is no property of the delta. An argument
                // with no name has none to report.
                if let Some(name) = &name {
                    self.diagnostics.emit(
                        error(
                            Category::Name,
                            codes::DELTA_ARGUMENT_NAME,
                            Location::new(self.file, name.span()),
                            format!("`{ty}` has no property `{}`", name.text()),
                            "TYPE-16",
                        )
                        .with_note(format!("it has {}", listed(&properties))),
                    );
                }
                arguments.push(hir::Argument {
                    parameter: None,
                    pack: None,
                    value: self.expression(&value, Some(&Type::Unknown)),
                });
                continue;
            };
            if std::mem::replace(&mut given[position], true)
                && let Some(name) = &name
            {
                self.diagnostics.emit(error(
                    Category::Name,
                    codes::DELTA_DUPLICATE_ARGUMENT,
                    Location::new(self.file, name.span()),
                    format!("`{}` is given twice", name.text()),
                    "TYPE-16",
                ));
            }
            let (property, expected) = &properties[position];
            let read = self.expression(&value, Some(expected));
            let place = format!("the property `{property}`");
            arguments.push(hir::Argument {
                parameter: Some(position),
                pack: None,
                value: self.fit(read, expected, &place),
            });
            written.push((position, value, expected.clone()));
        }
        self.delta_overlap(&ty, &written);
        self.delta_bounds(&ty, &written);
        (ExprKind::Delta { arguments }, ty)
    }

    /// Reports the constructor at `node`, whose arguments are `list`, of
    /// a delta that is `ty`, another type the language has: at each
    /// property it names, or at the constructor where it names none
    /// (TYPE-16, `delta.argument_name`).
    fn no_delta_constructor(&mut self, node: &SyntaxNode, list: &SyntaxNode, ty: &Type) {
        let names: Vec<SyntaxToken> = self
            .call_arguments(list)
            .into_iter()
            .filter_map(|(name, _)| name)
            .collect();
        let places = if names.is_empty() {
            vec![(Location::new(self.file, node.span()), None)]
        } else {
            names
                .iter()
                .map(|name| {
                    (
                        Location::new(self.file, name.span()),
                        Some(name.text().to_string()),
                    )
                })
                .collect()
        };
        for (at, property) in places {
            let message = match property {
                Some(property) => format!(
                    "this delta is `{ty}`, another type the language has, which has no \
                     constructor and no property `{property}`"
                ),
                None => format!(
                    "this delta is `{ty}`, another type the language has, which has no \
                     constructor"
                ),
            };
            self.diagnostics.emit(
                error(
                    Category::Name,
                    codes::DELTA_ARGUMENT_NAME,
                    at,
                    message,
                    "TYPE-16",
                )
                .with_note(format!("a `{ty}` is written as its type writes it")),
            );
        }
    }

    /// The entries of the properties `written` gives at `position` that
    /// are written out, `[a, b]` or `[k: v]`: each member, or each key,
    /// with what it is as a constant of the property's element or key
    /// type (`crate::constant`). One that is no constant is known only
    /// as the program runs, and is left out.
    fn delta_entries(
        &self,
        written: &[(usize, SyntaxNode, Type)],
        position: usize,
    ) -> Vec<(Location, ConstantValue)> {
        let mut entries = Vec::new();
        for (_, value, ty) in written.iter().filter(|(at, _, _)| *at == position) {
            let element = match ty {
                Type::Set(element) | Type::List { element, .. } => element.as_ref(),
                Type::Dict { key, .. } => key.as_ref(),
                _ => continue,
            };
            let value = ungrouped(value.clone());
            if value.kind() != SyntaxKind::SequenceExpr {
                continue;
            }
            for entry in value.children() {
                // `key: value`: the key is the first child.
                let entry = match entry.kind() {
                    SyntaxKind::SequencePair => match entry.children().next() {
                        Some(key) => key,
                        None => continue,
                    },
                    _ => entry,
                };
                if let Ok(constant) = self.read_constant(&entry, Some(element)) {
                    entries.push((self.at(&entry), constant));
                }
            }
        }
        entries
    }

    /// TYPE-16: "A `list<V, N>` keeps its bounds: a constant entry past
    /// them rejects (`delta.index_bounds`)". The catalogue's row is
    /// "Negative/out-of-fixed-range index": a constant position of any
    /// list's delta, `updated` or `removed`, that is negative rejects
    /// too. A position known only as the program runs is checked then.
    fn delta_bounds(&mut self, ty: &Type, written: &[(usize, SyntaxNode, Type)]) {
        let Type::Delta(of) = ty else {
            return;
        };
        let Type::List { size, .. } = &**of else {
            return;
        };
        let size = match size.as_ref().map(|size| &size.value) {
            Some(ConstantValue::Int(size)) => Some(*size),
            _ => None,
        };
        // A list's `updated` is keyed by position, and its `removed`
        // holds positions.
        let mut outside = Vec::new();
        for position in [0, 1] {
            for (at, value) in self.delta_entries(written, position) {
                if let ConstantValue::Int(value) = value
                    && (value < 0 || size.is_some_and(|size| value >= size))
                {
                    outside.push((at, value));
                }
            }
        }
        for (at, value) in outside {
            let bounds = match size {
                Some(0) => "it has none".to_string(),
                Some(size) => format!("its positions are 0 to {}", size - 1),
                None => "a position is 0 or more".to_string(),
            };
            self.diagnostics.emit(error(
                Category::Type,
                codes::DELTA_INDEX_BOUNDS,
                at,
                format!("`{of}` has no position {value}: {bounds}"),
                "TYPE-16",
            ));
        }
    }

    /// TYPE-16: "A delta that names one member, key or position both as
    /// added or updated and as removed is malformed. A constant one is
    /// rejected (`delta.overlap`)." A set's `added`, a dictionary's and
    /// a list's `updated`, are the first property, and `removed` the
    /// second. What is compared is each entry written as a literal; one
    /// that is not is known only as the graph runs, which "makes no
    /// check". The later of the two entries is reported.
    fn delta_overlap(&mut self, ty: &Type, written: &[(usize, SyntaxNode, Type)]) {
        let Type::Delta(of) = ty else {
            return;
        };
        if !matches!(**of, Type::Set(_) | Type::Dict { .. } | Type::List { .. }) {
            return;
        }
        // The constant a member, a key or a position is, compared as the
        // language's `==` compares it (`ConstantValue::equals`), as the
        // collection does when the graph runs: `-1`, an enum's member
        // however it is written, a module's constant, `60s` and `1m`
        // alike.
        let removed = self.delta_entries(written, 1);
        let changed = self.delta_entries(written, 0);
        for (changed, value) in changed {
            let clash = removed
                .iter()
                .find(|(_, other)| other.equals(&value) == Some(true))
                .map(|(gone, _)| *gone);
            if let Some(gone) = clash {
                let later = if gone.span.start > changed.span.start {
                    gone
                } else {
                    changed
                };
                self.diagnostics.emit(
                    error(
                        Category::Type,
                        codes::DELTA_OVERLAP,
                        later,
                        format!("`{of}`'s delta names this both as changed and as removed"),
                        "TYPE-16",
                    )
                    .with_note("a delta that names one entry both ways is malformed"),
                );
            }
        }
    }
}

/// The names of a delta's properties, for a message: `` `added` and
/// `removed` ``.
fn listed(properties: &[(String, Type)]) -> String {
    let names: Vec<String> = properties
        .iter()
        .map(|(name, _)| format!("`{name}`"))
        .collect();
    match names.as_slice() {
        [] => "no property".to_string(),
        [only] => only.clone(),
        [first @ .., last] => format!("{} and {last}", first.join(", ")),
    }
}
