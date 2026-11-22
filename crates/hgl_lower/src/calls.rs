//! What a call gives what it does not write, as the language says it:
//! the default or the null a parameter left out takes, and a field's
//! default. Part of lowering: a transpiler writes each call with these
//! (UNIT-7, UNIT-8).

use std::ops::Deref;

use hgl_analyser::hir::{Argument, Body, Callee, DefaultExpr, Expr, ExprKind};
use hgl_analyser::{FieldRef, Parameter, Program, Type};

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

/// A checked default, with any operator generics of the call put in place.
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
