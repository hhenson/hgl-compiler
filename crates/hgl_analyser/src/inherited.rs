//! Compares the defaults a struct has for one field from two of its
//! parents (UNIT-9).
//!
//! `declared` works out the fields each struct has, and finds where two
//! parents give one field a default and the struct gives it none of its
//! own. What each default is, is a question of what it is written as,
//! which `values` reads. So this pass runs after it, with the arguments
//! each default's path gives the struct that writes it in place.
//!
//! UNIT-9: "Compare inherited defaults by bound expression and
//! canonical type after substitution, before folding. Declaration
//! identity matters; source location and alias spelling do not.
//! Conflicting inherited defaults require an explicit child override."
//!
//! The pass rejects only what it can tell: two defaults that are each a
//! constant value of the field's type, the type the struct has the field
//! with, and that are two values. A default it cannot read as such a
//! value, a call, an expression of operators, a generic value parameter
//! left as one, a date or time among them, is not judged, and nothing is
//! reported for its pair.

use hgl_diagnostics::{Category, Code, Diagnostic, Diagnostics, Location};
use hgl_lexer::literal::{decode_float, decode_string};

use crate::codes;
use crate::declared::bare_literal;
use crate::hir::{Callee, Expr, ExprKind, Form, UnaryOperator, Value};
use crate::program::Program;
use crate::types::{Argument, Constant, ConstantValue, Detail, FieldRef, Scalar, Type};

/// The defaults two parents give one field of a struct, which gives
/// that field none of its own.
#[derive(Debug, Clone)]
pub(crate) struct InheritedDefaults {
    /// The struct: the position of its module in `Program::modules`,
    /// and its own in that module's declarations.
    pub(crate) owner: (usize, usize),
    /// The field's name.
    pub(crate) field: String,
    /// The field's type as the struct has it, its parents' generic
    /// arguments in place.
    pub(crate) ty: Type,
    /// The default the earlier parent gives it, then the later's.
    pub(crate) defaults: [InheritedDefault; 2],
}

/// One default a struct has for a field from a parent.
#[derive(Debug, Clone)]
pub(crate) struct InheritedDefault {
    /// The field that writes it, whose value is in `Module::defaults`.
    pub(crate) written: FieldRef,
    /// What the generic parameters of the struct that writes it stand
    /// for in the struct that has the field, each by where it is
    /// declared.
    pub(crate) arguments: Vec<(Location, Argument)>,
    /// Where it is written.
    pub(crate) at: Location,
    /// Where the parent it comes through is written, after the `:`.
    pub(crate) parent: Location,
}

/// How many constants a default's value is followed through: one that
/// names more is not judged.
const DEPTH: usize = 32;

/// Reports each struct that has two different defaults for one field
/// from its parents and gives the field none of its own: once for each
/// field, at the later parent.
///
/// Only a field whose type is concrete, with no generic parameter in it
/// and nothing unknown, is judged, and only where each default is a
/// constant value of that type: see [`Known`].
pub(crate) fn check(
    program: &Program,
    pending: &[InheritedDefaults],
    diagnostics: &mut Diagnostics,
) {
    let mut reported: Vec<((usize, usize), &str)> = Vec::new();
    for pair in pending {
        if reported.contains(&(pair.owner, pair.field.as_str())) || !concrete(&pair.ty) {
            continue;
        }
        let [earlier, later] = &pair.defaults;
        let (Some(first), Some(second)) = (
            value_of(program, earlier, &pair.ty),
            value_of(program, later, &pair.ty),
        ) else {
            continue;
        };
        if first == second {
            continue;
        }
        reported.push((pair.owner, pair.field.as_str()));
        let owner = &program.modules[pair.owner.0].declarations[pair.owner.1].name;
        let message = format!(
            "`{owner}` has `{}` from two parents with different defaults, and gives it none \
             of its own: write `{} = ...` in `{owner}`",
            pair.field, pair.field
        );
        diagnostics.emit(
            Diagnostic::error(
                Category::Type,
                Code::Local(codes::local::DEFAULT_CONFLICT),
                later.parent,
                message,
            )
            .with_rule("UNIT-9")
            .with_related(earlier.at, "one default")
            .with_related(later.at, "the other"),
        );
    }
}

