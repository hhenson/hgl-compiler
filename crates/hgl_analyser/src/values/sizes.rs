//! The sizes a generic application gives a list or a window (TYPE-10,
//! TYPE-11). A size written as a constant is held to its rules where it
//! is read (`read`). One that names a generic value parameter, the `N`
//! of `items: list<i64, N>`, is known only where the parameter is given
//! a value: by a struct application written in a type, `Fixed<-1>`, or
//! by a call of a generic function, `sized<-1>(xs)`. Each such size is
//! held to the same rules there, and reported at the application. Part
//! of the typing pass ([`super`]).
//!
//! The sizes inside a generic body, a local's `list<i64, N - 3>`, are
//! not held here: GEN-7 has a body "judged once, where it is declared
//! ... and never again for the generic arguments a call of it gives".

use hgl_diagnostics::{Category, Diagnostic, Diagnostics, Location};

use super::inference::mentions;
use crate::declared::substitute_within;
use crate::program::Program;
use crate::types::{Argument, Constant, ConstantValue, Detail, GenericKind, NamedKind, Type};
use crate::{codes, error};

/// How deep the fields of the structs a type applies are followed. A
/// struct may hold another of itself with other arguments, `list<G<N +
/// 1>>` in `G<const N: i64>`, so the structs reached need not end; past
/// this depth nothing more is held.
const DEPTH: usize = 32;

/// How many struct applications one type written, or one call, has
/// followed at most. Two fields that each apply the struct again with
/// other arguments would double the applications at each depth.
const APPLICATIONS: usize = 256;

/// How many parts, types and constants and the values inside them, a
/// struct application followed may have, a string counting a part for
/// each byte. A struct may apply itself with an argument that doubles at
/// each depth, `Grow<[N, N]>` in `Grow<N>` or `Grow<S + S>` in `Grow<const
/// S: str>`, and an application past this is not followed.
const PARTS: usize = 4096;

/// A size that TYPE-10 or TYPE-11 rejects, as an application gives it.
struct Fault {
    /// The field it is in, through the fields of the structs on the
    /// way: `["inner", "items"]`. Empty where the type given is itself
    /// the list or the window.
    path: Vec<String>,
    /// The list or the window, with the size the application gives it:
    /// `list<i64, -1>`.
    holder: Type,
    /// The catalogued code: `list.size`, `rolling_window.size_bounds` or
    /// `rolling_window.size_kind`.
    code: &'static str,
    /// The rule it breaks: TYPE-10 or TYPE-11.
    rule: &'static str,
    /// What is wrong with the size: `a list's size is not negative`.
    problem: String,
}

impl Fault {
    /// The report of this fault at `at`, an application described as
    /// `what` (`` `Fixed<-1>` ``, `this call of \`sized\``) that gives
    /// it to `place` (`items`, `its result`).
    fn report(&self, at: Location, what: &str, place: Option<&str>) -> Diagnostic {
        let field = (!self.path.is_empty()).then(|| format!("`{}`", self.path.join(".")));
        let place = match (place, field) {
            (Some(place), Some(field)) => format!("{place}'s field {field}"),
            (Some(place), None) => place.to_string(),
            (None, Some(field)) => field,
            (None, None) => "a type".to_string(),
        };
        error(
            Category::Type,
            self.code,
            at,
            format!("{what} makes {place} a `{}`: {}", self.holder, self.problem),
            self.rule,
        )
    }
}

/// Holds each struct application written in the program's types to the
/// sizes it gives: TYPE-10, "fixed sizes are nonnegative integer
/// constants", and TYPE-11's bounds and kinds of a window's sizes. TYPE-4:
/// a struct's own field has its type "with the arguments the struct is
/// given", and a field from a parent "the type that parent's arguments
/// give it". A fault is reported where the type is written, once for
/// each type written.
pub(super) fn applied_sizes(program: &Program, diagnostics: &mut Diagnostics) {
    for module in &program.modules {
        for written in &module.written {
            let mut applications = Vec::new();
            applications_in(&written.ty, &mut applications);
            for application in applications {
                let mut seen = Vec::new();
                if let Some(fault) = struct_fault(program, application, &[], 0, &mut seen) {
                    let what = format!("`{application}`");
                    diagnostics.emit(fault.report(written.at, &what, None));
                }
            }
        }
    }
}

/// The first fault in the sizes `declared` has once `given` is put in
/// place of the generic parameters declared at `own`, at any depth.
/// `declared` is a type of a callee's signature, written in its own
/// generic parameters; `given` gives each of them its argument.
fn signature_fault(
    program: &Program,
    declared: &Type,
    own: &[Location],
    given: &[Argument],
) -> Option<Fault> {
    let map: Vec<(Location, &Argument)> = own.iter().copied().zip(given).collect();
    let mut seen = Vec::new();
    fault_in(program, declared, (&map, own), (&[], 0), &mut seen)
}

