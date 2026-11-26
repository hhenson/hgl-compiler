//! What a place expects of a value: whether a value of one type is a
//! value of another, an integer widened to an `f64`, a struct where an
//! abstract one it inherits from is expected, and the form a result or a
//! `T?` takes there (EXPR-5, TYPE-6, TYPE-14, ERR-12, ERR-15).
//! Part of the typing pass ([`super`]).

use std::collections::HashMap;

use hgl_diagnostics::{Category, Location};
use hgl_parser::SyntaxKind;

use super::Typer;
use super::arguments::Takes;
use super::calls::removes;
use super::forms::received_type;
use crate::declared::{agree, agree_with_input_pattern, names_one_of};
use crate::hir::{
    BinaryOperator, Callee, ConfigurationRead, EndpointOperation, Expr, ExprKind, UnaryOperator,
    Value, ValueCategory,
};
use crate::program::{DeclarationKind, Program};
use crate::syntax::has_token;
use crate::types::{Argument, Detail, GenericKind, Scalar, Type};
use crate::{codes, error};

/// The complete fallible form of a child checked at runtime (ERR-14).
/// Operator admission uses it before the parameter chooses a read form.
pub(super) fn checked_child_result(value: &Expr) -> Option<Expr> {
    let ExprKind::CheckedChild {
        read,
        result: false,
    } = &value.kind
    else {
        return None;
    };
    (whole(&value.ty) && !open(&value.ty)).then(|| Expr {
        kind: ExprKind::CheckedChild {
            read: read.clone(),
            result: true,
        },
        ty: Type::Result {
            value: Box::new(value.ty.clone()),
            error: Box::new(Type::Error),
        },
        at: value.at,
        category: value.category,
    })
}

/// TYPE-10: when an operand fits a public input pattern, keep its extents in
/// the type the operand is fitted to. Leave other conversions to `fit`.
pub(super) fn fitted_input_pattern(supplied: &Type, pattern: &Type) -> Option<Type> {
    let matching = |ty: &Type| {
        if !whole(ty) || open(ty) {
            return None;
        }
        if agree_with_input_pattern(ty, pattern) {
            return Some(ty.clone());
        }
        match pattern {
            Type::Nullable(payload) if agree_with_input_pattern(ty, payload) => {
                Some(Type::Nullable(Box::new(ty.clone())))
            }
            _ => None,
        }
    };
    // A full result can itself match. Try it before looking at what an
    // ordinary read takes, leaving `fit` to perform the actual read.
    matching(supplied).or_else(|| matching(received_type(supplied, |_| true)))
}

impl<'a> Typer<'a> {
    // ---- What a place expects ----

    /// The expression `value` in a place that expects `expected`: as it
    /// is when its type is the one expected, widened when it is an
    /// `i64` where an `f64` is expected (EXPR-5: "Numeric widening
    /// follows the admitted destination relationship"), and reported
    /// otherwise. `what` names the place: `the function's result`.
    pub(super) fn fit(&mut self, value: Expr, expected: &Type, what: &str) -> Expr {
        self.fit_with(value, expected, what, None)
    }

    /// Fits a value with an optional read context from a required call
    /// that the analyser is now specializing (GEN-16, EXPR-5).
    pub(super) fn fit_with(
        &mut self,
        value: Expr,
        expected: &Type,
        what: &str,
        context: Option<&ConfigurationRead>,
    ) -> Expr {
        if context.is_none() && matches!(expected, Type::Nullable(_)) {
            self.record_result_read(&value, Some((expected, what)));
        }
        let value = self.endpoint_read_form(value, expected);
        if is_integer_literal(&value) && accepts_float(expected) {
            let at = value.at;
            return Expr {
                kind: ExprKind::Widen(Box::new(value)),
                ty: Type::Scalar(Scalar::F64),
                at,
                category: ValueCategory::Unsettled,
            };
        }
        // EXPR-5: "Two are the exception, read by the place and not
        // converted: a `result<T, error>`, a call's or a binding's, takes
        // its form from the place (ERR-15) ... and a `T?` where a T is
        // expected is read (ERR-12)."
        if let Some(taken) = self.taken(expected, &value.ty) {
            return self.take_with(value, taken, context);
        }
        self.conforms(&value.ty, expected, value.at, what);
        value
    }