/// A default as the constant value it is, of the field's type.
///
/// It is a literal; `null`; a member of an enum; a module's constant,
/// as its own value is; or a list or a tuple of those. A minus before a
/// number, by symbol or as core's `neg_`, is that negative number, and
/// an integer where an `f64` is wanted is that `f64` (EXPR-5). Two
/// values are compared as values: `(true)` is `true`, `1_000` is
/// `1000`. A date or time literal is not judged: two written
/// differently may be one instant.
#[derive(Debug, Clone, PartialEq)]
enum Known {
    /// A literal or `null`, of the type it is in its place.
    Scalar(Value),
    /// A member of an enum: the enum's module and declaration, and the
    /// member's position.
    Member(usize, usize, usize),
    /// The elements of a list, or the positions of a tuple, in order.
    Items(Vec<Known>),
}

/// The value `default` gives a field of type `ty`, with the arguments of
/// its path in place of the generic parameters of the struct that
/// writes it. `None` where it is no constant value of that type.
fn value_of(program: &Program, default: &InheritedDefault, ty: &Type) -> Option<Known> {
    let written = default.written;
    let mut value = program.modules[written.module]
        .defaults
        .get(written.declaration)?
        .get(written.field)?
        .clone()?;
    // `walk_mut` visits each expression before those inside it, and
    // then those inside what the visit made of it.
    value.walk_mut(&mut |expression| {
        // A generic value parameter of the struct that writes the
        // default: the value its path gives it, where that is held.
        if let ExprKind::GenericValue { at, .. } = &expression.kind
            && let Some((_, Argument::Constant(constant))) = default
                .arguments
                .iter()
                .find(|(declared, _)| declared == at)
            && let Some(kind) = given_value(program, constant, &expression.ty)
                .or_else(|| given_value(program, constant, ty))
        {
            expression.kind = kind;
        }
    });
    numbers(program, &mut value);
    known(program, &value, ty, 0)
}

/// What `value`, its numbers read by [`numbers`], is as a value of
/// `ty`; `None` where it is no constant value of that type. `depth`
/// counts the constants followed to reach it.
fn known(program: &Program, value: &Expr, ty: &Type, depth: usize) -> Option<Known> {
    match ty {
        Type::Nullable(inner) => {
            return match value.kind {
                ExprKind::Literal(Value::Null) => Some(Known::Scalar(Value::Null)),
                _ => known(program, value, inner, depth),
            };
        }
        // TYPE-9: an `atomic<V>` holds a complete V.
        Type::Atomic(inner) => return known(program, value, inner, depth),
        _ => {}
    }
    match &value.kind {
        ExprKind::Literal(literal) => fits(literal, ty).map(Known::Scalar),
        ExprKind::EnumMember {
            module,
            declaration,
            member,
        } => match ty {
            Type::Named {
                module: enum_module,
                declaration: enum_declaration,
                ..
            } if (enum_module, enum_declaration) == (module, declaration) => {
                Some(Known::Member(*module, *declaration, *member))
            }
            _ => None,
        },
        // UNIT-13: "The constant has that one value wherever it is
        // read". Its value is followed, as far as `DEPTH`: a constant
        // that depends on itself is reported as that. A constant is of
        // the type it is declared with, and is not widened (EXPR-5: "A
        // value that has a type is not converted by its place"): one of
        // another type than the field's is not judged.
        ExprKind::Constant {
            module,
            declaration,
        } if depth < DEPTH => {
            let unit = &program.modules[*module];
            match unit.details.get(*declaration) {
                Some(Detail::Constant(declared)) if declared == ty => {}
                _ => return None,
            }
            let mut value = unit.constants.get(*declaration)?.clone()?;
            numbers(program, &mut value);
            known(program, &value, ty, depth + 1)
        }
        ExprKind::Sequence(items) => match ty {
            Type::List { element, .. } => items
                .iter()
                .map(|item| known(program, item, element, depth))
                .collect::<Option<Vec<_>>>()
                .map(Known::Items),
            // A set's and a dictionary's entries are no order of their
            // own; they are not judged.
            _ => None,
        },
        ExprKind::Tuple(items) => match ty {
            Type::Tuple(types) if types.len() == items.len() => items
                .iter()
                .zip(types)
                .map(|(item, ty)| known(program, item, ty, depth))
                .collect::<Option<Vec<_>>>()
                .map(Known::Items),
            _ => None,
        },
        _ => None,
    }
}

