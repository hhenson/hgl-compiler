//! A generic declaration for the types of one call of it (GEN-2,
//! GEN-16).
//!
//! A generic declaration is typed once, with each generic parameter
//! standing for itself: the `T` of
//!
//! ```text
//! fn sum<T>(a: T, b: T) -> T requires add_(T, T) -> T => add_(a, b)
//! ```
//!
//! is the type of `a`, and the body's `add_(a, b)` is recorded as an
//! operation the declaration requires, with no candidate
//! ([`Callee::Required`]). A call says what `T` is there, and what
//! answers the requirement ([`Answer`]).
//!
//! A target with no generics of its own writes such a declaration once
//! for each set of types it is called with. [`Program::instance`] gives
//! it what to write: the signature and the body with each generic
//! parameter replaced by what it stands for at one call, and each
//! required operation replaced by the call of the candidate that
//! answers it there. `sum(1.5, 2.5)` gives
//!
//! ```text
//! fn sum(a: f64, b: f64) -> f64 => <core's add_(f64, f64)>(a, b)
//! ```
//!
//! The body was checked under its stated requirements (GEN-7), and
//! each call recorded the candidates that meet them: this puts the two
//! together. One call is read again: a call of an operator that a
//! membership the declaration states authorises, `requires T in {i64,
//! f64}`, is the call of the candidate it selects with the instance's
//! types (GEN-7: "Each instance then selects its own candidate";
//! [`crate::values::Selecting`]).

use hgl_diagnostics::Location;

use crate::declared::substitute_within;
use crate::hir::{Answer, Body, Callee, Expr, ExprKind, walk_statements_mut};
use crate::program::Program;
use crate::types::{Argument, Constant, ConstantValue, Detail, Parameter, Scalar, Signature, Type};

/// Where each generic parameter of a declaration is declared, with what
/// it stands for at one call.
type Map<'a> = [(Location, &'a Argument)];

impl Program {
    /// The signature and the body of `declaration` of `module` as one
    /// call takes it: `None` for a declaration that is no function, or
    /// has no body.
    ///
    /// `generics` is what each generic parameter stands for, in the
    /// order declared, and `requirements` what answers each requirement
    /// the declaration states: both as a call records them
    /// ([`Callee::Function`], [`Callee::Candidate`]).
    ///
    /// A call written in a generic body records what names that body's
    /// own generic parameters, and requirements that body states
    /// ([`Answer::Stated`]). Those are given as the body's own instance
    /// has them: a call in the body this returns names no generic
    /// parameter of the declaration, and no requirement of it.
    pub fn instance(
        &self,
        module: usize,
        declaration: usize,
        generics: &[Argument],
        requirements: &[Answer],
    ) -> Option<(Signature, Body)> {
        let declared = self.modules.get(module)?;
        let Some(Detail::Callable(signature)) = declared.details.get(declaration) else {
            return None;
        };
        let body = declared.bodies.get(declaration)?.as_ref()?;
        let map: Vec<(Location, &Argument)> = signature
            .generics
            .iter()
            .map(|generic| generic.at)
            .zip(generics)
            .collect();
        let taken = Signature {
            generics: Vec::new(),
            parameters: signature
                .parameters
                .iter()
                .map(|parameter| Parameter {
                    ty: substitute_within(&parameter.ty, &map),
                    ..parameter.clone()
                })
                .collect(),
            result: signature
                .result
                .as_ref()
                .map(|result| substitute_within(result, &map)),
            reference: signature.reference,
        };
        let mut body = body.clone();
        for local in &mut body.locals {
            local.ty = substitute_within(&local.ty, &map);
        }
        walk_statements_mut(&mut body.statements, &mut |expression| {
            self.place(expression, &map, requirements);
        });
        // GEN-5: a call a membership authorises is the call of the
        // candidate it selects with these arguments in place, which is
        // read here.
        crate::values::select_in_instance(
            self,
            (module, declaration),
            generics,
            &mut body,
            &|call| call.walk_mut(&mut |inner| self.place(inner, &map, requirements)),
        );
        Some((taken, body))
    }

    /// Puts what `map` gives in place in one expression: in its type,
    /// in what it calls, and where it reads a generic value parameter.
    fn place(&self, expression: &mut Expr, map: &Map, answers: &[Answer]) {
        expression.ty = substitute_within(&expression.ty, map);
        match &mut expression.kind {
            // GEN-13: a generic value parameter read as a value is the
            // constant the call gives it.
            ExprKind::GenericValue { at, .. } => {
                let given = map.iter().find_map(|(declared, argument)| match argument {
                    Argument::Constant(given) if declared == at => Some(given),
                    _ => None,
                });
                let read = given
                    .and_then(|given| self.constant(&given.value, &expression.ty, expression.at));
                if let Some(kind) = read {
                    expression.kind = kind;
                }
            }
            ExprKind::Call { callee, .. } => place_callee(callee, map, answers),
            _ => {}
        }
    }

