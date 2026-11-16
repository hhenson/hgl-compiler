//! Checks the signatures of callables against rules that need their
//! types read: what `read` made of each generic parameter, and what
//! `declared` made of each enum.
//!
//! * a generic value parameter's default is of the parameter's type
//!   (GEN-13, GEN-2);
//! * an option's name is one kind across the candidates of an operator
//!   (GEN-6).

use std::collections::HashMap;

use hgl_diagnostics::{Category, Diagnostics, Location};
use hgl_parser::HasSpan;

use crate::program::Program;
use crate::types::{Detail, GenericKind, Type};
use crate::values::{Written, constant_fits, generic_default, written};
use crate::{codes, error};

/// Checks the signature of every callable of the program.
pub(crate) fn check(program: &Program, diagnostics: &mut Diagnostics) {
    generic_defaults(program, diagnostics);
    option_kinds(program, diagnostics);
}

/// GEN-13: "An explicit `const N: V` declares a value parameter of
/// ordinary type V", and the default a callable gives it (GEN-2) is a
/// V. A default is checked here as far as how it is written says what
/// it is; one whose type names another generic parameter,
/// `<T, const fallback: T = 2>`, is checked where a call resolves `T`.
fn generic_defaults(program: &Program, diagnostics: &mut Diagnostics) {
    for module in &program.modules {
        for (index, declaration) in module.declarations.iter().enumerate() {
            let Some(Detail::Callable(signature)) = module.details.get(index) else {
                continue;
            };
            let file = module.files[declaration.file].file;
            for generic in &signature.generics {
                let GenericKind::Value(ty) = &generic.kind else {
                    continue;
                };
                let Some(default) = generic_default(&declaration.syntax, generic) else {
                    continue;
                };
                if constant_fits(program, &default, ty) != Some(false) {
                    continue;
                }
                let at = Location::new(file, default.span());
                let Some(kind) = written(&default) else {
                    continue;
                };
                // TYPE-7: `.member` "names a member of the enum type its
                // position expects".
                let of = match ty {
                    Type::Nullable(inner) => inner.as_ref(),
                    other => other,
                };
                if let (Written::Member(name), Type::Named { .. }) = (&kind, of) {
                    diagnostics.emit(error(
                        Category::Name,
                        codes::MEMBER_UNKNOWN,
                        at,
                        format!("`{ty}` has no member `{name}`"),
                        "TYPE-7",
                    ));
                    continue;
                }
                diagnostics.emit(error(
                    Category::Type,
                    codes::TYPE_MISMATCH,
                    at,
                    format!(
                        "this is {}, and the generic value parameter `{}` is `{ty}`",
                        kind.described(),
                        generic.name
                    ),
                    "GEN-13",
                ));
            }
        }
    }
}

/// GEN-6: "Across the candidates of one operator an option's name is
/// one kind: a generic value parameter in every candidate that declares
/// it, or a parameter in every one, so that what `{zero: .error}` names
/// is known before a candidate is selected."
///
/// A candidate that declares as a parameter an option another candidate
/// of its operator declares as a generic value parameter is reported,
/// at the parameter. The generic value parameter is the stricter
/// declaration, and no order among candidates says which came first.
///
/// A generic parameter is a value by what `read` settled (GEN-13):
/// written `const N: i64`, or plain and used as one, the `N` of
/// `value: list<i64, N>`.
fn option_kinds(program: &Program, diagnostics: &mut Diagnostics) {
    // The candidates of the program, each with its operator, by the
    // operator's module and declaration.
    let candidates = || {
        program.modules.iter().flat_map(|module| {
            module
                .declarations
                .iter()
                .enumerate()
                .filter_map(move |(index, declaration)| {
                    let (_, operator) = declaration.kind.candidate()?;
                    let operator = operator?;
                    match module.details.get(index) {
                        Some(Detail::Callable(signature)) => {
                            Some(((operator.module, operator.declaration), signature))
                        }
                        _ => None,
                    }
                })
        })
    };
    // For each operator: the options its candidates declare as generic
    // value parameters, each with where one of them does.
    let mut selecting: HashMap<(usize, usize), HashMap<&str, Location>> = HashMap::new();
    for (operator, signature) in candidates() {
        for generic in &signature.generics {
            if matches!(generic.kind, GenericKind::Value(_)) {
                selecting
                    .entry(operator)
                    .or_default()
                    .entry(generic.name.as_str())
                    .or_insert(generic.at);
            }
        }
    }
    if selecting.is_empty() {
        return;
    }
    // A second pass, in the order the candidates are declared, so that
    // what is reported comes in one order on every run.
    for (operator, signature) in candidates() {
        let Some(names) = selecting.get(&operator) else {
            continue;
        };
        let public = &program.modules[operator.0].declarations[operator.1];
        // A candidate's first parameters are its operator's; those after
        // are its own options.
        let own = match program.modules[operator.0].details.get(operator.1) {
            Some(Detail::Callable(public)) => public.parameters.len(),
            _ => continue,
        };
        for parameter in signature.parameters.iter().skip(own) {
            let Some(other) = names.get(parameter.name.as_str()) else {
                continue;
            };
            diagnostics.emit(
                error(
                    Category::Operator,
                    codes::IMPL_OPTION_KIND,
                    parameter.at,
                    format!(
                        "`{}` is a parameter here and a generic value parameter in another \
                         candidate of `{}`: an option is one kind in every candidate",
                        parameter.name, public.name
                    ),
                    "GEN-6",
                )
                .with_related(*other, "a generic value parameter here"),
            );
        }
    }
}