    /// ERR-14: a checked temporal read takes its form at this use.
    /// Reuse the result read for a nullable place, and let ERR-15
    /// discard the error. A nullable success is itself the T form.
    pub(super) fn endpoint_read_form(&mut self, value: Expr, expected: &Type) -> Expr {
        let nullable = matches!(expected, Type::Nullable(payload)
            if !matches!(value.ty, Type::Nullable(_)) && self.admits(payload, &value.ty));
        let result_type = match expected {
            Type::Result {
                value: success,
                error,
            } if **error == Type::Error && self.admits(success, &value.ty) => expected.clone(),
            _ if nullable => Type::Result {
                value: Box::new(value.ty.clone()),
                error: Box::new(Type::Error),
            },
            _ => return value,
        };
        let at = value.at;
        let kind = match value.kind {
            ExprKind::CheckedChild {
                read,
                result: false,
            } => ExprKind::CheckedChild { read, result: true },
            _ if self.window_read(&value) => ExprKind::CheckedWindow(Box::new(value)),
            _ if self.checked_whole_read(&value) => ExprKind::CheckedWhole(Box::new(value)),
            _ if reads_delta(&value) || (!nullable && reads_cycle(self.program, &value)) => {
                ExprKind::CheckedDelta(Box::new(value))
            }
            _ => return value,
        };
        let result = Expr {
            kind,
            ty: result_type,
            at,
            category: ValueCategory::Unsettled,
        };
        if nullable {
            self.take(result, super::forms::Taken::Nullable)
        } else {
            result
        }
    }

    /// [`Self::fit`], for a function's argument or a graph's result. In
    /// composition a connection is taken there by a `signal`, and where
    /// the `?` of an input that may be left out is no part of it
    /// (`signals::crosses`): TYPE-12, "`signal` is a complete temporal
    /// input constraint with no readable payload". A `ref` parameter of
    /// T takes a connection of T as any parameter of T does (FN-16).
    pub(super) fn fit_input(&mut self, value: Expr, expected: &Type, what: &str) -> Expr {
        if self.composes(&value)
            && crate::signals::crosses(expected, &value.ty, &|expected, ty| {
                self.admits(expected, ty)
            })
        {
            return value;
        }
        self.fit(value, expected, what)
    }

    /// The type `value` says the generic parameters of the place it is
    /// given to stand for; `listed` is true for a parameter of a value
    /// function. GEN-11: "No type holds a reference
    /// (TYPE-12), so argument inference removes none: a generic binds
    /// from the type of the connection it is given, at a `ref` parameter
    /// as at any other (FN-16)." A connection given to a function's
    /// parameter in composition says it with the `?` that marks omission
    /// removed, which is no part of what it is given (TYPE-14), and a
    /// window's given to a value function's parameter as the list it
    /// holds (`lifted_window`).
    ///
    /// FN-1: a value function's "parameter and result types are ordinary
    /// value types", and GEN-1: "ordinary positions require value
    /// types", which a window is not (TYPE-12). TYPE-11: a window's
    /// value "given to a lifted helper (FN-12), is a `list<V>` of the
    /// values held". So what a window says of the parameter it is given
    /// is that list, whatever the parameter's pattern: the `T` of
    /// `list<T>` is V, and a bare `T` is `list<V>`.
    pub(super) fn inferred_from(&self, listed: bool, value: &Expr, takes: Takes) -> Type {
        if takes != Takes::Parameters || !self.composes(value) {
            return value.ty.clone();
        }
        let given = value.ty.omission_removed();
        if listed && matches!(given, Type::RollingWindow { .. }) {
            return window_value(given);
        }
        given.clone()
    }

