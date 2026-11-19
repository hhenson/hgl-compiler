//! Compile-time constants: how one is read from source, and how two are
//! compared.
//!
//! A compile-time constant stands where the compiler needs its value
//! before the program runs (GEN-8, UNIT-13): a generic value argument,
//! `Box<2.0>` or `pick<.slow>(x)`; the operand of a `requires`; the
//! default of a generic value parameter, of a parameter or of a field;
//! a `switch` label (CTRL-6); a list's size and a window's (TYPE-10,
//! TYPE-11); a delta's key (TYPE-16); a tuple's index (EXPR-7); an
//! enum's number (TYPE-7). Each of those places reads it here, with
//! [`read`], and holds what it comes to as a
//! [`ConstantValue`]: the value, not how it is written. Two are
//! compared with [`ConstantValue::same`], the one comparison every
//! place asks.
//!
//! The reader is given the type the constant's place expects. EXPR-5:
//! "An integer literal is an `f64` where one is expected", so `1` given
//! to `const X: f64` is the `f64` one, and is `1.0`. A constant of
//! another type than its place's is a [`Fault::Mismatch`], which the
//! place reports (`type.mismatch`): `IBox<"a">` for `const X: i64`.
//!
//! What it reads, GEN-8's "a literal, an enum member however it is
//! written (TYPE-7), `Zero.nan` or `.nan`, a constant (UNIT-13), a
//! generic value parameter in scope, or a constant expression of
//! those":
//!
//! * every literal, its value read as the lexer reads it;
//! * `.member`, `Enum.member`, `alias::Enum.member`, `Enum(1)` and
//!   `Enum("name")`;
//! * the name of a constant of a module, followed to its value, which
//!   is read as a value of the constant's own type;
//! * the name of a generic value parameter, held as that parameter;
//! * parentheses; `-` and `!`; `+`, `-`, `*`, `/`, `//` and `%` of
//!   numbers as core's numeric contract has them (NUM-1, NUM-3); `+`
//!   and `-` of durations, and a duration times an `i64` (OPS-1); the
//!   sum, difference, negation and scaling of tenors (TEN-2); `+` of two
//!   strings; `==`, `!=` and the order of numbers and durations; `&&`
//!   and `||`;
//! * a list and a tuple written out, of constants.
//!
//! What it cannot work out is [`ConstantValue::Unevaluated`]: an
//! operation that overflows, a division by zero, a symbol given
//! configuration, a date moved by a tenor. That is "not valued": equal to
//! nothing and unequal to nothing. What is no compile-time constant at
//! all, a local or a call, is [`Fault::NotConstant`].

use std::cell::RefCell;
use std::collections::HashMap;

use hgl_diagnostics::{Category, Diagnostic, Location};
use hgl_lexer::literal::{
    MAX_INTEGER_MAGNITUDE, decode_duration, decode_float, decode_integer, decode_string,
    decode_temporal, decode_tenor,
};
use hgl_parser::{HasSpan, SyntaxKind, SyntaxNode, SyntaxToken};
use hgl_source::FileId;

use crate::codes;
use crate::hir::{BinaryOperator, CalendarDate, ClockTime, Temporal, Tenor, UnaryOperator, Value};
use crate::program::{Binding, DeclarationKind, LocalKind, Program};
use crate::syntax::{name_token, tokens};
use crate::types::{ConstantValue, Detail, NamedKind, Scalar, Type};

/// How deep one expression is read. A constant nested deeper is not
/// valued, so that reading keeps little on the call stack whatever it
/// is given.
const DEPTH: u32 = 32;

// ---- What a reading is given ----

/// What the reader asks of the place a constant is written in.
///
/// `&dyn Fn` is a reference to any function or closure with that
/// signature: each pass that reads constants has its own tables of
/// what a name is bound to.
pub(crate) struct Scope<'s> {
    /// The program.
    pub(crate) program: &'s Program,
    /// The file the constant is written in.
    pub(crate) file: FileId,
    /// What the name used at an offset of a file is bound to (UNIT-10).
    pub(crate) binding: &'s dyn Fn(FileId, u32) -> Option<&'s Binding>,
    /// The type of the generic value parameter declared at a place, one
    /// of the declaration the constant is written in. `None` where it is
    /// not known.
    pub(crate) parameter: &'s dyn Fn(Location) -> Option<Type>,
    /// The constants of modules followed so far.
    pub(crate) constants: &'s Constants,
}

/// What following the constants of modules keeps (UNIT-13: "The
/// constant has that one value wherever it is read").
#[derive(Default)]
pub(crate) struct Constants {
    /// The constants being worked out, so that one whose value depends
    /// on itself, which is reported as that (`constant.cycle`), ends
    /// here.
    following: RefCell<Vec<(usize, usize)>>,
    /// The values worked out, by module and declaration. A constant many
    /// others name is read once.
    known: RefCell<HashMap<(usize, usize), Result<Typed, Fault>>>,
}

// ---- What a reading gives ----

/// Why a constant has no value for its place.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Fault {
    /// It is no compile-time constant (GEN-8: "Any other rejects"): it
    /// reads a local, a parameter, a field, or calls a function.
    NotConstant,
    /// It is a value of another type than its place expects (EXPR-5):
    /// this one, or `null` where the type has no `?` (TYPE-14).
    Mismatch(Option<Box<Type>>),
    /// It names a member its enum does not have (TYPE-7): what it names,
    /// for the message: `` `fast` ``, `` named `fast` `` or `numbered 7`.
    NoMember(String),
    /// It is `.member` where no enum is expected (TYPE-7: "With no enum
    /// type expected, `.member` rejects").
    NoEnum,
    /// Something in it has been reported already: a name nothing binds,
    /// a literal the lexer rejected, a constant in a cycle (ERR-9).
    Reported,
}

/// A constant as it was read, before it is held to its place's type.
#[derive(Debug, Clone)]
pub(crate) struct Typed {
    /// What it comes to.
    pub(crate) value: ConstantValue,
    /// Its type; `None` where that is not known.
    ty: Option<Type>,
    /// How its place may still give it a type.
    open: Open,
    /// Of a list or a tuple read with no type expected of its elements:
    /// each element as it was read, for its place to give a type later.
    parts: Vec<Typed>,
}

/// How a constant read with no type expected may still take its
/// place's.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Open {
    /// It has its type.
    No,
    /// An integer written with literals alone, `2` or `1 + 1`: an `f64`
    /// where one is expected (EXPR-5).
    Integer,
    /// `.member`: a member of the enum its place expects (TYPE-7).
    Member,
}

impl Typed {
    /// Its type, where that is known.
    pub(crate) fn ty(&self) -> Option<&Type> {
        self.ty.as_ref()
    }

    fn of(value: ConstantValue, ty: Option<Type>) -> Typed {
        Typed {
            value,
            ty,
            open: Open::No,
            parts: Vec::new(),
        }
    }

    fn scalar(value: ConstantValue, scalar: Scalar) -> Typed {
        Typed::of(value, Some(Type::Scalar(scalar)))
    }

    fn unevaluated(ty: Option<Type>) -> Typed {
        Typed::of(ConstantValue::Unevaluated, ty)
    }
}

impl Fault {
    /// How a place reports this fault, where a report is due: for a
    /// constant given `place`, described as `` `X` of `Box` ``, which
    /// expects `expected`. `None` for what is no constant, which each
    /// place says in its own words, and for what has been reported.
    pub(crate) fn report(&self, at: Location, place: &str, expected: &Type) -> Option<Diagnostic> {
        Some(match self {
            Fault::Mismatch(found) => {
                let found = match found {
                    Some(found) => described_type(found),
                    None => "`null`".to_string(),
                };
                crate::error(
                    Category::Type,
                    codes::TYPE_MISMATCH,
                    at,
                    format!("this is {found}, and {place} is `{expected}`"),
                    "EXPR-5",
                )
            }
            Fault::NoMember(what) => crate::error(
                Category::Name,
                codes::MEMBER_UNKNOWN,
                at,
                format!("`{expected}` has no member {what}"),
                "TYPE-7",
            ),
            Fault::NoEnum => crate::error(
                Category::Type,
                codes::ENUM_CONTEXT,
                at,
                format!(
                    "`.member` names a member of the enum its place expects, and {place} is `{expected}`"
                ),
                "TYPE-7",
            ),
            Fault::NotConstant | Fault::Reported => return None,
        })
    }
}

/// What a constant read as `read` is, for a message: `an \`i64\``,
/// `` `null` ``, `the enum member \`.fast\``.
pub(crate) fn described(read: &Typed) -> String {
    match (&read.value, read.open, &read.ty) {
        (ConstantValue::Null, _, _) => "`null`".to_string(),
        (ConstantValue::Member(name), Open::Member, _) => format!("the enum member `.{name}`"),
        (_, _, Some(ty)) => described_type(ty),
        _ => "a constant".to_string(),
    }
}

/// A value of type `ty`, for a message: `an \`i64\``, `a \`str\``.
pub(crate) fn described_type(ty: &Type) -> String {
    let text = ty.to_string();
    let vowel = matches!(ty, Type::Scalar(Scalar::I64 | Scalar::F64 | Scalar::Any));
    format!("{} `{text}`", if vowel { "an" } else { "a" })
}

/// A constant's kind in words, for a message: `a float`, `a string`.
pub(crate) fn kind_of(value: &ConstantValue, ty: &Type) -> String {
    match ty {
        Type::Scalar(scalar) => match scalar {
            Scalar::I64 => "an integer".to_string(),
            Scalar::F64 => "a float".to_string(),
            Scalar::Str => "a string".to_string(),
            Scalar::Bool => "a bool".to_string(),
            Scalar::Duration => "a duration".to_string(),
            Scalar::Tenor => "a tenor".to_string(),
            Scalar::Date
            | Scalar::Time
            | Scalar::Datetime
            | Scalar::CivilDatetime
            | Scalar::Timezone
            | Scalar::ZonedDatetime
            | Scalar::ZonedTime => "a date or time".to_string(),
            other => format!("a `{}`", other.as_str()),
        },
        Type::Named {
            kind: NamedKind::Enum,
            ..
        } => "a member of an enum".to_string(),
        _ if matches!(value, ConstantValue::Null) => "null".to_string(),
        other => format!("a `{other}`"),
    }
}

// ---- Reading ----

/// What the constant expression `node` comes to as a value of
/// `expected`, the type its place expects. `None` where that type is
/// not known: the constant is then held as it reads, `1` as an `i64`.
pub(crate) fn read(
    node: &SyntaxNode,
    expected: Option<&Type>,
    scope: &Scope,
) -> Result<ConstantValue, Fault> {
    let read = typed(node, expected, scope)?;
    fit(read, expected, scope.program)
}

/// [`read`] with nothing yet held to the place: what [`fit`] gives a
/// type later, once the place's type is known. A call's arguments are
/// read so before its candidate is selected (GEN-6).
pub(crate) fn typed(
    node: &SyntaxNode,
    expected: Option<&Type>,
    scope: &Scope,
) -> Result<Typed, Fault> {
    let expected = expected.filter(|expected| !unknown(expected));
    value(node, expected, scope, 0)
}