/// Reports the first fault [`signature_fault`] finds in a call's
/// parameters or result: the call written at `at` of `callee`, which
/// gives the generic parameters declared at `own` the arguments `given`.
/// GEN-2: a call resolves its callee's generics from what it writes, its
/// place, its arguments, or a default; TYPE-10 and TYPE-11 hold of the
/// types the callee's signature has with them in place.
pub(super) fn call_sizes(
    program: &Program,
    (at, callee): (Location, &str),
    signature: &crate::types::Signature,
    (own, given): (&[Location], &[Argument]),
    diagnostics: &mut Diagnostics,
) {
    let what = format!("this call of `{callee}`");
    for parameter in &signature.parameters {
        if let Some(fault) = signature_fault(program, &parameter.ty, own, given) {
            let place = format!("its parameter `{}`", parameter.name);
            diagnostics.emit(fault.report(at, &what, Some(&place)));
            return;
        }
    }
    if let Some(result) = &signature.result
        && let Some(fault) = signature_fault(program, result, own, given)
    {
        diagnostics.emit(fault.report(at, &what, Some("its result")));
    }
}

/// Every struct application in `ty`, at any depth: `Fixed<-1>` in
/// `list<Fixed<-1>>`.
fn applications_in<'t>(ty: &'t Type, found: &mut Vec<&'t Type>) {
    if let Type::Named {
        kind: NamedKind::Struct,
        ..
    } = ty
    {
        found.push(ty);
    }
    for part in parts(ty) {
        applications_in(part, found);
    }
}

/// The types directly inside `ty`: a list's element, a struct's type
/// arguments, a tuple's elements.
fn parts(ty: &Type) -> Vec<&Type> {
    match ty {
        Type::Named { arguments, .. } => arguments
            .iter()
            .filter_map(|argument| match argument {
                Argument::Type(ty) => Some(ty),
                Argument::Constant(_) => None,
            })
            .collect(),
        Type::Tuple(elements) => elements.iter().collect(),
        Type::List { element, .. } => vec![element],
        Type::Set(inner)
        | Type::Endpoint(inner)
        | Type::Atomic(inner)
        | Type::Delta(inner)
        | Type::Trace { cell: inner, .. }
        | Type::Nullable(inner)
        | Type::RollingWindow { value: inner, .. } => vec![inner],
        Type::Dict { key, value } => vec![key, value],
        Type::Result { value, error } => vec![value, error],
        Type::Scalar(_)
        | Type::Parameter { .. }
        | Type::Signal
        | Type::Error
        | Type::ErrorContext
        | Type::Unknown => Vec::new(),
        Type::Function(function) => function.types().collect(),
    }
}

/// The first fault in the fields of the struct `application` names,
/// with the arguments it gives in place of the struct's parameters.
/// `path` is the field the application is the type of, through the
/// fields above it.
fn struct_fault(
    program: &Program,
    application: &Type,
    path: &[String],
    depth: usize,
    seen: &mut Vec<Type>,
) -> Option<Fault> {
    let Type::Named {
        module,
        declaration,
        arguments,
        kind: NamedKind::Struct,
        ..
    } = application
    else {
        return None;
    };
    if depth >= DEPTH || seen.len() >= APPLICATIONS || seen.contains(application) {
        return None;
    }
    // Below the type written, a struct is followed only where its
    // arguments are known. One that still names a generic parameter
    // gives no size that is known, and putting its arguments in place
    // again would grow them at each depth: `Grow<N + N>` in the fields of
    // `Grow<const N: i64>` would double.
    if depth > 0 && (names_a_parameter(application) || too_large(application)) {
        return None;
    }
    seen.push(application.clone());
    let Some(Detail::Struct(read)) = program.modules.get(*module)?.details.get(*declaration) else {
        return None;
    };
    // An argument is given to each parameter before a type pack, place
    // for place (GEN-2); a pack takes a sequence of them, which is not
    // put in place here.
    let own: Vec<Location> = read
        .generics
        .iter()
        .take_while(|generic| !matches!(generic.kind, GenericKind::Pack { .. }))
        .map(|generic| generic.at)
        .collect();
    let map: Vec<(Location, &Argument)> = own.iter().copied().zip(arguments).collect();
    read.members.iter().find_map(|member| {
        let mut path = path.to_vec();
        path.push(member.name.clone());
        fault_in(program, &member.ty, (&map, &own), (&path, depth + 1), seen)
    })
}

