//! Structural equality for analyser data and value-aware comparison for type checking.

use std::collections::BTreeSet;
use std::hash::{Hash, Hasher};

use hgl_diagnostics::Location;

use super::{
    Argument, Constant, ConstantValue, EnumOf, FunctionParameter, FunctionType, MemberOf, Type,
};
use crate::hir::{BinaryOperator, Temporal, Tenor, UnaryOperator};

impl PartialEq for Constant {
    fn eq(&self, other: &Self) -> bool {
        self.value == other.value
    }
}

impl Eq for Constant {}

impl Hash for Constant {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.value.hash(state);
    }
}

impl Constant {
    /// Compares the constants by known value (UNIT-13). Returns `None`
    /// when the values cannot yet be compared.
    pub fn same_value(&self, other: &Self) -> Option<bool> {
        self.value.same(&other.value)
    }

    /// True for two absent constants, or two known to have the same value.
    pub(crate) fn same_optional_value(left: Option<&Self>, right: Option<&Self>) -> bool {
        same_optional(left, right, |left, right| {
            left.same_value(right) == Some(true)
        })
    }
}

impl PartialEq for ConstantValue {
    fn eq(&self, other: &Self) -> bool {
        self.structure() == other.structure()
    }
}

impl Eq for ConstantValue {}

impl Hash for ConstantValue {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.structure().hash(state);
    }
}

