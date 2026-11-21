//! `base.member`, `.member` and `base[index]`: a member of an enum, a
//! field of a struct, a part of a value, and an element of a list, a
//! tuple or a dictionary (TYPE-7, EXPR-7, ERR-6, ERR-12). Part of the
//! typing pass ([`super`]).

use hgl_diagnostics::{Category, Location};
use hgl_parser::{HasSpan, SyntaxKind, SyntaxNode, SyntaxToken};

use super::Typer;
use super::fit::{not_null, open};
use super::inference::{fixed, placed};
use crate::constant::Fault;
use crate::declared::substitute_within;
use crate::hir::ExprKind;
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
        let mut base = self.expression(&base, None);
        // ERR-7: a capture is "read through two connections".
        let captured = match &base.kind {
            ExprKind::Local(local) => self.captures.get(local).cloned(),
            ExprKind::Capture(call) => Some(self.captured_output(call)),
            _ => None,
        };
        if let Some(output) = captured {
            return self.captured(base, &name, output);
        }
        // What the member is read from, in the form that place takes it:
        // each form taken is read again for its member.
        loop {
            match &base.ty {
                // ERR-3: `.value` and `.error` "read from a result ...
                // are the result's own".
                Type::Result { value, error } if matches!(name.text(), "value" | "error") => {
                    let sides = ((**value).clone(), (**error).clone());
                    return self.payload(base, &name, sides);
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
                Type::Error => return self.error_part(base, &name),
                Type::ErrorContext => return self.context_part(base, &name),
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
                    return self.delta_property(base, &name, &of);
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
                .position(|property| property.name == name.text())
            else {
                self.diagnostics.emit(error(
                    Category::Name,
                    codes::MEMBER_UNKNOWN,
                    Location::new(self.file, name.span()),
                    format!("`{}` has no property `{}`", base.ty, name.text()),
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
        let Some(member) = read
            .members
            .iter()
            .position(|member| member.name == name.text())
        else {
            self.diagnostics.emit(error(
                Category::Name,
                codes::MEMBER_UNKNOWN,
                Location::new(self.file, name.span()),
                format!("`{}` has no field `{}`", base.ty, name.text()),
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
        let ty = if atomic { Type::atomic(ty) } else { ty };
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
        let base = self.received(base, |payload| {
            matches!(
                payload,
                Type::List { .. } | Type::Tuple(_) | Type::Dict { .. }
            )
        });
        let (index, ty) = match &base.ty {
            Type::List { element, .. } => {
                let integer = Type::Scalar(Scalar::I64);
                let index = self.expression(&index, Some(&integer));
                (
                    self.fit(index, &integer, "the index of a list"),
                    (**element).clone(),
                )
            }
            // EXPR-7: "A tuple's index is a constant within its length:
            // it selects the element's type."
            Type::Tuple(elements) => {
                let integer = Type::Scalar(Scalar::I64);
                // UNIT-13: an index that names a constant of a module is
                // that constant's value: the index is read as every
                // constant is (`crate::constant`).
                let found = self.read_constant(&index, Some(&integer));
                let index = self.expression(&index, Some(&integer));
                // EXPR-7: "by an `i64` index". An index of another type,
                // `pair[0 / 1]` or `pair[0 == 0]`, is reported as that,
                // and is not evaluated.
                let is_integer = self.admits(&integer, &index.ty);
                let index = self.fit(index, &integer, "the index of a tuple");
                let element = match found {
                    _ if !is_integer => Some(&Type::Unknown),
                    Ok(ConstantValue::Int(value)) => usize::try_from(value)
                        .ok()
                        .and_then(|position| elements.get(position)),
                    // No compile-time constant: a local, a parameter, a
                    // call.
                    Err(Fault::NotConstant) => None,
                    // A constant whose value is not known here, a
                    // generic value parameter's or one this pass does
                    // not work out, or one reported: the element's type
                    // is then not known, and nothing is reported of it.
                    _ => Some(&Type::Unknown),
                };
                let ty = match element {
                    Some(element) => element.clone(),
                    None => {
                        self.diagnostics.emit(error(
                            Category::Type,
                            codes::TUPLE_INDEX,
                            index.at,
                            format!(
                                "a tuple's index is a constant, here one of 0 to {}: it \
                                 selects the element's type",
                                elements.len().saturating_sub(1)
                            ),
                            "EXPR-7",
                        ));
                        Type::Unknown
                    }
                };
                (index, ty)
            }
            Type::Dict { key, value } => {
                let index = self.expression(&index, Some(key));
                (
                    self.fit(index, key, "the key of the dictionary"),
                    (**value).clone(),
                )
            }
            // EXPR-7: "Brackets read an element of a list or a tuple by
            // an `i64` index, and the value a dictionary has for a
            // key." EXPR-3: "A set has no place to assign."
            Type::Set(_) => {
                self.diagnostics.emit(error(
                    Category::Type,
                    codes::TYPE_MISMATCH,
                    base.at,
                    format!(
                        "this is `{}`, and a set has no element at an index: brackets read a \
                         list, a tuple, or a dictionary",
                        base.ty
                    ),
                    "EXPR-7",
                ));
                (self.expression(&index, Some(&Type::Unknown)), Type::Unknown)
            }
            _ => (self.expression(&index, Some(&Type::Unknown)), Type::Unknown),
        };
        let kind = ExprKind::Index {
            base: Box::new(base),
            index: Box::new(index),
        };
        (kind, ty)
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
