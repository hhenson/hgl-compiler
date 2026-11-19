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
    /// `tenor`: a rule for moving a date (TYPE-17).
    Tenor,
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
            SyntaxKind::KwTenor => Scalar::Tenor,
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
            Scalar::Tenor => "tenor",
            Scalar::CivilDatetime => "civil_datetime",
            Scalar::Timezone => "timezone",
            Scalar::ZonedDatetime => "zoned_datetime",
            Scalar::ZonedTime => "zoned_time",
        }
    }
}

/// How a trace is written (TST-7): "Within one eval all inputs and
/// expectation use the same dense/timed style."
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TraceStyle {
    /// `[7, _, 9]`: one cell for each engine step from the run's start.
    Dense,
    /// `[0s: 7, 2s: 9]`: each cell at a `duration` from the run's start
    /// or at a `datetime`, which the scalar says.
    Timed(Scalar),
}

impl TraceStyle {
    /// TST-7 distinguishes dense and timed notation across traces. Absolute
    /// datetimes and offsets can name the same instant in different traces.
    pub(crate) fn same_notation(self, other: Self) -> bool {
        matches!(
            (self, other),
            (Self::Dense, Self::Dense) | (Self::Timed(_), Self::Timed(_))
        )
    }
}

/// What a named type is declared as. A type carries it so that what its
/// delta is (TYPE-16) can be said from the type alone, wherever a
/// generic parameter is replaced by it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NamedKind {
    /// A struct: its delta is a type of its own.
    Struct,
    /// An enum: it publishes whole, and is its own delta.
    Enum,
    /// A native type: it publishes whole, and is its own delta.
    Native,
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
        /// What the name is declared as.
        kind: NamedKind,
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
    /// `rolling_window<f64, 5m>` or `rolling_window<f64, 5, 2>`: a window (TYPE-11).
    RollingWindow {
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
    /// `endpoint<f64>`: the endpoint of a temporal value, where a native
    /// value function takes the endpoint and not its payload (TYPE-12,
    /// TIME-9).
    Endpoint(Box<Type>),
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
    /// What an error's `.context` holds when it is not null: "a
    /// read-only nullable record of `.node: str`, `.phase: str` ... and
    /// `.evaluation_time: datetime`" (ERR-4). Source writes no type for
    /// it: it is a type of this compiler's, read through its three
    /// parts.
    ErrorContext,
    /// `delta<list<f64>>`
    Delta(Box<Type>),
    /// What an `eval` gives, and a trace written out for one: "a trace
    /// of that result's type, and source writes no type for it"
    /// (TST-3). It is a type of this compiler's, for a value of a test.
    Trace {
        /// What each cell is: one cycle's publication, the `delta<T>`
        /// of the type the trace is of (TST-5).
        cell: Box<Type>,
        /// Dense or timed, where a trace that has cells says which.
        style: Option<TraceStyle>,
    },
    /// `f64?`: an `f64` or null (TYPE-14). The `?` covers the whole type
    /// before it (TYPE-15).
    Nullable(Box<Type>),
    /// Not known. The name is left for the prelude, or the type has been
    /// reported; nothing is reported about a use of it (ERR-9).
    Unknown,
}

impl Type {
    /// What `atomic<payload>` is. TYPE-9: "`atomic<V>` stops expansion
    /// at a complete ordinary V. For non-composite S, `atomic<S>`
    /// normalizes to S before matching, recursively."
    ///
    /// A composite is what temporalization expands: a tuple, a struct,
    /// a list, a dictionary and a set. Any other type publishes whole as
    /// it is, and so is its own atomic: a scalar, an enum, a native
    /// type, a `result`, an `error`, a delta that is a type of its own,
    /// and a nullable of one of those. An atomic of an atomic is that
    /// atomic. The atomic of a type not yet known, a generic parameter's,
    /// stays as written, and is formed again, here, when the parameter is
    /// replaced.
    pub fn atomic(payload: Type) -> Type {
        let whole = match &payload {
            Type::Scalar(_) | Type::Error | Type::Result { .. } | Type::Atomic(_) => true,
            Type::Named {
                kind: NamedKind::Enum | NamedKind::Native,
                ..
            } => true,
            Type::Delta(of) => !matches!(**of, Type::Parameter { .. } | Type::Unknown),
            Type::Nullable(inner) => !matches!(Type::atomic((**inner).clone()), Type::Atomic(_)),
            _ => false,
        };
        if whole {
            payload
        } else {
            Type::Atomic(Box::new(payload))
        }
    }

