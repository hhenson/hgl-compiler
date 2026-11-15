//! Types: what a written type means.
//!
//! The parser keeps a type as it is spelled: `list<f64, 3>` is a
//! `ListType` node holding tokens. The stages after the analyser ask
//! other questions of it: is this the same type as that one, what does
//! the list hold, how long is it. [`Type`] is the form that answers them:
//!
//! ```text
//! list<pc::Quote, 3>   --read-->   List { element: Named { Quote of pricing.curves }, size: 3 }
//! ```
//!
//! Two types are the same type when their [`Type`] values are equal. A
//! struct, enum or native type is known by where it is declared, not by
//! how it is spelled where it is used (UNIT-4): `Quote` through an import
//! and `pc::Quote` through an alias are one `Named` value.
//!
//! These are plain records. The pass in `read` fills them in; nothing
//! here checks a rule.

use std::fmt;

use hgl_diagnostics::Location;
use hgl_parser::SyntaxKind;

/// The scalar types (TYPE-1, TYPE-2): each is written with a reserved
/// word and has an identity of its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Scalar {
    /// `bool`
    Bool,
    /// `i64`: a signed 64-bit integer.
    I64,
    /// `f64`: a binary64 float.
    F64,
    /// `str`: Unicode text.
    Str,
    /// `bytes`: a sequence of octets.
    Bytes,
    /// `any`: a boxed value that keeps its exact type (TYPE-8).
    Any,
    /// `date`: a Gregorian date.
    Date,
    /// `time`: a time of day, to the microsecond.
    Time,
    /// `datetime`: a UTC instant.
    Datetime,
    /// `duration`: elapsed time.
    Duration,
    /// `civil_datetime`: a local date and time.
    CivilDatetime,
    /// `timezone`: a named zone.
    Timezone,
    /// `zoned_datetime`: an instant resolved in a zone.
    ZonedDatetime,
    /// `zoned_time`: a time of day in a zone, on no date.
    ZonedTime,
}

impl Scalar {
    /// The scalar a reserved word names, or `None` for any other token.
    pub(crate) fn from_keyword(kind: SyntaxKind) -> Option<Scalar> {
        Some(match kind {
            SyntaxKind::KwBool => Scalar::Bool,
            SyntaxKind::KwI64 => Scalar::I64,
            SyntaxKind::KwF64 => Scalar::F64,
            SyntaxKind::KwStr => Scalar::Str,
            SyntaxKind::KwBytes => Scalar::Bytes,
            SyntaxKind::KwAny => Scalar::Any,
            SyntaxKind::KwDate => Scalar::Date,
            SyntaxKind::KwTime => Scalar::Time,
            SyntaxKind::KwDatetime => Scalar::Datetime,
            SyntaxKind::KwDuration => Scalar::Duration,
            SyntaxKind::KwCivilDatetime => Scalar::CivilDatetime,
            SyntaxKind::KwTimezone => Scalar::Timezone,
            SyntaxKind::KwZonedDatetime => Scalar::ZonedDatetime,
            SyntaxKind::KwZonedTime => Scalar::ZonedTime,
            _ => return None,
        })
    }

    /// The word the scalar is written with.
    pub fn as_str(self) -> &'static str {
        match self {
            Scalar::Bool => "bool",
            Scalar::I64 => "i64",
            Scalar::F64 => "f64",
            Scalar::Str => "str",
            Scalar::Bytes => "bytes",
            Scalar::Any => "any",
            Scalar::Date => "date",
            Scalar::Time => "time",
            Scalar::Datetime => "datetime",
            Scalar::Duration => "duration",
            Scalar::CivilDatetime => "civil_datetime",
            Scalar::Timezone => "timezone",
            Scalar::ZonedDatetime => "zoned_datetime",
            Scalar::ZonedTime => "zoned_time",
        }
    }
}