/// `read` as a value of `expected`, the type its place expects
/// (EXPR-5). `None` where that is not known.
pub(crate) fn fit(
    read: Typed,
    expected: Option<&Type>,
    program: &Program,
) -> Result<ConstantValue, Fault> {
    let Some(expected) = expected.filter(|expected| !unknown(expected)) else {
        return Ok(read.value);
    };
    // TYPE-14: "`T?` denotes an ordinary value that is either a T or
    // null."
    if let Type::Nullable(inner) = expected {
        if matches!(read.value, ConstantValue::Null) {
            return Ok(ConstantValue::Null);
        }
        // One whose own type is nullable, a constant `const A: i64? = 1`
        // or a parameter of a `T?`, is held to the whole type: it is a
        // `T?` that is no null, and no `T`.
        if let Some(ty @ Type::Nullable(_)) = &read.ty {
            return if fits(ty, expected) {
                Ok(read.value)
            } else {
                Err(Fault::Mismatch(Some(Box::new(ty.clone()))))
            };
        }
        return fit(read, Some(inner), program);
    }
    match (&read.value, read.open) {
        // "It does not make null an admitted value of unqualified T."
        (ConstantValue::Null, _) => return Err(Fault::Mismatch(None)),
        (ConstantValue::Member(name), Open::Member) => {
            return match enum_of(expected) {
                Some(of) => member_named(program, of, name),
                None => Err(Fault::NoEnum),
            };
        }
        // EXPR-5: "An integer literal is an `f64` where one is
        // expected." So is one made of literals alone, `1 + 1`, read
        // where no type was known (GEN-5: core's arithmetic selects the
        // `f64` candidate for the requested output).
        (ConstantValue::Int(value), Open::Integer) if is_float(expected) => {
            // `as` converts an integer to the nearest `f64`.
            return Ok(ConstantValue::Float(*value as f64));
        }
        _ => {}
    }
    // A list or a tuple read before its place's type was known: each
    // element is given its type now, as it would have been as it was
    // read (EXPR-5): `[1]` given to a `list<f64>` is `[1.0]`.
    let read = match (&read.value, expected) {
        (ConstantValue::List(values), Type::List { element, .. })
            if !read.parts.is_empty() && read.parts.len() == values.len() =>
        {
            let values = read
                .parts
                .into_iter()
                .map(|part| fit(part, Some(element), program))
                .collect::<Result<Vec<ConstantValue>, Fault>>()?;
            Typed::of(ConstantValue::List(values), Some(expected.clone()))
        }
        (ConstantValue::Tuple(values), Type::Tuple(elements))
            if !read.parts.is_empty()
                && read.parts.len() == values.len()
                && elements.len() == values.len() =>
        {
            let values = read
                .parts
                .into_iter()
                .zip(elements)
                .map(|(part, element)| fit(part, Some(element), program))
                .collect::<Result<Vec<ConstantValue>, Fault>>()?;
            return Ok(ConstantValue::Tuple(values));
        }
        _ => read,
    };
    // TYPE-10: "Size participates in identity": a list of another
    // length than the size expected is no value of it.
    if let (
        ConstantValue::List(values),
        Type::List {
            element,
            size: Some(size),
        },
    ) = (&read.value, expected)
        && let ConstantValue::Int(count) = size.value
        && i64::try_from(values.len()).ok() != Some(count)
    {
        let length = values.len().to_string();
        return Err(Fault::Mismatch(Some(Box::new(Type::List {
            element: element.clone(),
            size: Some(crate::types::Constant {
                text: length.clone(),
                value: ConstantValue::Int(i64::try_from(values.len()).unwrap_or(i64::MAX)),
            }),
        }))));
    }
    match &read.ty {
        Some(ty) if !fits(ty, expected) => Err(Fault::Mismatch(Some(Box::new(ty.clone())))),
        _ => Ok(read.value),
    }
}

/// True when a value of type `ty` is a value of `expected`.
fn fits(ty: &Type, expected: &Type) -> bool {
    match (ty, expected) {
        _ if unknown(ty) || unknown(expected) => true,
        (Type::Nullable(left), Type::Nullable(right)) => fits(left, right),
        (_, Type::Nullable(inner)) => fits(ty, inner),
        (Type::List { element, .. }, Type::List { element: other, .. }) => fits(element, other),
        (Type::Tuple(left), Type::Tuple(right)) => {
            left.len() == right.len()
                && left
                    .iter()
                    .zip(right)
                    .all(|(left, right)| fits(left, right))
        }
        (
            Type::Named {
                module,
                declaration,
                ..
            },
            Type::Named {
                module: other_module,
                declaration: other_declaration,
                ..
            },
        ) => (module, declaration) == (other_module, other_declaration),
        // A generic parameter's type is known once the parameter is
        // given a value: here it is not.
        (Type::Parameter { .. }, _) | (_, Type::Parameter { .. }) => true,
        (left, right) => left == right,
    }
}

/// True for a type that says nothing of what a constant of it is: one
/// not known, `any`, or a generic parameter's not yet given.
fn unknown(ty: &Type) -> bool {
    matches!(
        ty,
        Type::Unknown | Type::Scalar(Scalar::Any) | Type::Parameter { .. }
    )
}

/// True where an `f64` is expected.
fn is_float(ty: &Type) -> bool {
    matches!(ty, Type::Scalar(Scalar::F64))
}

/// `ty` under any `?`.
fn non_null(ty: &Type) -> &Type {
    match ty {
        Type::Nullable(inner) => non_null(inner),
        other => other,
    }
}

/// The enum `ty` is, under any `?`.
fn enum_of(ty: &Type) -> Option<(usize, usize)> {
    match ty {
        Type::Nullable(inner) => enum_of(inner),
        Type::Named {
            module,
            declaration,
            kind: NamedKind::Enum,
            ..
        } => Some((*module, *declaration)),
        _ => None,
    }
}

fn value(
    node: &SyntaxNode,
    expected: Option<&Type>,
    scope: &Scope,
    depth: u32,
) -> Result<Typed, Fault> {
    if depth > DEPTH {
        return Ok(Typed::unevaluated(None));
    }
    // TYPE-14: a value of a `T?` other than `null` is a T, and is read
    // as one: `[1]` where a `list<f64>?` is expected is a list of
    // `f64`. `null` is held to the whole type by `fit`.
    let expected = expected.map(non_null);
    match node.kind() {
        SyntaxKind::Literal => literal_constant(node, expected),
        // EXPR-1: "Parentheses select grouping": `(2.0)` is `2.0`.
        SyntaxKind::ParenExpr => match node.children().next() {
            Some(inner) => value(&inner, expected, scope, depth + 1),
            None => Err(Fault::Reported),
        },
        SyntaxKind::NameExpr | SyntaxKind::NamedType => name(node, scope),
        // TYPE-7: "Where a position expects the enum's type, `.member`
        // names one too."
        SyntaxKind::ImpliedMember => {
            let Some(member) = tokens(node).last() else {
                return Err(Fault::Reported);
            };
            match expected.and_then(enum_of) {
                Some(of) => Ok(Typed::of(
                    member_named(scope.program, of, member.text())?,
                    Some(enum_type(scope.program, of)),
                )),
                None if expected.is_some_and(|expected| !unknown(expected)) => Err(Fault::NoEnum),
                None => Ok(Typed {
                    value: ConstantValue::Member(member.text().to_string()),
                    ty: None,
                    open: Open::Member,
                    parts: Vec::new(),
                }),
            }
        }
        SyntaxKind::MemberExpr => qualified_member(node, scope),
        SyntaxKind::CallExpr => constructed_member(node, scope, depth),
        SyntaxKind::UnaryExpr => unary(node, expected, scope, depth),
        SyntaxKind::BinaryExpr => binary(node, expected, scope, depth),
        SyntaxKind::SequenceExpr => sequence(node, expected, scope, depth),
        // TYPE-10: "`unbounded` is the size sentinel."
        SyntaxKind::UnboundedSize => Ok(Typed::of(ConstantValue::Unbounded, None)),
        SyntaxKind::TupleExpr => tuple(node, expected, scope, depth),
        _ => Err(Fault::NotConstant),
    }
}

/// A literal's value and type (TYPE-1, TYPE-2, LEX-11), read as the
/// lexer reads it. `None` for `null`, whose type is its place's, and for
/// a literal the lexer has reported.
pub(crate) fn literal(token: &SyntaxToken) -> Option<(Value, Scalar)> {
    let text = token.text();
    match token.kind() {
        SyntaxKind::Int => decode_integer(text)
            .ok()
            .and_then(|value| i64::try_from(value).ok())
            .map(|value| (Value::Int(value), Scalar::I64)),
        SyntaxKind::Float => decode_float(text)
            .ok()
            .map(|value| (Value::Float(value), Scalar::F64)),
        SyntaxKind::Str => decode_string(text)
            .ok()
            .map(|value| (Value::Str(value), Scalar::Str)),
        SyntaxKind::KwTrue => Some((Value::Bool(true), Scalar::Bool)),
        SyntaxKind::KwFalse => Some((Value::Bool(false), Scalar::Bool)),
        SyntaxKind::Duration => decode_duration(text)
            .ok()
            .map(|value| (Value::Duration(value), Scalar::Duration)),
        // LEX-28, TYPE-17: a tenor literal is a `tenor`.
        SyntaxKind::Tenor => decode_tenor(text)
            .ok()
            .map(|value| (Value::Tenor(value), Scalar::Tenor)),
        SyntaxKind::Temporal => {
            temporal(text).map(|(parts, scalar)| (Value::Temporal(parts), scalar))
        }
        _ => None,
    }
}

/// The parts of a temporal literal and its type, which goes by the
/// parts it writes (LEX-11: "Offset-free date/time is civil, not an
/// implicit UTC instant. A zoned datetime requires an explicit
/// offset"). `None` for one the lexer has reported.
fn temporal(text: &str) -> Option<(Temporal, Scalar)> {
    let literal = decode_temporal(text).ok()?;
    let parts = Temporal {
        date: literal.date.map(|date| CalendarDate {
            year: date.year,
            month: date.month,
            day: date.day,
        }),
        time: literal.time.map(|time| ClockTime {
            hour: time.hour,
            minute: time.minute,
            second: time.second,
            microsecond: time.microsecond,
        }),
        offset: literal.offset.map(|offset| offset.minutes),
        zone: literal.zone.map(str::to_string),
    };
    let scalar = temporal_type(&parts);
    Some((parts, scalar))
}

/// The type of a temporal value, which goes by the parts it has (the
/// table of `hir::Temporal`).
pub(crate) fn temporal_type(parts: &Temporal) -> Scalar {
    let zone = parts.zone.is_some();
    match (parts.date, parts.time, parts.offset) {
        (Some(_), None, _) => Scalar::Date,
        (Some(_), Some(_), None) => Scalar::CivilDatetime,
        (Some(_), Some(_), Some(_)) if zone => Scalar::ZonedDatetime,
        (Some(_), Some(_), Some(_)) => Scalar::Datetime,
        (None, Some(_), _) if zone => Scalar::ZonedTime,
        (None, Some(_), _) => Scalar::Time,
        (None, None, _) => Scalar::Timezone,
    }
}