/// True when `ty` names a generic parameter, as a type or in a
/// constant: `Grow<N + N>`, `Box<T>`.
fn names_a_parameter(ty: &Type) -> bool {
    ty.any_part(
        &|part| matches!(part, Type::Parameter { .. }),
        &|constant| symbolic(&constant.value),
    )
}

/// True when `ty` has more than [`PARTS`] parts: types, constants, the
/// values inside a constant that is a list, a tuple or an expression,
/// and the bytes of a string. Counting stops once past it.
fn too_large(ty: &Type) -> bool {
    // `Cell` lets the two counting closures, which `any_part` takes as
    // shared references, add to one count.
    let count = std::cell::Cell::new(0usize);
    ty.any_part(
        &|_| {
            count.set(count.get() + 1);
            count.get() > PARTS
        },
        &|constant| {
            count.set(count.get().saturating_add(value_parts(&constant.value)));
            count.get() > PARTS
        },
    )
}

/// How many values `value` is made of, itself included, a string
/// counting one for each of its bytes too: the parts that may grow where
/// a struct applies itself.
fn value_parts(value: &ConstantValue) -> usize {
    1 + match value {
        ConstantValue::Str(text) => text.len(),
        ConstantValue::Unary(_, operand) | ConstantValue::Constructed { from: operand, .. } => {
            value_parts(operand)
        }
        ConstantValue::Binary(_, left, right) => {
            value_parts(left).saturating_add(value_parts(right))
        }
        ConstantValue::List(values) | ConstantValue::Tuple(values) => values
            .iter()
            .map(value_parts)
            .fold(0, usize::saturating_add),
        _ => 0,
    }
}

/// True when `value` names a generic value parameter, at any depth.
fn symbolic(value: &ConstantValue) -> bool {
    match value {
        ConstantValue::Parameter { .. } => true,
        ConstantValue::Unary(_, operand) | ConstantValue::Constructed { from: operand, .. } => {
            symbolic(operand)
        }
        ConstantValue::Binary(_, left, right) => symbolic(left) || symbolic(right),
        ConstantValue::List(values) | ConstantValue::Tuple(values) => values.iter().any(symbolic),
        _ => false,
    }
}

/// The first fault in the sizes of `declared`, a type written in the
/// generic parameters declared at `own`, once `map` puts each one's
/// argument in its place. Only a size that names one of them is held:
/// one written as a constant has been held where it is written, and so
/// has a struct applied with arguments that name none of them.
fn fault_in(
    program: &Program,
    declared: &Type,
    (map, own): (&[(Location, &Argument)], &[Location]),
    (path, depth): (&[String], usize),
    seen: &mut Vec<Type>,
) -> Option<Fault> {
    let fault = |holder: Type, (code, rule, problem): (&'static str, &'static str, String)| Fault {
        path: path.to_vec(),
        holder: counted(holder),
        code,
        rule,
        problem,
    };
    match declared {
        Type::List {
            size: Some(size), ..
        } if size.value.names_one_of(own) => {
            let holder = substitute_within(declared, map);
            if let Type::List {
                size: Some(given), ..
            } = &holder
                && let Some(problem) = list_size_problem(&given.value)
            {
                return Some(fault(holder, (codes::LIST_SIZE, "TYPE-10", problem)));
            }
        }
        Type::RollingWindow {
            maximum, minimum, ..
        } => {
            let named = (
                maximum.value.names_one_of(own),
                minimum
                    .as_ref()
                    .is_some_and(|minimum| minimum.value.names_one_of(own)),
            );
            if named.0 || named.1 {
                let holder = substitute_within(declared, map);
                if let Type::RollingWindow {
                    maximum, minimum, ..
                } = &holder
                    && let Some(problem) = window_problem((maximum, minimum.as_ref()), named)
                {
                    return Some(fault(holder, problem));
                }
            }
        }
        // A struct applied with arguments that name a parameter being
        // put in place: its fields have the sizes those arguments give.
        Type::Named {
            kind: NamedKind::Struct,
            ..
        } if mentions(declared, own) => {
            if let Some(found) = parts(declared)
                .into_iter()
                .find_map(|part| fault_in(program, part, (map, own), (path, depth), seen))
            {
                return Some(found);
            }
            let application = substitute_within(declared, map);
            return struct_fault(program, &application, path, depth, seen);
        }
        _ => {}
    }
    parts(declared)
        .into_iter()
        .find_map(|part| fault_in(program, part, (map, own), (path, depth), seen))
}