    /// True where `value`, given to a parameter of type `ty`, is a
    /// window's connection given in composition to a value function's
    /// `list<V>` (`listed`), which the function is lifted over. FN-12: "A
    /// window is passed only where it is `all_valid` ... That value, read
    /// in a hook or given to a lifted helper (FN-12), is a `list<V>` of
    /// the values held" (TYPE-11). FN-11: a window "is supplied only
    /// where it is `all_valid` as well, whatever its default or null
    /// contract": a `list<V>?`, whose contract admits null, takes it as
    /// `list<V>` does.
    pub(super) fn lifted_window(
        &self,
        ty: &Type,
        listed: bool,
        value: &Expr,
        takes: Takes,
    ) -> bool {
        let given = value.ty.omission_removed();
        takes == Takes::Parameters
            && listed
            && self.composes(value)
            && matches!(given, Type::RollingWindow { .. })
            && matches!(not_null(Some(ty)), Some(Type::List { size: None, .. }))
            && self.admits(ty, &window_value(given))
    }

    /// Reports a value of type `ty` in a place that expects `expected`
    /// when it is no value of that type.
    pub(super) fn conforms(&mut self, ty: &Type, expected: &Type, at: Location, what: &str) {
        if self.admits(expected, ty) {
            return;
        }
        self.diagnostics.emit(error(
            Category::Type,
            codes::TYPE_MISMATCH,
            at,
            format!("this is `{ty}`, and {what} is `{expected}`"),
            "EXPR-5",
        ));
    }

    /// True when an argument of type `later` widens a generic parameter
    /// that an earlier argument decided is `decided`.
    ///
    /// GEN-2: "One later argument changes what an earlier argument
    /// decided: an abstract struct that the type decided inherits from
    /// widens the parameter to that struct, so the order of two such
    /// arguments does not matter."
    pub(super) fn widens(&self, decided: &Type, later: &Type) -> bool {
        let Type::Named {
            module,
            declaration,
            ..
        } = later
        else {
            return false;
        };
        let declared = &self.program.modules[*module].declarations[*declaration];
        declared.kind == DeclarationKind::Struct
            && has_token(&declared.syntax, SyntaxKind::KwAbstract)
            && whole(decided)
            && !agree(later, decided)
            && self.inherits(decided, later)
    }

    /// True when a value of type `ty` is a value of `expected`: the same
    /// type as far as each is known; a `T` where a `T?` is expected
    /// (TYPE-14: "either a T or null"); or a struct where an abstract
    /// struct it inherits from is expected.
    pub(super) fn admits(&self, expected: &Type, ty: &Type) -> bool {
        if open(expected) || open(ty) || agree(expected, ty) {
            return true;
        }
        // A delayed declaration query has no identity until this body is
        // checked at its making call. Keep checking the known parts here.
        if !self.signature.generics.is_empty()
            && self.instance.is_empty()
            && self.pack_inputs.is_none()
            && (crate::constant::type_has_instance_query(expected)
                || crate::constant::type_has_instance_query(ty))
            && crate::declared::same_type(expected, ty) != Some(false)
        {
            if let Some(deferred) = self.deferred_type_fit {
                deferred.set(true);
            }
            return true;
        }
        match expected {
            Type::Nullable(inner) => self.admits(inner, ty),
            Type::Named { .. } => self.inherits(ty, expected),
            // TYPE-16: "A `delta<V>` is a value where a `delta<V?>` is
            // expected, as a T is where a `T?` is".
            Type::Delta(of) => match &**of {
                Type::Nullable(payload) => self.admits(&Type::delta((**payload).clone()), ty),
                _ => false,
            },
            _ => false,
        }
    }