impl From<Value> for ConstantValue {
    fn from(value: Value) -> ConstantValue {
        match value {
            Value::Int(value) => ConstantValue::Int(value),
            // NUM-5: a NaN is equal to nothing; no constant is one.
            Value::Float(value) if value.is_nan() => ConstantValue::Unevaluated,
            Value::Float(value) => ConstantValue::Float(value),
            Value::Bool(value) => ConstantValue::Bool(value),
            Value::Str(value) => ConstantValue::Str(value),
            Value::Null => ConstantValue::Null,
            Value::Duration(value) => ConstantValue::Duration(value),
            Value::Tenor(value) => ConstantValue::Tenor(value),
            Value::Temporal(value) => ConstantValue::Temporal(value),
        }
    }
}

impl ConstantValue {
    /// The literal this constant is, where it is one: what the stages
    /// after the analyser write for a generic value parameter's value.
    pub fn literal(&self) -> Option<Value> {
        Some(match self {
            ConstantValue::Int(value) => Value::Int(*value),
            ConstantValue::Float(value) => Value::Float(*value),
            ConstantValue::Bool(value) => Value::Bool(*value),
            ConstantValue::Str(value) => Value::Str(value.clone()),
            ConstantValue::Null => Value::Null,
            ConstantValue::Duration(value) => Value::Duration(*value),
            ConstantValue::Tenor(value) => Value::Tenor(*value),
            ConstantValue::Temporal(value) => Value::Temporal(value.clone()),
            _ => return None,
        })
    }
}

/// A literal as a constant. An integer literal takes the type its place
/// expects where that is an `f64` (EXPR-5), and is otherwise an `i64`
/// that may yet become one.
fn literal_constant(node: &SyntaxNode, expected: Option<&Type>) -> Result<Typed, Fault> {
    let token = tokens(node).next().ok_or(Fault::Reported)?;
    if token.kind() == SyntaxKind::KwNull {
        return Ok(Typed::of(ConstantValue::Null, None));
    }
    let (value, scalar) = literal(&token).ok_or(Fault::Reported)?;
    match value {
        Value::Int(value) if expected.is_some_and(is_float) => Ok(Typed::scalar(
            ConstantValue::Float(value as f64),
            Scalar::F64,
        )),
        Value::Int(value) => Ok(Typed {
            value: ConstantValue::Int(value),
            ty: Some(Type::Scalar(Scalar::I64)),
            open: Open::Integer,
            parts: Vec::new(),
        }),
        value => Ok(Typed::scalar(value.into(), scalar)),
    }
}

/// What a lone name is bound to, in the file it is written in.
fn bound<'s>(node: &SyntaxNode, scope: &Scope<'s>) -> Option<(SyntaxToken, &'s Binding)> {
    let name = name_token(node)?;
    let binding = (scope.binding)(scope.file, name.span().start)?;
    Some((name, binding))
}

/// A name: a generic value parameter, or a constant of a module.
fn name(node: &SyntaxNode, scope: &Scope) -> Result<Typed, Fault> {
    if node
        .children()
        .any(|child| child.kind() == SyntaxKind::GenericArgs)
    {
        return Err(Fault::NotConstant);
    }
    // A name nothing binds has been reported, or is left for the
    // prelude (UNIT-11).
    let Some((name, binding)) = bound(node, scope) else {
        return Err(Fault::Reported);
    };
    match binding {
        // "a generic value parameter in scope".
        Binding::Local {
            at,
            kind: LocalKind::GenericParameter,
        } => Ok(Typed::of(
            ConstantValue::Parameter {
                name: name.text().to_string(),
                at: *at,
            },
            (scope.parameter)(*at),
        )),
        Binding::Local { .. } => Err(Fault::NotConstant),
        // "a constant (UNIT-13)".
        Binding::Declarations {
            module,
            declarations,
        } => match declarations.as_slice() {
            [declaration]
                if scope.program.modules[*module].declarations[*declaration].kind
                    == DeclarationKind::Constant =>
            {
                module_constant(scope, (*module, *declaration))
            }
            _ => Err(Fault::NotConstant),
        },
    }
}

/// The value of the constant `declaration` of `module`, read as a
/// value of its own type (UNIT-13: "names one ordinary value of the
/// type T"), in the file it is declared in. EXPR-5: "A value that has a
/// type is not converted by its place", so an `i64` constant is no
/// `f64` where one is expected.
///
/// The constants it names are worked out first, and the ones they name
/// before them, so that reading each finds what it names known: a chain
/// of constants however long is followed on no call stack, and each is
/// read once.
fn module_constant(scope: &Scope, key: (usize, usize)) -> Result<Typed, Fault> {
    let constants = scope.constants;
    if let Some(known) = constants.known.borrow().get(&key) {
        return known.clone();
    }
    // A constant whose value depends on itself is reported as that
    // (`constant.cycle`).
    if constants.following.borrow().contains(&key) {
        return Err(Fault::Reported);
    }
    let depth = constants.following.borrow().len();
    let mut pending = vec![key];
    while let Some(&top) = pending.last() {
        if constants.known.borrow().contains_key(&top) {
            pending.pop();
            continue;
        }
        if !constants.following.borrow().contains(&top) {
            constants.following.borrow_mut().push(top);
        }
        let named: Vec<(usize, usize)> = names_constants(scope, top)
            .into_iter()
            .filter(|named| {
                !constants.known.borrow().contains_key(named)
                    && !constants.following.borrow().contains(named)
            })
            .collect();
        if !named.is_empty() {
            pending.extend(named);
            continue;
        }
        // What it names is known, or is in a cycle with it.
        let read = constant_expression(scope, top);
        constants.known.borrow_mut().insert(top, read);
        constants
            .following
            .borrow_mut()
            .retain(|following| *following != top);
        pending.pop();
    }
    constants.following.borrow_mut().truncate(depth);
    constants
        .known
        .borrow()
        .get(&key)
        .cloned()
        .unwrap_or(Err(Fault::Reported))
}

/// The constants of modules the constant `key` names, in its type or
/// its expression, at any depth inside them.
fn names_constants(scope: &Scope, (module, declaration): (usize, usize)) -> Vec<(usize, usize)> {
    let program = scope.program;
    let unit = &program.modules[module];
    let declared = &unit.declarations[declaration];
    let file = unit.files[declared.file].file;
    // Its type and its expression: a size in its type may name one
    // too.
    declared
        .syntax
        .descendants()
        .filter(|node| matches!(node.kind(), SyntaxKind::NameExpr | SyntaxKind::NamedType))
        .filter_map(|node| {
            let name = name_token(&node)?;
            match (scope.binding)(file, name.span().start)? {
                Binding::Declarations {
                    module,
                    declarations,
                } => match declarations.as_slice() {
                    [declaration]
                        if program.modules[*module].declarations[*declaration].kind
                            == DeclarationKind::Constant =>
                    {
                        Some((*module, *declaration))
                    }
                    _ => None,
                },
                Binding::Local { .. } => None,
            }
        })
        .collect()
}

/// What the expression of the constant `key` comes to, read where it
/// is declared, the constants it names known ([`module_constant`]).
fn constant_expression(
    scope: &Scope,
    (module, declaration): (usize, usize),
) -> Result<Typed, Fault> {
    let program = scope.program;
    let unit = &program.modules[module];
    let declared = &unit.declarations[declaration];
    let file = unit.files[declared.file].file;
    let written = crate::read::type_child(&declared.syntax);
    let ty = match unit.details.get(declaration) {
        Some(Detail::Constant(ty)) => Some(ty.clone()),
        _ => written.as_ref().and_then(|written| {
            written_type(written, program, file, scope.binding, scope.constants)
        }),
    };
    // What is not worked out here is held as the constant it is: the
    // same as itself, and not known beside any other.
    let named = Typed::of(
        ConstantValue::Named {
            module,
            declaration,
        },
        ty.clone(),
    );
    let expression = declared
        .syntax
        .children()
        .find(|child| Some(child) != written.as_ref());
    let inner = Scope {
        program,
        file,
        binding: scope.binding,
        parameter: &|_| None,
        constants: scope.constants,
    };
    match &expression {
        // Its own expression is read from its top.
        Some(expression) => match value(expression, ty.as_ref(), &inner, 0)
            .and_then(|read| fit(read, ty.as_ref(), program))
        {
            Ok(ConstantValue::Unevaluated) | Err(Fault::NotConstant) => Ok(named),
            Ok(value) => Ok(Typed::of(value, ty)),
            // What is wrong with a constant's own value is reported
            // where it is declared.
            Err(_) => Err(Fault::Reported),
        },
        None => Err(Fault::Reported),
    }
}

/// The type a constant's declaration, or a generic value parameter's,
/// writes, as far as it can be read before the program's types are:
/// a scalar, an enum, a generic parameter, and a nullable, list or
/// tuple of those. `None` for any other. A constant's place is given
/// such a type; another is read when the types are.
pub(crate) fn written_type<'s>(
    node: &SyntaxNode,
    program: &Program,
    file: FileId,
    binding: &dyn Fn(FileId, u32) -> Option<&'s Binding>,
    constants: &Constants,
) -> Option<Type> {
    let part = |node: &SyntaxNode| -> Option<Type> {
        written_type(
            &crate::read::type_child(node)?,
            program,
            file,
            binding,
            constants,
        )
    };
    match node.kind() {
        SyntaxKind::ScalarType => {
            Scalar::from_keyword(tokens(node).next()?.kind()).map(Type::Scalar)
        }
        SyntaxKind::NullableType => Some(Type::Nullable(Box::new(part(node)?))),
        SyntaxKind::ListType => {
            // TYPE-10: a size is an `i64` constant, or `unbounded`.
            let size = node
                .children()
                .find(|child| !crate::read::is_type(child.kind()))
                .map(|size| {
                    // A closure, so that what it gives lives as long as
                    // this reading needs and no longer. The constants
                    // being followed are shared, so that one whose type
                    // names itself, `const A: list<i64, A>`, ends.
                    let binding = |file: FileId, offset: u32| binding(file, offset);
                    let scope = Scope {
                        program,
                        file,
                        binding: &binding,
                        parameter: &|_| None,
                        constants,
                    };
                    let value = read(&size, Some(&Type::Scalar(Scalar::I64)), &scope)
                        .unwrap_or(ConstantValue::Unknown);
                    crate::types::Constant {
                        text: size.text().to_string().trim().to_string(),
                        value,
                    }
                });
            Some(Type::List {
                element: Box::new(part(node)?),
                size,
            })
        }
        SyntaxKind::TupleType => node
            .children()
            .filter(|child| crate::read::is_type(child.kind()))
            .map(|element| written_type(&element, program, file, binding, constants))
            .collect::<Option<Vec<Type>>>()
            .map(Type::Tuple),
        SyntaxKind::NamedType => {
            if node
                .children()
                .any(|child| child.kind() == SyntaxKind::GenericArgs)
            {
                return None;
            }
            let name = name_token(node)?;
            match binding(file, name.span().start)? {
                Binding::Local {
                    at,
                    kind: LocalKind::GenericParameter,
                } => Some(Type::Parameter {
                    name: name.text().to_string(),
                    at: *at,
                }),
                Binding::Declarations {
                    module,
                    declarations,
                } => declarations
                    .iter()
                    .copied()
                    .find(|&declaration| {
                        program.modules[*module].declarations[declaration].kind
                            == DeclarationKind::Enum
                    })
                    .map(|declaration| enum_type(program, (*module, declaration))),
                Binding::Local { .. } => None,
            }
        }
        _ => None,
    }
}

