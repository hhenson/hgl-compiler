//! What a call gives what it does not write, as the language says it:
//! the default or the null a parameter left out takes, and a field's
//! default. Part of lowering: a transpiler writes each call with these
//! (UNIT-7, UNIT-8).

use std::collections::HashMap;
use std::ops::Deref;

use hgl_analyser::hir::{Argument, Body, Callee, DefaultExpr, Expr, ExprKind, Value};
use hgl_analyser::{Detail, FieldRef, GenericKind, Parameter, Program, Type, substitute_within};

/// The evaluated arguments of a call in written order, keeping their
/// positions in the analysed argument list (EXPR-2). A function
/// argument is a compile-time binding and has no runtime value (FN-17).
/// Values supplied in configuration braces (FN-18, EXPR-10) use the
/// same parameter slots and keep their place in that order.
pub fn runtime_arguments(arguments: &[Argument]) -> impl Iterator<Item = (usize, &Argument)> {
    arguments
        .iter()
        .enumerate()
        .filter(|(_, argument)| !matches!(argument.value.kind, ExprKind::StaticCallable(_)))
}

/// What a call that leaves a parameter out gives it (UNIT-8).
pub enum LeftOut<'p> {
    /// Its default, in the declaration’s enclosing scope (UNIT-7).
    Default(DefaultValue<'p>),
    /// `null`. UNIT-8: "A parameter whose type is written `T?` needs no
    /// default to be omitted: it is taken to have a default of `null`".
    Null,
}

/// A checked default, with the arguments to its enclosing declaration in place.
pub enum DefaultValue<'p> {
    /// The original checked expression.
    Declared(&'p DefaultExpr),
    /// An expression specialized for this call (GEN-6, UNIT-7).
    Specialized(Box<DefaultExpr>),
}

impl Deref for DefaultValue<'_> {
    type Target = Expr;

    fn deref(&self) -> &Expr {
        &self.checked().value
    }
}

impl DefaultValue<'_> {
    /// The expression and its local bindings as an isolated body (UNIT-7).
    pub fn body(&self) -> Body {
        self.checked().clone().into_body()
    }

    fn checked(&self) -> &DefaultExpr {
        match self {
            Self::Declared(default) => default,
            Self::Specialized(default) => default,
        }
    }
}

/// What a call gives parameter `index` when it leaves it out (UNIT-8).
/// A named `callee` may declare a default. A `T?` parameter without a
/// declared default is given `null`, including in an anonymous function.
pub fn left_out<'p>(
    program: &'p Program,
    callee: &Callee,
    index: usize,
    parameter: &Parameter,
) -> Option<LeftOut<'p>> {
    if parameter.configuration
        && let Callee::Candidate {
            operator,
            operator_generics,
            ..
        } = callee
        && let Some(default) = program.inherited_configuration_default(callee, index)
    {
        let mut diagnostics = hgl_diagnostics::Diagnostics::new();
        let default = program.specialize_operator_default(
            default,
            *operator,
            operator_generics,
            parameter,
            &mut diagnostics,
        );
        return (!diagnostics.has_errors())
            .then(|| LeftOut::Default(DefaultValue::Specialized(Box::new(default))));
    }
    let default = callee.declaration().and_then(|(module, declaration)| {
        program.modules[module]
            .defaults
            .get(declaration)
            .and_then(|defaults| defaults.get(index))
            .and_then(Option::as_ref)
    });
    if let Some(default) = default {
        Some(LeftOut::Default(DefaultValue::Declared(default)))
    } else if matches!(parameter.ty, Type::Nullable(_)) {
        Some(LeftOut::Null)
    } else {
        None
    }
}

/// The default of the field `default` of a struct, which a construction
/// that leaves the field out gives it (TYPE-5), read with no local in
/// scope (UNIT-7). `None` where the analysed program has none for it.
pub fn field_default(program: &Program, default: FieldRef) -> Option<&DefaultExpr> {
    program.modules[default.module]
        .defaults
        .get(default.declaration)
        .and_then(|defaults| defaults.get(default.field))
        .and_then(Option::as_ref)
}

/// TYPE-5: the checked default of an omitted field at an applied struct type.
/// Its own defining declaration supplies the lexical and generic scope (UNIT-7).
pub fn applied_field_default<'p>(
    program: &'p Program,
    field: FieldRef,
    structure: &Type,
    expected: &Type,
) -> Option<DefaultValue<'p>> {
    let default = field_default(program, field)?;
    let Detail::Struct(owner) = program
        .modules
        .get(field.module)?
        .details
        .get(field.declaration)?
    else {
        return None;
    };
    if owner.generics.is_empty()
        || matches!(&default.value.kind, ExprKind::Literal(Value::Null))
        || matches!(&default.value.kind,
        ExprKind::Sequence(entries) if entries.is_empty())
        || matches!(&default.value.kind,
        ExprKind::Pairs(entries) if entries.is_empty())
    {
        return Some(DefaultValue::Declared(default));
    }
    let arguments = field_arguments(program, field, structure)?;
    program
        .specialize_field_default(field, &arguments, expected)
        .map(|value| DefaultValue::Specialized(Box::new(value)))
}

/// The arguments of the struct that writes this default, following only
/// parents that supply it. A repeated path to the same application is one.
fn field_arguments(
    program: &Program,
    field: FieldRef,
    structure: &Type,
) -> Option<Vec<hgl_analyser::Argument>> {
    let mut pending = vec![structure.clone()];
    let mut visited: HashMap<_, Vec<Vec<_>>> = HashMap::new();
    let mut budget = hgl_analyser::INSTANCES;
    let mut found = None;
    while let Some(current) = pending.pop() {
        let Type::Named {
            module,
            declaration,
            arguments,
            ..
        } = current
        else {
            continue;
        };
        if (module, declaration) == (field.module, field.declaration) {
            if found
                .as_ref()
                .is_some_and(|previous| previous != &arguments)
            {
                return None;
            }
            found = Some(arguments);
            continue;
        }
        let Some(Detail::Struct(read)) = program.modules[module].details.get(declaration) else {
            continue;
        };
        if !read
            .members
            .iter()
            .any(|member| member.default == Some(field))
        {
            continue;
        }
        let applications = visited.entry((module, declaration)).or_default();
        if applications.contains(&arguments) {
            continue;
        }
        budget = budget.checked_sub(1)?;
        let bindings: Vec<_> = read
            .generics
            .iter()
            .take_while(|generic| !matches!(generic.kind, GenericKind::Pack { .. }))
            .map(|generic| generic.at)
            .zip(&arguments)
            .collect();
        pending.extend(
            read.parents
                .iter()
                .map(|parent| substitute_within(parent, &bindings)),
        );
        applications.push(arguments);
    }
    found
}