/// The literal `literal` as a value of the scalar type `ty`; `None`
/// where it is none. EXPR-5: "An integer literal is an `f64` where one
/// is expected".
fn fits(literal: &Value, ty: &Type) -> Option<Value> {
    let Type::Scalar(scalar) = ty else {
        return None;
    };
    match (literal, scalar) {
        // `as` gives the nearest `f64`, as EXPR-5's widening does.
        (Value::Int(whole), Scalar::F64) => Some(Value::Float(*whole as f64)),
        (Value::Int(_), Scalar::I64)
        | (Value::Float(_), Scalar::F64)
        | (Value::Bool(_), Scalar::Bool)
        | (Value::Str(_), Scalar::Str)
        | (Value::Duration(_), Scalar::Duration)
        | (Value::Tenor(_), Scalar::Tenor) => Some(literal.clone()),
        _ => None,
    }
}

/// Reads each number `value` writes as the number it is: a minus before
/// a number literal, by symbol or as core's `neg_` (EXPR-10), is the
/// negative number, as a generic argument `-3` is (LEX-7 reads the least
/// `i64` so); and an integer widened to an `f64` is that `f64` (EXPR-5).
/// Each change takes a node away, so the passes end; one exposes the
/// next, as in `-1` where an `f64` is wanted.
fn numbers(program: &Program, value: &mut Expr) {
    loop {
        let mut changed = false;
        value.walk_mut(&mut |expression| {
            let literal = match &expression.kind {
                ExprKind::Widen(inner) => match inner.kind {
                    ExprKind::Literal(Value::Int(whole)) => Some(Value::Float(whole as f64)),
                    _ => None,
                },
                kind => negated(program, kind),
            };
            if let Some(literal) = literal {
                expression.kind = ExprKind::Literal(literal);
                changed = true;
            }
        });
        if !changed {
            break;
        }
    }
}

/// A constant as the value it is: what this pass holds of it, or for a
/// literal it holds as written (a float, a string, a bool, `null`), the
/// literal read through the parentheses and the minus around it
/// (EXPR-1). `None` for one it does not hold: an expression not
/// evaluated, a date or time, a tenor.
fn held(constant: &Constant) -> Option<Value> {
    Some(match &constant.value {
        ConstantValue::Int(value) => Value::Int(*value),
        ConstantValue::Duration(value) => Value::Duration(*value),
        ConstantValue::Other(kind) => {
            let (negative, text) = bare_literal(&constant.text);
            match *kind {
                "a float" => {
                    let value = decode_float(text).ok()?;
                    Value::Float(if negative { -value } else { value })
                }
                "a string" if !negative => Value::Str(decode_string(text).ok()?),
                "a bool" if !negative => match text {
                    "true" => Value::Bool(true),
                    "false" => Value::Bool(false),
                    _ => return None,
                },
                "null" if !negative => Value::Null,
                _ => return None,
            }
        }
        _ => return None,
    })
}