/// The enum declared as `declaration` of `module`, as a type.
fn enum_type(program: &Program, (module, declaration): (usize, usize)) -> Type {
    Type::Named {
        name: program.modules[module].declarations[declaration]
            .name
            .clone(),
        module,
        declaration,
        arguments: Vec::new(),
        kind: NamedKind::Enum,
    }
}

/// The members of an enum: each name and number. `None` before the
/// enum's members are read.
fn members(
    program: &Program,
    (module, declaration): (usize, usize),
) -> Option<&[crate::types::EnumMember]> {
    match program.modules.get(module)?.details.get(declaration)? {
        Detail::Enum(read) => Some(&read.members),
        _ => None,
    }
}

/// The member of the enum `of` named `name` (TYPE-7).
fn member_named(program: &Program, of: (usize, usize), name: &str) -> Result<ConstantValue, Fault> {
    match members(program, of) {
        Some(members) if !members.iter().any(|member| member.name == name) => {
            Err(Fault::NoMember(format!("`{name}`")))
        }
        Some(_) => Ok(ConstantValue::Member(name.to_string())),
        None => Ok(ConstantValue::Unevaluated),
    }
}

/// The enum a lone name, or `alias::Name`, names.
fn names_enum(node: &SyntaxNode, scope: &Scope) -> Option<(usize, usize)> {
    if node.kind() != SyntaxKind::NameExpr {
        return None;
    }
    let (_, binding) = bound(node, scope)?;
    let Binding::Declarations {
        module,
        declarations,
    } = binding
    else {
        return None;
    };
    declarations.iter().copied().find_map(|declaration| {
        (scope.program.modules[*module].declarations[declaration].kind == DeclarationKind::Enum)
            .then_some((*module, declaration))
    })
}

/// `Mode.slow`, `alias::Mode.slow` (TYPE-7: "`Type.member` names one,
/// through an alias as `alias::Type.member`").
fn qualified_member(node: &SyntaxNode, scope: &Scope) -> Result<Typed, Fault> {
    let base = node.children().next().ok_or(Fault::Reported)?;
    let Some(of) = names_enum(&base, scope) else {
        // A field of a value is known only when the program runs.
        return Err(Fault::NotConstant);
    };
    // The member's name is the last token: any word (LEX-9).
    let member = tokens(node).last().ok_or(Fault::Reported)?;
    Ok(Typed::of(
        member_named(scope.program, of, member.text())?,
        Some(enum_type(scope.program, of)),
    ))
}

/// `Mode(1)`, `Mode("slow")`. TYPE-7: "A call of the enum's name
/// constructs a member. It takes one argument, by position, that is an
/// `i64` or a `str` ... `Side(1)` is the member whose number is 1, and
/// `Side("buy")` the member of that exact name. Construction is
/// checked: a constant that is no member's number or name rejects." A
/// call of anything else is no constant.
fn constructed_member(node: &SyntaxNode, scope: &Scope, depth: u32) -> Result<Typed, Fault> {
    let callee = node.children().next().ok_or(Fault::Reported)?;
    // A name nothing binds has been reported, or is left for the
    // prelude (UNIT-11).
    if callee.kind() == SyntaxKind::NameExpr && bound(&callee, scope).is_none() {
        return Err(Fault::Reported);
    }
    let Some(of) = names_enum(&callee, scope) else {
        return Err(Fault::NotConstant);
    };
    let ty = enum_type(scope.program, of);
    let list = node
        .children()
        .find(|child| child.kind() == SyntaxKind::ArgList)
        .ok_or(Fault::Reported)?;
    let arguments: Vec<SyntaxNode> = list
        .children()
        .filter(|argument| argument.kind() == SyntaxKind::Arg)
        .collect();
    // One argument with no name: anything else is reported where the
    // construction is typed.
    let ([argument], false) = (
        arguments.as_slice(),
        arguments
            .iter()
            .any(|argument| tokens(argument).any(|token| token.kind() == SyntaxKind::Colon)),
    ) else {
        return Err(Fault::Reported);
    };
    let given = argument.children().next().ok_or(Fault::Reported)?;
    let given = value(&given, None, scope, depth + 1)?.value;
    let Some(members) = members(scope.program, of) else {
        return Ok(Typed::unevaluated(Some(ty)));
    };
    let member = match given {
        // A member whose number is not known says nothing sure of which
        // numbers the enum has.
        ConstantValue::Int(number) if members.iter().all(|member| member.value.is_some()) => {
            members
                .iter()
                .find(|member| member.value == Some(number))
                .ok_or_else(|| Fault::NoMember(format!("numbered {number}")))?
        }
        ConstantValue::Str(name) => members
            .iter()
            .find(|member| member.name == name)
            .ok_or_else(|| Fault::NoMember(format!("named `{name}`")))?,
        // Of a generic value parameter: which member is known once it
        // is given a value.
        given @ (ConstantValue::Parameter { .. }
        | ConstantValue::Unary(..)
        | ConstantValue::Binary(..)
        | ConstantValue::Constructed { .. }) => {
            return Ok(Typed::of(
                ConstantValue::Constructed {
                    members: members
                        .iter()
                        .map(|member| (member.name.clone(), member.value))
                        .collect(),
                    from: Box::new(given),
                },
                Some(ty),
            ));
        }
        ConstantValue::Int(_)
        | ConstantValue::Named { .. }
        | ConstantValue::Unevaluated
        | ConstantValue::Unknown => return Ok(Typed::unevaluated(Some(ty))),
        // Of another type: reported where the construction is typed.
        _ => return Err(Fault::Reported),
    };
    Ok(Typed::of(
        ConstantValue::Member(member.name.clone()),
        Some(ty),
    ))
}

/// `[a, b]`: a list of constants.
fn sequence(
    node: &SyntaxNode,
    expected: Option<&Type>,
    scope: &Scope,
    depth: u32,
) -> Result<Typed, Fault> {
    let element = match expected {
        Some(Type::List { element, .. }) => Some(&**element),
        // A set's or a dictionary's constant is not read: its entries
        // have no order to compare them by.
        Some(Type::Set(_) | Type::Dict { .. }) => return Ok(Typed::unevaluated(None)),
        _ => None,
    };
    let mut values = Vec::new();
    let mut types = Vec::new();
    let mut parts = Vec::new();
    for entry in node.children() {
        if entry.kind() == SyntaxKind::SequencePair {
            return Ok(Typed::unevaluated(None));
        }
        let read = value(&entry, element, scope, depth + 1)?;
        types.push(read.ty.clone());
        values.push(match element {
            Some(element) => fit(read, Some(element), scope.program)?,
            None => {
                parts.push(read.clone());
                read.value
            }
        });
    }
    let ty = element.cloned().or_else(|| {
        let first = types.first()?.clone()?;
        types
            .iter()
            .all(|ty| ty.as_ref() == Some(&first))
            .then_some(first)
    });
    Ok(Typed {
        parts,
        ..Typed::of(
            ConstantValue::List(values),
            ty.map(|element| Type::List {
                element: Box::new(element),
                size: None,
            }),
        )
    })
}

/// `(a, b)`: a tuple of constants.
fn tuple(
    node: &SyntaxNode,
    expected: Option<&Type>,
    scope: &Scope,
    depth: u32,
) -> Result<Typed, Fault> {
    let entries: Vec<SyntaxNode> = node.children().collect();
    let elements = match expected {
        Some(Type::Tuple(elements)) if elements.len() == entries.len() => Some(elements),
        _ => None,
    };
    let mut values = Vec::new();
    let mut types = Vec::new();
    let mut parts = Vec::new();
    for (index, entry) in entries.iter().enumerate() {
        let element = elements.map(|elements| &elements[index]);
        let read = value(entry, element, scope, depth + 1)?;
        types.push(read.ty.clone());
        if element.is_none() {
            parts.push(read.clone());
        }
        values.push(fit(read, element, scope.program)?);
    }
    let ty = match elements {
        Some(elements) => Some(Type::Tuple(elements.clone())),
        None => types
            .into_iter()
            .collect::<Option<Vec<Type>>>()
            .map(Type::Tuple),
    };
    Ok(Typed {
        parts,
        ..Typed::of(ConstantValue::Tuple(values), ty)
    })
}

// ---- Symbols ----

/// True when a symbol is given configuration, `-{twice: true} x`: an
/// option may change what it computes, so it is not worked out here.
fn configured(node: &SyntaxNode) -> bool {
    node.children()
        .any(|child| child.kind() == SyntaxKind::OperatorConfig)
}

/// What a symbol given configuration is, as a constant: no constant
/// where an entry of its configuration reads a local or a parameter,
/// `0 +{offset: n} 0`, and otherwise one not worked out.
fn configured_value(node: &SyntaxNode, scope: &Scope, ty: Option<Type>) -> Result<Typed, Fault> {
    let runtime = node
        .children()
        .filter(|child| child.kind() == SyntaxKind::OperatorConfig)
        .flat_map(|config| config.descendants())
        .filter(|used| used.kind() == SyntaxKind::NameExpr)
        .any(|used| {
            matches!(
                bound(&used, scope),
                Some((_, Binding::Local { kind, .. })) if *kind != LocalKind::GenericParameter
            )
        });
    if runtime {
        return Err(Fault::NotConstant);
    }
    Ok(Typed::unevaluated(ty))
}

/// `-x` or `!x`.
fn unary(
    node: &SyntaxNode,
    expected: Option<&Type>,
    scope: &Scope,
    depth: u32,
) -> Result<Typed, Fault> {
    let operator = match tokens(node).next().map(|token| token.kind()) {
        Some(SyntaxKind::Minus) => UnaryOperator::Negate,
        Some(SyntaxKind::Bang) => UnaryOperator::Not,
        _ => return Err(Fault::NotConstant),
    };
    // The operand is the last child node: configuration, when the
    // operator has some, stands before it.
    let operand = node.children().last().ok_or(Fault::Reported)?;
    // LEX-7: "Minus is an operator, not part of a numeric literal", and
    // the least `i64` is written as a minus before 2^63, which is itself
    // no `i64`.
    if operator == UnaryOperator::Negate && !configured(node) && is_least_magnitude(&operand) {
        return Ok(match expected.filter(|expected| is_float(expected)) {
            Some(_) => Typed::scalar(ConstantValue::Float(i64::MIN as f64), Scalar::F64),
            None => Typed {
                value: ConstantValue::Int(i64::MIN),
                ty: Some(Type::Scalar(Scalar::I64)),
                open: Open::Integer,
                parts: Vec::new(),
            },
        });
    }
    let read = value(&operand, numeric_hint(expected), scope, depth + 1)?;
    if configured(node) {
        return configured_value(node, scope, read.ty);
    }
    let value = negated_or_not(operator, &read.value);
    Ok(Typed {
        value,
        ty: read.ty,
        open: match read.open {
            Open::Integer => Open::Integer,
            _ => Open::No,
        },
        parts: Vec::new(),
    })
}