/// Borrows the fields so both traits use the same structure. Float bits
/// give Rust equality and hashing a total comparison, including for NaNs.
#[derive(PartialEq, Eq, Hash)]
enum Structure<'a> {
    Type(&'a Type),
    InstanceTypeQuery(Location, &'a [(Location, Argument)]),
    Int(i64),
    Float(u64),
    Bool(bool),
    Str(&'a str),
    Null,
    Duration(i64),
    Tenor(Tenor),
    Temporal(&'a Temporal),
    Member(&'a MemberOf),
    List(&'a [ConstantValue]),
    StringSet(&'a BTreeSet<String>),
    Tuple(&'a [ConstantValue]),
    Unbounded,
    Parameter(&'a str, Location),
    Named(usize, usize),
    Constructed(&'a EnumOf, &'a [(String, Option<i64>)], &'a ConstantValue),
    Unary(UnaryOperator, &'a ConstantValue),
    Binary(BinaryOperator, &'a ConstantValue, &'a ConstantValue),
    RangeLength(&'a ConstantValue, &'a ConstantValue),
    DeferredTypeQuery(Location),
    DeferredDefaultQuery(Location),
    Unevaluated,
    Unknown,
}

impl ConstantValue {
    fn structure(&self) -> Structure<'_> {
        match self {
            Self::Type(ty) => Structure::Type(ty),
            Self::InstanceTypeQuery { at, bindings } => Structure::InstanceTypeQuery(*at, bindings),
            Self::Int(value) => Structure::Int(*value),
            Self::Float(value) => Structure::Float(value.to_bits()),
            Self::Bool(value) => Structure::Bool(*value),
            Self::Str(value) => Structure::Str(value),
            Self::Null => Structure::Null,
            Self::Duration(value) => Structure::Duration(*value),
            Self::Tenor(value) => Structure::Tenor(*value),
            Self::Temporal(value) => Structure::Temporal(value),
            Self::Member(member) => Structure::Member(member),
            Self::List(values) => Structure::List(values),
            Self::StringSet(values) => Structure::StringSet(values),
            Self::Tuple(values) => Structure::Tuple(values),
            Self::Unbounded => Structure::Unbounded,
            Self::Parameter { name, at } => Structure::Parameter(name, *at),
            Self::Named {
                module,
                declaration,
            } => Structure::Named(*module, *declaration),
            Self::Constructed { of, members, from } => Structure::Constructed(of, members, from),
            Self::Unary(operator, operand) => Structure::Unary(*operator, operand),
            Self::Binary(operator, left, right) => Structure::Binary(*operator, left, right),
            Self::RangeLength(lower, upper) => Structure::RangeLength(lower, upper),
            Self::DeferredTypeQuery(at) => Structure::DeferredTypeQuery(*at),
            Self::DeferredDefaultQuery(at) => Structure::DeferredDefaultQuery(*at),
            Self::Unevaluated => Structure::Unevaluated,
            Self::Unknown => Structure::Unknown,
        }
    }
}

impl Argument {
    /// True when two generic arguments give the same type or are known
    /// to give the same constant (GEN-8, UNIT-13).
    pub fn same_argument(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Type(left), Self::Type(right)) => left.same_type(right),
            (Self::Constant(left), Self::Constant(right)) => left.same_value(right) == Some(true),
            (Self::NoResult, Self::NoResult) => true,
            _ => false,
        }
    }
}

impl Type {
    /// True when the types have matching structure and each corresponding
    /// constant is known to have the same value (TYPE-3, GEN-8, UNIT-13).
    /// For type checking with open positions, use the analyser's agreement
    /// and inference rules.
    pub fn same_type(&self, other: &Self) -> bool {
        match (self, other) {
            (
                Self::Named {
                    name,
                    module,
                    declaration,
                    arguments,
                    kind,
                },
                Self::Named {
                    name: other_name,
                    module: other_module,
                    declaration: other_declaration,
                    arguments: other_arguments,
                    kind: other_kind,
                },
            ) => {
                (name, module, declaration, kind)
                    == (other_name, other_module, other_declaration, other_kind)
                    && same_items(arguments, other_arguments, Argument::same_argument)
            }
            (
                Self::PackMember { pack, name, key },
                Self::PackMember {
                    pack: other_pack,
                    name: other_name,
                    key: other_key,
                },
            ) => {
                (pack, name) == (other_pack, other_name) && key.same_value(other_key) == Some(true)
            }
            (Self::Tuple(left), Self::Tuple(right)) => same_items(left, right, Self::same_type),
            (
                Self::List { element, size },
                Self::List {
                    element: other_element,
                    size: other_size,
                },
            ) => {
                element.same_type(other_element)
                    && Constant::same_optional_value(size.as_ref(), other_size.as_ref())
            }
            (Self::Set(left), Self::Set(right))
            | (Self::Endpoint(left), Self::Endpoint(right))
            | (Self::Atomic(left), Self::Atomic(right))
            | (Self::Delta(left), Self::Delta(right))
            | (Self::Nullable(left), Self::Nullable(right)) => left.same_type(right),
            (
                Self::Dict { key, value },
                Self::Dict {
                    key: other_key,
                    value: other_value,
                },
            ) => key.same_type(other_key) && value.same_type(other_value),
            (
                Self::RollingWindow {
                    value,
                    maximum,
                    minimum,
                },
                Self::RollingWindow {
                    value: other_value,
                    maximum: other_maximum,
                    minimum: other_minimum,
                },
            ) => {
                value.same_type(other_value)
                    && maximum.same_value(other_maximum) == Some(true)
                    && Constant::same_optional_value(minimum.as_ref(), other_minimum.as_ref())
            }
            (
                Self::Result { value, error },
                Self::Result {
                    value: other_value,
                    error: other_error,
                },
            ) => value.same_type(other_value) && error.same_type(other_error),
            (
                Self::Trace { cell, style },
                Self::Trace {
                    cell: other_cell,
                    style: other_style,
                },
            ) => style == other_style && cell.same_type(other_cell),
            (Self::Function(left), Self::Function(right)) => left.same_signature(right),
            (left, right) => left == right,
        }
    }

    /// True for two absent types, or types of the same shape and known constants.
    pub(crate) fn same_optional(left: Option<&Self>, right: Option<&Self>) -> bool {
        same_optional(left, right, Self::same_type)
    }
}

impl FunctionType {
    /// Compares callback templates by shape and known constants.
    pub(crate) fn same_signature(&self, other: &Self) -> bool {
        self.value == other.value
            && self.named_pack == other.named_pack
            && self.result_admits_none == other.result_admits_none
            && self.reference == other.reference
            && Type::same_optional(self.result.as_ref(), other.result.as_ref())
            && same_items(
                &self.parameters,
                &other.parameters,
                FunctionParameter::same_parameter,
            )
    }
}

impl FunctionParameter {
    fn same_parameter(&self, other: &Self) -> bool {
        match (self, other) {
            (
                Self::Fixed { name, ty },
                Self::Fixed {
                    name: other_name,
                    ty: other_ty,
                },
            ) => name == other_name && ty.same_type(other_ty),
            (Self::Declared(left), Self::Declared(right)) => {
                left.name == right.name
                    && left.at == right.at
                    && left.configuration == right.configuration
                    && left.constant == right.constant
                    && left.var == right.var
                    && left.reference == right.reference
                    && left.pack == right.pack
                    && left.pack_bounds == right.pack_bounds
                    && left.has_default == right.has_default
                    && left.ty.same_type(&right.ty)
            }
            _ => false,
        }
    }
}

fn same_items<T>(left: &[T], right: &[T], same: impl Fn(&T, &T) -> bool) -> bool {
    left.len() == right.len()
        && left
            .iter()
            .zip(right)
            .all(|(left, right)| same(left, right))
}

fn same_optional<T>(left: Option<&T>, right: Option<&T>, same: impl Fn(&T, &T) -> bool) -> bool {
    match (left, right) {
        (Some(left), Some(right)) => same(left, right),
        (None, None) => true,
        _ => false,
    }
}
