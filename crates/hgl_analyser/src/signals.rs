//! Signals and inputs that may be left out in composition: what `values`
//! reads a graph's arguments and result with where a connection meets a
//! `signal`, or an input whose `?` marks omission (TYPE-12, TYPE-14).
//!
//! TYPE-12: "`signal` is a complete temporal input constraint with no
//! readable payload", so in a graph any connection is given to a
//! `signal`. An ordinary value is not: it is read where it is given.
//!
//! A `ref` parameter and a `ref` result need nothing here. FN-16: "The
//! mark is a mode ... and no part of the type (TYPE-12): a caller and the
//! type checker see the T after it. A connection of T is given to a `ref`
//! parameter, and a `ref` result is a connection of T to whoever reads
//! it", so either is held to its type as any connection is.

use crate::types::Type;

/// True when a connection of `given` is taken by a temporal place of
/// `expected` because it is a `signal`, or because a `?` that marks an
/// input that may be left out is no part of either
/// (`Type::omission_removed`). `admits` says whether a value of the
/// second type is one of the first, as `values` judges any place.
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
        (expected, given) => admits(expected, given),
    }
}

impl Type {
    /// The type under a `?` that marks a temporal input or a field
    /// that may be left out; any other type as it is. TYPE-14: on a
    /// type that is no ordinary value, `rolling_window<f64, 5>?`,
    /// `atomic<Quote>?`, `signal?`, the `?` "just means the value can be
    /// omitted", and is no part of what is given or published. The
    /// analyser and a target both ask this here, so that they agree.
    pub fn omission_removed(&self) -> &Type {
        match self {
            Type::Nullable(inner)
                if matches!(
                    **inner,
                    Type::Signal | Type::Atomic(_) | Type::RollingWindow { .. }
                ) =>
            {
                inner
            }
            other => other,
        }
    }
}
