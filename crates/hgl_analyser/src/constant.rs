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
//! [`ConstantValue`]: the value, not how it is written. UNIT-13: "Two
//! compile-time constants are one where those values are the same,
//! however each is written", which [`ConstantValue::same`] asks, and
//! "An equality the program writes, in a `requires` as elsewhere, is the
//! language's `==` (GEN-3)", which [`ConstantValue::equals`] asks.
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
//! * a list, a tuple and a `set<str>` written out, of constants.
//!
//! What it cannot work out is [`ConstantValue::Unevaluated`]: an
//! operation that overflows, a division by zero, a symbol given
//! configuration, a date moved by a tenor. That is "not valued": equal to
//! nothing and unequal to nothing. What is no compile-time constant at
//! all, a local or a call, is [`Fault::NotConstant`].

use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};

use hgl_diagnostics::{Category, Diagnostic, Location};
use hgl_lexer::literal::{
    MAX_INTEGER_MAGNITUDE, decode_duration, decode_float, decode_integer, decode_string,
    decode_temporal, decode_tenor,
};
use hgl_parser::{HasSpan, SyntaxKind, SyntaxNode, SyntaxToken};
use hgl_source::FileId;

use crate::codes;
use crate::hir::{
    BinaryOperator, CalendarDate, ClockTime, Temporal, Tenor, TypePackArgument, UnaryOperator,
    Value,
};
use crate::program::{Binding, DeclarationKind, LocalKind, Program};
use crate::syntax::{name_token, tokens, ungrouped};
use crate::types::{Argument, ConstantValue, Detail, EnumOf, MemberOf, NamedKind, Scalar, Type};

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
    /// The arguments already given to the generic parameters of this declaration.
    pub(crate) arguments: &'s [(Location, &'s Argument)],
    /// The resolved type packs of this instance, by declaration (GEN-17).
    pub(crate) type_packs: &'s [(Location, &'s TypePackArgument)],
    /// The constants of modules followed so far.
    pub(crate) constants: &'s Constants,
}

/// The static result of checking a cold `type(e)` query (TYPE-18).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum QueryType {
    /// The operand checked cleanly with this type.
    Resolved(Type),
    /// Checking the operand emitted a diagnostic (ERR-9).
    Reported,
    /// The ordinary reader deferred selecting an operator until instantiation.
    InstanceDependent(Vec<(Location, Argument)>),
    /// Checking the operand emitted no error, yet produced no resolved type.
    Unresolved,
}

/// What following the constants of modules keeps (UNIT-13: "The
/// constant has that one value wherever it is read").
#[derive(Default)]
pub(crate) struct Constants {
    /// True during `read`, before declaration details, struct fields and
    /// body locals exist: `type(e)` queries defer to a later pass (TYPE-18).
    defer_type_queries: bool,
    /// Types already checked in the enclosing lexical reading (TYPE-18).
    types: RefCell<HashMap<Location, QueryType>>,
    /// The constants being worked out, so that one whose value depends
    /// on itself, which is reported as that (`constant.cycle`), ends
    /// here.
    following: RefCell<Vec<(usize, usize)>>,
    /// The values worked out, by module and declaration. A constant many
    /// others name is read once.
    known: RefCell<HashMap<(usize, usize), Result<Typed, Fault>>>,
}

impl Constants {
    /// A cache for the early `read` pass that defers `type(e)` queries until
    /// signatures, struct fields, and body locals are available (TYPE-18).
    pub(crate) fn deferred() -> Self {
        Self {
            defer_type_queries: true,
            ..Self::default()
        }
    }

    /// Keep a type query's checked operand type without its value or effects,
    /// returning any earlier entry so speculative rollbacks restore it.
    pub(crate) fn remember_type(&self, at: Location, ty: QueryType) -> Option<QueryType> {
        self.types.borrow_mut().insert(at, ty)
    }

    /// Remove a speculative type query entry after rolling back a trial read.
    pub(crate) fn remove_type(&self, at: Location) {
        self.types.borrow_mut().remove(&at);
    }

    /// Look up a type query already checked in this reading.
    pub(crate) fn remembered_type(&self, at: Location) -> Option<QueryType> {
        self.types.borrow().get(&at).cloned()
    }
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
    /// A member of a written string set is the same constant as one
    /// before it (EXPR-9).
    DuplicateSetMember { first: Location, again: Location },
    /// Something in it has been reported already: a name nothing binds,
    /// a literal the lexer rejected, a constant in a cycle (ERR-9).
    Reported,
    /// It is a module's constant whose evaluation fails ([`fails`]),
    /// which is reported where it is declared (`constant.failure`) and
    /// says nothing more where it is read (ERR-9).
    Failed,
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
    /// True where its evaluation fails ([`fails`]): it has no value, and
    /// a constant a module declares with it is a required-constant
    /// failure (UNIT-13).
    failed: bool,
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

    /// True for a known evaluation failure (ERR-9).
    pub(crate) fn evaluation_failed(&self) -> bool {
        self.failed
    }

    /// A constant that already has the type of its declaring place.
    pub(crate) fn of(value: ConstantValue, ty: Option<Type>) -> Typed {
        Typed {
            value,
            ty,
            open: Open::No,
            parts: Vec::new(),
            failed: false,
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
            Fault::DuplicateSetMember { .. } => return self.set_duplicate_report(),
            Fault::NotConstant | Fault::Reported | Fault::Failed => return None,
        })
    }

    /// EXPR-9: report a duplicate at the later member, with the first
    /// written member as its related location.
    pub(crate) fn set_duplicate_report(&self) -> Option<Diagnostic> {
        let Self::DuplicateSetMember { first, again } = *self else {
            return None;
        };
        Some(
            crate::error(
                Category::Type,
                codes::SET_DUPLICATE_MEMBER,
                again,
                "this member of the set is the same constant as an earlier member".to_string(),
                "EXPR-9",
            )
            .with_related(first, "the first member"),
        )
    }
}

/// What a constant read as `read` is, for a message: `an \`i64\``,
/// `` `null` ``, `the enum member \`.fast\``.
pub(crate) fn described(read: &Typed) -> String {
    match (&read.value, read.open, &read.ty) {
        (ConstantValue::Null, _, _) => "`null`".to_string(),
        (ConstantValue::Member(member), Open::Member, _) => {
            format!("the enum member `.{}`", member.name)
        }
        (_, _, Some(ty)) => described_type(ty),
        _ => "a constant".to_string(),
    }
}

/// A value of type `ty`, for a message: `an \`i64\``, `a \`str\``.
pub(crate) fn described_type(ty: &Type) -> String {
    // A list or a tuple whose elements' types are not known is told by
    // its shape.
    match ty {
        Type::List { element, .. } if partly_unknown(element) => return "a list".to_string(),
        Type::Tuple(elements) if elements.iter().any(partly_unknown) => {
            return "a tuple".to_string();
        }
        _ => {}
    }
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
        other => described_type(other),
    }
}