    /// `delta<of>`: "the ordinary value type of what a temporal value of
    /// T publishes in one cycle" (TYPE-16).
    ///
    /// For most T it is another type the language has, and that type is
    /// what this returns: `f64` for `f64`, `tuple<f64?, i64?>` for
    /// `tuple<f64, i64>`. A nullable's, a list's, a set's, a
    /// dictionary's and a struct's delta is a type of its own, `Type::Delta`. So is the delta of a type not
    /// yet known, a generic parameter's: "In a generic declaration it
    /// stays as written until T is known", and it is formed again, here,
    /// when the parameter is replaced.
    pub fn delta(of: Type) -> Type {
        let optional = |ty: Type| match ty {
            Type::Nullable(_) => ty,
            other => Type::Nullable(Box::new(other)),
        };
        match of {
            // "A scalar, an enum, a `result`, an `error`, a native type,
            // a delta that is a type of its own: T itself".
            Type::Scalar(_) | Type::Error | Type::ErrorContext | Type::Result { .. } => of,
            Type::Named {
                kind: NamedKind::Enum | NamedKind::Native,
                ..
            } => of,
            Type::Delta(ref inner)
                if !matches!(**inner, Type::Parameter { .. } | Type::Unknown) =>
            {
                of
            }
            // "`atomic<V>`: V: an atomic publishes whole";
            // "`rolling_window<V, Max, Min>`: V: the value that arrived".
            Type::Atomic(value) | Type::RollingWindow { value, .. } => *value,
            // "each position's delta, null where it did not change".
            Type::Tuple(elements) => Type::Tuple(
                elements
                    .into_iter()
                    .map(|element| optional(Type::delta(element)))
                    .collect(),
            ),
            // "`V?`, V an ordinary value type: a type of its own, an
            // optional, with the property `value`, a `delta<V>?`: null
            // for a published null"; where V is none, "the `?` marks an
            // argument or a field that may be left out, and is no part
            // of what is published".
            Type::Nullable(inner) => match *inner {
                inner @ (Type::RollingWindow { .. }
                | Type::Atomic(_)
                | Type::Ref(_)
                | Type::Signal) => Type::delta(inner),
                inner => Type::Delta(Box::new(Type::Nullable(Box::new(inner)))),
            },
            // A type of its own; one not yet known; and one that has no
            // delta, a `ref` or a `signal`, which is reported where it
            // is written (`delta.unsupported_shape`).
            Type::Set(_)
            | Type::List { .. }
            | Type::Dict { .. }
            | Type::Named {
                kind: NamedKind::Struct,
                ..
            }
            | Type::Delta(_)
            | Type::Parameter { .. }
            | Type::Ref(_)
            | Type::Signal
            | Type::Endpoint(_)
            | Type::Trace { .. }
            | Type::Unknown => Type::Delta(Box::new(of)),
        }
    }
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
    /// The expression as written: each token as it stands, and one
    /// space wherever the source has whitespace or a comment between
    /// two. `N + 1`.
    pub text: String,
    /// What it comes to.
    pub value: ConstantValue,
}

// Two constants are the same when they come to the same value. One
// whose value this pass does not hold, a literal of another kind or an
// expression not evaluated yet, is compared as it is written, token for
// token: `true` is not `false`, though both are "a bool", and
// `limit( 1 )` is `limit(1)`.
impl PartialEq for Constant {
    fn eq(&self, other: &Constant) -> bool {
        match (&self.value, &other.value) {
            (ConstantValue::Unevaluated, ConstantValue::Unevaluated)
            | (ConstantValue::Other(_), ConstantValue::Other(_)) => {
                unspaced(&self.text) == unspaced(&other.text)
            }
            (left, right) => left == right,
        }
    }
}

