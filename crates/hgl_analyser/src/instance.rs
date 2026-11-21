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
use crate::hir::{
    AnonymousFunction, Answer, Body, Callee, Expr, ExprKind, ValueCategory, walk_statements,
    walk_statements_mut,
};
use crate::program::{LocalKind, Phase, Program, Role};
use crate::types::{Argument, Constant, ConstantValue, Detail, Parameter, Scalar, Signature, Type};

/// Where each generic parameter of a declaration is declared, with what
/// it stands for at one call.
type Map<'a> = [(Location, &'a Argument)];

/// The enclosing declaration and the arguments common to its body and
/// any anonymous bodies written inside it.
struct InstanceArguments<'a> {
    declaration: (usize, usize),
    generics: &'a [Argument],
    types: &'a Map<'a>,
    requirements: &'a [Answer],
}

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
    /// parameter of the declaration, and no requirement of it. What each
    /// expression is and which calls are lifted are recorded again for
    /// the instance ([`Expr::category`]): the generic body could not say
    /// what an operation it requires gives (GEN-7).
    pub fn instance(
        &self,
        module: usize,
        declaration: usize,
        generics: &[Argument],
        requirements: &[Answer],
    ) -> Option<(Signature, Body)> {
        self.instance_with_functions(module, declaration, generics, requirements, &[])
    }

    /// Instantiate a declaration with the compile-time functions supplied
    /// at their original parameter slots (FN-17).
    pub fn instance_with_functions(
        &self,
        module: usize,
        declaration: usize,
        generics: &[Argument],
        requirements: &[Answer],
        functions: &[(usize, Callee)],
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
        let mut taken = Signature {
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
        let temporal = declared.declarations[declaration].kind.role() == Some(Role::Temporal);
        let functions = hidden_captures(functions, &mut taken, &mut body, temporal);
        let arguments = InstanceArguments {
            declaration: (module, declaration),
            generics,
            types: &map,
            requirements,
        };
        self.place_body(&mut body, &arguments, &functions);
        // FN-3, FN-6: what each expression is, and which calls are
        // lifted, with the candidates in place, which the generic body
        // could not say of an operation it requires.
        let graph = declared.phases.get(declaration) == Some(&Some(Phase::Graph));
        crate::values::record(self, &mut body, graph);
        Some((taken, body))
    }

    /// Specialize one lexical body using its own local table. A call
    /// a membership authorizes is selected with the instance's types
    /// in place (GEN-5 and GEN-7), including in an anonymous body.
    fn place_body(
        &self,
        body: &mut Body,
        instance: &InstanceArguments<'_>,
        functions: &[(usize, Callee)],
    ) {
        for local in &mut body.locals {
            local.ty = substitute_within(&local.ty, instance.types);
        }
        walk_statements_mut(&mut body.statements, &mut |expression| {
            self.place(expression, instance, functions);
        });
        crate::values::select_in_instance(
            self,
            instance.declaration,
            instance.generics,
            body,
            &|call| call.walk_mut(&mut |inner| self.place(inner, instance, functions)),
        );
    }

    /// Puts the instance's arguments in place in an expression's type,
    /// in what it calls, and where it reads a generic value parameter.
    fn place(
        &self,
        expression: &mut Expr,
        instance: &InstanceArguments<'_>,
        functions: &[(usize, Callee)],
    ) {
        expression.ty = substitute_within(&expression.ty, instance.types);
        match &mut expression.kind {
            // GEN-13: a generic value parameter read as a value is the
            // constant the call gives it.
            ExprKind::GenericValue { at, .. } => {
                let given = instance
                    .types
                    .iter()
                    .find_map(|(declared, argument)| match argument {
                        Argument::Constant(given) if declared == at => Some(given),
                        _ => None,
                    });
                let read = given
                    .and_then(|given| self.constant(&given.value, &expression.ty, expression.at));
                if let Some(kind) = read {
                    expression.kind = kind;
                }
            }
            ExprKind::Call { callee, .. } => self.place_target(callee, instance, functions),
            ExprKind::StaticCallable(callee) => self.place_target(callee, instance, functions),
            _ => {}
        }
    }

    fn place_target(
        &self,
        callee: &mut Callee,
        instance: &InstanceArguments<'_>,
        functions: &[(usize, Callee)],
    ) {
        if let Callee::Anonymous(function) = callee {
            let functions = inherited_functions(function, functions);
            let template = Type::Function(Box::new(function.template.clone()));
            if let Type::Function(template) = substitute_within(&template, instance.types) {
                function.template = *template;
            }
            function
                .specialization
                .extend(instance.types.iter().map(|(_, value)| (*value).clone()));
            self.place_body(&mut function.body, instance, &functions);
            crate::values::record(self, &mut function.body, !function.template.value);
        } else {
            place_callee(callee, instance.types, instance.requirements, functions);
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
                category: ValueCategory::Unsettled,
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

/// FN-17: a nested function that uses a static parameter also needs the
/// ordinary values its target captures. Relay only the captures used by
/// this scope or its descendants.
fn inherited_functions(
    function: &mut AnonymousFunction,
    enclosing: &[(usize, Callee)],
) -> Vec<(usize, Callee)> {
    if enclosing.is_empty() {
        return Vec::new();
    }
    let mut used = Vec::new();
    used_function_parameters(&function.body, &mut used);
    let mut functions: Vec<_> = enclosing
        .iter()
        .filter(|(slot, _)| used.contains(slot))
        .cloned()
        .collect();
    let inputs = function.template.parameters.len();
    let first_local = inputs + function.captures.len();
    let mut added = Vec::new();
    for (_, target) in &mut functions {
        let Callee::Anonymous(target) = target else {
            continue;
        };
        let target_inputs = target.template.parameters.len();
        for (index, captured) in target.captures.iter_mut().enumerate() {
            let position = function.captures.iter().position(|outer| outer == captured);
            let offset = position.unwrap_or_else(|| {
                let offset = function.captures.len();
                function.captures.push(*captured);
                added.push(target.body.locals[target_inputs + index].clone());
                offset
            });
            *captured = inputs + offset;
        }
    }
    if !added.is_empty() {
        let remapped: Vec<_> = (0..function.body.locals.len())
            .map(|local| {
                if local < first_local {
                    local
                } else {
                    local + added.len()
                }
            })
            .collect();
        crate::values::remap_locals(&mut function.body, &remapped);
        function.body.locals.splice(first_local..first_local, added);
    }
    functions
}

/// Record the declaration slots used by this anonymous body, including
/// the slots referenced in a nested anonymous argument.
fn used_function_parameters(body: &Body, used: &mut Vec<usize>) {
    walk_statements(&body.statements, &mut |expression| {
        let target = match &expression.kind {
            ExprKind::Call { callee, .. } => callee,
            ExprKind::StaticCallable(callee) => callee,
            _ => return,
        };
        match target {
            Callee::Parameter { parameter, .. } => used.push(*parameter),
            Callee::Anonymous(nested) => used_function_parameters(&nested.body, used),
            _ => {}
        }
    });
}

/// Carry an anonymous function's immutable environment as ordinary
/// parameters of the specialized function. Its compile-time identity is
/// kept separately from the captures supplied by each invocation.
fn hidden_captures(
    functions: &[(usize, Callee)],
    signature: &mut Signature,
    body: &mut Body,
    temporal: bool,
) -> Vec<(usize, Callee)> {
    let mut functions = functions.to_vec();
    functions.sort_by_key(|(parameter, _)| *parameter);
    let original = signature.parameters.len();
    let mut locals = Vec::new();
    for (slot, callee) in &mut functions {
        let Callee::Anonymous(function) = callee else {
            continue;
        };
        let start = function.template.parameters.len();
        for (index, capture) in function.captures.iter_mut().enumerate() {
            let mut local = function.body.locals[start + index].clone();
            let mut name = format!("fn_capture_{slot}_{index}");
            while body.locals.iter().any(|other| other.name == name) {
                name.push('_');
            }
            local.name = name.clone();
            local.kind = LocalKind::Parameter;
            *capture = signature.parameters.len();
            signature.parameters.push(Parameter {
                configuration: false,
                name,
                at: local.at,
                constant: temporal && local.category == ValueCategory::Value,
                var: false,
                reference: false,
                ty: local.ty.clone(),
                pack: None,
                pack_bounds: (0, None),
                has_default: false,
            });
            locals.push(local);
        }
    }
    if !locals.is_empty() {
        let added = locals.len();
        let remapped = (0..body.locals.len())
            .map(|index| {
                if index < original {
                    index
                } else {
                    index + added
                }
            })
            .collect::<Vec<_>>();
        crate::values::remap_locals(body, &remapped);
        body.locals.splice(original..original, locals);
    }
    functions
}

/// Puts what `map` gives in place in what a call calls. A call of an
/// operation the declaration requires becomes the call of the candidate
/// that answers the requirement (GEN-16); one nothing answers is left
/// as it is.
fn place_callee(callee: &mut Callee, map: &Map, answers: &[Answer], functions: &[(usize, Callee)]) {
    if let Callee::Parameter { parameter, .. } = callee
        && let Some((_, target)) = functions.iter().find(|(index, _)| index == parameter)
    {
        *callee = target.clone();
        // The argument belongs to the caller's context, which was already placed.
        return;
    }
    if let Callee::Function { type_packs, .. } | Callee::Candidate { type_packs, .. } = callee {
        for pack in type_packs {
            for ty in &mut pack.types {
                *ty = substitute_within(ty, map);
            }
        }
    }
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
                    type_packs: Vec::new(),
                    requirements: requirements.clone(),
                };
            }
        }
        Callee::Parameter { .. } | Callee::Anonymous(_) | Callee::Unread => {}
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