    /// True when the struct type `ty` inherits from `expected`, through
    /// any number of parents, each with the generic arguments the
    /// struct below gives it.
    ///
    /// TYPE-6: "A field whose type is an abstract struct holds every
    /// struct that inherits from it": a value of a struct is a value of
    /// the abstract structs it inherits from. TYPE-3: "Generic
    /// applications are invariant", so a `Child : Base<i64>` is no
    /// `Base<f64>`.
    pub(super) fn inherits(&self, ty: &Type, expected: &Type) -> bool {
        // The parents still to look at.
        let mut to_do = vec![ty.clone()];
        // The applications of each struct whose parents have been
        // looked at. A struct reached by two lines of parents is looked
        // at once for each application, so that a ladder of diamonds
        // is walked in step with its length.
        let mut followed: HashMap<(usize, usize), Vec<Vec<Argument>>> = HashMap::new();
        // Structs that inherit from one another have been reported;
        // the count ends the walk for them.
        let mut budget = 100_000;
        while let Some(current) = to_do.pop() {
            let Type::Named {
                module,
                declaration,
                arguments,
                ..
            } = &current
            else {
                continue;
            };
            let Some(Detail::Struct(read)) =
                self.program.modules[*module].details.get(*declaration)
            else {
                continue;
            };
            let applications = followed.entry((*module, *declaration)).or_default();
            if applications.contains(arguments) {
                continue;
            }
            applications.push(arguments.clone());
            // Each generic parameter before a type pack, with the
            // argument it is given.
            let map: Vec<(Location, &Argument)> = read
                .generics
                .iter()
                .take_while(|generic| !matches!(generic.kind, GenericKind::Pack { .. }))
                .map(|generic| generic.at)
                .zip(arguments)
                .collect();
            // The struct's own parameters: one a parent still names
            // once the arguments are in place has been given none here.
            let own: Vec<Location> = read.generics.iter().map(|generic| generic.at).collect();
            for parent in &read.parents {
                let parent = self.program.specialize_type(parent, &map);
                // Where the struct passes a type pack on to its parent,
                // the pack is not put in place here: the parent may be
                // the application expected, and nothing is reported of
                // it (ERR-9). A parameter of the function being read
                // is a type like any other, and is compared.
                let may_be = names_one_of(&parent, &own) && same_declaration(expected, &parent);
                if may_be || agree(expected, &parent) {
                    return true;
                }
                if budget == 0 {
                    return false;
                }
                budget -= 1;
                to_do.push(parent);
            }
        }
        false
    }
}

/// The type of an endpoint read in a hook. An atomic holds its entire
/// value (TYPE-9), and a window reads as the list it holds (TYPE-11).
/// An omission mark on these inputs adds no null payload (TYPE-14).
pub(super) fn hook_payload_type(ty: &Type) -> Type {
    match ty.omission_removed() {
        Type::Atomic(payload) => payload.as_ref().clone(),
        window @ Type::RollingWindow { .. } => window_value(window),
        under => under.clone(),
    }
}

/// The type of a window's value, `window` being a `rolling_window<V, ...>`;
/// any other type as it is. TYPE-11: "That value, read in a hook or given
/// to a lifted helper (FN-12), is a `list<V>` of the values held, oldest
/// first".
pub(super) fn window_value(window: &Type) -> Type {
    match window {
        Type::RollingWindow { value, .. } => Type::List {
            element: value.clone(),
            size: None,
        },
        other => other.clone(),
    }
}

/// True where `value` reads what an endpoint published in the cycle,
/// `delta_value` of an input, of `out` or of a child, or what a window
/// lost in it, core's `removed_value`: each takes its form from its place
/// (ERR-14).
pub(super) fn reads_cycle(program: &Program, value: &Expr) -> bool {
    if reads_delta(value) {
        return true;
    }
    match &value.kind {
        ExprKind::Call {
            callee:
                Callee::Function {
                    module,
                    declaration,
                    ..
                },
            ..
        } => removes(program, (*module, *declaration)),
        _ => false,
    }
}

/// True for `delta_value`, whose nullable form is admitted even for a
/// duration window. `removed_value` remains result-only (TYPE-11).
fn reads_delta(value: &Expr) -> bool {
    matches!(
        value.kind,
        ExprKind::Endpoint {
            operation: EndpointOperation::DeltaValue,
            ..
        } | ExprKind::ChildEndpoint {
            operation: EndpointOperation::DeltaValue,
            ..
        }
    )
}