impl Constant {
    /// The constant's text without the spaces between its tokens: what
    /// two constants whose value is not held here are compared by. A
    /// stage after the analyser that needs one text for each such
    /// constant uses this, and not `text`.
    pub fn unspaced(&self) -> String {
        unspaced(&self.text)
    }
}

/// A constant's text without the spaces between its tokens. The spaces
/// inside a string literal are the literal's own, and are kept.
fn unspaced(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    // `chars().next()` is the first character, or `None` at the end.
    while let Some(first) = rest.chars().next() {
        if first != '"' {
            if first != ' ' {
                out.push(first);
            }
            rest = &rest[first.len_utf8()..];
            continue;
        }
        // A string literal: a text block between `"""` and `"""`
        // (LEX-25), or a string up to the next `"` that no backslash
        // precedes (LEX-2). It is copied whole.
        let length = if let Some(block) = rest.strip_prefix("\"\"\"") {
            block.find("\"\"\"").map_or(rest.len(), |end| end + 6)
        } else {
            let mut escaped = false;
            let mut end = rest.len();
            for (index, character) in rest.char_indices().skip(1) {
                match character {
                    _ if escaped => escaped = false,
                    '\\' => escaped = true,
                    '"' => {
                        end = index + 1;
                        break;
                    }
                    _ => {}
                }
            }
            end
        };
        out.push_str(&rest[..length]);
        rest = &rest[length..];
    }
    out
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
    /// A member of an enum, by its name. The enum is the type of the
    /// generic value parameter the constant is given to. An enum
    /// member is one however it is written (TYPE-7): `.nan`,
    /// `Zero.nan`, `(Zero.nan)` and `Zero("nan")` all come to `nan`.
    Member(String),
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
            Type::RollingWindow {
                value,
                maximum,
                minimum,
            } => match minimum {
                Some(minimum) => write!(f, "rolling_window<{value}, {maximum}, {minimum}>"),
                None => write!(f, "rolling_window<{value}, {maximum}>"),
            },
            Type::Ref(target) => write!(f, "ref<{target}>"),
            Type::Endpoint(target) => write!(f, "endpoint<{target}>"),
            Type::Signal => f.write_str("signal"),
            Type::Atomic(payload) => write!(f, "atomic<{payload}>"),
            Type::Result { value, error } => write!(f, "result<{value}, {error}>"),
            Type::Error => f.write_str("error"),
            // No type that source writes: named in words.
            Type::ErrorContext => f.write_str("error context"),
            Type::Delta(shape) => write!(f, "delta<{shape}>"),
            // No type that source writes: named in words.
            Type::Trace { cell, .. } => write!(f, "trace of {cell}"),
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

/// The generic arguments a call writes after its callee's name,
/// `count<i64>(3)` (GEN-8).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CallArguments {
    /// Where the callee is written, from the start of its name to the
    /// `>` that closes the arguments.
    pub at: Location,
    /// Each argument with where it is written, in order.
    pub arguments: Vec<(Location, Argument)>,
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
    /// An enum.
    Enum(EnumType),
    /// A native type: its read-only properties (NATIVE-6).
    Native(NativeType),
    /// A constant: the type it is written with (UNIT-13).
    Constant(Type),
    /// An enum, before its members are read, and any other declaration
    /// of which nothing is a written type.
    Plain,
}

/// A native type as it is declared: "A native type may declare read-only
/// properties, each a name and an ordinary value type" (NATIVE-6).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeType {
    /// Its properties, in the order written. `has_default` is false for
    /// each: a native type is not constructed in HGL.
    pub properties: Vec<Member>,
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