/// What a written type means.
///
/// `Box<Type>` is a `Type` stored elsewhere and pointed to: a type that
/// holds another, as `list<f64>` holds `f64`, cannot hold it directly,
/// since its size would then have no end.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Type {
    /// `i64`, `str`, `datetime` and the other scalars.
    Scalar(Scalar),
    /// A struct, an enum or a native type: `Quote`, `pc::Curve`,
    /// `Box<i64>`.
    Named {
        /// The declared name, for messages. Identity is the two
        /// positions below (UNIT-4).
        name: String,
        /// The position of the declaring module in `Program::modules`.
        module: usize,
        /// The position of the declaration in that module's
        /// `declarations`.
        declaration: usize,
        /// The generic arguments, in order; empty for a type that takes
        /// none.
        arguments: Vec<Argument>,
    },
    /// A generic parameter of the declaration the type is written in:
    /// the `T` of `fn first<T>(items: list<T>) -> T`.
    Parameter {
        /// Its name.
        name: String,
        /// Where it is declared. Two parameters are the same parameter
        /// when they are declared in the same place.
        at: Location,
    },
    /// `tuple<i64, str>`
    Tuple(Vec<Type>),
    /// `list<f64>`, `list<f64, 3>` or `list<f64, unbounded>` (TYPE-10).
    List {
        /// What the list holds.
        element: Box<Type>,
        /// Its size; `None` for `list<T>`, which grows.
        size: Option<Constant>,
    },
    /// `set<str>`
    Set(Box<Type>),
    /// `dict<str, f64>`
    Dict {
        /// The key type.
        key: Box<Type>,
        /// The value type.
        value: Box<Type>,
    },
    /// `rolling<f64, 5m>` or `rolling<f64, 5, 2>`: a window (TYPE-11).
    Rolling {
        /// What the window holds.
        value: Box<Type>,
        /// The most it holds: a tick count or a duration.
        maximum: Constant,
        /// The least it holds before it is ready; `None` when it is not
        /// written, which means the maximum.
        minimum: Option<Constant>,
    },
    /// `ref<f64>`: an endpoint, not a payload (TYPE-12).
    Ref(Box<Type>),
    /// `signal`: an input with no payload (TYPE-12).
    Signal,
    /// `atomic<Quote>`: whole snapshots of an ordinary value (TYPE-9).
    Atomic(Box<Type>),
    /// `result<f64, error>` (TYPE-13).
    Result {
        /// The type of a success.
        value: Box<Type>,
        /// The type of a failure.
        error: Box<Type>,
    },
    /// `error` (TYPE-13).
    Error,
    /// `delta<list<f64>>`
    Delta(Box<Type>),
    /// `f64?`: an `f64` or null (TYPE-14). The `?` covers the whole type
    /// before it (TYPE-15).
    Nullable(Box<Type>),
    /// Not known. The name is left for the prelude, or the type has been
    /// reported; nothing is reported about a use of it (ERR-9).
    Unknown,
}

/// One generic argument: a type, or a constant.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Argument {
    /// `i64` in `Box<i64>`.
    Type(Type),
    /// `3` in `Matrix<3, 4>`.
    Constant(Constant),
}

/// A constant written inside a type: a list's size, a window's size, a
/// generic argument.
#[derive(Debug, Clone)]
pub struct Constant {
    /// The expression as written, with single spaces: `N + 1`.
    pub text: String,
    /// What it comes to.
    pub value: ConstantValue,
}

// Two constants are the same when they come to the same value. One
// whose value this pass does not hold, a literal of another kind or an
// expression not evaluated yet, is compared as it is written: `true` is
// not `false`, though both are "a bool".
impl PartialEq for Constant {
    fn eq(&self, other: &Constant) -> bool {
        match (&self.value, &other.value) {
            (ConstantValue::Unevaluated, ConstantValue::Unevaluated)
            | (ConstantValue::Other(_), ConstantValue::Other(_)) => self.text == other.text,
            (left, right) => left == right,
        }
    }
}

