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

use crate::types::Type;

/// True when a connection of `given` is taken by a temporal place of
/// `expected` across a reference boundary or by a `signal`. `admits`
/// says whether a value of the second type is one of the first, as
/// `values` judges any place. A `?` that marks an input that may be
/// left out is no part of either (`Type::omission_removed`).
pub(crate) fn crosses(
    expected: &Type,
    given: &Type,
    admits: &dyn Fn(&Type, &Type) -> bool,
) -> bool {
    let (expected, given) = (expected.omission_removed(), given.omission_removed());
    match (expected, given) {
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
        // designates (`Type::followed`).
        (expected, given) if !holds_ref(expected) && holds_ref(given) => {
            admits(expected, &given.followed())
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
/// `ref<i64>` finds `T` an `i64` (`Type::followed`), and `value: ref<T>`
/// given an `i64`, or a `ref<i64>`, finds it an `i64` too. A pattern
/// that holds a `ref` below its top is matched against `given` as it is.
pub(crate) fn inferred_from(pattern: &Type, given: &Type) -> Type {
    let (pattern, given) = (pattern.omission_removed(), given.omission_removed());
    match (pattern, given) {
        (Type::Ref(_), Type::Ref(_)) => given.clone(),
        (Type::Ref(_), given) => Type::Ref(Box::new(given.clone())),
        (pattern, given) if !holds_ref(pattern) => given.followed(),
        _ => given.clone(),
    }
}

impl Type {
    /// The type under a `?` that marks a temporal input or a field
    /// that may be left out; any other type as it is. TYPE-14: on a
    /// type that is no ordinary value, `rolling_window<f64, 5>?`,
    /// `atomic<Quote>?`, `ref<f64>?`, `signal?`, the `?` "just means
    /// the value can be omitted", and is no part of what is given or
    /// published. The analyser and a target both ask this here, so
    /// that they agree.
    pub fn omission_removed(&self) -> &Type {
        match self {
            Type::Nullable(inner)
                if matches!(
                    **inner,
                    Type::Ref(_) | Type::Signal | Type::Atomic(_) | Type::RollingWindow { .. }
                ) =>
            {
                inner
            }
            other => other,
        }
    }

    /// What an ordinary input given a connection of this type observes:
    /// the type with the `?` that marks omission removed
    /// (`Type::omission_removed`) and every reference boundary removed,
    /// at any depth. TIME-10: "Ordinary followers observe the selected
    /// target"; GEN-11: such an input "observes values". A type that
    /// holds no reference is itself. The analyser admits a follower by
    /// this, and a target writes the follower's input as it says.
    pub fn followed(&self) -> Type {
        dereferenced(self.omission_removed())
    }
}

/// True when `ty` is or holds a `ref`, at any depth.
pub(crate) fn holds_ref(ty: &Type) -> bool {
    ty.any_part(&|part| matches!(part, Type::Ref(_)), &|_| false)
}

/// `ty` with every reference boundary removed, at any depth (GEN-11):
/// `ref<list<ref<i64>, 2>>` is `list<i64, 2>`. A generic struct's
/// arguments are kept as they are: a reference there is part of the
/// struct's identity.
fn dereferenced(ty: &Type) -> Type {
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