/// True when `node` is the integer literal 2^63. LEX-7: "Minus is an
/// operator, not part of a numeric literal", and the least `i64` is
/// written as a minus before 2^63, the one magnitude that is an
/// integer only there.
pub(crate) fn is_least_magnitude(node: &SyntaxNode) -> bool {
    node.kind() == SyntaxKind::Literal
        && tokens(node).next().is_some_and(|token| {
            // A literal the lexer rejected has been reported there.
            token.kind() == SyntaxKind::Int
                && decode_integer(token.text()) == Ok(MAX_INTEGER_MAGNITUDE)
        })
}

/// The operand's expected type, for a symbol whose result is of its
/// operands' type: an `f64` makes the integer literals under it `f64`
/// (EXPR-5). Nothing else is passed down, since `5m * 2` is a
/// `duration` of a duration and an `i64`.
fn numeric_hint(expected: Option<&Type>) -> Option<&Type> {
    expected.filter(|expected| is_float(expected))
}

/// `left op right`.
fn binary(
    node: &SyntaxNode,
    expected: Option<&Type>,
    scope: &Scope,
    depth: u32,
) -> Result<Typed, Fault> {
    // The configuration, when the operator has some, stands between
    // the operands.
    let mut operands = node
        .children()
        .filter(|child| child.kind() != SyntaxKind::OperatorConfig);
    let (Some(left), Some(right)) = (operands.next(), operands.next()) else {
        return Err(Fault::Reported);
    };
    let operator = tokens(node)
        .find_map(|token| binary_operator(token.kind()))
        .ok_or(Fault::NotConstant)?;
    // GEN-5: "where a candidate that gives the requested output takes
    // every integer literal as an `i64`, none is widened". `+`, `-` and
    // `*` of two `i64` give an `i64`, so where an `f64` is requested
    // their literals are widened; `/` of two gives an `f64` already
    // (NUM-1), and its literals are not.
    let hint = match operator {
        BinaryOperator::Add | BinaryOperator::Subtract | BinaryOperator::Multiply => {
            numeric_hint(expected)
        }
        _ => None,
    };
    let mut left = value(&left, hint, scope, depth + 1)?;
    let mut right = value(&right, hint, scope, depth + 1)?;
    if configured(node) {
        return configured_value(node, scope, None);
    }
    // An integer literal beside an `f64` in arithmetic is taken as the
    // `f64` it is nearest: core's mixed candidates make it one (NUM-1,
    // "Unconfigured mixed i64/f64 and f64 arithmetic returns f64"). A
    // comparison keeps it an `i64`: NUM-2, "Mixed numeric comparison
    // must not first round an i64 operand to f64".
    let float = Some(Type::Scalar(Scalar::F64));
    let arithmetic = matches!(
        operator,
        BinaryOperator::Add
            | BinaryOperator::Subtract
            | BinaryOperator::Multiply
            | BinaryOperator::Divide
    );
    if arithmetic && left.open == Open::Integer && right.ty == float {
        left = widened(left);
    }
    if arithmetic && right.open == Open::Integer && left.ty == float {
        right = widened(right);
    }
    let ty = binary_type(operator, left.ty.as_ref(), right.ty.as_ref());
    let value = combined(operator, &left.value, &right.value);
    // Integer literals joined by core's arithmetic are an `f64` where
    // one is expected (EXPR-5), and that `f64` arithmetic is the integer
    // arithmetic where each operand and the result is exact as an `f64`,
    // within 2^53. Past that the two may differ, and the place must be
    // known as the literals are read (`numeric_hint`).
    let exact = |value: &ConstantValue| matches!(value, ConstantValue::Int(value) if value.unsigned_abs() <= 1 << 53);
    let open = match (operator, left.open, right.open) {
        (
            BinaryOperator::Add
            | BinaryOperator::Subtract
            | BinaryOperator::Multiply
            | BinaryOperator::FloorDivide
            | BinaryOperator::Modulo,
            Open::Integer,
            Open::Integer,
        ) if exact(&left.value) && exact(&right.value) && exact(&value) => Open::Integer,
        _ => Open::No,
    };
    Ok(Typed {
        value,
        ty,
        open,
        parts: Vec::new(),
    })
}

/// An integer literal as the `f64` it is where one is expected.
fn widened(read: Typed) -> Typed {
    match read.value {
        // `as` converts an integer to the nearest `f64`.
        ConstantValue::Int(value) => Typed::scalar(ConstantValue::Float(value as f64), Scalar::F64),
        _ => read,
    }
}

/// The operator a token is, between two operands.
fn binary_operator(kind: SyntaxKind) -> Option<BinaryOperator> {
    Some(match kind {
        SyntaxKind::Plus => BinaryOperator::Add,
        SyntaxKind::Minus => BinaryOperator::Subtract,
        SyntaxKind::Star => BinaryOperator::Multiply,
        SyntaxKind::Slash => BinaryOperator::Divide,
        SyntaxKind::SlashSlash => BinaryOperator::FloorDivide,
        SyntaxKind::Percent => BinaryOperator::Modulo,
        SyntaxKind::EqEq => BinaryOperator::Equal,
        SyntaxKind::BangEq => BinaryOperator::NotEqual,
        SyntaxKind::Lt => BinaryOperator::Less,
        SyntaxKind::LtEq => BinaryOperator::LessEqual,
        SyntaxKind::Gt => BinaryOperator::Greater,
        SyntaxKind::GtEq => BinaryOperator::GreaterEqual,
        SyntaxKind::Spaceship => BinaryOperator::Compare,
        SyntaxKind::AmpAmp => BinaryOperator::And,
        SyntaxKind::PipePipe => BinaryOperator::Or,
        _ => return None,
    })
}

/// The type of `left op right`, from its operands' (OPS-1, NUM-1,
/// TEN-1). `None` where that is not known.
fn binary_type(
    operator: BinaryOperator,
    left: Option<&Type>,
    right: Option<&Type>,
) -> Option<Type> {
    let scalar = |ty: Option<&Type>| match ty {
        Some(Type::Scalar(scalar)) => Some(*scalar),
        _ => None,
    };
    let result = match (operator, scalar(left), scalar(right)) {
        (
            BinaryOperator::Equal
            | BinaryOperator::NotEqual
            | BinaryOperator::Less
            | BinaryOperator::LessEqual
            | BinaryOperator::Greater
            | BinaryOperator::GreaterEqual
            | BinaryOperator::And
            | BinaryOperator::Or,
            _,
            _,
        ) => Scalar::Bool,
        // NUM-1: "Unconfigured mixed i64/f64 and f64 arithmetic returns
        // f64".
        (
            BinaryOperator::Add | BinaryOperator::Subtract | BinaryOperator::Multiply,
            Some(Scalar::I64 | Scalar::F64),
            Some(Scalar::F64),
        )
        | (
            BinaryOperator::Add | BinaryOperator::Subtract | BinaryOperator::Multiply,
            Some(Scalar::F64),
            Some(Scalar::I64),
        ) => Scalar::F64,
        // NUM-3: "Division whose numeric result is f64"; a duration by
        // a duration is an `f64` too (OPS-1).
        (BinaryOperator::Divide, Some(Scalar::I64 | Scalar::F64), _)
        | (BinaryOperator::Divide, Some(Scalar::Duration), Some(Scalar::Duration)) => Scalar::F64,
        // OPS-1: "`//` | `duration` by `duration` | `i64`".
        (BinaryOperator::FloorDivide, Some(Scalar::Duration), Some(Scalar::Duration)) => {
            Scalar::I64
        }
        (
            BinaryOperator::Multiply,
            Some(Scalar::I64),
            Some(other @ (Scalar::Duration | Scalar::Tenor)),
        ) => other,
        // `<=>` gives a `CmpResult` (OPS-3), which is not worked out.
        (BinaryOperator::Compare, _, _) => return None,
        (_, Some(left), _) => left,
        _ => return None,
    };
    Some(Type::Scalar(result))
}

/// `-value` or `!value`, worked out where it can be.
fn negated_or_not(operator: UnaryOperator, value: &ConstantValue) -> ConstantValue {
    use ConstantValue as V;
    // `checked_` operations give `None` where the result does not fit:
    // NUM-2, "An unrepresentable i64 result produces an HGL error".
    let worked_out = match (operator, value) {
        (_, V::Unknown) => return V::Unknown,
        (
            _,
            V::Parameter { .. }
            | V::Named { .. }
            | V::Constructed { .. }
            | V::Unary(..)
            | V::Binary(..),
        ) => {
            return V::Unary(operator, Box::new(value.clone()));
        }
        (UnaryOperator::Negate, V::Int(value)) => value.checked_neg().map(V::Int),
        // NUM-1: "Unary negation preserves the numeric type."
        (UnaryOperator::Negate, V::Float(value)) => Some(V::Float(-value)),
        (UnaryOperator::Negate, V::Duration(value)) => value.checked_neg().map(V::Duration),
        // TEN-2: "a negation ... appl[ies] to each count".
        (UnaryOperator::Negate, V::Tenor(tenor)) => negated_tenor(*tenor).map(V::Tenor),
        (UnaryOperator::Not, V::Bool(value)) => Some(V::Bool(!value)),
        _ => None,
    };
    worked_out.unwrap_or(V::Unevaluated)
}

fn negated_tenor(tenor: Tenor) -> Option<Tenor> {
    Some(match tenor {
        Tenor::Moves { months, days } => Tenor::Moves {
            months: months.checked_neg()?,
            days: days.checked_neg()?,
        },
        Tenor::BusinessDays(days) => Tenor::BusinessDays(days.checked_neg()?),
    })
}

