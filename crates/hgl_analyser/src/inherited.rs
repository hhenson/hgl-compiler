//! Compares the defaults a struct has for one field from two of its
//! parents (UNIT-9).
//!
//! `declared` works out the fields each struct has, and finds where two
//! parents give one field a default and the struct gives it none of its
//! own. Whether those are one default is a question of what each is
//! written as, which `values` reads. So this pass runs after it: each
//! default is compared as `values` has read it, with the arguments its
//! path gives the struct that writes it in place.
//!
//! UNIT-9: "Compare inherited defaults by bound expression and
//! canonical type after substitution, before folding. Declaration
//! identity matters; source location and alias spelling do not.
//! Conflicting inherited defaults require an explicit child override."

use hgl_diagnostics::{Category, Code, Diagnostic, Diagnostics, Location};
use hgl_lexer::literal::{decode_float, decode_string};
use hgl_source::Span;

use crate::codes;
use crate::declared::substitute_within;
use crate::hir::{Callee, Expr, ExprKind, Form, Stmt, UnaryOperator, Value};
use crate::program::Program;
use crate::types::{Argument, Constant, ConstantValue, Detail, FieldRef, Type};

/// The defaults two parents give one field of a struct, which gives
/// that field none of its own.
#[derive(Debug, Clone)]
pub(crate) struct InheritedDefaults {
    /// The struct: the position of its module in `Program::modules`,
    /// and its own in that module's declarations.
    pub(crate) owner: (usize, usize),
    /// The field's name.
    pub(crate) field: String,
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

/// Reports each struct that has two different defaults for one field
/// from its parents and gives the field none of its own: once for each
/// field, at the later parent.
///
/// A pair either of whose defaults is not known, or names what has no
/// value here, is left out (ERR-9).
pub(crate) fn check(
    program: &Program,
    pending: &[InheritedDefaults],
    diagnostics: &mut Diagnostics,
) {
    let mut reported: Vec<((usize, usize), &str)> = Vec::new();
    for pair in pending {
        if reported.contains(&(pair.owner, pair.field.as_str())) {
            continue;
        }
        let [earlier, later] = &pair.defaults;
        // Every expression of both is put at one place, so that where
        // each is written does not tell them apart.
        let place = Location::new(earlier.at.file, Span::new(0, 0));
        let (Some(mut first), Some(mut second)) =
            (bound(program, earlier, place), bound(program, later, place))
        else {
            continue;
        };
        // A default whose type is not known, as one of a field whose
        // type names a generic parameter is not (it is read with no
        // type expected), is compared by what it is written as alone:
        // the field, and so the type each default is for, is one.
        if !knows_types(&first) || !knows_types(&second) {
            forget_types(&mut first);
            forget_types(&mut second);
        }
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

/// The default `default` names, as `values` has read it, with the
/// arguments of its path in place of the generic parameters of the
/// struct that writes it, and every expression put at `place`. `None`
/// where it is not read, or names what this pass cannot put in place.
fn bound(program: &Program, default: &InheritedDefault, place: Location) -> Option<Expr> {
    let written = default.written;
    let mut value = program.modules[written.module]
        .defaults
        .get(written.declaration)?
        .get(written.field)?
        .clone()?;
    let map: Vec<(Location, &Argument)> = default
        .arguments
        .iter()
        .map(|(at, argument)| (*at, argument))
        .collect();
    let mut known = true;
    // `walk_mut` visits each expression before those inside it, and
    // then those inside what the visit made of it.
    value.walk_mut(&mut |expression| {
        expression.at = place;
        expression.ty = substitute_within(&expression.ty, &map);
        canonical_type(&mut expression.ty);
        // A minus before a number is the negative number it writes, as
        // a generic argument `-3` is: LEX-7 reads the least `i64` so.
        if let Some(value) = negated(&expression.kind) {
            expression.kind = ExprKind::Literal(value);
        }
        match &mut expression.kind {
            ExprKind::Unread => known = false,
            // A generic value parameter of the struct that writes the
            // default: the constant its path gives it.
            ExprKind::GenericValue { at, .. } => {
                let given = map
                    .iter()
                    .find(|(declared, _)| declared == at)
                    .map(|(_, argument)| *argument);
                match given {
                    // One of the struct that has the field: itself.
                    None => {}
                    Some(Argument::Constant(constant)) => match &constant.value {
                        ConstantValue::Int(value) => {
                            expression.kind = ExprKind::Literal(Value::Int(*value));
                        }
                        ConstantValue::Duration(value) => {
                            expression.kind = ExprKind::Literal(Value::Duration(*value));
                        }
                        ConstantValue::Parameter { name, at } => {
                            expression.kind = ExprKind::GenericValue {
                                name: name.clone(),
                                at: *at,
                            };
                        }
                        ConstantValue::Member(name) => {
                            match member(program, &expression.ty, name) {
                                Some(kind) => expression.kind = kind,
                                None => known = false,
                            }
                        }
                        // A `bool`, an `f64` or a `str`, which this pass
                        // holds as it is written: the literal it is.
                        ConstantValue::Other(_) => match literal_of(constant) {
                            Some(value) => expression.kind = ExprKind::Literal(value),
                            None => known = false,
                        },
                        _ => known = false,
                    },
                    Some(Argument::Type(_)) => known = false,
                }
            }
            ExprKind::Call { callee, form, .. } => {
                // A symbol binds the operator a call by its name does
                // (EXPR-10): how the call is written is no part of what
                // it is bound to.
                *form = Form::Named;
                let generics = match callee {
                    Callee::Function { generics, .. } | Callee::Candidate { generics, .. } => {
                        Some(generics)
                    }
                    _ => None,
                };
                for generic in generics.into_iter().flatten() {
                    match generic {
                        Argument::Type(ty) => *ty = substitute_within(ty, &map),
                        // A generic value parameter given as a callee's
                        // argument: `limit<N>()` is `limit<3>()` where
                        // the path gives `N` 3.
                        Argument::Constant(constant) => {
                            if let ConstantValue::Parameter { at, .. } = &constant.value {
                                let given = map
                                    .iter()
                                    .find(|(declared, _)| declared == at)
                                    .map(|(_, argument)| *argument);
                                match given {
                                    None => {}
                                    Some(Argument::Constant(given)) => *constant = given.clone(),
                                    Some(Argument::Type(_)) => known = false,
                                }
                            }
                        }
                    }
                    canonical_argument(generic);
                }
            }
            ExprKind::Block { statements, .. } => {
                for statement in statements {
                    put(statement, place);
                }
            }
            _ => {}
        }
    });
    known.then_some(value)
}

/// The member `name` of the enum `ty`, as an expression.
fn member(program: &Program, ty: &Type, name: &str) -> Option<ExprKind> {
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

/// The literal a constant is, where this pass holds it as it is
/// written: a `bool`, an `f64` or a `str` of one token. `None` for any
/// other.
fn literal_of(constant: &Constant) -> Option<Value> {
    let text = constant.unspaced();
    match text.as_str() {
        "true" => Some(Value::Bool(true)),
        "false" => Some(Value::Bool(false)),
        text if text.starts_with('"') => decode_string(text).ok().map(Value::Str),
        // A float after one minus is the negative float.
        text => match text.strip_prefix('-') {
            Some(number) => decode_float(number).ok().map(|value| Value::Float(-value)),
            None => decode_float(text).ok().map(Value::Float),
        },
    }
}

/// The negative number `kind` writes, where it is a minus before a
/// number literal: by symbol, read as core's `neg_`, or not read as a
/// call. `None` for any other, and for the negation of the least `i64`,
/// which is no `i64`.
fn negated(kind: &ExprKind) -> Option<Value> {
    let operand = match kind {
        ExprKind::Call {
            form: Form::Unary(UnaryOperator::Negate),
            arguments,
            ..
        } if arguments.len() == 1 => &arguments[0].value,
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

/// Writes a constant this pass holds as it is written in one form for
/// each value, so that two that are one value compare equal: `1.0` and
/// `1.00`, or a string written with an escape and without one. Its
/// other constants it holds by their value.
fn canonical(constant: &mut Constant) {
    if !matches!(constant.value, ConstantValue::Other(_)) {
        return;
    }
    let text = constant.unspaced();
    // A float after a minus: the minus, then the float's own form.
    let (sign, number) = match text.strip_prefix('-') {
        Some(number) => ("-", number.to_string()),
        None => ("", text),
    };
    let one = Constant {
        text: number,
        value: constant.value.clone(),
    };
    let written = match literal_of(&one) {
        Some(Value::Float(value)) => format!("{sign}{value:?}"),
        // `{:?}` writes a string with its escapes in one form.
        Some(Value::Str(value)) if sign.is_empty() => format!("{value:?}"),
        Some(Value::Bool(value)) if sign.is_empty() => value.to_string(),
        _ => return,
    };
    constant.text = written;
}

/// [`canonical`], for a generic argument: a constant, or each constant
/// inside a type.
fn canonical_argument(argument: &mut Argument) {
    match argument {
        Argument::Type(ty) => canonical_type(ty),
        Argument::Constant(constant) => canonical(constant),
    }
}

/// [`canonical`], for each constant inside `ty`: a size, a window's
/// bounds, a generic argument.
fn canonical_type(ty: &mut Type) {
    match ty {
        Type::Named { arguments, .. } => arguments.iter_mut().for_each(canonical_argument),
        Type::Tuple(elements) => elements.iter_mut().for_each(canonical_type),
        Type::List { element, size } => {
            canonical_type(element);
            size.iter_mut().for_each(canonical);
        }
        Type::RollingWindow {
            value,
            maximum,
            minimum,
        } => {
            canonical_type(value);
            canonical(maximum);
            minimum.iter_mut().for_each(canonical);
        }
        Type::Set(inner)
        | Type::Ref(inner)
        | Type::Endpoint(inner)
        | Type::Atomic(inner)
        | Type::Delta(inner)
        | Type::Trace { cell: inner, .. }
        | Type::Nullable(inner) => canonical_type(inner),
        Type::Dict { key, value } => {
            canonical_type(key);
            canonical_type(value);
        }
        Type::Result { value, error } => {
            canonical_type(value);
            canonical_type(error);
        }
        Type::Scalar(_)
        | Type::Parameter { .. }
        | Type::Signal
        | Type::Error
        | Type::ErrorContext
        | Type::Unknown => {}
    }
}

/// Puts `statement`, and every statement inside it, at `place`. The
/// expressions inside are put there by the walk that calls this.
fn put(statement: &mut Stmt, place: Location) {
    let mut bodies: Vec<&mut Vec<Stmt>> = Vec::new();
    match statement {
        Stmt::Declare { .. } | Stmt::Expr(_) => {}
        Stmt::Assign { at, .. }
        | Stmt::AssignPlace { at, .. }
        | Stmt::Inject { at, .. }
        | Stmt::Return { at, .. }
        | Stmt::Assert { at, .. }
        | Stmt::Yield { at, .. }
        | Stmt::Unread { at } => *at = place,
        Stmt::Hook { at, body, .. }
        | Stmt::AssertRaises { at, body, .. }
        | Stmt::For { at, body, .. }
        | Stmt::While { at, body, .. } => {
            *at = place;
            bodies.push(body);
        }
        Stmt::Switch {
            at, cases, default, ..
        } => {
            *at = place;
            bodies.extend(cases.iter_mut().map(|case| &mut case.body));
            bodies.extend(default.as_mut());
        }
    }
    for body in bodies {
        for statement in body {
            put(statement, place);
        }
    }
}

/// True when the type of every expression of `value` is known.
fn knows_types(value: &Expr) -> bool {
    let mut known = true;
    value.walk(&mut |expression| known &= knows(&expression.ty));
    known
}

/// Makes the type of every expression of `value` not known.
fn forget_types(value: &mut Expr) {
    value.walk_mut(&mut |expression| expression.ty = Type::Unknown);
}

/// True when nothing in `ty` is [`Type::Unknown`].
fn knows(ty: &Type) -> bool {
    let argument = |argument: &Argument| match argument {
        Argument::Type(ty) => knows(ty),
        Argument::Constant(_) => true,
    };
    match ty {
        Type::Unknown => false,
        Type::Named { arguments, .. } => arguments.iter().all(argument),
        Type::Tuple(elements) => elements.iter().all(knows),
        Type::List { element, .. } => knows(element),
        Type::Set(inner)
        | Type::Ref(inner)
        | Type::Endpoint(inner)
        | Type::Atomic(inner)
        | Type::Delta(inner)
        | Type::Trace { cell: inner, .. }
        | Type::Nullable(inner) => knows(inner),
        Type::Dict { key, value } => knows(key) && knows(value),
        Type::RollingWindow { value, .. } => knows(value),
        Type::Result { value, error } => knows(value) && knows(error),
        Type::Scalar(_)
        | Type::Parameter { .. }
        | Type::Signal
        | Type::Error
        | Type::ErrorContext => true,
    }
}