    /// A constant a generic value parameter is given, `value` of type
    /// `ty`, as the expression that reads it, written at `at`: `None`
    /// for one whose value is not held. A list and a tuple are each
    /// element as its own expression.
    fn constant(&self, value: &ConstantValue, ty: &Type, at: Location) -> Option<ExprKind> {
        // TYPE-14: "`T?` denotes an ordinary value that is either a T or
        // null": a value of it other than null is a T.
        let of = match ty {
            Type::Nullable(inner) if !matches!(value, ConstantValue::Null) => inner.as_ref(),
            other => other,
        };
        // A constant under `any`, an element of a `list<any>`, says its
        // own type: a member its enum, and a list's or a tuple's
        // elements are each an `any` in turn.
        let any = matches!(of, Type::Scalar(Scalar::Any));
        let element = |value: &ConstantValue, ty: &Type| -> Option<Expr> {
            Some(Expr {
                kind: self.constant(value, ty, at)?,
                ty: ty.clone(),
                at,
            })
        };
        match value {
            // TYPE-7: a member is known by its name, and the enum is
            // the parameter's type.
            ConstantValue::Member(named) => {
                let (module, declaration) = match of {
                    Type::Named {
                        module,
                        declaration,
                        ..
                    } => (module, declaration),
                    _ if any => {
                        let (module, declaration) = &named.of.as_ref()?.declared;
                        (module, declaration)
                    }
                    _ => return None,
                };
                let Some(Detail::Enum(read)) = self.modules.get(*module)?.details.get(*declaration)
                else {
                    return None;
                };
                let member = read
                    .members
                    .iter()
                    .position(|member| member.name == named.name)?;
                Some(ExprKind::EnumMember {
                    module: *module,
                    declaration: *declaration,
                    member,
                })
            }
            ConstantValue::List(values) => {
                let of = match of {
                    Type::List { element, .. } => element.as_ref(),
                    _ if any => of,
                    _ => return None,
                };
                values
                    .iter()
                    .map(|value| element(value, of))
                    .collect::<Option<Vec<Expr>>>()
                    .map(ExprKind::Sequence)
            }
            ConstantValue::Tuple(values) => {
                let anys;
                let elements = match of {
                    Type::Tuple(elements) => elements,
                    _ if any => {
                        anys = vec![of.clone(); values.len()];
                        &anys
                    }
                    _ => return None,
                };
                if elements.len() != values.len() {
                    return None;
                }
                values
                    .iter()
                    .zip(elements)
                    .map(|(value, of)| element(value, of))
                    .collect::<Option<Vec<Expr>>>()
                    .map(ExprKind::Tuple)
            }
            // A literal's value: an integer, a float, a string.
            value => value.literal().map(ExprKind::Literal),
        }
    }
}

/// Puts what `map` gives in place in what a call calls. A call of an
/// operation the declaration requires becomes the call of the candidate
/// that answers the requirement (GEN-16); one nothing answers is left
/// as it is.
fn place_callee(callee: &mut Callee, map: &Map, answers: &[Answer]) {
    match callee {
        Callee::Function {
            generics,
            requirements,
            ..
        }
        | Callee::Candidate {
            generics,
            requirements,
            ..
        } => {
            for argument in generics {
                *argument = placed(argument, map);
            }
            for answer in requirements {
                place_answer(answer, map, answers);
            }
        }
        Callee::Required { requirement, .. } => {
            if let Some(Answer::Candidate {
                operator,
                module,
                declaration,
                generics,
                requirements,
            }) = answers.get(*requirement)
            {
                *callee = Callee::Candidate {
                    operator: *operator,
                    module: *module,
                    declaration: *declaration,
                    generics: generics.clone(),
                    requirements: requirements.clone(),
                };
            }
        }
        Callee::Unread => {}
    }
}

/// Puts what `map` gives in place in an answer a call records. One the
/// enclosing declaration states (GEN-7) is what answers that
/// requirement of the declaration here.
fn place_answer(answer: &mut Answer, map: &Map, answers: &[Answer]) {
    match answer {
        Answer::Stated(index) => {
            *answer = answers.get(*index).cloned().unwrap_or(Answer::Unread);
        }
        Answer::Candidate {
            generics,
            requirements,
            ..
        } => {
            for argument in generics {
                *argument = placed(argument, map);
            }
            for own in requirements {
                place_answer(own, map, answers);
            }
        }
        Answer::Unread => {}
    }
}

/// A generic argument with what `map` gives in place: a type with its
/// parameters replaced, a constant that is a generic value parameter
/// replaced by the constant given for it, and one that holds such a
/// parameter, `N + 1` or `[N]`, worked out with it.
fn placed(argument: &Argument, map: &Map) -> Argument {
    let given = |at: Location| {
        map.iter().find_map(|(declared, given)| match given {
            Argument::Constant(given) if *declared == at => Some(given),
            _ => None,
        })
    };
    match argument {
        Argument::Type(ty) => Argument::Type(substitute_within(ty, map)),
        Argument::Constant(constant) => match &constant.value {
            ConstantValue::Parameter { at, .. } => given(*at)
                .map(|given| Argument::Constant(given.clone()))
                .unwrap_or_else(|| argument.clone()),
            value => Argument::Constant(Constant {
                text: constant.text.clone(),
                value: value.substituted(&|at| given(at).map(|given| given.value.clone())),
            }),
        },
    }
}