/// `left op right`, worked out where it can be: core's own arithmetic
/// on two constants (NUM-1, NUM-3, OPS-1, TEN-2). One that names a
/// generic value parameter is held as the expression it is, and worked
/// out when the parameter is given a value.
pub(crate) fn combined(
    operator: BinaryOperator,
    left: &ConstantValue,
    right: &ConstantValue,
) -> ConstantValue {
    use BinaryOperator as Op;
    use ConstantValue as V;
    let symbolic = |value: &ConstantValue| {
        matches!(
            value,
            V::Parameter { .. }
                | V::Named { .. }
                | V::Constructed { .. }
                | V::Unary(..)
                | V::Binary(..)
        )
    };
    if matches!(left, V::Unknown) || matches!(right, V::Unknown) {
        return V::Unknown;
    }
    if matches!(left, V::Unevaluated) || matches!(right, V::Unevaluated) {
        return V::Unevaluated;
    }
    if symbolic(left) || symbolic(right) {
        return V::Binary(operator, Box::new(left.clone()), Box::new(right.clone()));
    }
    let worked_out = match (operator, left, right) {
        // OPS-2: "Numbers compare as the numeric contract says, an
        // `i64` with an `f64` included", and exactly (NUM-2).
        (Op::Equal, V::Int(_), V::Float(_)) | (Op::Equal, V::Float(_), V::Int(_)) => {
            ordered(left, right).map(|order| V::Bool(order.is_eq()))
        }
        (Op::NotEqual, V::Int(_), V::Float(_)) | (Op::NotEqual, V::Float(_), V::Int(_)) => {
            ordered(left, right).map(|order| V::Bool(order.is_ne()))
        }
        // OPS-2: "`==` and `!=` compare two values of one type"; two of
        // two types have no candidate, and are not worked out here.
        (Op::Equal | Op::NotEqual, _, _)
            if std::mem::discriminant(left) != std::mem::discriminant(right) =>
        {
            None
        }
        (Op::Equal | Op::NotEqual, V::Temporal(left), V::Temporal(right))
            if temporal_type(left) != temporal_type(right) =>
        {
            None
        }
        (Op::Equal, _, _) => left.same(right).map(V::Bool),
        (Op::NotEqual, _, _) => left.same(right).map(|same| V::Bool(!same)),
        (Op::And, V::Bool(left), V::Bool(right)) => Some(V::Bool(*left && *right)),
        (Op::Or, V::Bool(left), V::Bool(right)) => Some(V::Bool(*left || *right)),
        (Op::Less | Op::LessEqual | Op::Greater | Op::GreaterEqual, _, _) => ordered(left, right)
            .map(|order| {
                V::Bool(match operator {
                    Op::Less => order.is_lt(),
                    Op::LessEqual => order.is_le(),
                    Op::Greater => order.is_gt(),
                    _ => order.is_ge(),
                })
            }),
        // NUM-1: "For i64 operands, `+`, `-`, `*`, `//`, and `%` return
        // i64"; NUM-2: one that does not fit is an error, and no value.
        // "Integer floor division rounds toward negative infinity;
        // modulo has the divisor's sign when nonzero." A zero divisor of
        // two `i64` fails under the default `.fail` (NUM-12): `checked_`
        // gives `None` for it, and for `i64::MIN // -1`.
        (Op::Add, V::Int(left), V::Int(right)) => left.checked_add(*right).map(V::Int),
        (Op::Subtract, V::Int(left), V::Int(right)) => left.checked_sub(*right).map(V::Int),
        (Op::Multiply, V::Int(left), V::Int(right)) => left.checked_mul(*right).map(V::Int),
        (Op::FloorDivide, V::Int(left), V::Int(right)) => {
            left.checked_div(*right).map(|quotient| {
                // `/` of Rust's rounds toward zero: one less where the
                // operands' signs differ and something remains.
                let remains = left % right != 0;
                V::Int(if remains && ((*left < 0) != (*right < 0)) {
                    quotient - 1
                } else {
                    quotient
                })
            })
        }
        // The one remainder `checked_rem` cannot give, of the least
        // `i64` by -1, is 0, which is an `i64`.
        (Op::Modulo, V::Int(_), V::Int(-1)) => Some(V::Int(0)),
        (Op::Modulo, V::Int(left), V::Int(right)) => left.checked_rem(*right).map(|remainder| {
            // `%` of Rust's has the dividend's sign: the divisor's is
            // wanted.
            V::Int(if remainder != 0 && ((remainder < 0) != (*right < 0)) {
                remainder + right
            } else {
                remainder
            })
        }),
        // NUM-3: `/` of numbers is true division, an `f64`, under the
        // `ieee` zero policy by default: a finite nonzero numerator by a
        // zero is a signed infinity, and zero by zero a NaN, which is no
        // constant (NUM-5).
        (Op::Divide, V::Int(left), V::Int(right)) => float(divided(*left, *right)),
        // NUM-1: "Unconfigured mixed i64/f64 and f64 arithmetic returns
        // f64": the `i64` as the `f64` nearest it.
        (Op::Add | Op::Subtract | Op::Multiply | Op::Divide, V::Int(whole), V::Float(_)) => {
            return combined(operator, &V::Float(*whole as f64), right);
        }
        (Op::Add | Op::Subtract | Op::Multiply | Op::Divide, V::Float(_), V::Int(whole)) => {
            return combined(operator, left, &V::Float(*whole as f64));
        }
        (Op::Add, V::Float(left), V::Float(right)) => float(left + right),
        (Op::Subtract, V::Float(left), V::Float(right)) => float(left - right),
        (Op::Multiply, V::Float(left), V::Float(right)) => float(left * right),
        (Op::Divide, V::Float(left), V::Float(right)) => float(left / right),
        // OPS-1: durations add and subtract, and a duration times an
        // `i64`, in either order, is a duration. OPS-10: one outside the
        // type's range is an error.
        (Op::Add, V::Duration(left), V::Duration(right)) => {
            left.checked_add(*right).map(V::Duration)
        }
        (Op::Subtract, V::Duration(left), V::Duration(right)) => {
            left.checked_sub(*right).map(V::Duration)
        }
        (Op::Multiply, V::Duration(left), V::Int(right))
        | (Op::Multiply, V::Int(right), V::Duration(left)) => {
            left.checked_mul(*right).map(V::Duration)
        }
        // TEN-2: "A sum, a difference, a negation, and a scaling apply
        // to each count. The operands of a sum or a difference are of
        // one form".
        (Op::Add | Op::Subtract, V::Tenor(left), V::Tenor(right)) => {
            tenor_sum(operator, *left, *right).map(V::Tenor)
        }
        (Op::Multiply, V::Tenor(tenor), V::Int(factor))
        | (Op::Multiply, V::Int(factor), V::Tenor(tenor)) => {
            scaled_tenor(*tenor, *factor).map(V::Tenor)
        }
        // OPS-1: "`+` | `str` and `str` | `str`".
        (Op::Add, V::Str(left), V::Str(right)) => Some(V::Str(format!("{left}{right}"))),
        _ => None,
    };
    worked_out.unwrap_or(V::Unevaluated)
}

/// `left / right` of two `i64`, true division (NUM-3): the `f64`
/// nearest the exact quotient, a tie to the even one. Each operand is
/// not made an `f64` first, which for one past 2^53 would round it
/// before the division does: `9007199254740993 / 3` is
/// `3002399751580331.0`. A zero divisor gives what IEEE division of the
/// operands does under the default `ieee` policy.
fn divided(left: i64, right: i64) -> f64 {
    const EXACT: u64 = 1 << 53;
    if right == 0 || (left.unsigned_abs() <= EXACT && right.unsigned_abs() <= EXACT) {
        // `as` is exact for an integer of at most 53 bits, and IEEE
        // division rounds the exact quotient once.
        return left as f64 / right as f64;
    }
    if left == 0 {
        return if right < 0 { -0.0 } else { 0.0 };
    }
    let negative = (left < 0) != (right < 0);
    let (numerator, denominator) = (
        u128::from(left.unsigned_abs()),
        u128::from(right.unsigned_abs()),
    );
    let bits = |value: u128| 128 - i32::try_from(value.leading_zeros()).unwrap_or(128);
    // The quotient times 2^scale, to 53 bits: a scale that gives 53 or
    // 54, and one less where it gave 54. Each shifted value fits a
    // `u128`: the numerator is at most 64 + 64 bits wide.
    let quotient = |scale: i32| {
        let (numerator, denominator) = if scale >= 0 {
            (numerator << scale, denominator)
        } else {
            (numerator, denominator << -scale)
        };
        (
            numerator / denominator,
            numerator % denominator,
            denominator,
        )
    };
    let mut scale = 53 - (bits(numerator) - bits(denominator));
    let (mut whole, mut remainder, mut divisor) = quotient(scale);
    if whole >= 1 << 53 {
        scale -= 1;
        (whole, remainder, divisor) = quotient(scale);
    }
    // Round to nearest, a tie to the even one.
    if 2 * remainder > divisor || (2 * remainder == divisor && whole % 2 == 1) {
        whole += 1;
    }
    // `whole` is at most 2^53 and so exact as an `f64`, and a power of
    // two scales it exactly: the quotient of two `i64` is far from the
    // bounds of an `f64`.
    #[allow(clippy::cast_precision_loss)]
    let magnitude = whole as f64 * 2f64.powi(-scale);
    if negative { -magnitude } else { magnitude }
}

/// An `f64` result as a constant: a NaN is none (NUM-5).
fn float(value: f64) -> Option<ConstantValue> {
    (!value.is_nan()).then_some(ConstantValue::Float(value))
}

/// The order of two numbers, two durations, two strings or two bools
/// (OPS-2). `None` for two values of any other type.
fn ordered(left: &ConstantValue, right: &ConstantValue) -> Option<std::cmp::Ordering> {
    match (left, right) {
        (ConstantValue::Int(left), ConstantValue::Int(right))
        | (ConstantValue::Duration(left), ConstantValue::Duration(right)) => Some(left.cmp(right)),
        (ConstantValue::Float(left), ConstantValue::Float(right)) => left.partial_cmp(right),
        // A string by its characters, which is the order of their
        // UTF-8 bytes; `false` before `true`.
        (ConstantValue::Str(left), ConstantValue::Str(right)) => Some(left.cmp(right)),
        (ConstantValue::Bool(left), ConstantValue::Bool(right)) => Some(left.cmp(right)),
        (ConstantValue::Int(whole), ConstantValue::Float(real)) => Some(mixed(*whole, *real)),
        (ConstantValue::Float(real), ConstantValue::Int(whole)) => {
            Some(mixed(*whole, *real).reverse())
        }
        _ => None,
    }
}

/// How the `i64` `whole` orders beside the `f64` `real`, exactly: the
/// integer is not rounded to an `f64` first (NUM-2). No constant is a
/// NaN.
fn mixed(whole: i64, real: f64) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    // 2^63 is exact as an `f64`: every `i64` is below it, and at or
    // above -2^63.
    const BOUND: f64 = 9_223_372_036_854_775_808.0;
    if real >= BOUND {
        return Ordering::Less;
    }
    if real < -BOUND {
        return Ordering::Greater;
    }
    // In range, the floor of `real` is an `i64` exactly, and `as`
    // gives it.
    let floor = real.floor();
    #[allow(clippy::cast_possible_truncation)]
    let floor_whole = floor as i64;
    match whole.cmp(&floor_whole) {
        Ordering::Equal if real > floor => Ordering::Less,
        order => order,
    }
}