impl Signature {
    /// The signature a struct's declaration is read with: its generic
    /// parameters, and neither a parameter nor a result. What a field's
    /// type or default may name of it is its generic parameters
    /// (UNIT-7: "with its generic environment").
    pub(crate) fn of_struct(read: &StructType) -> Signature {
        Signature {
            generics: read.generics.clone(),
            parameters: Vec::new(),
            result: None,
        }
    }
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
    /// True when it is marked `var`: a call gives it a place, which the
    /// call assigns (FN-15).
    pub var: bool,
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
    /// `T`, where its kind is not settled: nothing in the signature
    /// uses it, its uses disagree, or only a constraint names it.
    Plain,
    /// `T`, used as a type in the signature.
    Type,
    /// `...Ts`: a type pack, with the cardinality written after it
    /// (GEN-10).
    Pack {
        /// The fewest types it holds.
        minimum: u64,
        /// The most; `None` for no limit.
        maximum: Option<u64>,
    },
    /// `const N: i64`: a value, of the type given. A parameter written
    /// plain and used as a value, such as a size, is a value too; its
    /// type is not known here.
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
    /// Every field the struct has, its parents' first and each once:
    /// what a constructor is given and a member access reads. It is
    /// filled in once the structs have been read against one another.
    pub members: Vec<Member>,
}

/// A field a struct has: its own, or one from a parent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Member {
    /// Its name.
    pub name: String,
    /// Where it is declared.
    pub at: Location,
    /// Its type, in the struct's own generic parameters: a parent's
    /// field has the arguments the struct gives that parent in place.
    pub ty: Type,
    /// True when a construction that omits the field has a default to
    /// take: the field's own, or one a struct on the way down gives it.
    pub has_default: bool,
    /// The field that writes that default (TYPE-5), whose value is in
    /// `Module::defaults`. `None` for a field with no default.
    pub default: Option<FieldRef>,
}

/// A field a struct writes, by where it is: one of the struct's own
/// `fields`, a `name = value` among them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FieldRef {
    /// The position of the struct's module in `Program::modules`.
    pub module: usize,
    /// The position of the struct in that module's `declarations`.
    pub declaration: usize,
    /// The field's position among the struct's own `fields`.
    pub field: usize,
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
    /// True when that default is the literal `null`. Only an optional
    /// field may have it: one whose type is nullable, or an
    /// `atomic<...>` of a nullable type (TYPE-4).
    pub null_default: bool,
    /// True when that default is the empty collection, `[]` (TYPE-6).
    pub empty_default: bool,
    /// Where the default is written.
    pub default_at: Option<Location>,
}

/// The members of an enum (TYPE-7).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnumType {
    /// Its members, in order.
    pub members: Vec<EnumMember>,
}

/// One member of an enum.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnumMember {
    /// Its name.
    pub name: String,
    /// Where the name is written.
    pub at: Location,
    /// Its number: the one written, or one more than the member before
    /// it, from zero. `None` when it is not known: the number written is
    /// not an integer, or is an expression the analyser does not
    /// evaluate yet.
    pub value: Option<i64>,
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
            GenericKind::Plain | GenericKind::Type => f.write_str(&self.name),
            GenericKind::Pack { minimum, maximum } => {
                write!(f, "...{}", self.name)?;
                match (minimum, maximum) {
                    (0, None) => Ok(()),
                    (minimum, None) => write!(f, "{{{minimum}:*}}"),
                    (minimum, Some(maximum)) if minimum == maximum => write!(f, "{{{minimum}}}"),
                    (minimum, Some(maximum)) => write!(f, "{{{minimum}:{maximum}}}"),
                }
            }
            GenericKind::Value(Type::Unknown) => write!(f, "const {}", self.name),
            GenericKind::Value(ty) => write!(f, "const {}: {ty}", self.name),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::unspaced;

    /// The spaces between tokens are dropped; a string literal is kept
    /// whole, escapes and all (LEX-2), and so is a text block (LEX-25).
    #[test]
    fn unspaced_keeps_string_literals_whole() {
        assert_eq!(unspaced("limit( 1 ) + N"), "limit(1)+N");
        assert_eq!(unspaced("\"bid  ask\""), "\"bid  ask\"");
        assert_eq!(unspaced("f( \"a \\\" b\" , 2 )"), "f(\"a \\\" b\",2)");
        assert_eq!(
            unspaced("f( \"\"\"a \" b\"\"\" , 2 )"),
            "f(\"\"\"a \" b\"\"\",2)"
        );
        // A string the lexer left unclosed has been reported; what is
        // there is kept.
        assert_eq!(unspaced("f( \"a b"), "f(\"a b");
        assert_eq!(unspaced("π + \"é é\""), "π+\"é é\"");
    }
}