/// `T?` for a `T`; a type that is nullable already, or not known, as it
/// is.
pub(super) fn nullable(ty: Type) -> Type {
    if open(&ty) || matches!(ty, Type::Nullable(_)) {
        ty
    } else {
        Type::Nullable(Box::new(ty))
    }
}

/// True when a type is known all the way down: no part of it is
/// `Type::Unknown`.
pub(super) fn whole(ty: &Type) -> bool {
    !ty.any_part(&|part| matches!(part, Type::Unknown), &|_| false)
}

/// True when nothing can be said against a value of this type: it is
/// not known, or it is `any`, whose typing is for a later pass
/// (TYPE-8).
pub(super) fn open(ty: &Type) -> bool {
    matches!(ty, Type::Unknown | Type::Scalar(Scalar::Any))
}

/// True when an `i64` is widened in a place that expects `expected`: an
/// `f64`, or an `f64?`.
pub(super) fn accepts_float(expected: &Type) -> bool {
    match expected {
        Type::Scalar(Scalar::F64) => true,
        Type::Nullable(inner) => accepts_float(inner),
        // TYPE-16: an `f64` is a value where a `delta<f64?>` is
        // expected.
        Type::Delta(of) => matches!(&**of, Type::Nullable(payload) if accepts_float(payload)),
        _ => false,
    }
}

/// EXPR-5: "An integer literal is an `f64` where one is expected." True
/// when an expression is one, under a minus or not.
pub(super) fn is_integer_literal(value: &Expr) -> bool {
    match &value.kind {
        ExprKind::Literal(Value::Int(_)) => true,
        // `-1`, as it is written or as the call of core's `neg_` it
        // stands for.
        _ => negated(value).is_some_and(is_integer_literal),
    }
}

/// True for the call a symbol stands for where it is core's own and
/// nothing more: of a candidate the operator's own module declares,
/// with no argument beside its `operands` operands. What a module's own
/// candidate computes, or a call given an option as an argument, is not
/// known to this pass. An option that is a generic value parameter is
/// no argument: it selects the candidate, a division's `zero` say, and
/// what that computes is core's.
///
/// A symbol recorded as it is written, an [`ExprKind::Unary`] or an
/// [`ExprKind::Binary`], is core's own.
fn is_plain(kind: &ExprKind, operands: usize) -> bool {
    match kind {
        ExprKind::Call {
            callee: Callee::Candidate {
                operator, module, ..
            },
            arguments,
            ..
        } => operator.0 == *module && arguments.len() == operands,
        ExprKind::Call { .. } => false,
        _ => true,
    }
}

/// The operand of a minus that is core's negation: `-value` as it is
/// written, or the plain call of `neg_` it stands for (see
/// [`is_plain`]).
pub(super) fn negated(value: &Expr) -> Option<&Expr> {
    match value.kind.unary() {
        Some((UnaryOperator::Negate, operand)) if is_plain(&value.kind, 1) => Some(operand),
        _ => None,
    }
}

/// The operator and operands of a symbol between two that is core's
/// arithmetic: as it is written, or the plain call it stands for (see
/// [`is_plain`]).
pub(super) fn arithmetic(value: &Expr) -> Option<(BinaryOperator, &Expr, &Expr)> {
    value.kind.binary().filter(|_| is_plain(&value.kind, 2))
}

/// True when two types are applications of one struct, whatever their
/// generic arguments.
fn same_declaration(left: &Type, right: &Type) -> bool {
    match (left, right) {
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
        _ => false,
    }
}

/// The type a value that is not `null` is expected to have: of `T?`,
/// `T` (TYPE-14: "either a T or null").
pub(super) fn not_null(expected: Option<&Type>) -> Option<&Type> {
    match expected {
        Some(Type::Nullable(inner)) => not_null(Some(inner)),
        other => other,
    }
}