fn tenor_sum(operator: BinaryOperator, left: Tenor, right: Tenor) -> Option<Tenor> {
    let add = |left: i64, right: i64| match operator {
        BinaryOperator::Add => left.checked_add(right),
        _ => left.checked_sub(right),
    };
    match (left, right) {
        (
            Tenor::Moves { months, days },
            Tenor::Moves {
                months: other_months,
                days: other_days,
            },
        ) => Some(Tenor::Moves {
            months: add(months, other_months)?,
            days: add(days, other_days)?,
        }),
        (Tenor::BusinessDays(left), Tenor::BusinessDays(right)) => {
            Some(Tenor::BusinessDays(add(left, right)?))
        }
        // `tenor.mixed_units`: an error, and no value.
        _ => None,
    }
}

fn scaled_tenor(tenor: Tenor, factor: i64) -> Option<Tenor> {
    Some(match tenor {
        Tenor::Moves { months, days } => Tenor::Moves {
            months: months.checked_mul(factor)?,
            days: days.checked_mul(factor)?,
        },
        Tenor::BusinessDays(days) => Tenor::BusinessDays(days.checked_mul(factor)?),
    })
}

/// False for a constant expression whose value this compiler does not
/// work out, or one that holds such: `@2024-01-31 + 1M`. A generic
/// value parameter is valued where the declaration is called, and an
/// expression of one with it. A constant of a module is a compile-time
/// constant (UNIT-13) whatever its expression calls: one whose value is
/// not worked out here is held by its name (`ConstantValue::Named`), and
/// what is wrong with its expression is reported where it is declared.
pub(crate) fn worked_out(value: &ConstantValue) -> bool {
    match value {
        ConstantValue::Unevaluated => false,
        ConstantValue::Unary(_, operand) | ConstantValue::Constructed { from: operand, .. } => {
            worked_out(operand)
        }
        ConstantValue::Binary(_, left, right) => worked_out(left) && worked_out(right),
        ConstantValue::List(values) | ConstantValue::Tuple(values) => values.iter().all(worked_out),
        _ => true,
    }
}

// ---- Comparing ----

impl ConstantValue {
    /// Whether two constants are one value: `Some(true)` where they are,
    /// `Some(false)` where they are not, `None` where that is not known.
    ///
    /// Two values of one type are the same where they are equal as that
    /// type's `==` has them (core's OPS-2), however each is written:
    /// `1.0` and `1.00`, `0.0` and `-0.0` (NUM-4: "Plain numeric `==` is
    /// exact"), `60s` and `1m`, `1Y` and `12M` (TYPE-17). Two temporal
    /// values are compared by [`same_temporal`].
    ///
    /// What is not valued is neither: [`ConstantValue::Unevaluated`]
    /// gives `None` beside anything. So does a generic value parameter,
    /// which is the same as itself and may be any value beside another,
    /// and an expression of one, which is the same as one written alike.
    /// An `i64` and an `f64` meet only where their place's type was not
    /// known, for a literal where an `f64` is expected is read as one
    /// (EXPR-5): beside each other they give `None` too.
    pub fn same(&self, other: &ConstantValue) -> Option<bool> {
        use ConstantValue as V;
        // All of two lists of answers: false where one is, true where
        // each is, and otherwise not known.
        let all = |left: &[ConstantValue], right: &[ConstantValue]| {
            if left.len() != right.len() {
                return Some(false);
            }
            let answers: Vec<Option<bool>> = left
                .iter()
                .zip(right)
                .map(|(left, right)| left.same(right))
                .collect();
            if answers.contains(&Some(false)) {
                Some(false)
            } else if answers.iter().all(|answer| *answer == Some(true)) {
                Some(true)
            } else {
                None
            }
        };
        // True where two expressions of parameters are written alike:
        // they are then one value whatever the parameters are.
        let alike = |same: Option<bool>| (same == Some(true)).then_some(true);
        match (self, other) {
            (V::Unevaluated | V::Unknown, _) | (_, V::Unevaluated | V::Unknown) => None,
            (V::Parameter { at, .. }, V::Parameter { at: other, .. }) => {
                (at == other).then_some(true)
            }
            (
                V::Named {
                    module,
                    declaration,
                },
                V::Named {
                    module: other_module,
                    declaration: other_declaration,
                },
            ) => ((module, declaration) == (other_module, other_declaration)).then_some(true),
            (V::Unary(operator, operand), V::Unary(other_operator, other_operand))
                if operator == other_operator =>
            {
                alike(operand.same(other_operand))
            }
            (
                V::Binary(operator, left, right),
                V::Binary(other_operator, other_left, other_right),
            ) if operator == other_operator => alike(all(
                &[(**left).clone(), (**right).clone()],
                &[(**other_left).clone(), (**other_right).clone()],
            )),
            (V::Constructed { from: left, .. }, V::Constructed { from: right, .. }) => {
                alike(left.same(right))
            }
            (
                V::Parameter { .. }
                | V::Named { .. }
                | V::Constructed { .. }
                | V::Unary(..)
                | V::Binary(..),
                _,
            )
            | (
                _,
                V::Parameter { .. }
                | V::Named { .. }
                | V::Constructed { .. }
                | V::Unary(..)
                | V::Binary(..),
            ) => None,
            (V::Int(_), V::Float(_)) | (V::Float(_), V::Int(_)) => None,
            (V::Int(left), V::Int(right)) => Some(left == right),
            (V::Float(left), V::Float(right)) => Some(left == right),
            (V::Bool(left), V::Bool(right)) => Some(left == right),
            (V::Str(left), V::Str(right)) => Some(left == right),
            (V::Null, V::Null) | (V::Unbounded, V::Unbounded) => Some(true),
            (V::Duration(left), V::Duration(right)) => Some(left == right),
            // TYPE-17: "Two tenors are equal when they are of the same
            // form and their counts are equal, however each was
            // written": the lexer has read `1Y` as twelve months.
            (V::Tenor(left), V::Tenor(right)) => Some(left == right),
            (V::Temporal(left), V::Temporal(right)) => Some(same_temporal(left, right)),
            (V::Member(left), V::Member(right)) => Some(left == right),
            (V::List(left), V::List(right)) | (V::Tuple(left), V::Tuple(right)) => all(left, right),
            // Values of two types: no one place holds both.
            _ => Some(false),
        }
    }
}

/// Whether two temporal constants are one value: two values of one
/// type, equal by that type's own equality (the owner's decision of
/// 2026-11-19 on hhenson/hgl-compiler#211, "two datetimes are equal as
/// instants"; https://github.com/hhenson/hgl-compiler/issues/211#issuecomment-7008815914).
///
/// TYPE-2: a `datetime` is a "UTC-instant", so two are one where they
/// are one instant: `@2024-01-01T01:00+01:00` is `@2024-01-01T00:00Z`.
/// "Same-instant zoned values need not be equal; zone aliases are
/// distinct names": a `zoned_datetime` is one instant in one named zone,
/// and two are one where both are the same. A `date`, a `time` and a
/// `civil_datetime` are their parts; a `timezone` is its name; a
/// `zoned_time`, which is of no date and so no instant, is its time of
/// day, its offset where written, and its zone.
pub(crate) fn same_temporal(left: &Temporal, right: &Temporal) -> bool {
    let types = (temporal_type(left), temporal_type(right));
    match types {
        (Scalar::Datetime, Scalar::Datetime) => instant(left) == instant(right),
        (Scalar::ZonedDatetime, Scalar::ZonedDatetime) => {
            instant(left) == instant(right) && left.zone == right.zone
        }
        _ => types.0 == types.1 && left == right,
    }
}

/// The microseconds from 0001-01-01T00:00Z to the instant a
/// `datetime` or a `zoned_datetime` is. `i128` holds every one without overflow.
fn instant(parts: &Temporal) -> Option<i128> {
    let date = parts.date?;
    let time = parts.time?;
    let offset = parts.offset?;
    let days = days_from_civil(date);
    let seconds =
        i128::from(time.hour) * 3600 + i128::from(time.minute) * 60 + i128::from(time.second)
            - i128::from(offset) * 60;
    Some((days * 86_400 + seconds) * 1_000_000 + i128::from(time.microsecond))
}

/// The days from 0001-01-01 to `date` in the proleptic Gregorian
/// calendar (TYPE-2: "Gregorian `date`").
fn days_from_civil(date: CalendarDate) -> i128 {
    let (year, month, day) = (
        i128::from(date.year),
        i128::from(date.month),
        i128::from(date.day),
    );
    // Days in the whole years before, with their leap days.
    let before = year - 1;
    let mut days = before * 365 + before / 4 - before / 100 + before / 400;
    const CUMULATIVE: [i128; 12] = [0, 31, 59, 90, 120, 151, 181, 212, 243, 273, 304, 334];
    days += CUMULATIVE[(month - 1).clamp(0, 11) as usize];
    let leap = (year % 4 == 0 && year % 100 != 0) || year % 400 == 0;
    if leap && month > 2 {
        days += 1;
    }
    days + day - 1
}

// ---- Keys ----

impl ConstantValue {
    /// The constant as text that no constant other than one the same
    /// ([`ConstantValue::same`]) is written as: what tells one instance
    /// of a generic declaration from another on a target. `None` for
    /// one not valued, or that names a generic parameter.
    pub fn key(&self) -> Option<String> {
        use ConstantValue as V;
        let all = |values: &[ConstantValue]| -> Option<String> {
            Some(
                values
                    .iter()
                    .map(ConstantValue::key)
                    .collect::<Option<Vec<String>>>()?
                    .join(", "),
            )
        };
        Some(match self {
            V::Int(value) => value.to_string(),
            // `0.0` and `-0.0` are one value (NUM-4); `{:?}` writes a
            // float so that it reads back as itself.
            V::Float(value) if *value == 0.0 => "0.0".to_string(),
            V::Float(value) => format!("{value:?}"),
            V::Bool(value) => value.to_string(),
            // `{:?}` writes a string quoted, each character that needs
            // it escaped.
            V::Str(value) => format!("{value:?}"),
            V::Null => "null".to_string(),
            V::Duration(microseconds) => format!("{microseconds}us"),
            V::Tenor(Tenor::Moves { months, days }) => match (months, days) {
                (months, 0) => format!("{months}M"),
                (0, days) => format!("{days}D"),
                (months, days) => format!("{months}M{days}D"),
            },
            V::Tenor(Tenor::BusinessDays(days)) => format!("{days}B"),
            V::Temporal(parts) => temporal_key(parts),
            V::Member(name) => format!(".{name}"),
            V::List(values) => format!("[{}]", all(values)?),
            V::Tuple(values) => format!("({})", all(values)?),
            V::Unbounded => "unbounded".to_string(),
            V::Parameter { .. }
            | V::Named { .. }
            | V::Constructed { .. }
            | V::Unary(..)
            | V::Binary(..)
            | V::Unevaluated
            | V::Unknown => return None,
        })
    }
}

