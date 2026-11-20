//! What a call gives what it does not write, as the language says it:
//! the default or the null a parameter left out takes, the input that is
//! then never valid, a field's default, and the value of a duration
//! worked out from how it is written. Part of lowering: a transpiler
//! writes each call with these (UNIT-7, UNIT-8, TIME-8).

use std::collections::HashMap;

use hgl_analyser::hir::{Argument, Expr, ExprKind, Value};
use hgl_analyser::{FieldRef, Parameter, Program, Role, Scalar, Type};

/// The value of `value` in microseconds, where it is a duration whose
/// value is worked out from how it is written: a literal, a constant
/// followed to what it is written as, and core's `-value`, `+` and
/// `-` of two of them and `*` of one by an `i64` literal (OPS-1), with
/// checked arithmetic. `None` for any other: a local, a call of
/// another function, one that overflows. The analyser has rejected a
/// constant that depends on itself (`constant.cycle`), so a chain of
/// constants ends. `known` holds what each constant, by its module
/// and declaration, came to, so that one named many times, `D0 + D0`,
/// is worked out once.
pub fn known_duration(
    program: &Program,
    value: &Expr,
    known: &mut HashMap<(usize, usize), Option<i64>>,
) -> Option<i64> {
    match &value.kind {
        ExprKind::Literal(Value::Duration(micros)) => Some(*micros),
        ExprKind::Constant {
            module,
            declaration,
        } => {
            if let Some(value) = known.get(&(*module, *declaration)) {
                return *value;
            }
            let value = match program.modules[*module].constants.get(*declaration) {
                Some(Some(constant)) => known_duration(program, constant, known),
                _ => None,
            };
            known.insert((*module, *declaration), value);
            value
        }
        ExprKind::Call {
            callee, arguments, ..
        } => {
            let (module, declaration) = callee.declaration()?;
            if Some(module) != program.core() {
                return None;
            }
            // An operand that is a duration, or an `i64` literal.
            let mut operand = |argument: &Argument| match (&argument.value.ty, &argument.value.kind)
            {
                (Type::Scalar(Scalar::Duration), _) => {
                    known_duration(program, &argument.value, known).map(Operand::Duration)
                }
                (Type::Scalar(Scalar::I64), ExprKind::Literal(Value::Int(count))) => {
                    Some(Operand::Count(*count))
                }
                _ => None,
            };
            // In the order of the parameters, however they are
            // written: `sub_(rhs: a, lhs: b)` is `b - a`.
            let mut ordered: Vec<&Argument> = arguments.iter().collect();
            ordered.sort_by_key(|argument| argument.parameter);
            let operands: Option<Vec<Operand>> = ordered.into_iter().map(&mut operand).collect();
            let name = program.modules[module].declarations[declaration]
                .name
                .as_str();
            match (name, operands?.as_slice()) {
                ("neg_", [Operand::Duration(micros)]) => micros.checked_neg(),
                ("add_", [Operand::Duration(lhs), Operand::Duration(rhs)]) => lhs.checked_add(*rhs),
                ("sub_", [Operand::Duration(lhs), Operand::Duration(rhs)]) => lhs.checked_sub(*rhs),
                ("mul_", [Operand::Duration(micros), Operand::Count(count)])
                | ("mul_", [Operand::Count(count), Operand::Duration(micros)]) => {
                    micros.checked_mul(*count)
                }
                _ => None,
            }
        }
        _ => None,
    }
}

/// What a call that leaves a parameter out gives it (UNIT-8), where it
/// is not a temporal input that is then never valid
/// ([`left_out_never_valid`]).
pub enum LeftOut<'p> {
    /// Its default, read with no local in scope (UNIT-7).
    Default(&'p Expr),
    /// `null`. UNIT-8: "A parameter whose type is written `T?` needs no
    /// default to be omitted: it is taken to have a default of `null`".
    Null,
}

/// What parameter `index` of the function `callee`, a module and a
/// declaration of it, is given when a call leaves it out: its default,
/// or `null` for one written `T?` with none (UNIT-8). `None` for a
/// parameter that cannot be left out.
pub fn left_out<'p>(
    program: &'p Program,
    callee: (usize, usize),
    index: usize,
    parameter: &Parameter,
) -> Option<LeftOut<'p>> {
    let defaults = program.modules[callee.0]
        .defaults
        .get(callee.1)
        .map_or(&[][..], Vec::as_slice);
    if let Some(Some(default)) = defaults.get(index) {
        Some(LeftOut::Default(default))
    } else if matches!(parameter.ty, Type::Nullable(_)) {
        Some(LeftOut::Null)
    } else {
        None
    }
}

/// True where a call of the function `callee`, a module and a
/// declaration of it, that leaves `parameter` out binds it to an input
/// that is never valid: a temporal input written `T?` with no default.
/// UNIT-8: "an omitted temporal input is never valid". Any other
/// parameter takes what [`left_out`] gives it.
pub fn left_out_never_valid(
    program: &Program,
    callee: (usize, usize),
    parameter: &Parameter,
) -> bool {
    let temporal =
        program.modules[callee.0].declarations[callee.1].kind.role() == Some(Role::Temporal);
    temporal
        && !parameter.constant
        && !parameter.has_default
        && matches!(parameter.ty, Type::Nullable(_))
}

/// The default of the field `default` of a struct, which a construction
/// that leaves the field out gives it (TYPE-5), read with no local in
/// scope (UNIT-7). `None` where the analysed program has none for it.
pub fn field_default(program: &Program, default: FieldRef) -> Option<&Expr> {
    program.modules[default.module]
        .defaults
        .get(default.declaration)
        .and_then(|defaults| defaults.get(default.field))
        .and_then(Option::as_ref)
}

/// An operand of core's arithmetic on durations, as `known_duration`
/// works it out: a duration in microseconds, or an `i64` count.
enum Operand {
    /// A duration, in microseconds.
    Duration(i64),
    /// An `i64`.
    Count(i64),
}
