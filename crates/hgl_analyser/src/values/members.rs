//! `base.member`, `.member` and `base[index]`: a member of an enum, a
//! field of a struct, a part of a value, and an element of a list, a
//! tuple or a dictionary (TYPE-7, EXPR-7, ERR-6, ERR-12). Part of the
//! typing pass ([`super`]).

use hgl_diagnostics::{Category, Location};
use hgl_parser::{HasSpan, SyntaxKind, SyntaxNode, SyntaxToken};

use super::Typer;
use super::fit::{not_null, open, whole};
use super::forms::received_type;
use super::inference::{fixed, placed};
use super::results::captured_output;
use crate::constant::Fault;
use crate::declared::substitute_within;
use crate::hir::{Expr, ExprKind, Value, ValueCategory};
use crate::program::DeclarationKind;
use crate::syntax::{name_token, tokens};
use crate::types::{Argument, ConstantValue, Detail, NamedKind, Scalar, Type};
use crate::{codes, error};

impl<'a> Typer<'a> {
    // ---- Members and elements ----

    /// `base.member`: a member of an enum, or a field of a struct
    /// value. A result and a `T?` are read for the T they hold (ERR-12,
    /// ERR-15). Any other base is for a later part.
    pub(super) fn member(&mut self, node: &SyntaxNode) -> (ExprKind, Type) {
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
        // ERR-7: a capture is "read through two connections".
        let captured = match &base.kind {
            ExprKind::Local(local) | ExprKind::DeferredPresent(local) => {
                self.captures.get(local).cloned()
            }
            ExprKind::Capture(call) => Some(captured_output(program, call)),
            _ => None,
        };
        if let Some(output) = captured {
            return self.captured(base, &name, output);
        }
        let at = Location::new(self.file, name.span());
        let variant_proven = self.payload_proven(&base, name.text());
        if self.graph
            && !matches!(base.kind, ExprKind::Unread)
            && not_null(Some(&base.ty)).is_some_and(|ty| match ty {
                Type::Unknown => true,
                // ERR-15 cannot take the success until the result
                // settles. Explicit payloads still use ERR-3 here.
                Type::Result { .. } if !matches!(name.text(), "value" | "error") => !whole(ty),
                _ => false,
            })
        {
            return (
                ExprKind::DeferredMember {
                    base: Box::new(base),
                    name: name.text().to_owned(),
                    at,
                    variant_proven,
                },
                Type::Unknown,
            );
        }
        self.member_value(base, name.text(), at, variant_proven)
    }

    /// Replay a projection when its input becomes concrete. A saved
    /// ERR-3 guard can prove a variant only of an ordinary result.
    pub(super) fn settle_member(
        &mut self,
        base: &Expr,
        name: &str,
        at: Location,
        mut variant_proven: bool,
    ) -> Option<(ExprKind, Type)> {
        if !whole(&base.ty) {
            return None;
        }
        if variant_proven && matches!(base.ty, Type::Result { .. }) {
            match self.category(base) {
                ValueCategory::Unsettled => return None,
                ValueCategory::Connection => variant_proven = false,
                ValueCategory::Value => {}
            }
        }
        Some(self.member_value(base.clone(), name, at, variant_proven))
    }