/// `holder`, a list or a window, with each size that is a count written
/// as the count it comes to: `list<i64, -2>`, and not `list<i64, N - 3>`
/// as its declaration writes it.
fn counted(holder: Type) -> Type {
    let count = |size: Constant| match size.value {
        ConstantValue::Int(count) => Constant {
            text: count.to_string(),
            value: size.value,
        },
        _ => size,
    };
    match holder {
        Type::List { element, size } => Type::List {
            element,
            size: size.map(count),
        },
        Type::RollingWindow {
            value,
            maximum,
            minimum,
        } => Type::RollingWindow {
            value,
            maximum: count(maximum),
            minimum: minimum.map(count),
        },
        other => other,
    }
}

/// What TYPE-10 rejects of a list's size `size`, where it is known:
/// "fixed sizes are nonnegative integer constants".
fn list_size_problem(size: &ConstantValue) -> Option<String> {
    match size {
        ConstantValue::Int(count) if *count < 0 => {
            Some("a list's size is not negative".to_string())
        }
        other => {
            kind_words(other).map(|wrong| format!("a list's size is a count, and this is {wrong}"))
        }
    }
}

/// The kind of a size that is no count and no duration, in words: `a
/// string`. `None` for a count, a duration, `unbounded`, and what is not
/// known.
fn kind_words(value: &ConstantValue) -> Option<&'static str> {
    Some(match value {
        ConstantValue::Float(_) => "a float",
        ConstantValue::Bool(_) => "a bool",
        ConstantValue::Str(_) => "a string",
        ConstantValue::Null => "null",
        ConstantValue::Duration(_) => "a duration",
        ConstantValue::Tenor(_) => "a tenor",
        ConstantValue::Temporal(_) => "a date or time",
        ConstantValue::Member(_) => "a member of an enum",
        ConstantValue::List(_) => "a list",
        ConstantValue::Tuple(_) => "a tuple",
        _ => return None,
    })
}

/// What TYPE-11 rejects of a window's sizes, `maximum` and `minimum`,
/// where they are known: the code and what is wrong. `named` says which
/// of them a generic parameter gives; only those are held, and the two
/// together where either is.
///
/// TYPE-11: "Its maximum and minimum have one constant kind: tick count
/// or duration ... Tick sizes are positive; duration maximum is positive
/// and minimum may be zero ... Minimum cannot exceed maximum."
fn window_problem(
    (maximum, minimum): (&Constant, Option<&Constant>),
    (maximum_named, minimum_named): (bool, bool),
) -> Option<(&'static str, &'static str, String)> {
    let kind = |problem: String| Some((codes::ROLLING_WINDOW_SIZE_KIND, "TYPE-11", problem));
    let bounds = |problem: &str| {
        Some((
            codes::ROLLING_WINDOW_SIZE_BOUNDS,
            "TYPE-11",
            problem.to_string(),
        ))
    };
    // A size of a kind other than a tick count or a duration. A
    // duration is a window's size, so it is not one of these here.
    let wrong = |value: &ConstantValue| match value {
        ConstantValue::Duration(_) => None,
        ConstantValue::Unbounded => Some("`unbounded`"),
        other => kind_words(other),
    };
    for (size, named) in [(Some(maximum), maximum_named), (minimum, minimum_named)] {
        if let Some(size) = size.filter(|_| named)
            && let Some(wrong) = wrong(&size.value)
        {
            return kind(format!(
                "a window's size is a tick count or a duration, and this is {wrong}"
            ));
        }
    }
    let described = |value: &ConstantValue| match value {
        ConstantValue::Int(_) => Some("a tick count"),
        ConstantValue::Duration(_) => Some("a duration"),
        _ => None,
    };
    if let Some(minimum) = minimum
        && let (Some(of_maximum), Some(of_minimum)) =
            (described(&maximum.value), described(&minimum.value))
        && of_maximum != of_minimum
    {
        return kind(format!(
            "a window's sizes are of one kind: the maximum is {of_maximum} and the minimum is \
             {of_minimum}"
        ));
    }
    if maximum_named {
        match maximum.value {
            ConstantValue::Int(ticks) if ticks <= 0 => {
                return bounds("a window holds at least one tick");
            }
            ConstantValue::Duration(length) if length <= 0 => {
                return bounds("a window's duration is more than zero");
            }
            _ => {}
        }
    }
    let minimum = minimum?;
    if minimum_named {
        match minimum.value {
            ConstantValue::Int(ticks) if ticks <= 0 => {
                return bounds("a window's minimum is at least one tick");
            }
            ConstantValue::Duration(length) if length < 0 => {
                return bounds("a window's minimum duration is not negative");
            }
            _ => {}
        }
    }
    match (&maximum.value, &minimum.value) {
        (ConstantValue::Int(maximum), ConstantValue::Int(minimum))
        | (ConstantValue::Duration(maximum), ConstantValue::Duration(minimum))
            if minimum > maximum =>
        {
            bounds("a window's minimum cannot exceed its maximum")
        }
        _ => None,
    }
}
