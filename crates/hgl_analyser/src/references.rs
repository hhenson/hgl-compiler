//! References and signals in composition: what `values` reads a graph's
//! arguments and result with where a connection meets a temporal place
//! across a reference boundary, or at a `signal` (TYPE-12, TIME-10,
//! GEN-11).
//!
//! TYPE-12: "`ref<T>` designates an endpoint, not an ordinary payload
//! type", and "`signal` is a complete temporal input constraint with no
//! readable payload". TIME-10: "Ordinary followers observe the selected
//! target". So in a graph a connection of `T` given to a `ref<T>` input
//! is designated by it, one of `ref<T>` given to a `T` input is followed
//! by it, and any connection is given to a `signal`. Neither holds of an
//! ordinary value, which is read where it is given.

use crate::types::{Argument, Type};

/// True when a connection of `given` is taken by a temporal place of
/// `expected` across a reference boundary or by a `signal`. `admits`
/// says whether a value of the second type is one of the first, as
/// `values` judges any place.
pub(crate) fn crosses(
    expected: &Type,
    given: &Type,
    admits: &dyn Fn(&Type, &Type) -> bool,
) -> bool {
    let given = omittable(given);
    match (expected, given) {
        // TYPE-14: on a type that is no ordinary value the `?` marks an
        // input that may be left out; what is given is the type under it.
        (Type::Nullable(inner), _) if matches!(**inner, Type::Signal | Type::Ref(_)) => {
            crosses(inner, given, admits)
        }
        // TYPE-12: a `signal` constrains an input to tick, and reads no
        // payload: any connection is one.
        (Type::Signal, _) => !matches!(given, Type::Unknown),
        // A connection that may have been left out, given where one that
        // may not is expected.
        (expected, given) if admits(expected, given) => true,
        // TYPE-12: the input designates the endpoint it is given.
        (Type::Ref(target), given) if !matches!(given, Type::Ref(_) | Type::Signal) => {
            admits(target, given)
        }
        // TIME-10: an ordinary input follows what the reference
        // designates. GEN-11: such an input "observes values", with
        // each reference boundary removed, at any depth.
        (expected, given) if !holds_ref(expected) && holds_ref(given) => {
            admits(expected, &dereferenced(given))
        }
        _ => false,
    }
}

/// GEN-11: the type a connection of `given`, given to a place declared
/// `pattern`, says the generic parameters in `pattern` stand for.
///
/// "Argument inference removes reference boundaries recursively from an
/// otherwise unbound generic; such an input observes values. An explicit
/// ref pattern binds its underlying type." So `value: T` given a
/// `ref<i64>` finds `T` an `i64`, and `value: ref<T>` given an `i64`, or
/// a `ref<i64>`, finds it an `i64` too. A pattern that holds a `ref`
/// below its top is matched against `given` as it is.
pub(crate) fn inferred_from(pattern: &Type, given: &Type) -> Type {
    let pattern = match pattern {
        Type::Nullable(inner) => inner,
        other => other,
    };
    let given = omittable(given);
    match (pattern, given) {
        (Type::Ref(_), Type::Ref(_)) => given.clone(),
        (Type::Ref(_), given) => Type::Ref(Box::new(given.clone())),
        (pattern, given) if !holds_ref(pattern) => dereferenced(given),
        _ => given.clone(),
    }
}

/// `ty` without the `?` that marks a connection that may be left out.
/// TYPE-14: on `ref<f64>`, `atomic<Quote>` and `signal`, which are no
/// ordinary value types, the `?` "just means the value can be omitted":
/// what is given, where it is given, is the type under it.
fn omittable(ty: &Type) -> &Type {
    match ty {
        Type::Nullable(inner)
            if matches!(**inner, Type::Ref(_) | Type::Signal | Type::Atomic(_)) =>
        {
            inner
        }
        other => other,
    }
}

/// True when `ty` is or holds a `ref`, at any depth.
pub(crate) fn holds_ref(ty: &Type) -> bool {
    match ty {
        Type::Ref(_) => true,
        Type::Named { arguments, .. } => arguments.iter().any(|argument| match argument {
            Argument::Type(argument) => holds_ref(argument),
            Argument::Constant(_) => false,
        }),
        Type::Tuple(elements) => elements.iter().any(holds_ref),
        Type::List { element: inner, .. }
        | Type::Set(inner)
        | Type::Endpoint(inner)
        | Type::Atomic(inner)
        | Type::Delta(inner)
        | Type::Trace { cell: inner, .. }
        | Type::Nullable(inner)
        | Type::RollingWindow { value: inner, .. } => holds_ref(inner),
        Type::Dict { key, value } => holds_ref(key) || holds_ref(value),
        Type::Result { value, error } => holds_ref(value) || holds_ref(error),
        Type::Scalar(_)
        | Type::Parameter { .. }
        | Type::Signal
        | Type::Error
        | Type::ErrorContext
        | Type::Unknown => false,
    }
}

/// `ty` with every reference boundary removed, at any depth (GEN-11):
/// `ref<list<ref<i64>, 2>>` is `list<i64, 2>`. A generic struct's
/// arguments are kept as they are: a reference there is part of the
/// struct's identity.
pub(crate) fn dereferenced(ty: &Type) -> Type {
    let part = |inner: &Type| Box::new(dereferenced(inner));
    match ty {
        Type::Ref(inner) => dereferenced(inner),
        Type::Tuple(elements) => Type::Tuple(elements.iter().map(dereferenced).collect()),
        Type::List { element, size } => Type::List {
            element: part(element),
            size: size.clone(),
        },
        Type::Set(inner) => Type::Set(part(inner)),
        Type::Dict { key, value } => Type::Dict {
            key: part(key),
            value: part(value),
        },
        Type::Nullable(inner) => match dereferenced(inner) {
            inner @ Type::Nullable(_) => inner,
            inner => Type::Nullable(Box::new(inner)),
        },
        _ => ty.clone(),
    }
}