impl Eq for Constant {}

/// What a constant inside a type comes to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConstantValue {
    /// An integer.
    Int(i64),
    /// A duration, in microseconds.
    Duration(i64),
    /// `unbounded`, the size sentinel of a list (TYPE-10).
    Unbounded,
    /// A generic value parameter of the declaration it is written in:
    /// the `N` of `list<f64, N>`.
    Parameter {
        /// Its name.
        name: String,
        /// Where it is declared.
        at: Location,
    },
    /// A constant of another kind, named for messages: `a float`. Its
    /// value is not held here; the constant's text is.
    Other(&'static str),
    /// Not evaluated by this pass: an expression that calls a function
    /// or reads a name. The pass that evaluates constants in full reads
    /// it again.
    Unevaluated,
}

impl fmt::Display for Constant {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.text)
    }
}

/// Writes `items` with `, ` between them.
fn comma_separated<T: fmt::Display>(f: &mut fmt::Formatter<'_>, items: &[T]) -> fmt::Result {
    for (index, item) in items.iter().enumerate() {
        if index > 0 {
            f.write_str(", ")?;
        }
        write!(f, "{item}")?;
    }
    Ok(())
}

impl fmt::Display for Argument {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Argument::Type(argument) => write!(f, "{argument}"),
            Argument::Constant(argument) => write!(f, "{argument}"),
        }
    }
}

/// A type as HGL writes it: `list<Quote, 3>`. A named type is written by
/// its declared name alone, whatever alias the source used.
impl fmt::Display for Type {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Type::Scalar(scalar) => f.write_str(scalar.as_str()),
            Type::Named {
                name, arguments, ..
            } => {
                f.write_str(name)?;
                if !arguments.is_empty() {
                    f.write_str("<")?;
                    comma_separated(f, arguments)?;
                    f.write_str(">")?;
                }
                Ok(())
            }
            Type::Parameter { name, .. } => f.write_str(name),
            Type::Tuple(elements) => {
                f.write_str("tuple<")?;
                comma_separated(f, elements)?;
                f.write_str(">")
            }
            Type::List { element, size } => match size {
                Some(size) => write!(f, "list<{element}, {size}>"),
                None => write!(f, "list<{element}>"),
            },
            Type::Set(element) => write!(f, "set<{element}>"),
            Type::Dict { key, value } => write!(f, "dict<{key}, {value}>"),
            Type::Rolling {
                value,
                maximum,
                minimum,
            } => match minimum {
                Some(minimum) => write!(f, "rolling<{value}, {maximum}, {minimum}>"),
                None => write!(f, "rolling<{value}, {maximum}>"),
            },
            Type::Ref(target) => write!(f, "ref<{target}>"),
            Type::Signal => f.write_str("signal"),
            Type::Atomic(payload) => write!(f, "atomic<{payload}>"),
            Type::Result { value, error } => write!(f, "result<{value}, {error}>"),
            Type::Error => f.write_str("error"),
            Type::Delta(shape) => write!(f, "delta<{shape}>"),
            Type::Nullable(inner) => write!(f, "{inner}?"),
            Type::Unknown => f.write_str("_"),
        }
    }
}

// ---- What a declaration's types are ----

/// A type written in a module, and what it means.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WrittenType {
    /// Where it is written: the whole of it, `list<f64, 3>` and not its
    /// parts.
    pub at: Location,
    /// What it means.
    pub ty: Type,
}

/// What the types of one declaration are. `Module::details` has one for
/// each of `Module::declarations`, in the same order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Detail {
    /// A function, an operator, an operator implementation or a native
    /// function.
    Callable(Signature),
    /// A struct.
    Struct(StructType),
    /// An enum or a native type: nothing of them is a written type.
    Plain,
}