/// True for a type with a part not known: `list<_>`, `(i64, _)`.
fn partly_unknown(ty: &Type) -> bool {
    match ty {
        Type::Unknown => true,
        Type::List { element, .. } => partly_unknown(element),
        Type::Tuple(elements) => elements.iter().any(partly_unknown),
        _ => false,
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
    value(node, expected, scope)
}

/// `read` as a value of `expected`, the type its place expects
/// (EXPR-5). `None` where that is not known.
pub(crate) fn fit(
    read: Typed,
    expected: Option<&Type>,
    program: &Program,
) -> Result<ConstantValue, Fault> {
    // TYPE-7: "With no enum type expected, `.member` rejects": `any` is
    // no enum's type, nor gives one to a list's or a tuple's elements.
    if expected == Some(&Type::Scalar(Scalar::Any))
        && (read.open == Open::Member || implies(&read.value))
    {
        return Err(Fault::NoEnum);
    }
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
        (ConstantValue::Member(member), Open::Member) => {
            return match enum_place(expected) {
                Some(of) => member_named(program, of, &member.name),
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
        // A sequence read without a set or dictionary type cannot give
        // a value of that type afterwards (`sequence`).
        (ConstantValue::List(_), Type::Set(_) | Type::Dict { .. }) => {
            return Ok(ConstantValue::Unevaluated);
        }
        _ => read,
    };
    sizes_hold(&read.value, expected)?;
    match &read.ty {
        Some(ty) if !fits(ty, expected) => Err(Fault::Mismatch(Some(Box::new(ty.clone())))),
        _ => Ok(read.value),
    }
}

/// TYPE-10: "Size participates in identity": a list of another length
/// than the size expected is no value of it, at any depth. A constant
/// of `list<list<i64>>` holding `[[1, 2]]` is no `list<list<i64, 3>>`,
/// though its type says no size.
fn sizes_hold(value: &ConstantValue, expected: &Type) -> Result<(), Fault> {
    match (value, expected) {
        (_, Type::Nullable(inner)) => sizes_hold(value, inner),
        (ConstantValue::List(values), Type::List { element, size }) => {
            if let Some(size) = size
                && let ConstantValue::Int(count) = size.value
                && i64::try_from(values.len()).ok() != Some(count)
            {
                let length = values.len().to_string();
                return Err(Fault::Mismatch(Some(Box::new(Type::List {
                    element: element.clone(),
                    size: Some(crate::types::Constant {
                        text: length,
                        value: ConstantValue::Int(i64::try_from(values.len()).unwrap_or(i64::MAX)),
                    }),
                }))));
            }
            values
                .iter()
                .try_for_each(|value| sizes_hold(value, element))
        }
        (ConstantValue::Tuple(values), Type::Tuple(elements)) if values.len() == elements.len() => {
            values
                .iter()
                .zip(elements)
                .try_for_each(|(value, element)| sizes_hold(value, element))
        }
        _ => Ok(()),
    }
}

/// True when a value of type `ty` is a value of `expected`.
fn fits(ty: &Type, expected: &Type) -> bool {
    match (ty, expected) {
        _ if unknown(ty) || unknown(expected) => true,
        (Type::Nullable(left), Type::Nullable(right)) => fits(left, right),
        (_, Type::Nullable(inner)) => fits(ty, inner),
        // TYPE-10: "Size participates in identity": a size known to be
        // another is another list.
        (
            Type::List { element, size },
            Type::List {
                element: other,
                size: other_size,
            },
        ) => {
            // A list written out has no size in its type: its length is
            // held to the size its place expects by `fit`.
            let sizes = match (size, other_size) {
                (Some(size), Some(other_size)) => size.same_value(other_size) != Some(false),
                _ => true,
            };
            sizes && fits(element, other)
        }
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
        (Type::Parameter { .. } | Type::PackMember { .. }, _)
        | (_, Type::Parameter { .. } | Type::PackMember { .. }) => true,
        (left, right) => left.same_type(right),
    }
}

/// True for a type that says nothing of what a constant of it is: one
/// not known, `any`, or a generic parameter's not yet given.
fn unknown(ty: &Type) -> bool {
    matches!(
        ty,
        Type::Unknown
            | Type::Scalar(Scalar::Any)
            | Type::Parameter { .. }
            | Type::PackMember { .. }
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
fn enum_place(ty: &Type) -> Option<(usize, usize)> {
    match ty {
        Type::Nullable(inner) => enum_place(inner),
        Type::Named {
            module,
            declaration,
            kind: NamedKind::Enum,
            ..
        } => Some((*module, *declaration)),
        _ => None,
    }
}

/// What `node` comes to as a value of `expected`, read as it is written
/// (see [`read`]).
///
/// Reading calls itself once for each level of the tree under `node`,
/// and no more: the parser builds no tree deeper than its limit
/// (`hgl_parser::Limits::tree_depth`, a chain such as `1 + 1 + 1`
/// counted link by link), and the analyser is given a stack for that
/// depth (`crate::stack_size`). A constant a module declares is read
/// from the top of its own expression ([`module_constant`]).
fn value(node: &SyntaxNode, expected: Option<&Type>, scope: &Scope) -> Result<Typed, Fault> {
    // TYPE-14: a value of a `T?` other than `null` is a T, and is read
    // as one: `[1]` where a `list<f64>?` is expected is a list of
    // `f64`. `null` is held to the whole type by `fit`.
    let expected = expected.map(non_null);
    match node.kind() {
        SyntaxKind::Literal => literal_constant(node, expected),
        SyntaxKind::TypeOfExpr => {
            let at = Location::new(scope.file, node.span());
            let read = |known: QueryType| match known {
                QueryType::Resolved(ty) => Ok(Typed::of(
                    ConstantValue::Type(Box::new(ty)),
                    Some(Type::TypeValue),
                )),
                QueryType::InstanceDependent(bindings) => Ok(Typed::of(
                    scope.program.specialize_query(
                        &ConstantValue::InstanceTypeQuery { at, bindings },
                        scope.arguments,
                    ),
                    Some(Type::TypeValue),
                )),
                QueryType::Reported => Err(Fault::Reported),
                QueryType::Unresolved => Ok(Typed::unevaluated(Some(Type::TypeValue))),
            };
            if let Some(known) = scope.constants.remembered_type(at) {
                return read(known);
            }
            if scope.constants.defer_type_queries {
                return Ok(Typed::of(
                    ConstantValue::DeferredTypeQuery(at),
                    Some(Type::TypeValue),
                ));
            }
            read(crate::values::query_type(scope.program, scope.file, node))
        }
        // EXPR-1: "Parentheses select grouping": `(2.0)` is `2.0`.
        SyntaxKind::ParenExpr => match node.children().next() {
            Some(inner) => value(&inner, expected, scope),
            None => Err(Fault::Reported),
        },
        SyntaxKind::NameExpr | SyntaxKind::NamedType => name(node, scope),
        // TYPE-7: "Where a position expects the enum's type, `.member`
        // names one too."
        SyntaxKind::ImpliedMember => {
            let Some(member) = tokens(node).last() else {
                return Err(Fault::Reported);
            };
            match expected.and_then(enum_place) {
                Some(of) => Ok(Typed::of(
                    member_named(scope.program, of, member.text())?,
                    Some(enum_type(scope.program, of)),
                )),
                None if expected.is_some_and(|expected| !unknown(expected)) => Err(Fault::NoEnum),
                None => Ok(Typed {
                    open: Open::Member,
                    ..Typed::of(
                        ConstantValue::Member(MemberOf {
                            of: None,
                            name: member.text().to_string(),
                        }),
                        None,
                    )
                }),
            }
        }
        SyntaxKind::MemberExpr => qualified_member(node, scope),
        SyntaxKind::CallExpr => constructed_member(node, scope),
        SyntaxKind::PackKeyMembership if !scope.type_packs.is_empty() => {
            pack_key_membership(node, scope)
        }
        SyntaxKind::UnaryExpr => unary(node, expected, scope),
        SyntaxKind::BinaryExpr => binary(node, expected, scope),
        SyntaxKind::SequenceExpr => sequence(node, expected, scope),
        SyntaxKind::SetExpr => match expected {
            Some(ty @ Type::Set(element)) if **element == Type::Scalar(Scalar::Str) => {
                string_set(node, ty, scope)
            }
            Some(ty @ Type::Set(_)) => Ok(Typed::unevaluated(Some(ty.clone()))),
            _ => Ok(Typed::unevaluated(None)),
        },
        // TYPE-10: "`unbounded` is the size sentinel."
        SyntaxKind::UnboundedSize => Ok(Typed::of(ConstantValue::Unbounded, None)),
        SyntaxKind::TupleExpr | SyntaxKind::SequencePair => tuple(node, expected, scope),
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
            Value::Type(ty) => ConstantValue::Type(ty),
            Value::Int(value) => ConstantValue::Int(value),
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
            ConstantValue::Type(ty) => Value::Type(ty.clone()),
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
            open: Open::Integer,
            ..Typed::scalar(ConstantValue::Int(value), Scalar::I64)
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

/// The compile-time name of a reflected pack key (GEN-17).
fn is_pack_key(node: &SyntaxNode, scope: &Scope) -> bool {
    let node = ungrouped(node.clone());
    matches!(
        bound(&node, scope),
        Some((
            _,
            Binding::Local {
                kind: LocalKind::PackKey | LocalKind::PackIterationKey,
                ..
            }
        ))
    )
}

/// GEN-17: a positional key and a named key are unequal across kinds.
pub(crate) fn different_pack_key_kinds(left: &ConstantValue, right: &ConstantValue) -> bool {
    matches!(
        (left, right),
        (ConstantValue::Int(_), ConstantValue::Str(_))
            | (ConstantValue::Str(_), ConstantValue::Int(_))
    )
}

/// GEN-17: a guard in a concrete pass can test whether its key is in a
/// resolved type pack. Its result is an ordinary constant `bool`.
fn pack_key_membership(node: &SyntaxNode, scope: &Scope) -> Result<Typed, Fault> {
    let mut operands = node.children();
    let (Some(key), Some(pack)) = (operands.next(), operands.next()) else {
        return Err(Fault::Reported);
    };
    let (_, binding) = bound(&pack, scope).ok_or(Fault::Reported)?;
    let Binding::Local { at, .. } = binding else {
        return Err(Fault::NotConstant);
    };
    let (_, pack) = scope
        .type_packs
        .iter()
        .find(|(declared, _)| declared == at)
        .ok_or(Fault::NotConstant)?;
    let key = name(&key, scope)?;
    if pack.forwarded_from.is_some()
        || pack.infer_from_arguments
        || !matches!(key.value, ConstantValue::Int(_) | ConstantValue::Str(_))
    {
        return Ok(Typed::unevaluated(Some(Type::Scalar(Scalar::Bool))));
    }
    Ok(Typed::scalar(
        ConstantValue::Bool(pack.member(&key.value).is_some()),
        Scalar::Bool,
    ))
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
    named(&name_token(node).ok_or(Fault::Reported)?, scope)
}

/// The constant a name denotes, including a pack key (GEN-17).
pub(crate) fn named(name: &SyntaxToken, scope: &Scope) -> Result<Typed, Fault> {
    let binding = (scope.binding)(scope.file, name.span().start).ok_or(Fault::Reported)?;
    match binding {
        // "a generic value parameter in scope".
        Binding::Local {
            at,
            kind: LocalKind::GenericParameter | LocalKind::PackKey | LocalKind::PackIterationKey,
        } => {
            let given = scope.arguments.iter().find_map(|(parameter, argument)| {
                if parameter == at
                    && let Argument::Constant(given) = argument
                {
                    Some(given.value.clone())
                } else {
                    None
                }
            });
            let value = given.unwrap_or_else(|| ConstantValue::Parameter {
                name: name.text().to_string(),
                at: *at,
            });
            Ok(Typed::of(value, (scope.parameter)(*at)))
        }
        // CTRL-16: only an iteration binding given a value by this
        // constant scope can be a struct field name.
        Binding::Local {
            at,
            kind: LocalKind::Iteration,
        } => scope
            .arguments
            .iter()
            .find_map(|(binding, argument)| match argument {
                Argument::Constant(given) if at == binding => match &given.value {
                    ConstantValue::Str(name) => {
                        Some(Typed::scalar(ConstantValue::Str(name.clone()), Scalar::Str))
                    }
                    _ => None,
                },
                _ => None,
            })
            .ok_or(Fault::NotConstant),
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

/// True where evaluating the constant `key` fails ([`fails`]): UNIT-13,
/// "a failure in evaluating it is a required-constant failure (ERR-9)".
pub(crate) fn fails_to_evaluate(scope: &Scope, key: (usize, usize)) -> bool {
    matches!(module_constant(scope, key), Err(Fault::Failed))
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
    // A call of a value function is held as the constant it is: the
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
        arguments: &[],
        type_packs: &[],
        constants: scope.constants,
    };
    let Some(expression) = &expression else {
        return Err(Fault::Reported);
    };
    // Its own expression is read from its top.
    match value(expression, ty.as_ref(), &inner) {
        // UNIT-13: "a failure in evaluating it is a required-constant
        // failure (ERR-9)", reported where it is declared.
        Ok(read) if read.failed => Err(Fault::Failed),
        Ok(read) => match fit(read, ty.as_ref(), program) {
            // One this reader does not work out is not taken as known:
            // it is no value a generic value argument may be given
            // (GEN-8), as the same expression written in place is not.
            Ok(ConstantValue::Unevaluated) => Ok(Typed::unevaluated(ty)),
            Ok(value) => Ok(Typed::of(value, ty)),
            Err(Fault::NotConstant) => Ok(named),
            // What is wrong with a constant's own value is reported
            // where it is declared.
            Err(_) => Err(Fault::Reported),
        },
        Err(Fault::NotConstant) => Ok(named),
        Err(_) => Err(Fault::Reported),
    }
}

/// The type a constant's declaration, or a generic value parameter's,
/// writes, as far as it can be read before the program's types are:
/// a scalar, an enum, a generic parameter, and a nullable, list, set
/// or tuple of those. `None` for any other. A constant's place is given
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
        SyntaxKind::SetType => Some(Type::Set(Box::new(part(node)?))),
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
                        arguments: &[],
                        type_packs: &[],
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
            // TYPE-18: `Type` is the intrinsic descriptor type in type
            // positions such as `const Kind: Type` and `const K: Type = ...`.
            if crate::syntax::child(node, SyntaxKind::QualifiedName).is_none()
                && name.text() == "Type"
            {
                return Some(Type::TypeValue);
            }
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

/// The enum declared as `declaration` of `module`, as a constant names it.
fn enum_of(program: &Program, (module, declaration): (usize, usize)) -> EnumOf {
    let unit = &program.modules[module];
    let declared = &unit.declarations[declaration];
    EnumOf {
        declared: (module, declaration),
        written: unit.qualified_declaration_name(&declared.name, declared.test_owner),
    }
}

/// The member `name` of the enum `of`, as a constant.
fn member_of(of: EnumOf, name: &str) -> ConstantValue {
    ConstantValue::Member(MemberOf {
        of: Some(of),
        name: name.to_string(),
    })
}

/// The member of the enum `of` named `name` (TYPE-7).
fn member_named(program: &Program, of: (usize, usize), name: &str) -> Result<ConstantValue, Fault> {
    match members(program, of) {
        Some(members) if !members.iter().any(|member| member.name == name) => {
            Err(Fault::NoMember(format!("`{name}`")))
        }
        Some(_) => Ok(member_of(enum_of(program, of), name)),
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
fn constructed_member(node: &SyntaxNode, scope: &Scope) -> Result<Typed, Fault> {
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
    let given = value(&given, None, scope)?;
    // "that is an `i64` or a `str`": one known to be of another type, a
    // `const A: i64? = 0` among them (TYPE-14), constructs no member
    // here.
    let input = |scalar: Scalar| Some(Type::Scalar(scalar));
    if given.ty.as_ref().is_some_and(|known| {
        !unknown(known) && given.ty != input(Scalar::I64) && given.ty != input(Scalar::Str)
    }) {
        return Ok(Typed::unevaluated(Some(ty)));
    }
    let given = given.value;
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
                    of: enum_of(scope.program, of),
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
        member_of(enum_of(scope.program, of), &member.name),
        Some(ty),
    ))
}

/// `[a, b]`: a list of constants, or a string set in a place for one.
fn sequence(node: &SyntaxNode, expected: Option<&Type>, scope: &Scope) -> Result<Typed, Fault> {
    let element = match expected {
        Some(Type::List { element, .. }) => Some(&**element),
        Some(ty @ Type::Set(element)) if **element == Type::Scalar(Scalar::Str) => {
            return string_set(node, ty, scope);
        }
        // Other sets and dictionaries are not worked out here.
        Some(Type::Set(_) | Type::Dict { .. }) => return Ok(Typed::unevaluated(None)),
        _ => None,
    };
    let mut values = Vec::new();
    let mut types = Vec::new();
    let mut parts = Vec::new();
    let mut failed = false;
    for entry in node.children() {
        let read = value(&entry, element, scope)?;
        failed |= read.failed;
        types.push(read.ty.clone());
        values.push(match element {
            Some(element) => fit(read, Some(element), scope.program)?,
            None => {
                parts.push(read.clone());
                read.value
            }
        });
    }
    // A list whose elements' type is not known, `[]` or `[1, "a"]`, is
    // still a list: no place of another shape holds it.
    let element = element.cloned().or_else(|| {
        let first = types.first()?.clone()?;
        types
            .iter()
            .all(|ty| Type::same_optional(ty.as_ref(), Some(&first)))
            .then_some(first)
    });
    Ok(Typed {
        parts,
        failed,
        ..Typed::of(
            ConstantValue::List(values),
            Some(Type::List {
                element: Box::new(element.unwrap_or(Type::Unknown)),
                size: None,
            }),
        )
    })
}

/// FN-8: a written `set<str>` gives the names its members hold. EXPR-9
/// rejects repeated constants even where another member is not worked
/// out. Keep the source location for a repeated member and store only
/// the string values for a complete set (UNIT-13).
fn string_set(node: &SyntaxNode, ty: &Type, scope: &Scope) -> Result<Typed, Fault> {
    let element = Type::Scalar(Scalar::Str);
    let mut names = BTreeMap::new();
    let mut fault = None;
    let mut known = true;
    let mut failed = false;
    for entry in node.children() {
        if entry.kind() == SyntaxKind::SequencePair {
            return Ok(Typed::unevaluated(None));
        }
        let read = value(&entry, Some(&element), scope).and_then(|read| {
            failed |= read.failed;
            fit(read, Some(&element), scope.program)
        });
        match read {
            Ok(ConstantValue::Str(name)) => {
                let again = Location::new(scope.file, entry.span());
                if let Some(first) = names.insert(name, again) {
                    return Err(Fault::DuplicateSetMember { first, again });
                }
            }
            Ok(_) => known = false,
            Err(why) => {
                fault.get_or_insert(why);
            }
        }
    }
    if let Some(fault) = fault {
        return Err(fault);
    }
    let value = if known {
        ConstantValue::StringSet(names.into_keys().collect())
    } else {
        ConstantValue::Unevaluated
    };
    Ok(Typed {
        failed,
        ..Typed::of(value, Some(ty.clone()))
    })
}

/// `(a, b)`: a tuple of constants.
fn tuple(node: &SyntaxNode, expected: Option<&Type>, scope: &Scope) -> Result<Typed, Fault> {
    let entries: Vec<SyntaxNode> = node.children().collect();
    let elements = match expected {
        Some(Type::Tuple(elements)) if elements.len() == entries.len() => Some(elements),
        _ => None,
    };
    let mut values = Vec::new();
    let mut types = Vec::new();
    let mut parts = Vec::new();
    let mut failed = false;
    for (index, entry) in entries.iter().enumerate() {
        let element = elements.map(|elements| &elements[index]);
        let read = value(entry, element, scope)?;
        failed |= read.failed;
        types.push(read.ty.clone());
        if element.is_none() {
            parts.push(read.clone());
        }
        values.push(fit(read, element, scope.program)?);
    }
    // A tuple is one of its length whatever its elements' types.
    let ty = match elements {
        Some(elements) => Type::Tuple(elements.clone()),
        None => Type::Tuple(
            types
                .into_iter()
                .map(|ty| ty.unwrap_or(Type::Unknown))
                .collect(),
        ),
    };
    Ok(Typed {
        parts,
        failed,
        ..Typed::of(ConstantValue::Tuple(values), Some(ty))
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
fn unary(node: &SyntaxNode, expected: Option<&Type>, scope: &Scope) -> Result<Typed, Fault> {
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
                open: Open::Integer,
                ..Typed::scalar(ConstantValue::Int(i64::MIN), Scalar::I64)
            },
        });
    }
    let read = value(&operand, numeric_hint(expected), scope)?;
    if configured(node) {
        return configured_value(node, scope, read.ty);
    }
    // TYPE-14: "It does not make null an admitted value of unqualified
    // T": `-` and `!` take no `T?`, a `const A: i64? = 1` among them.
    if nullable(&read) {
        return Ok(Typed::unevaluated(None));
    }
    if operator == UnaryOperator::Not && has_deferred_type_query(&read.value) {
        return Ok(Typed::of(
            ConstantValue::DeferredTypeQuery(Location::new(scope.file, node.span())),
            read.ty,
        ));
    }
    let value = negated_or_not(operator, &read.value);
    // NUM-2: the negation of the least `i64` is no `i64`; OPS-10 and
    // TEN-2 say the same of a duration and a tenor counts.
    let fails = operator == UnaryOperator::Negate
        && matches!(value, ConstantValue::Unevaluated)
        && matches!(
            read.value,
            ConstantValue::Int(_) | ConstantValue::Duration(_) | ConstantValue::Tenor(_)
        );
    Ok(Typed {
        value,
        ty: read.ty,
        open: match read.open {
            Open::Integer => Open::Integer,
            _ => Open::No,
        },
        parts: Vec::new(),
        failed: read.failed || fails,
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

/// `left op right`, and a chain of them, `a + b + c`, which is
/// `(a + b) + c`.
///
/// A chain is read in a loop, from its innermost left operand out, and
/// not by reading calls nested as deep as it is long: the parser nests
/// a chain's links in the tree, though not in the source, and a chain
/// may be as long as the tree is deep (`hgl_parser::Limits`). An
/// operand that is itself nested, `(a + b)`, is read by a call, as deep
/// as the source nests.
fn binary(node: &SyntaxNode, expected: Option<&Type>, scope: &Scope) -> Result<Typed, Fault> {
    // Each link, outermost first, with the type expected of it.
    let mut links: Vec<(SyntaxNode, Option<Type>)> = Vec::new();
    let mut link = node.clone();
    let mut expected = expected.cloned();
    let innermost = loop {
        let (left, _, operator) = operands(&link)?;
        let hint = operand_hint(operator, expected.as_ref()).cloned();
        links.push((link, expected));
        if left.kind() != SyntaxKind::BinaryExpr {
            break value(&left, hint.as_ref(), scope)?;
        }
        link = left;
        expected = hint;
    };
    let mut read = innermost;
    for (link, expected) in links.iter().rev() {
        read = joined(link, read, expected.as_ref(), scope)?;
    }
    Ok(read)
}

/// The left and the right operand of the binary expression `node`, and
/// its operator.
fn operands(node: &SyntaxNode) -> Result<(SyntaxNode, SyntaxNode, BinaryOperator), Fault> {
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
    Ok((left, right, operator))
}

/// The type expected of each operand of `operator` where `expected` is
/// expected of the whole.
///
/// GEN-5: "where a candidate that gives the requested output takes
/// every integer literal as an `i64`, none is widened". `+`, `-`, `*`,
/// `//` and `%` of two `i64` give an `i64`, so where an `f64` is
/// requested their literals are widened; `/` of two gives an `f64`
/// already (NUM-1), and its literals are not.
fn operand_hint(operator: BinaryOperator, expected: Option<&Type>) -> Option<&Type> {
    match operator {
        BinaryOperator::Add
        | BinaryOperator::Subtract
        | BinaryOperator::Multiply
        | BinaryOperator::FloorDivide
        | BinaryOperator::Modulo => numeric_hint(expected),
        _ => None,
    }
}

/// `left op right` of the binary expression `node`, its left operand
/// read as `left`.
fn joined(
    node: &SyntaxNode,
    left: Typed,
    expected: Option<&Type>,
    scope: &Scope,
) -> Result<Typed, Fault> {
    let (left_node, right_node, operator) = operands(node)?;
    let mut left = left;
    let mut right = value(&right_node, operand_hint(operator, expected), scope)?;
    // EXPR-6: "Boolean connectives short-circuit left to right": the
    // right operand of `false &&` and of `true ||` is not evaluated, so
    // a failure of its evaluation is none, and the connective is the
    // left operand. Its type is still held to a `bool`, and one of no
    // type is still of none.
    let skipped = matches!(
        (operator, &left.value),
        (BinaryOperator::And, ConstantValue::Bool(false))
            | (BinaryOperator::Or, ConstantValue::Bool(true))
    );
    if skipped && right.failed {
        right.value = left.value.clone();
        right.failed = false;
    }
    if configured(node) {
        return configured_value(node, scope, None);
    }
    // TYPE-14: "It does not make null an admitted value of unqualified
    // T": core's operators take no `T?` operand, a `const A: i64? = 1`
    // among them, but `==` and `!=`, which compare two of a `T?`; "A T
    // is a value where a `T?` is expected", and so is `null`.
    let equality = matches!(operator, BinaryOperator::Equal | BinaryOperator::NotEqual);
    if !equality && (nullable(&left) || nullable(&right)) {
        return Ok(Typed::unevaluated(None));
    }
    // EXPR-6: "The operands of `&&`, `||`, and `!` are bools."
    let boolean = Type::Scalar(Scalar::Bool);
    let other = |read: &Typed| {
        read.ty
            .as_ref()
            .is_some_and(|ty| !unknown(ty) && *ty != boolean)
    };
    if matches!(operator, BinaryOperator::And | BinaryOperator::Or)
        && (other(&left) || other(&right))
    {
        return Ok(Typed::unevaluated(None));
    }
    if equality {
        // GEN-17: a positional key differs from a name. Ordinary `i64`
        // and `str` values still need the same scalar type for equality.
        if different_pack_key_kinds(&left.value, &right.value)
            && (is_pack_key(&left_node, scope) || is_pack_key(&right_node, scope))
        {
            return Ok(Typed::scalar(
                ConstantValue::Bool(operator == BinaryOperator::NotEqual),
                Scalar::Bool,
            ));
        }
        // TYPE-7: an implied member is of the enum its place expects:
        // `Mode.fast == .slow` compares two of `Mode`, as core's `==`
        // is selected for them, and `[Mode.fast] == [.slow]` two lists
        // of them. Implied members on both sides, or one beside no
        // enum's value or naming no member of it, are of no type `==`
        // takes (OPS-2), and not worked out.
        let boolean = Some(Type::Scalar(Scalar::Bool));
        match (implies(&left.value), implies(&right.value)) {
            (true, true) => return Ok(Typed::unevaluated(boolean)),
            (true, false) => match implied(left, &right, scope.program) {
                Some(member) => left = member,
                None => return Ok(Typed::unevaluated(boolean)),
            },
            (false, true) => match implied(right, &left, scope.program) {
                Some(member) => right = member,
                None => return Ok(Typed::unevaluated(boolean)),
            },
            (false, false) => {}
        }
        // OPS-2: "`==` and `!=` compare two values of one type", and so
        // of two whose types are known, a generic value parameter's
        // among them: `N == "a"` with `const N: i64` is of none.
        if let (Some(left), Some(right)) = (&left.ty, &right.ty)
            && !one_scalar_type(left, right)
        {
            return Ok(Typed::unevaluated(boolean));
        }
        if has_deferred_type_query(&left.value) || has_deferred_type_query(&right.value) {
            return Ok(Typed::of(
                ConstantValue::DeferredTypeQuery(Location::new(scope.file, node.span())),
                boolean,
            ));
        }
    } else if matches!(operator, BinaryOperator::And | BinaryOperator::Or)
        && (has_deferred_type_query(&left.value) || has_deferred_type_query(&right.value))
    {
        return Ok(Typed::of(
            ConstantValue::DeferredTypeQuery(Location::new(scope.file, node.span())),
            Some(boolean),
        ));
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
            | BinaryOperator::FloorDivide
            | BinaryOperator::Modulo
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
    // For a short-circuiting operator with an unresolved left value,
    // failure on the right is conditional until the left value is known.
    let evaluates_right = match operator {
        BinaryOperator::And => matches!(left.value, ConstantValue::Bool(true)),
        BinaryOperator::Or => matches!(left.value, ConstantValue::Bool(false)),
        _ => true,
    };
    let failed = left.failed
        || (evaluates_right && right.failed)
        || (matches!(value, ConstantValue::Unevaluated)
            && fails(operator, &left.value, &right.value));
    Ok(Typed {
        value,
        ty,
        open,
        parts: Vec::new(),
        failed,
    })
}

/// True where `left op right` of these values has no value because it
/// fails when it is evaluated, and not because this reader does not
/// work it out: an `i64` result outside the `i64`s (NUM-2: "An
/// unrepresentable i64 result produces an HGL error"), a zero divisor
/// of two `i64` under the default `.fail` (NUM-12), a duration outside
/// its range (OPS-10), and a tenor sum of two forms (TEN-2).
fn fails(operator: BinaryOperator, left: &ConstantValue, right: &ConstantValue) -> bool {
    use BinaryOperator as Op;
    use ConstantValue as V;
    matches!(
        (operator, left, right),
        (
            Op::Add | Op::Subtract | Op::Multiply | Op::FloorDivide | Op::Modulo,
            V::Int(_),
            V::Int(_)
        ) | (Op::Add | Op::Subtract, V::Duration(_), V::Duration(_))
            | (Op::Multiply, V::Duration(_), V::Int(_))
            | (Op::Multiply, V::Int(_), V::Duration(_))
            | (Op::Add | Op::Subtract, V::Tenor(_), V::Tenor(_))
            | (Op::Multiply, V::Tenor(_), V::Int(_))
            | (Op::Multiply, V::Int(_), V::Tenor(_))
    )
}

/// `read`, which holds an implied member, as a value of the type
/// `other` is of: `.slow` as `Mode.slow`, `[.slow]` as `[Mode.slow]`.
/// `Some` of it unchanged where `other`'s type is not known; `None`
/// where it is no value of that type, or still holds an implied member
/// (`[.slow]` beside a `list<_>`).
fn implied(read: Typed, other: &Typed, program: &Program) -> Option<Typed> {
    let Some(ty) = other.ty.as_ref().filter(|ty| !unknown(ty)) else {
        return Some(read);
    };
    let value = fit(read, Some(ty), program).ok()?;
    (!implies(&value)).then(|| Typed::of(value, Some(ty.clone())))
}

/// False where `left` and `right` are two scalar or named types that no
/// one `==` takes: `i64` and `str`, two enums. An `i64` and an `f64`
/// compare (OPS-2, "an `i64` with an `f64` included"), and a `T` with a
/// `T?` (TYPE-14). A collection's elements are held to one type by
/// `one_type`, as their values are known.
fn one_scalar_type(left: &Type, right: &Type) -> bool {
    let plain = |ty: &Type| match ty {
        Type::Nullable(inner) => (**inner).clone(),
        other => other.clone(),
    };
    let (left, right) = (plain(left), plain(right));
    let numeric = |ty: &Type| matches!(ty, Type::Scalar(Scalar::I64 | Scalar::F64));
    match (&left, &right) {
        (Type::Scalar(_) | Type::Named { .. }, Type::Scalar(_) | Type::Named { .. }) => {
            unknown(&left)
                || unknown(&right)
                || (numeric(&left) && numeric(&right))
                || fits(&left, &right)
        }
        _ => true,
    }
}

/// True where `read` is of a nullable type, `T?`.
fn nullable(read: &Typed) -> bool {
    matches!(read.ty, Some(Type::Nullable(_)))
}

/// True where `value` is or holds an implied member, `.slow`, that is
/// not yet given its enum.
fn implies(value: &ConstantValue) -> bool {
    match value {
        ConstantValue::Member(member) => member.of.is_none(),
        ConstantValue::List(values) | ConstantValue::Tuple(values) => values.iter().any(implies),
        _ => false,
    }
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
            BinaryOperator::Add
            | BinaryOperator::Subtract
            | BinaryOperator::Multiply
            | BinaryOperator::FloorDivide
            | BinaryOperator::Modulo,
            Some(Scalar::I64 | Scalar::F64),
            Some(Scalar::F64),
        )
        | (
            BinaryOperator::Add
            | BinaryOperator::Subtract
            | BinaryOperator::Multiply
            | BinaryOperator::FloorDivide
            | BinaryOperator::Modulo,
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
pub(crate) fn negated_or_not(operator: UnaryOperator, value: &ConstantValue) -> ConstantValue {
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
            | V::Binary(..)
            | V::RangeLength(..),
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
    // A value is symbolic where any part of it is: `[N] == [1]` is
    // worked out once `N` is given its value.
    fn symbolic(value: &ConstantValue) -> bool {
        match value {
            V::Parameter { .. }
            | V::Named { .. }
            | V::Constructed { .. }
            | V::Unary(..)
            | V::Binary(..) => true,
            V::List(values) | V::Tuple(values) => values.iter().any(symbolic),
            _ => false,
        }
    }
    // EXPR-6: "Boolean connectives short-circuit left to right": the
    // right operand of `false &&` and of `true ||` is not needed where
    // it is a parameter or a constant not yet given its value, which
    // `binary` holds to a `bool`. One not worked out stays so: it may
    // be an operation of no type (`1 + "a"`) as well as one that fails.
    match (operator, left, right) {
        (Op::And, V::Bool(false), V::Parameter { .. } | V::Named { .. }) => {
            return V::Bool(false);
        }
        (Op::Or, V::Bool(true), V::Parameter { .. } | V::Named { .. }) => return V::Bool(true),
        _ => {}
    }
    if matches!(left, V::Unknown) || matches!(right, V::Unknown) {
        return V::Unknown;
    }
    if matches!(left, V::Unevaluated) || matches!(right, V::Unevaluated) {
        return V::Unevaluated;
    }
    if symbolic(left) || symbolic(right) {
        return V::Binary(operator, Box::new(left.clone()), Box::new(right.clone()));
    }
    // NUM-5: "NaN is neither equal nor close to any value, including
    // itself", and core's comparisons are false for a NaN operand but
    // `!=`. A NaN is a constant, UNIT-13: "any two NaNs, `0.0 / 0.0`
    // among them, are one constant, though `==` holds for no NaN". It is
    // told here before a number is ordered.
    let number = |value: &ConstantValue| matches!(value, V::Int(_) | V::Float(_));
    let nan = |value: &ConstantValue| matches!(value, V::Float(value) if value.is_nan());
    if matches!(
        operator,
        Op::Equal | Op::NotEqual | Op::Less | Op::LessEqual | Op::Greater | Op::GreaterEqual
    ) && number(left)
        && number(right)
        && (nan(left) || nan(right))
    {
        return V::Bool(operator == Op::NotEqual);
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
        (Op::Equal | Op::NotEqual, _, _) if !one_type(left, right) => None,
        (Op::Equal, _, _) => left.equals(right).map(V::Bool),
        (Op::NotEqual, _, _) => left.equals(right).map(|equal| V::Bool(!equal)),
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
        // zero is a signed infinity, and zero by zero a NaN, which is a
        // constant, the same as any NaN constant (UNIT-13).
        (Op::Divide, V::Int(left), V::Int(right)) => float(divided(*left, *right)),
        // NUM-1: "Unconfigured mixed i64/f64 and f64 arithmetic returns
        // f64": the `i64` as the `f64` nearest it.
        (
            Op::Add | Op::Subtract | Op::Multiply | Op::Divide | Op::FloorDivide | Op::Modulo,
            V::Int(whole),
            V::Float(_),
        ) => {
            return combined(operator, &V::Float(*whole as f64), right);
        }
        (
            Op::Add | Op::Subtract | Op::Multiply | Op::Divide | Op::FloorDivide | Op::Modulo,
            V::Float(_),
            V::Int(whole),
        ) => {
            return combined(operator, left, &V::Float(*whole as f64));
        }
        // NUM-1: `//` and `%` of finite `f64` operands round toward
        // negative infinity and take the divisor's sign. One of an
        // infinity, or by a zero, is IEEE's, and not worked out here.
        (Op::FloorDivide | Op::Modulo, V::Float(left), V::Float(right))
            if left.is_finite() && right.is_finite() && *right != 0.0 =>
        {
            let (quotient, remainder) = floored(*left, *right);
            float(match operator {
                Op::FloorDivide => quotient,
                _ => remainder,
            })
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

/// The floored quotient and the remainder of two finite `f64`, the
/// divisor not zero: the remainder has the divisor's sign (NUM-1). It
/// is worked out as the runtime library works it out
/// (`runtime/cpp/src/scalar.cpp`, after CPython's `float_floor_div`),
/// from the IEEE remainder (`%` of Rust's `f64`), so a constant has the
/// value its expression has when the program runs.
fn floored(left: f64, right: f64) -> (f64, f64) {
    let mut remainder = left % right;
    let mut quotient = (left - remainder) / right;
    if remainder != 0.0 {
        if (right < 0.0) != (remainder < 0.0) {
            remainder += right;
            quotient -= 1.0;
        }
    } else {
        remainder = 0.0_f64.copysign(right);
    }
    let floor = if quotient != 0.0 {
        let floor = quotient.floor();
        if quotient - floor > 0.5 {
            floor + 1.0
        } else {
            floor
        }
    } else {
        0.0_f64.copysign(left / right)
    };
    (floor, remainder)
}

/// An `f64` result as a constant: a NaN and an infinity too (UNIT-13:
/// "any two NaNs, `0.0 / 0.0` among them, are one constant").
fn float(value: f64) -> Option<ConstantValue> {
    Some(ConstantValue::Float(value))
}

/// False where `left` and `right` are known to be values of two types,
/// which OPS-2 gives no `==`: `1` and `"a"`, two enums' members, a date
/// and a time, lists of those. An `i64` and an `f64` compare (OPS-2,
/// "an `i64` with an `f64` included").
fn one_type(left: &ConstantValue, right: &ConstantValue) -> bool {
    use ConstantValue as V;
    match (left, right) {
        (V::Int(_), V::Float(_)) | (V::Float(_), V::Int(_)) => true,
        // TYPE-14: "`null` has the nullable type its place expects":
        // beside a `T`, a `T?`'s.
        (V::Null, _) | (_, V::Null) => true,
        (V::Temporal(left), V::Temporal(right)) => temporal_type(left) == temporal_type(right),
        // TYPE-7: "Enum members are distinct typed constants"; one whose
        // enum is not given it yet may be of either.
        (V::Member(left), V::Member(right)) => match (&left.of, &right.of) {
            (Some(left), Some(right)) => left.declared == right.declared,
            _ => true,
        },
        (V::List(left), V::List(right)) => left
            .iter()
            .zip(right)
            .all(|(left, right)| one_type(left, right)),
        (V::Tuple(left), V::Tuple(right)) => {
            left.len() == right.len()
                && left
                    .iter()
                    .zip(right)
                    .all(|(left, right)| one_type(left, right))
        }
        _ => std::mem::discriminant(left) == std::mem::discriminant(right),
    }
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

/// True when `value` contains a `type(...)` query deferred from `read`
/// until declaration details, struct fields, and body locals exist (TYPE-18).
pub(crate) fn has_deferred_type_query(value: &ConstantValue) -> bool {
    contains_query(value, &|value| {
        matches!(
            value,
            ConstantValue::DeferredTypeQuery(_) | ConstantValue::DeferredDefaultQuery(_)
        )
    })
}

fn contains_query(value: &ConstantValue, matches: &dyn Fn(&ConstantValue) -> bool) -> bool {
    let nested = |value: &ConstantValue| contains_query(value, matches);
    match value {
        ConstantValue::Type(ty) => ty.any_part(&|_| false, &|c| nested(&c.value)),
        ConstantValue::Unary(_, operand) | ConstantValue::Constructed { from: operand, .. } => {
            nested(operand)
        }
        ConstantValue::Binary(_, left, right) | ConstantValue::RangeLength(left, right) => {
            nested(left) || nested(right)
        }
        ConstantValue::List(values) | ConstantValue::Tuple(values) => values.iter().any(nested),
        _ => matches(value),
    }
}

/// True when `ty` contains any deferred `type(...)` constant argument (TYPE-18).
pub(crate) fn type_has_deferred_query(ty: &Type) -> bool {
    ty.any_part(&|_| false, &|constant| {
        has_deferred_type_query(&constant.value)
    })
}

/// Whether a type still carries the operand of an instance-dependent query.
pub(crate) fn type_has_instance_query(ty: &Type) -> bool {
    ty.any_part(&|_| false, &|c| {
        contains_query(&c.value, &|value| {
            matches!(value, ConstantValue::InstanceTypeQuery { .. })
        })
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
        ConstantValue::Unevaluated
        | ConstantValue::DeferredTypeQuery(_)
        | ConstantValue::DeferredDefaultQuery(_) => false,
        ConstantValue::Type(ty) => !type_has_deferred_query(ty),
        ConstantValue::Unary(_, operand) | ConstantValue::Constructed { from: operand, .. } => {
            worked_out(operand)
        }
        ConstantValue::Binary(_, left, right) | ConstantValue::RangeLength(left, right) => {
            worked_out(left) && worked_out(right)
        }
        ConstantValue::List(values) | ConstantValue::Tuple(values) => values.iter().all(worked_out),
        _ => true,
    }
}

/// True where working `value` out with what `given` gives each generic
/// value parameter in it fails when it is evaluated ([`fails`]), and not
/// because this reader does not work it out: `M * 4611686018427387904`
/// with `M` 4 is an `i64` out of range (NUM-2: "An unrepresentable i64
/// result produces an HGL error"), and `10 // M` with `M` 0 a division
/// of two `i64` by zero under the default `.fail` (NUM-12).
pub(crate) fn fails_when_given(
    value: &ConstantValue,
    given: &dyn Fn(Location) -> Option<ConstantValue>,
) -> bool {
    match value {
        ConstantValue::Binary(operator, left, right) => {
            if fails_when_given(left, given) || fails_when_given(right, given) {
                return true;
            }
            let (left, right) = (left.substituted(given), right.substituted(given));
            matches!(
                combined(*operator, &left, &right),
                ConstantValue::Unevaluated
            ) && fails(*operator, &left, &right)
        }
        ConstantValue::Unary(operator, operand) => {
            if fails_when_given(operand, given) {
                return true;
            }
            // OPS-10, TEN-2: the negation of the least `i64`, and of
            // the least duration or tenor count, is none.
            let operand = operand.substituted(given);
            *operator == UnaryOperator::Negate
                && matches!(
                    negated_or_not(*operator, &operand),
                    ConstantValue::Unevaluated
                )
                && matches!(
                    operand,
                    ConstantValue::Int(_) | ConstantValue::Duration(_) | ConstantValue::Tenor(_)
                )
        }
        ConstantValue::Constructed { from, .. } => fails_when_given(from, given),
        ConstantValue::List(values) | ConstantValue::Tuple(values) => {
            values.iter().any(|value| fails_when_given(value, given))
        }
        _ => false,
    }
}

// ---- Comparing ----

impl ConstantValue {
    /// Whether two constants are the same constant: `Some(true)` where
    /// they are, `Some(false)` where they are not, `None` where that is
    /// not known. This is what type identity and a target's instances
    /// ask (GEN-8, TYPE-3). A comparison the program makes asks
    /// [`ConstantValue::equals`].
    ///
    /// UNIT-13: "Two compile-time constants are one where those values
    /// are the same, however each is written: `1`, `1.0` and `1.00` as an
    /// `f64`; `60s` and `1m`", and `1Y` and `12M` (TYPE-17). Two temporal
    /// values are compared by [`same_temporal`]. "The same value is not
    /// `==` ...: `-0.0` and `0.0` are two constants, though `-0.0 ==
    /// 0.0`, and any two NaNs, `0.0 / 0.0` among them, are one constant,
    /// though `==` holds for no NaN": two `f64` are compared by
    /// [`same_float`].
    ///
    /// What is not valued is neither: [`ConstantValue::Unevaluated`]
    /// gives `None` beside anything. So does a generic value parameter,
    /// which is the same as itself and may be any value beside another,
    /// and an expression of one, which is the same as one written alike.
    /// An `i64` and an `f64` meet only where their place's type was not
    /// known, for a literal where an `f64` is expected is read as one
    /// (EXPR-5): beside each other they give `None` too.
    pub fn same(&self, other: &ConstantValue) -> Option<bool> {
        self.compared(other, &same_float)
    }

    /// Whether two constants are equal as the language's `==` compares
    /// them (OPS-2): what a `requires` asks of an option, and what a
    /// `switch` label's duplicates and a delta's overlap ask, which the
    /// program compares as it runs. UNIT-13: "An equality the program
    /// writes, in a `requires` as elsewhere, is the language's `==`
    /// (GEN-3)", and GEN-3: "with `const k: f64`, `requires k == 0.0`
    /// holds where `k` is `-0.0`". For a label and a delta's keys that is
    /// proposed (CTRL-6, TYPE-16: "Proposed: ... and not by their
    /// sameness as constants"). It is [`ConstantValue::same`] but that
    /// two `f64` are equal as numbers
    /// (NUM-5: "opposite signed zeros are equal", and "NaN is neither
    /// equal nor close to any value, including itself").
    ///
    /// One constant held by its name, or one generic value parameter,
    /// written twice is taken to equal itself, as it is the same
    /// constant: whether it is a NaN is not known.
    pub fn equals(&self, other: &ConstantValue) -> Option<bool> {
        self.compared(other, &|left, right| left == right)
    }

    /// [`ConstantValue::same`] and [`ConstantValue::equals`], which
    /// differ in how they compare two `f64`: `floats`.
    fn compared(&self, other: &ConstantValue, floats: &dyn Fn(f64, f64) -> bool) -> Option<bool> {
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
                .map(|(left, right)| left.compared(right, floats))
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
            (
                V::Unevaluated | V::Unknown | V::DeferredTypeQuery(_) | V::DeferredDefaultQuery(_),
                _,
            )
            | (
                _,
                V::Unevaluated | V::Unknown | V::DeferredTypeQuery(_) | V::DeferredDefaultQuery(_),
            ) => None,
            (
                V::InstanceTypeQuery { at, bindings },
                V::InstanceTypeQuery {
                    at: other,
                    bindings: args,
                },
            ) => (at == other
                && bindings.len() == args.len()
                && bindings
                    .iter()
                    .zip(args)
                    .all(|((at, bound), (other_at, other))| {
                        at == other_at && bound.same_argument(other)
                    }))
            .then_some(true),
            (V::InstanceTypeQuery { .. }, _) | (_, V::InstanceTypeQuery { .. }) => None,
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
                alike(operand.compared(other_operand, floats))
            }
            (
                V::Binary(operator, left, right),
                V::Binary(other_operator, other_left, other_right),
            ) if operator == other_operator => alike(all(
                &[(**left).clone(), (**right).clone()],
                &[(**other_left).clone(), (**other_right).clone()],
            )),
            (V::RangeLength(lower, upper), V::RangeLength(other_lower, other_upper)) => {
                alike(lower.compared(other_lower, floats).and_then(|same| {
                    if same {
                        upper.compared(other_upper, floats)
                    } else {
                        Some(false)
                    }
                }))
            }
            (
                V::Constructed {
                    of: left_of,
                    from: left,
                    ..
                },
                V::Constructed {
                    of: right_of,
                    from: right,
                    ..
                },
            ) if left_of.declared == right_of.declared => alike(left.compared(right, floats)),
            (
                V::Parameter { .. }
                | V::Named { .. }
                | V::Constructed { .. }
                | V::Unary(..)
                | V::Binary(..)
                | V::RangeLength(..),
                _,
            )
            | (
                _,
                V::Parameter { .. }
                | V::Named { .. }
                | V::Constructed { .. }
                | V::Unary(..)
                | V::Binary(..)
                | V::RangeLength(..),
            ) => None,
            (V::Int(_), V::Float(_)) | (V::Float(_), V::Int(_)) => None,
            (V::Int(left), V::Int(right)) => Some(left == right),
            (V::Float(left), V::Float(right)) => Some(floats(*left, *right)),
            (V::Type(left), V::Type(right)) => match (left.identity(), right.identity()) {
                (Some(left), Some(right)) => Some(left == right),
                // TYPE-18: the same symbolic type in a generic body remains
                // the same for every instance, even before it has an identity.
                _ if left.same_type(right)
                    && !left.any_part(&|part| matches!(part, Type::Unknown), &|_| false) =>
                {
                    Some(true)
                }
                _ => None,
            },
            (V::Bool(left), V::Bool(right)) => Some(left == right),
            (V::Str(left), V::Str(right)) => Some(left == right),
            (V::StringSet(left), V::StringSet(right)) => Some(left == right),
            (V::Null, V::Null) | (V::Unbounded, V::Unbounded) => Some(true),
            (V::Duration(left), V::Duration(right)) => Some(left == right),
            // TYPE-17: "Two tenors are equal when they are of the same
            // form and their counts are equal, however each was
            // written": the lexer has read `1Y` as twelve months.
            (V::Tenor(left), V::Tenor(right)) => Some(left == right),
            (V::Temporal(left), V::Temporal(right)) => Some(same_temporal(left, right)),
            // TYPE-7: "Enum members are distinct typed constants": two
            // are one where they are one enum's one member. One whose
            // enum its place has not given it yet is of its place's.
            (V::Member(left), V::Member(right)) => Some(
                left.name == right.name
                    && match (&left.of, &right.of) {
                        (Some(left), Some(right)) => left.declared == right.declared,
                        _ => true,
                    },
            ),
            (V::List(left), V::List(right)) | (V::Tuple(left), V::Tuple(right)) => all(left, right),
            // Values of two types: no one place holds both.
            _ => Some(false),
        }
    }
}

/// Whether two `f64` are the same constant: the same value, which is not
/// `==` (UNIT-13: "`-0.0` and `0.0` are two constants, though `-0.0 ==
/// 0.0`, and any two NaNs ... are one constant"): a NaN is not told
/// apart by its bits. This is the one place that says it.
pub(crate) fn same_float(left: f64, right: f64) -> bool {
    left.to_bits() == right.to_bits() || (left.is_nan() && right.is_nan())
}

/// Whether two temporal constants are one value: two values of one
/// type, equal by that type's own equality. UNIT-13: "the `datetime`s
/// `@2026-09-03T09:30Z` and `@2026-09-03T10:30+01:00`, one instant".
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
pub(crate) fn instant(parts: &Temporal) -> Option<i128> {
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
            V::Type(ty) => format!("type({})", ty.identity()?),
            V::Int(value) => value.to_string(),
            // `0.0` and `-0.0` are one value (NUM-4); `{:?}` writes a
            // float so that it reads back as itself.
            // One text for one value (`same_float`): `-0.0` is `-0.0`,
            // and every NaN is `NaN`.
            V::Float(value) if value.is_nan() => "NaN".to_string(),
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
            // The enum is written, as a named type is (`m::Mode`): a
            // place of `any` may be given members of two.
            V::Member(member) => match &member.of {
                Some(of) => format!("{}.{}", of.written, member.name),
                None => format!(".{}", member.name),
            },
            V::List(values) => format!("[{}]", all(values)?),
            V::StringSet(values) => {
                let names: Vec<String> = values.iter().map(|name| format!("{name:?}")).collect();
                format!("set<str>[{}]", names.join(", "))
            }
            V::Tuple(values) => format!("({})", all(values)?),
            V::Unbounded => "unbounded".to_string(),
            V::Parameter { .. }
            | V::Named { .. }
            | V::Constructed { .. }
            | V::Unary(..)
            | V::Binary(..)
            | V::RangeLength(..)
            | V::InstanceTypeQuery { .. }
            | V::DeferredTypeQuery(_)
            | V::DeferredDefaultQuery(_)
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
            ConstantValue::Type(ty) => crate::declared::mentions(ty, own),
            ConstantValue::InstanceTypeQuery { bindings, .. } => {
                bindings.iter().any(|(_, arg)| match arg {
                    Argument::Type(ty) => crate::declared::mentions(ty, own),
                    Argument::Constant(c) => c.value.names_one_of(own),
                    Argument::NoResult => false,
                })
            }
            ConstantValue::Unary(_, operand) | ConstantValue::Constructed { from: operand, .. } => {
                operand.names_one_of(own)
            }
            ConstantValue::Binary(_, left, right) | ConstantValue::RangeLength(left, right) => {
                left.names_one_of(own) || right.names_one_of(own)
            }
            ConstantValue::List(values) | ConstantValue::Tuple(values) => {
                values.iter().any(|value| value.names_one_of(own))
            }
            _ => false,
        }
    }

    /// The length of `lower..<upper`, as an i64 list size (EXPR-16,
    /// EXPR-17, TYPE-10), deferring arithmetic on generic bounds.
    pub(crate) fn range_length(lower: Self, upper: Self) -> Self {
        if let (Self::Int(lower), Self::Int(upper)) = (&lower, &upper) {
            return if upper <= lower {
                Self::Int(0)
            } else {
                upper
                    .checked_sub(*lower)
                    .map_or(Self::Unevaluated, Self::Int)
            };
        }
        Self::RangeLength(Box::new(lower), Box::new(upper))
    }

    /// TYPE-10: a fixed extent is nonnegative. When its written value
    /// is the range's difference, that difference needs no clamping.
    pub(crate) fn same_extent(&self, other: &Self) -> bool {
        let Self::RangeLength(lower, upper) = self else {
            return false;
        };
        if matches!(**lower, Self::Int(0)) && upper.same(other) == Some(true) {
            return true;
        }
        Self::Binary(BinaryOperator::Subtract, upper.clone(), lower.clone()).same(other)
            == Some(true)
    }

    /// This constant with each generic value parameter `given` gives a
    /// value put in its place, and what an expression of them then
    /// comes to worked out: `N + 1` with `N` given `2` is `3`, and
    /// `[N, 1]` is `[2, 1]`.
    pub(crate) fn substituted(
        &self,
        given: &dyn Fn(Location) -> Option<ConstantValue>,
    ) -> ConstantValue {
        self.substituted_with_types(given, &Clone::clone)
    }

    /// Evaluate the constant after placing both ordinary generic values
    /// and Type-valued descriptors, including those inside aggregates.
    pub(crate) fn substituted_with_types(
        &self,
        given: &dyn Fn(Location) -> Option<ConstantValue>,
        descriptor: &dyn Fn(&ConstantValue) -> ConstantValue,
    ) -> ConstantValue {
        let part = |value: &ConstantValue| value.substituted_with_types(given, descriptor);
        match self {
            ConstantValue::Type(_) | ConstantValue::InstanceTypeQuery { .. } => descriptor(self),
            ConstantValue::Parameter { at, .. } => given(*at).unwrap_or_else(|| self.clone()),
            ConstantValue::Unary(operator, operand) => negated_or_not(*operator, &part(operand)),
            ConstantValue::Binary(operator, left, right) => {
                combined(*operator, &part(left), &part(right))
            }
            ConstantValue::RangeLength(lower, upper) => {
                Self::range_length(part(lower), part(upper))
            }
            // TYPE-7: "`Side(1)` is the member whose number is 1, and
            // `Side("buy")` the member of that exact name." One that
            // names none is rejected where it is called, and has no
            // value here.
            ConstantValue::Constructed { of, members, from } => {
                let from = part(from);
                let named = match &from {
                    ConstantValue::Int(number)
                        if members.iter().all(|(_, value)| value.is_some()) =>
                    {
                        members.iter().find(|(_, value)| *value == Some(*number))
                    }
                    ConstantValue::Str(text) => members.iter().find(|(name, _)| name == text),
                    _ => {
                        return ConstantValue::Constructed {
                            of: of.clone(),
                            members: members.clone(),
                            from: Box::new(from),
                        };
                    }
                };
                match named {
                    Some((name, _)) => member_of(of.clone(), name),
                    None => ConstantValue::Unevaluated,
                }
            }
            // `[N, 1]`: each element given its value.
            ConstantValue::List(values) => ConstantValue::List(values.iter().map(part).collect()),
            ConstantValue::Tuple(values) => ConstantValue::Tuple(values.iter().map(part).collect()),
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

    /// NUM-1: `//` and `%` of `f64` round toward negative infinity and
    /// take the divisor's sign, as Python's do: `-7.0 // 2.0` is `-4.0`
    /// and `-7.0 % 2.0` is `1.0`.
    #[test]
    fn float_division_floors() {
        assert_eq!(floored(-7.0, 2.0), (-4.0, 1.0));
        assert_eq!(floored(7.0, -2.0), (-4.0, -1.0));
        assert_eq!(floored(7.5, 2.0), (3.0, 1.5));
        assert_eq!(
            floored(9_007_199_254_740_992.0, 1.0),
            (9_007_199_254_740_992.0, 0.0)
        );
        let (quotient, remainder) = floored(-0.0, 1.0);
        assert!(quotient == 0.0 && quotient.is_sign_negative() && remainder == 0.0);
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

    /// UNIT-13: "The same value is not `==` ...: `-0.0` and `0.0` are
    /// two constants, though `-0.0 == 0.0`, and any two NaNs, `0.0 / 0.0`
    /// among them, are one constant, though `==` holds for no NaN."
    /// GEN-3: "`requires k == 0.0` holds where `k` is `-0.0`."
    #[test]
    fn the_same_constant_is_not_the_languages_equality() {
        use ConstantValue as V;
        let (zero, minus_zero) = (V::Float(0.0), V::Float(-0.0));
        let (nan, other_nan) = (V::Float(f64::NAN), V::Float(-f64::NAN));
        assert_eq!(zero.same(&minus_zero), Some(false));
        assert_eq!(zero.equals(&minus_zero), Some(true));
        assert_eq!(nan.same(&other_nan), Some(true));
        assert_eq!(nan.equals(&other_nan), Some(false));
        let listed = |value: &V| V::List(vec![value.clone()]);
        assert_eq!(listed(&zero).same(&listed(&minus_zero)), Some(false));
        assert_eq!(listed(&zero).equals(&listed(&minus_zero)), Some(true));
        // One text for one constant, which names its instance.
        assert_ne!(zero.key(), minus_zero.key());
        assert_eq!(nan.key(), other_nan.key());
    }
}