    /// Resolve a member against a known value, retaining its implicit reads.
    fn member_value(
        &mut self,
        mut base: Expr,
        name: &str,
        at: Location,
        variant_proven: bool,
    ) -> (ExprKind, Type) {
        let program = self.program;
        // What the member is read from, in the form that place takes it:
        // each form taken is read again for its member.
        loop {
            match &base.ty {
                // ERR-3: `.value` and `.error` "read from a result ...
                // are the result's own".
                Type::Result { value, error } if matches!(name, "value" | "error") => {
                    let sides = ((**value).clone(), (**error).clone());
                    // An implicit read cannot transfer a binding's fact.
                    let variant_proven = variant_proven && matches!(base.kind, ExprKind::Local(_));
                    return self.payload(base, name, at, sides, variant_proven);
                }
                // ERR-15: "What a member or an index is read from ...
                // expect[s] the T too": `checked(quote).bid` is the bid of
                // the success, or raises the failure.
                Type::Result { .. } => {
                    let read = self.received(base, |_| false);
                    if matches!(read.ty, Type::Result { .. }) {
                        return (ExprKind::Unread, Type::Unknown);
                    }
                    base = read;
                }
                // ERR-4: a part of an error, and of its context.
                Type::Error => return self.error_part(base, name, at),
                Type::ErrorContext => return self.context_part(base, name, at),
                // ERR-12: "A `T?` value given where a T is expected is
                // read as its value": a member of a nullable's payload
                // reads it, where no test against null has proved it a
                // T. ERR-4: "A nullable code or context read where a
                // value is expected is read as ERR-12 says."
                Type::Nullable(inner) if has_members(inner) => {
                    base = self.received(base, has_members);
                }
                // TYPE-16: a set's delta has "the properties `added` and
                // `removed`, each a `set<K>`".
                Type::Delta(of) => {
                    let of = of.clone();
                    return self.delta_property(base, name, at, &of);
                }
                _ => break,
            }
        }
        // TYPE-9: "`atomic<V>` stops expansion at a complete ordinary
        // V": an `atomic<Quote>` holds whole quotes, and its field is
        // the field of each, itself an atomic of the field's type, as
        // complete as the quote it is read from.
        let (whole, atomic) = match &base.ty {
            Type::Atomic(payload) => (&**payload, true),
            other => (other, false),
        };
        let Type::Named {
            module,
            declaration,
            arguments,
            kind,
            ..
        } = whole
        else {
            return (ExprKind::Unread, Type::Unknown);
        };
        if atomic && *kind != NamedKind::Struct {
            return (ExprKind::Unread, Type::Unknown);
        }
        // NATIVE-6: "`value.property` reads one as an ordinary value."
        if let Some(Detail::Native(native)) = program.modules[*module].details.get(*declaration) {
            let Some(member) = native
                .properties
                .iter()
                .position(|property| property.name == name)
            else {
                self.diagnostics.emit(error(
                    Category::Name,
                    codes::MEMBER_UNKNOWN,
                    at,
                    format!("`{}` has no property `{name}`", base.ty),
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
        let Some(member) = read.members.iter().position(|member| member.name == name) else {
            self.diagnostics.emit(error(
                Category::Name,
                codes::MEMBER_UNKNOWN,
                at,
                format!("`{}` has no field `{name}`", base.ty),
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
        let ty = if atomic {
            // TYPE-9: an inferred field may become a scalar. Where
            // even its shape is open, wait to decide whether it is atomic.
            match &ty {
                Type::Unknown => Type::Unknown,
                Type::Nullable(payload) | Type::Delta(payload) if **payload == Type::Unknown => {
                    Type::Unknown
                }
                _ => Type::atomic(ty),
            }
        } else {
            ty
        };
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
            kind: NamedKind::Enum,
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
    pub(super) fn implied_member(
        &mut self,
        node: &SyntaxNode,
        expected: Option<&Type>,
    ) -> (ExprKind, Type) {
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
    pub(super) fn index(&mut self, node: &SyntaxNode) -> (ExprKind, Type) {
        let mut parts = node.children();
        let (Some(base), Some(index)) = (parts.next(), parts.next()) else {
            return (ExprKind::Unread, Type::Unknown);
        };
        let base = self.expression(&base, None);
        // ERR-15, ERR-12: what an index is read from expects the T: a
        // result's, and a nullable list's, tuple's or dictionary's.
        let base = self.received(base, index_collection);
        self.check_index_base(&base.ty, base.at);
        let (index, ty) = match &base.ty {
            Type::List { .. } | Type::Dict { .. } => {
                let (expected, place) =
                    index_operand(&base.ty).expect("a list or dictionary index");
                let index = self.expression(&index, Some(expected));
                let ty = index_result(&base.ty, None)
                    .cloned()
                    .unwrap_or(Type::Unknown);
                (self.fit(index, expected, place), ty)
            }
            Type::Tuple(_) => {
                let (integer, place) = index_operand(&base.ty).expect("a tuple index");
                let mut value = self.expression(&index, Some(integer));
                let ty = self.tuple_index(&base.ty, &mut value, Some(&index), true);
                (self.fit(value, integer, place), ty)
            }
            _ => (self.expression(&index, Some(&Type::Unknown)), Type::Unknown),
        };
        let kind = ExprKind::Index {
            base: Box::new(base),
            index: Box::new(index),
        };
        (kind, ty)
    }

    /// EXPR-7: brackets read a list, tuple or dictionary. A set has no
    /// element at an index, and no indexed place to assign (EXPR-3).
    pub(super) fn check_index_base(&mut self, ty: &Type, at: Location) {
        if matches!(ty, Type::Set(_)) {
            self.diagnostics.emit(error(
                Category::Type,
                codes::TYPE_MISMATCH,
                at,
                format!(
                    "this is `{ty}`, and a set has no element at an index: brackets read a \
                     list, a tuple, or a dictionary"
                ),
                "EXPR-7",
            ));
        }
    }

    /// EXPR-7: a tuple index is a constant within its length. Read it
    /// as UNIT-13 reads other constants and retain the folded position.
    /// Type settlement can recover the type without reporting until
    /// the instance checks its destinations.
    pub(super) fn tuple_index(
        &mut self,
        base: &Type,
        index: &mut Expr,
        written: Option<&SyntaxNode>,
        report: bool,
    ) -> Type {
        let Type::Tuple(elements) = base else {
            return Type::Unknown;
        };
        let (integer, _) = index_operand(base).expect("a tuple index");
        // A wrong index type is reported by the normal fit.
        if !self.admits(integer, &index.ty) {
            return Type::Unknown;
        }
        let found = match index.kind {
            ExprKind::Literal(Value::Int(position)) => Ok(ConstantValue::Int(position)),
            _ => written
                .cloned()
                .or_else(|| self.syntax_of(index))
                .map_or(Err(Fault::Reported), |node| {
                    self.read_constant(&node, Some(integer))
                }),
        };
        match found {
            Ok(ConstantValue::Int(position)) => {
                if let Some(ty) = index_result(base, Some(position)) {
                    index.kind = ExprKind::Literal(Value::Int(position));
                    index.ty = integer.clone();
                    return ty.clone();
                }
            }
            Err(Fault::NotConstant) => {}
            // A value not known yet, a failed constant or a reported
            // fault does not establish the type of the element.
            _ => return Type::Unknown,
        }
        if report {
            self.diagnostics.emit(error(
                Category::Type,
                codes::TUPLE_INDEX,
                index.at,
                format!(
                    "a tuple's index is a constant, here one of 0 to {}: it selects the element's type",
                    elements.len().saturating_sub(1)
                ),
                "EXPR-7",
            ));
        }
        Type::Unknown
    }
}

/// EXPR-7: the collection kinds read by brackets, including through a `T?`.
pub(super) fn index_collection(ty: &Type) -> bool {
    matches!(ty, Type::List { .. } | Type::Tuple(_) | Type::Dict { .. })
}

/// ERR-12 and ERR-15: the type an index sees after the same implicit
/// reads as `Typer::received`. Settlement uses it before inserting reads.
pub(super) fn indexed_payload(ty: &Type) -> &Type {
    received_type(ty, index_collection)
}

/// EXPR-7: what type a bracket index must have for each collection.
pub(super) fn index_operand(ty: &Type) -> Option<(&Type, &'static str)> {
    const INTEGER: &Type = &Type::Scalar(Scalar::I64);
    match ty {
        Type::List { .. } => Some((INTEGER, "the index of a list")),
        Type::Tuple(_) => Some((INTEGER, "the index of a tuple")),
        Type::Dict { key, .. } => Some((key, "the key of the dictionary")),
        _ => None,
    }
}

/// EXPR-7: the result of indexing an already selected collection type.
pub(super) fn index_result(ty: &Type, position: Option<i64>) -> Option<&Type> {
    match ty {
        Type::List { element, .. } => Some(element),
        Type::Dict { value, .. } => Some(value),
        Type::Tuple(elements) => elements.get(usize::try_from(position?).ok()?),
        _ => None,
    }
}

/// True when a value of `ty` has members that `value.member` reads: a
/// struct's fields, a native type's properties, a result's payloads, an
/// error's and its context's parts, and a delta's properties. An
/// `atomic<S>` under a `?` is an input that may be omitted, and is not
/// one (TYPE-14).
fn has_members(ty: &Type) -> bool {
    matches!(
        ty,
        Type::Named {
            kind: NamedKind::Struct | NamedKind::Native,
            ..
        } | Type::Result { .. }
            | Type::Error
            | Type::ErrorContext
            | Type::Delta(_)
    )
}