/// The parameters and result of a callable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Signature {
    /// Its generic parameters, in order.
    pub generics: Vec<GenericParameter>,
    /// Its parameters, in order.
    pub parameters: Vec<Parameter>,
    /// Its result type; `None` when it has no `->`. A temporal function
    /// without one has no output (FN-1).
    pub result: Option<Type>,
}

/// One parameter of a callable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Parameter {
    /// Its name.
    pub name: String,
    /// Where the name is written.
    pub at: Location,
    /// True when it is marked `const`: fixed configuration (FN-1).
    pub constant: bool,
    /// Its type. For a pack, the type after `...` or inside `...{ }`.
    pub ty: Type,
    /// What kind of pack it is, when it is one (FN-8).
    pub pack: Option<Pack>,
    /// True when it has a default.
    pub has_default: bool,
}

/// The two packs (FN-8).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pack {
    /// `...T`: the positional arguments the fixed parameters leave.
    Positional,
    /// `...{Fields}`: the named ones.
    Named,
}

/// One generic parameter of a declaration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GenericParameter {
    /// Its name.
    pub name: String,
    /// Where the name is written.
    pub at: Location,
    /// What it is declared as.
    pub kind: GenericKind,
}

/// What a generic parameter is declared as (GEN-13).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GenericKind {
    /// `T`: a type, or a value whose kind its uses decide. This pass
    /// does not infer which.
    Plain,
    /// `...Ts`: a type pack, with the cardinality written after it
    /// (GEN-10).
    Pack {
        /// The fewest types it holds.
        minimum: u64,
        /// The most; `None` for no limit.
        maximum: Option<u64>,
    },
    /// `const N: i64`: a value, of the type given.
    Value(Type),
}

/// The generic parameters, parents and fields of a struct.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StructType {
    /// Its generic parameters, in order.
    pub generics: Vec<GenericParameter>,
    /// The parents written after `:`, in order.
    pub parents: Vec<Type>,
    /// Its own members, in order: the parents' fields are not repeated
    /// here.
    pub fields: Vec<Field>,
}

/// One member of a struct.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Field {
    /// Its name.
    pub name: String,
    /// Where the name is written.
    pub at: Location,
    /// Its type; `None` for `name = value`, which overrides the default
    /// of a field a parent declares (TYPE-4).
    pub ty: Option<Type>,
    /// True when it has a default.
    pub has_default: bool,
}

impl fmt::Display for Signature {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if !self.generics.is_empty() {
            f.write_str("<")?;
            comma_separated(f, &self.generics)?;
            f.write_str(">")?;
        }
        f.write_str("(")?;
        comma_separated(f, &self.parameters)?;
        f.write_str(")")?;
        match &self.result {
            Some(result) => write!(f, " -> {result}"),
            None => Ok(()),
        }
    }
}

impl fmt::Display for Parameter {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.constant {
            f.write_str("const ")?;
        }
        write!(f, "{}: ", self.name)?;
        match self.pack {
            Some(Pack::Positional) => write!(f, "...{}", self.ty)?,
            Some(Pack::Named) => write!(f, "...{{{}}}", self.ty)?,
            None => write!(f, "{}", self.ty)?,
        }
        if self.has_default {
            // The default's value is for the pass that evaluates
            // constants.
            f.write_str(" = ...")?;
        }
        Ok(())
    }
}

impl fmt::Display for GenericParameter {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.kind {
            GenericKind::Plain => f.write_str(&self.name),
            GenericKind::Pack { minimum, maximum } => {
                write!(f, "...{}", self.name)?;
                match (minimum, maximum) {
                    (0, None) => Ok(()),
                    (minimum, None) => write!(f, "{{{minimum}:*}}"),
                    (minimum, Some(maximum)) if minimum == maximum => write!(f, "{{{minimum}}}"),
                    (minimum, Some(maximum)) => write!(f, "{{{minimum}:{maximum}}}"),
                }
            }
            GenericKind::Value(ty) => write!(f, "const {}: {ty}", self.name),
        }
    }
}