/// A temporal constant's key: a `datetime` as the instant it is, in
/// UTC, a `zoned_datetime` as that instant in its zone
/// ([`same_temporal`]), and any other by its parts.
fn temporal_key(parts: &Temporal) -> String {
    let mut key = "@".to_string();
    let utc;
    let parts = if matches!(
        temporal_type(parts),
        Scalar::Datetime | Scalar::ZonedDatetime
    ) && let Some(instant) = instant(parts)
    {
        utc = Temporal {
            zone: parts.zone.clone(),
            ..from_instant(instant)
        };
        &utc
    } else {
        parts
    };
    if let Some(date) = parts.date {
        key.push_str(&format!(
            "{:04}-{:02}-{:02}",
            date.year, date.month, date.day
        ));
    }
    if let Some(time) = parts.time {
        if parts.date.is_some() {
            key.push('T');
        }
        key.push_str(&format!(
            "{:02}:{:02}:{:02}.{:06}",
            time.hour, time.minute, time.second, time.microsecond
        ));
    }
    if let Some(offset) = parts.offset {
        key.push_str(&format!("{offset:+}m"));
    }
    if let Some(zone) = &parts.zone {
        key.push_str(&format!("[{zone}]"));
    }
    key
}

/// The UTC `datetime` of an instant ([`instant`]).
fn from_instant(instant: i128) -> Temporal {
    let microsecond = instant.rem_euclid(1_000_000);
    let seconds = instant.div_euclid(1_000_000);
    let of_day = seconds.rem_euclid(86_400);
    let mut days = seconds.div_euclid(86_400);
    // The year, then the month, counted off one at a time: a key is
    // made rarely, and this is plain to check.
    let mut year: i128 = 1;
    loop {
        let leap = (year % 4 == 0 && year % 100 != 0) || year % 400 == 0;
        let length = if leap { 366 } else { 365 };
        if days < length {
            break;
        }
        days -= length;
        year += 1;
    }
    let leap = (year % 4 == 0 && year % 100 != 0) || year % 400 == 0;
    let lengths = [
        31,
        if leap { 29 } else { 28 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    let mut month = 1;
    for length in lengths {
        if days < length {
            break;
        }
        days -= length;
        month += 1;
    }
    // The `try_from`s cannot fail for an instant of a date TYPE-2 holds.
    Temporal {
        date: Some(CalendarDate {
            year: u16::try_from(year).unwrap_or(u16::MAX),
            month: u8::try_from(month).unwrap_or(12),
            day: u8::try_from(days + 1).unwrap_or(31),
        }),
        time: Some(ClockTime {
            hour: u8::try_from(of_day / 3600).unwrap_or(0),
            minute: u8::try_from(of_day / 60 % 60).unwrap_or(0),
            second: u8::try_from(of_day % 60).unwrap_or(0),
            microsecond: u32::try_from(microsecond).unwrap_or(0),
        }),
        offset: Some(0),
        zone: None,
    }
}

// ---- Giving parameters values ----

impl ConstantValue {
    /// True when this constant names a generic value parameter declared
    /// at one of `own`, itself or anywhere inside it: `N`, `N + 1`,
    /// `[N]`, `Side(N)`.
    pub(crate) fn names_one_of(&self, own: &[Location]) -> bool {
        match self {
            ConstantValue::Parameter { at, .. } => own.contains(at),
            ConstantValue::Unary(_, operand) | ConstantValue::Constructed { from: operand, .. } => {
                operand.names_one_of(own)
            }
            ConstantValue::Binary(_, left, right) => {
                left.names_one_of(own) || right.names_one_of(own)
            }
            ConstantValue::List(values) | ConstantValue::Tuple(values) => {
                values.iter().any(|value| value.names_one_of(own))
            }
            _ => false,
        }
    }

    /// This constant with each generic value parameter `given` gives a
    /// value put in its place, and what an expression of them then
    /// comes to worked out: `N + 1` with `N` given `2` is `3`, and
    /// `[N, 1]` is `[2, 1]`.
    pub(crate) fn substituted(
        &self,
        given: &dyn Fn(Location) -> Option<ConstantValue>,
    ) -> ConstantValue {
        match self {
            ConstantValue::Parameter { at, .. } => given(*at).unwrap_or_else(|| self.clone()),
            ConstantValue::Unary(operator, operand) => {
                negated_or_not(*operator, &operand.substituted(given))
            }
            ConstantValue::Binary(operator, left, right) => combined(
                *operator,
                &left.substituted(given),
                &right.substituted(given),
            ),
            // TYPE-7: "`Side(1)` is the member whose number is 1, and
            // `Side("buy")` the member of that exact name." One that
            // names none is rejected where it is called, and has no
            // value here.
            ConstantValue::Constructed { members, from } => {
                let from = from.substituted(given);
                let named = match &from {
                    ConstantValue::Int(number)
                        if members.iter().all(|(_, value)| value.is_some()) =>
                    {
                        members.iter().find(|(_, value)| *value == Some(*number))
                    }
                    ConstantValue::Str(text) => members.iter().find(|(name, _)| name == text),
                    _ => {
                        return ConstantValue::Constructed {
                            members: members.clone(),
                            from: Box::new(from),
                        };
                    }
                };
                match named {
                    Some((name, _)) => ConstantValue::Member(name.clone()),
                    None => ConstantValue::Unevaluated,
                }
            }
            // `[N, 1]`: each element given its value.
            ConstantValue::List(values) => ConstantValue::List(
                values
                    .iter()
                    .map(|value| value.substituted(given))
                    .collect(),
            ),
            ConstantValue::Tuple(values) => ConstantValue::Tuple(
                values
                    .iter()
                    .map(|value| value.substituted(given))
                    .collect(),
            ),
            _ => self.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn datetime(hour: u8, offset: i16) -> Temporal {
        Temporal {
            date: Some(CalendarDate {
                year: 2024,
                month: 1,
                day: 1,
            }),
            time: Some(ClockTime {
                hour,
                minute: 0,
                second: 0,
                microsecond: 0,
            }),
            offset: Some(offset),
            zone: None,
        }
    }

    /// TYPE-2: a `datetime` is a UTC instant: one hour at `+01:00` is
    /// midnight UTC, and the key says so.
    #[test]
    fn a_datetime_is_its_instant() {
        let at_one = datetime(1, 60);
        let at_midnight = datetime(0, 0);
        assert!(same_temporal(&at_one, &at_midnight));
        assert!(!same_temporal(&datetime(1, 0), &at_midnight));
        assert_eq!(
            ConstantValue::Temporal(at_one).key(),
            ConstantValue::Temporal(at_midnight).key()
        );
    }

    /// The instant of a date and back again, across a leap day.
    #[test]
    fn an_instant_reads_back() {
        let leap = Temporal {
            date: Some(CalendarDate {
                year: 2024,
                month: 2,
                day: 29,
            }),
            time: Some(ClockTime {
                hour: 23,
                minute: 59,
                second: 1,
                microsecond: 7,
            }),
            offset: Some(0),
            zone: None,
        };
        assert_eq!(from_instant(instant(&leap).unwrap()), leap);
        let first = datetime(0, 0);
        assert_eq!(from_instant(instant(&first).unwrap()), first);
    }

    /// NUM-2: "Mixed numeric comparison must not first round an i64
    /// operand to f64": 2^53 + 1 is more than the `f64` 2^53.
    #[test]
    fn a_mixed_comparison_is_exact() {
        let compare = |operator, whole: i64, real: f64| {
            combined(
                operator,
                &ConstantValue::Int(whole),
                &ConstantValue::Float(real),
            )
            .key()
        };
        let yes = Some("true".to_string());
        let no = Some("false".to_string());
        assert_eq!(
            compare(
                BinaryOperator::Equal,
                9_007_199_254_740_993,
                9_007_199_254_740_992.0
            ),
            no
        );
        assert_eq!(
            compare(
                BinaryOperator::Greater,
                9_007_199_254_740_993,
                9_007_199_254_740_992.0
            ),
            yes
        );
        assert_eq!(compare(BinaryOperator::Equal, 1, 1.0), yes);
        assert_eq!(compare(BinaryOperator::Less, 1, 1.5), yes);
        assert_eq!(compare(BinaryOperator::Less, -2, -1.5), yes);
        assert_eq!(
            compare(BinaryOperator::Less, i64::MAX, 9_223_372_036_854_775_808.0),
            yes
        );
        assert_eq!(
            compare(BinaryOperator::Greater, i64::MIN, f64::NEG_INFINITY),
            yes
        );
        assert_eq!(
            compare(
                BinaryOperator::NotEqual,
                i64::MIN,
                -9_223_372_036_854_775_808.0
            ),
            no
        );
    }

    /// NUM-3: true division of two `i64` is the `f64` nearest the exact
    /// quotient, for operands past 2^53 too.
    #[test]
    fn integer_true_division_rounds_once() {
        assert_eq!(divided(9_007_199_254_740_993, 3), 3_002_399_751_580_331.0);
        assert_eq!(divided(7, 2), 3.5);
        assert_eq!(divided(-7, 2), -3.5);
        assert_eq!(divided(i64::MAX, 1), 9_223_372_036_854_775_807.0);
        assert_eq!(divided(1, i64::MIN), -(2f64.powi(-63)));
        // 2^53 + 1 over 1 is a tie between 2^53 and 2^53 + 2: the even.
        assert_eq!(divided(9_007_199_254_740_993, 1), 9_007_199_254_740_992.0);
        assert_eq!(divided(9_007_199_254_740_995, 1), 9_007_199_254_740_996.0);
        // Against the nearest `f64` to the exact quotient, worked out in
        // `f64` where the operands are exact.
        for (left, right) in [
            (10_i64.pow(18) + 7, 9_007_199_254_740_993),
            (-(10_i64.pow(17)), 3),
        ] {
            let exact = divided(left, right);
            assert!((exact - left as f64 / right as f64).abs() <= exact.abs() * f64::EPSILON);
        }
        assert!(divided(1, 0).is_infinite());
        assert!(divided(0, 0).is_nan());
    }

    /// NUM-1: floor division rounds toward negative infinity, and the
    /// modulo has the divisor's sign. The examples are NUM's own:
    /// `-7 // 3` is `-3` and `-7 % 3` is `2`.
    #[test]
    fn integer_division_floors() {
        let divided = |left: i64, right: i64| {
            (
                combined(
                    BinaryOperator::FloorDivide,
                    &ConstantValue::Int(left),
                    &ConstantValue::Int(right),
                )
                .key(),
                combined(
                    BinaryOperator::Modulo,
                    &ConstantValue::Int(left),
                    &ConstantValue::Int(right),
                )
                .key(),
            )
        };
        let text = |quotient: &str, remainder: &str| {
            (Some(quotient.to_string()), Some(remainder.to_string()))
        };
        assert_eq!(divided(-7, 3), text("-3", "2"));
        assert_eq!(divided(7, -3), text("-3", "-2"));
        assert_eq!(divided(-7, -3), text("2", "-1"));
        assert_eq!(divided(7, 3), text("2", "1"));
        assert_eq!(divided(7, 0), (None, None));
        // The least `i64` floor-divided by -1 does not fit; its remainder
        // is 0.
        assert_eq!(divided(i64::MIN, -1), (None, Some("0".to_string())));
    }
}