/// What a generic value parameter is, given `constant`, in a default of
/// a field of type `ty`: the literal of its value, or the member of the
/// field's enum it names. `None` where the value is not held: the
/// parameter is then left as it is, and the default is not judged.
fn given_value(program: &Program, constant: &Constant, ty: &Type) -> Option<ExprKind> {
    match &constant.value {
        ConstantValue::Member(name) => member(program, ty, name),
        _ => held(constant).map(ExprKind::Literal),
    }
}

/// The member `name` of the enum `ty`, or of the enum a `ty?` holds, as
/// an expression.
fn member(program: &Program, ty: &Type, name: &str) -> Option<ExprKind> {
    let ty = match ty {
        Type::Nullable(inner) => inner,
        other => other,
    };
    let Type::Named {
        module,
        declaration,
        ..
    } = ty
    else {
        return None;
    };
    let Some(Detail::Enum(read)) = program.modules[*module].details.get(*declaration) else {
        return None;
    };
    let member = read.members.iter().position(|member| member.name == name)?;
    Some(ExprKind::EnumMember {
        module: *module,
        declaration: *declaration,
        member,
    })
}

/// The negative number `kind` writes, where it is core's negation of a
/// number literal: `-0.5`, read as a call of `neg_` or not read as one,
/// and `neg_(0.5)`, which binds the same operator (EXPR-10). `None` for
/// any other, and for the negation of the least `i64`, which is no
/// `i64`.
fn negated(program: &Program, kind: &ExprKind) -> Option<Value> {
    // Core's `neg_`, the operator `-` before an operand stands for.
    let negation = |(module, declaration): (usize, usize)| {
        Some(module) == program.core()
            && program.modules[module].declarations[declaration].name
                == UnaryOperator::Negate.identity()
    };
    let operand = match kind {
        ExprKind::Call {
            form: Form::Unary(UnaryOperator::Negate),
            arguments,
            ..
        } if arguments.len() == 1 => &arguments[0].value,
        ExprKind::Call {
            callee: Callee::Candidate { operator, .. },
            arguments,
            ..
        } if arguments.len() == 1 && negation(*operator) => &arguments[0].value,
        ExprKind::Unary {
            operator: UnaryOperator::Negate,
            operand,
        } => operand,
        _ => return None,
    };
    match &operand.kind {
        ExprKind::Literal(Value::Int(value)) => value.checked_neg().map(Value::Int),
        ExprKind::Literal(Value::Float(value)) => Some(Value::Float(-value)),
        ExprKind::Literal(Value::Duration(value)) => value.checked_neg().map(Value::Duration),
        _ => None,
    }
}

/// True when `ty` is concrete: no generic parameter is in it, and
/// nothing that is not known.
fn concrete(ty: &Type) -> bool {
    let argument = |argument: &Argument| match argument {
        Argument::Type(ty) => concrete(ty),
        Argument::Constant(constant) => !matches!(
            constant.value,
            ConstantValue::Parameter { .. } | ConstantValue::Unevaluated
        ),
    };
    match ty {
        Type::Unknown | Type::Parameter { .. } => false,
        Type::Named { arguments, .. } => arguments.iter().all(argument),
        Type::Tuple(elements) => elements.iter().all(concrete),
        Type::List { element, size } => {
            concrete(element)
                && size
                    .as_ref()
                    .is_none_or(|size| argument(&Argument::Constant(size.clone())))
        }
        Type::Set(inner)
        | Type::Ref(inner)
        | Type::Endpoint(inner)
        | Type::Atomic(inner)
        | Type::Delta(inner)
        | Type::Trace { cell: inner, .. }
        | Type::Nullable(inner) => concrete(inner),
        Type::Dict { key, value } => concrete(key) && concrete(value),
        Type::RollingWindow { value, .. } => concrete(value),
        Type::Result { value, error } => concrete(value) && concrete(error),
        Type::Scalar(_) | Type::Signal | Type::Error | Type::ErrorContext => true,
    }
}
