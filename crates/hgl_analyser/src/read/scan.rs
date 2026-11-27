//! Finds written types in syntax trees and distinguishes constructor uses.

use hgl_diagnostics::Location;
use hgl_parser::{HasSpan, SyntaxKind, SyntaxNode};
use hgl_source::Span;

use crate::program::{Binding, DeclarationKind, LocalKind, Role};
use crate::syntax::{child, name_token, tokens};
use crate::types::{CallArguments, DeltaInput, Type};

use super::declarations::{Kind, is_configuration_parameter};
use super::{Id, Position, Reader, is_const, is_type, names_before_colon, parameters};

impl Reader<'_> {
    // ---- Finding the types ----

    /// Reads every type written in `root` that has not been read.
    ///
    /// `descendants` walks the whole tree under `root` without calling
    /// itself, so a deep tree costs no call stack here.
    pub(super) fn scan(&mut self, root: &SyntaxNode) {
        for node in root.descendants() {
            match node.kind() {
                kind if is_type(kind) => {
                    // A type inside another is read with it, and the
                    // arguments of a constructor with the constructor.
                    let inside = node.parent().is_some_and(|parent| {
                        is_type(parent.kind()) || parent.kind() == SyntaxKind::GenericArgs
                    });
                    // A const option compared in a constraint is a
                    // value: no type is written there. So is a constant
                    // compared with one (UNIT-13).
                    let category = node.parent().is_some_and(|parent| {
                        parent.kind() == SyntaxKind::ConstraintRelation
                            && parent
                                .children()
                                .any(|child| child.kind() == SyntaxKind::ConstraintCategory)
                    });
                    if !inside
                        && (category || !self.names_an_option(&node))
                        && !self.compared_constant(&node)
                        && !self.constraint_pack_key(&node)
                        // GEN-18: this operand names a formal function,
                        // rather than writing a type.
                        && !node.parent().is_some_and(|parent| {
                            crate::laws::requirement(self.module, self.file, &parent).is_some()
                        })
                    {
                        self.outermost(&node);
                    }
                }
                SyntaxKind::IndexExpr
                    if node.parent().is_some_and(|parent| {
                        matches!(
                            parent.kind(),
                            SyntaxKind::ConstraintRelation
                                | SyntaxKind::ConstraintSet
                                | SyntaxKind::ConstraintCall
                        )
                    }) && self.indexed_pack(&node) =>
                {
                    self.outermost(&node);
                }
                SyntaxKind::TypedConstructor | SyntaxKind::FunctionReference => {
                    self.constructor(&node);
                }
                SyntaxKind::UndecidedConstructor => {
                    // GEN-14: the resolver decided what the node is.
                    if self.constructors.contains(&(self.file, node.span().start)) {
                        self.constructor(&node);
                    }
                }
                SyntaxKind::DeltaConstructor => self.delta_constructor(&node),
                SyntaxKind::Instantiation => {
                    // GEN-8: `add<i64, _>` requests candidates. What the
                    // arguments are checked against is for the pass that
                    // selects them.
                    if let Some(list) = child(&node, SyntaxKind::GenericArgs) {
                        self.arguments(&list, Position::OPEN, None, None, false);
                    }
                }
                SyntaxKind::ConstraintRelation
                    if node
                        .children()
                        .any(|child| child.kind() == SyntaxKind::ConstraintCategory) =>
                {
                    if let Some(left) = node.children().next()
                        && !is_type(left.kind())
                        && !(left.kind() == SyntaxKind::IndexExpr && self.indexed_pack(&left))
                    {
                        self.diagnostics.emit(crate::error(
                            hgl_diagnostics::Category::Type,
                            crate::codes::TYPE_NOT_A_TYPE,
                            self.at(left.span()),
                            "a category membership needs a type".to_string(),
                            "GEN-3",
                        ));
                    }
                }
                _ => {}
            }
        }
    }

    /// TIME-8 admits a capability type listed directly in the type set
    /// of an operator declaration's membership (GEN-3).
    pub(super) fn operator_type_member(&self, node: &SyntaxNode) -> bool {
        let Some(set) = node
            .parent()
            .filter(|parent| parent.kind() == SyntaxKind::ConstraintSet)
        else {
            return false;
        };
        let Some(relation) = set
            .parent()
            .filter(|parent| parent.kind() == SyntaxKind::ConstraintRelation)
        else {
            return false;
        };
        let mut operands = relation.children();
        let (Some(left), Some(right)) = (operands.next(), operands.next()) else {
            return false;
        };
        right == set
            && tokens(&relation)
                .any(|token| token.kind() == SyntaxKind::Ident && token.text() == "in")
            && relation
                .ancestors()
                .find(|ancestor| ancestor.kind() == SyntaxKind::RequiresClause)
                .and_then(|clause| clause.parent())
                .is_some_and(|declaration| declaration.kind() == SyntaxKind::OperatorDecl)
            && self.constraint_type(&left)
            && set.children().all(|member| self.constraint_type(&member))
    }

    /// Constraint operands share a parser form for type and value names.
    fn constraint_type(&self, operand: &SyntaxNode) -> bool {
        (is_type(operand.kind()) || self.indexed_pack(operand))
            && !self.names_a_generic_value(operand)
            && !self.names_a_constant(operand)
            && !self.names_an_option(operand)
            && !self.constraint_pack_key(operand)
    }

    /// True when `node`, which the parser read as a type, names a const
    /// option that a constraint compares with constants.
    ///
    /// GEN-6: "A candidate states in `requires` the values of a const
    /// option it is for, by comparing the option with constants:
    /// `requires zero == .error`, `requires zero in {.ieee, .nan}`." A
    /// lone name among a constraint's operands may be a type or a
    /// value, which the parser cannot tell, so it guesses a type. Here
    /// the name is known: it is the option when it is a parameter
    /// marked `const`, or a parameter of a value candidate that its
    /// operator marks `const` or does not declare at all ("Every
    /// parameter of a value candidate is a value, so it writes the
    /// option with no marker").
    ///
    /// Any other parameter is left to be read as the type it is not,
    /// and is reported: GEN-3, "It cannot inspect temporal values".
    fn names_an_option(&self, node: &SyntaxNode) -> bool {
        // `zero`, and neither `pc::zero` nor `zero<i64>`.
        if node.kind() != SyntaxKind::NamedType
            || child(node, SyntaxKind::GenericArgs).is_some()
            || child(node, SyntaxKind::QualifiedName).is_some()
        {
            return false;
        }
        // An operand of `==` or `in`; `is` compares types. What it is
        // compared with is constants: no type, and no set that holds
        // one. `zero == i64` compares no option.
        // A constant of a module is a value there too (UNIT-13: "A
        // constant is read by its name wherever an ordinary value is").
        let value =
            |operand: &SyntaxNode| !is_type(operand.kind()) || self.names_a_constant(operand);
        let compared = node.parent().is_some_and(|relation| {
            relation.kind() == SyntaxKind::ConstraintRelation
                && tokens(&relation).any(|token| {
                    token.kind() == SyntaxKind::EqEq
                        || (token.kind() == SyntaxKind::Ident && token.text() == "in")
                })
                && relation.children().filter(|operand| operand != node).all(
                    |operand| match operand.kind() {
                        SyntaxKind::ConstraintSet => {
                            operand.children().all(|member| value(&member))
                        }
                        _ => value(&operand),
                    },
                )
        });
        if !compared {
            return false;
        }
        let Some(name) = name_token(node) else {
            return false;
        };
        let Some(Binding::Local {
            at,
            kind: LocalKind::Parameter,
        }) = self.bindings.get(&(self.file, name.span().start)).copied()
        else {
            return false;
        };
        // The declaration the constraint belongs to, and the parameter
        // of it the name is bound to.
        let Some(declaration) = node.ancestors().find(|ancestor| {
            matches!(
                ancestor.kind(),
                SyntaxKind::FnDecl | SyntaxKind::NativeFnDecl | SyntaxKind::OperatorDecl
            )
        }) else {
            return false;
        };
        let Some(parameter) = parameters(&declaration).into_iter().find(|parameter| {
            names_before_colon(parameter)
                .last()
                .is_some_and(|declared| declared.span().start == at.span.start)
        }) else {
            return false;
        };
        // FN-18: a configuration parameter, including a `const` one,
        // is only read by the body. Requirements read generic values.
        if is_configuration_parameter(&parameter) {
            return false;
        }
        if is_const(&parameter) {
            return true;
        }
        // A value candidate. GEN-6: "Every parameter of a value
        // candidate is a value, so it writes the option with no
        // marker." Of a parameter its operator declares, the mark is
        // the operator's. One the operator does not declare is the
        // candidate's own option: a candidate "may declare further
        // const options of its own".
        let Some(operator) = self.option_owner else {
            return false;
        };
        let operator = &self.program.modules[operator.module].declarations[operator.declaration];
        let declared = parameters(&operator.syntax).into_iter().find(|parameter| {
            names_before_colon(parameter)
                .last()
                .is_some_and(|declared| declared.text() == name.text())
        });
        match declared {
            Some(parameter) => !is_configuration_parameter(&parameter) && is_const(&parameter),
            None => true,
        }
    }

    /// True when `node`, which the parser read as a type, names a
    /// constant of a module (UNIT-13).
    fn names_a_constant(&self, node: &SyntaxNode) -> bool {
        if node.kind() != SyntaxKind::NamedType || child(node, SyntaxKind::GenericArgs).is_some() {
            return false;
        }
        let Some(name) = name_token(node) else {
            return false;
        };
        matches!(
            self.bindings.get(&(self.file, name.span().start)).copied(),
            Some(Binding::Declarations { module, declarations })
                if declarations.iter().any(|&declaration| {
                    self.program.modules[*module].declarations[declaration].kind
                        == DeclarationKind::Constant
                })
        )
    }

    /// True when `node`, which the parser read as a type, names a
    /// constant of a module that a constraint compares with a value: an
    /// operand of `==`, or of `in` or a member of its set, whose other
    /// side is a generic value parameter, a const option, a literal or
    /// another constant. UNIT-13: "A constant is read by its name
    /// wherever an ordinary value is." GEN-3: a `requires` may use
    /// "equalities, the membership of a constant in a set of
    /// constants". `N == C` and `N in {C, 5}` so compare `N` with the
    /// constant's value, which is read where the condition is told
    /// (`crate::constant`). Beside a type, `T == C`, it is left to be
    /// read as the type it is not, and is reported.
    fn compared_constant(&self, node: &SyntaxNode) -> bool {
        if !self.names_a_constant(node) {
            return false;
        }
        // The relation, and the operand on the other side of it.
        let Some(parent) = node.parent() else {
            return false;
        };
        let (relation, others): (SyntaxNode, Vec<SyntaxNode>) = match parent.kind() {
            SyntaxKind::ConstraintRelation => {
                let others = parent
                    .children()
                    .filter(|operand| operand != node)
                    .collect();
                (parent, others)
            }
            SyntaxKind::ConstraintSet => {
                let Some(relation) = parent.parent() else {
                    return false;
                };
                let others = relation
                    .children()
                    .filter(|operand| *operand != parent)
                    .collect();
                (relation, others)
            }
            _ => return false,
        };
        let compares = relation.kind() == SyntaxKind::ConstraintRelation
            && tokens(&relation).any(|token| {
                matches!(token.kind(), SyntaxKind::EqEq | SyntaxKind::BangEq)
                    || (token.kind() == SyntaxKind::Ident && token.text() == "in")
            });
        compares
            && !others.is_empty()
            && others.iter().all(|other| {
                !is_type(other.kind())
                    || self.names_a_generic_value(other)
                    || self.names_an_option(other)
                    || self.names_a_constant(other)
            })
    }

    /// A generic value parameter of the declaration, by its settled kind.
    fn names_a_generic_value(&self, operand: &SyntaxNode) -> bool {
        operand.kind() == SyntaxKind::NamedType
            && name_token(operand).is_some_and(|name| {
                matches!(
                    self.bindings.get(&(self.file, name.span().start)).copied(),
                    Some(Binding::Local {
                        at,
                        kind: LocalKind::GenericParameter,
                    }) if self.generics.iter().any(|generic| {
                        generic.at == *at && Kind::of(&generic.kind) == Kind::Value
                    })
                )
            })
    }

    // ---- Constructors ----

    /// `Box<i64>(value: 7)`: the type a constructor names.
    fn constructor(&mut self, node: &SyntaxNode) {
        let span = match (used_start(node), child(node, SyntaxKind::GenericArgs)) {
            (Some(start), Some(list)) => Span::new(start, list.span().end),
            _ => return,
        };
        if self.read.contains_key(&span.start) {
            return;
        }
        // GEN-8: "A call may supply its generic arguments as a generic
        // constructor does, `count<i64>(3)`." The name is then a
        // function or an operator, and no type is written: the
        // arguments are kept for the pass that reads the call, which
        // knows what the callee's parameters are.
        if self.names_callable(node)
            && let Some(list) = child(node, SyntaxKind::GenericArgs)
        {
            let places: Vec<Location> = list
                .children()
                .filter(|argument| argument.kind() != SyntaxKind::Error)
                .map(|argument| self.at(argument.span()))
                .collect();
            let callee = self.callable_named(node);
            let paired = self.callable_has_pair(node);
            let arguments = self.arguments(&list, Position::ARGUMENT, None, callee, paired);
            self.read.insert(span.start, Type::Unknown);
            self.calls.push(CallArguments {
                at: self.at(span),
                arguments: places.into_iter().zip(arguments).collect(),
            });
            return;
        }
        let ty = self.named(node, Position::PLAIN);
        self.record(span, ty);
    }

    /// The function or operator the name `node` uses is bound to: the
    /// first of a pair (FN-4). A contextual Type argument of a pair is
    /// deferred until the expression reader selects a role.
    pub(super) fn callable_named(&self, node: &SyntaxNode) -> Option<Id> {
        let name = name_token(node)?;
        let Some(Binding::Declarations {
            module,
            declarations,
        }) = self.bindings.get(&(self.file, name.span().start)).copied()
        else {
            return None;
        };
        declarations
            .iter()
            .copied()
            .find(|&index| {
                matches!(
                    self.program.modules[*module].declarations[index].kind,
                    DeclarationKind::Function(_)
                        | DeclarationKind::NativeFunction(_)
                        | DeclarationKind::Operator
                )
            })
            .map(|index| (*module, index))
    }

    /// FN-4/6: generic kinds belong to the selected definition of a pair.
    fn callable_has_pair(&self, node: &SyntaxNode) -> bool {
        let Some(name) = name_token(node) else {
            return false;
        };
        let Some(Binding::Declarations {
            module,
            declarations,
        }) = self.bindings.get(&(self.file, name.span().start)).copied()
        else {
            return false;
        };
        let has = |role| {
            declarations.iter().any(|&index| {
                self.program.modules[*module].declarations[index]
                    .kind
                    .binding_role()
                    == Some(role)
            })
        };
        has(Role::Value) && has(Role::Temporal)
    }

    /// True when the name `node` uses is bound to a function or an
    /// operator, and to no type.
    fn names_callable(&self, node: &SyntaxNode) -> bool {
        let Some(name) = name_token(node) else {
            return false;
        };
        let Some(Binding::Declarations {
            module,
            declarations,
        }) = self.bindings.get(&(self.file, name.span().start)).copied()
        else {
            return false;
        };
        let kind = |index: &usize| self.program.modules[*module].declarations[*index].kind;
        let is_type = declarations.iter().any(|index| {
            matches!(
                kind(index),
                DeclarationKind::Struct | DeclarationKind::Enum | DeclarationKind::NativeType
            )
        });
        let is_callable = declarations.iter().any(|index| {
            matches!(
                kind(index),
                DeclarationKind::Function(_)
                    | DeclarationKind::NativeFunction(_)
                    | DeclarationKind::Operator
            )
        });
        is_callable && !is_type
    }

    /// `delta<list<f64>>(added: values)`: the shape a delta is of.
    fn delta_constructor(&mut self, node: &SyntaxNode) {
        let Some(list) = child(node, SyntaxKind::GenericArgs) else {
            return;
        };
        let span = Span::new(node.span().start, list.span().end);
        if self.read.contains_key(&span.start) {
            return;
        }
        let shape = self.inner(&list, Position::PLAIN);
        // TYPE-16/18: retain the same operand evidence as a written
        // delta type before normalizing the constructor's result type.
        self.delta_inputs.push(DeltaInput {
            at: self.at(span),
            payload: shape.clone(),
        });
        self.record(span, Type::delta(shape));
    }
}

/// Where the name of a constructor starts: its alias, when it has one.
fn used_start(node: &SyntaxNode) -> Option<u32> {
    match child(node, SyntaxKind::QualifiedName) {
        Some(qualified) => Some(qualified.span().start),
        None => name_token(node).map(|name| name.span().start),
    }
}
