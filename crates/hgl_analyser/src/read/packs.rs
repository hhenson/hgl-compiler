//! Reads the declarations and types of dependent call packs (FN-8, GEN-17).

use hgl_diagnostics::{Category, Location};
use hgl_parser::{HasSpan, SyntaxKind, SyntaxNode, SyntaxToken};

use super::declarations::cardinality;
use super::{Reader, type_child};
use crate::constant::{self, Fault};
use crate::program::{Binding, DependentPack, LocalKind};
use crate::syntax::{child, name_token, pack_for_names, pack_member_names};
use crate::types::{Constant, GenericKind, Parameter, ReflectedPack, Scalar, Type};
use crate::{codes, error};

impl Reader<'_> {
    /// GEN-17: the named pack in a function type names a generic type pack.
    pub(super) fn reflected_pack(
        &mut self,
        input: &SyntaxNode,
        pack: &SyntaxNode,
    ) -> Option<ReflectedPack> {
        let written = type_child(pack)?;
        if written.kind() == SyntaxKind::NamedType
            && written.children().next().is_none()
            && let Some(name) = name_token(&written)
            && let Some(at) = self.pack_binding(&name)
        {
            return Some(ReflectedPack {
                at,
                name: name.text().to_string(),
                bounds: cardinality(child(input, SyntaxKind::Cardinality)),
            });
        }
        self.diagnostics.emit(error(
            Category::Type,
            codes::PACK_UNDECLARED,
            self.at(written.span()),
            "the function type's named pack must name a generic type pack".to_string(),
            "GEN-17",
        ));
        None
    }

    /// FN-8: `for` names a function parameter and `without` names one or
    /// more generic value parameters supplying a name or set of names.
    pub(super) fn dependent_pack(
        &mut self,
        clause: &SyntaxNode,
        parameters: &[Parameter],
    ) -> Option<DependentPack> {
        let (function, exclusions) = pack_for_names(clause)?;
        let binding = self.binding(&function)?;
        let function_at = match binding {
            Binding::Local {
                at,
                kind: LocalKind::Parameter,
            } if parameters.iter().any(|parameter| {
                parameter.at == *at && matches!(parameter.ty, Type::Function(_))
            }) =>
            {
                *at
            }
            _ => {
                self.pack_type_error(
                    &function,
                    "must name a parameter that takes a function",
                    "FN-8",
                );
                return None;
            }
        };
        let mut without = Vec::new();
        for name in exclusions {
            let binding = self.binding(&name)?;
            if let Binding::Local {
                at,
                kind: LocalKind::GenericParameter,
            } = binding && self.value_type(*at).is_some_and(|ty| {
                *ty == Type::Scalar(Scalar::Str)
                    || matches!(ty, Type::Set(element) if **element == Type::Scalar(Scalar::Str))
            }) {
                without.push(*at);
            } else {
                self.pack_type_error(
                    &name,
                    "must name a generic value parameter of type `str` or `set<str>`",
                    "FN-8",
                );
                return None;
            }
        }
        Some(DependentPack {
            function: function_at,
            without,
        })
    }

    /// The parser keeps the ordinary index tree for an ambiguous
    /// operand. It denotes a type here when its base names a type pack.
    pub(super) fn indexed_pack(&self, node: &SyntaxNode) -> bool {
        node.kind() == SyntaxKind::IndexExpr
            && pack_member_names(node).is_some_and(|(pack, _)| self.pack_binding(&pack).is_some())
    }

    /// GEN-17: a key by itself in a constraint is a value, as in
    /// `key in arg_keys(As)`. The parser reads such a bare name as a type.
    pub(super) fn constraint_pack_key(&self, node: &SyntaxNode) -> bool {
        node.kind() == SyntaxKind::NamedType
            && node.children().next().is_none()
            && node.parent().is_some_and(|parent| {
                matches!(
                    parent.kind(),
                    SyntaxKind::ConstraintRelation | SyntaxKind::ConstraintSet
                )
            })
            && name_token(node).is_some_and(|token| {
                matches!(
                    self.binding(&token),
                    Some(Binding::Local {
                        kind: LocalKind::PackKey,
                        ..
                    })
                )
            })
    }

    /// GEN-17: before specialization a member type carries its pack
    /// binding and key. The key's value can be a name or a position.
    pub(super) fn pack_member(&mut self, node: &SyntaxNode) -> Type {
        let Some((name, key)) = pack_member_names(node) else {
            return Type::Unknown;
        };
        let Some(pack) = self.pack_binding(&name) else {
            if self.binding(&name).is_some() {
                self.diagnostics.emit(error(
                    Category::Type,
                    codes::TYPE_NOT_A_TYPE,
                    self.at(name.span()),
                    format!("`{}` does not name a type pack", name.text()),
                    "GEN-17",
                ));
            }
            return Type::Unknown;
        };
        let read = self.constant_scope(|scope| constant::named(&key, scope));
        let value = read.and_then(|read| {
            let wrong_type = read.ty().is_some_and(|ty| {
                !matches!(
                    ty,
                    Type::Scalar(Scalar::Str | Scalar::I64)
                        | Type::Parameter { .. }
                        | Type::PackMember { .. }
                        | Type::Unknown
                )
            });
            let generic_type = matches!(
                self.binding(&key),
                Some(Binding::Local { at, kind: LocalKind::GenericParameter })
                    if self.value_type(*at).is_none()
            );
            if wrong_type || generic_type {
                return Err(Fault::NotConstant);
            }
            constant::fit(read, None, self.program)
        });
        match value {
            Ok(value) => Type::PackMember {
                pack,
                name: name.text().to_string(),
                key: Constant {
                    text: key.text().to_string(),
                    value,
                },
            },
            Err(Fault::Reported) => Type::Unknown,
            Err(_) => {
                self.pack_type_error(
                    &key,
                    "must name a compile-time `str` or `i64` pack key",
                    "GEN-17",
                );
                Type::Unknown
            }
        }
    }

    fn binding(&self, name: &SyntaxToken) -> Option<&Binding> {
        self.bindings.get(&(self.file, name.span().start)).copied()
    }

    fn pack_binding(&self, name: &SyntaxToken) -> Option<Location> {
        let Binding::Local {
            at,
            kind: LocalKind::GenericParameter,
        } = self.binding(name)?
        else {
            return None;
        };
        self.generics
            .iter()
            .any(|generic| generic.at == *at && matches!(generic.kind, GenericKind::Pack { .. }))
            .then_some(*at)
    }

    fn pack_type_error(&mut self, name: &SyntaxToken, expects: &str, rule: &'static str) {
        self.diagnostics.emit(error(
            Category::Type,
            codes::TYPE_MISMATCH,
            self.at(name.span()),
            format!("`{}` {expects}", name.text()),
            rule,
        ));
    }
}
