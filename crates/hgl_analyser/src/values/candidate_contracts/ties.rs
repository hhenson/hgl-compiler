//! GEN-6 declaration warnings for candidates with a demonstrably tied call.
//! Construct ordinary positional inputs, then use the normal call matcher
//! so a more specific candidate or an unsettled requirement can prevent a warning.

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};

use hgl_diagnostics::{Category, Diagnostics};
use hgl_parser::{HasSpan, SyntaxKind, ast::Item};
use hgl_source::{FileId, Span};

use super::declaration_has_error;
use crate::declared::names_parameter;
use crate::hir::{Expr, ExprKind, Local, ValueCategory};
use crate::program::{
    Binding, Declaration, ImportKind, LocalKind, Module, OperatorRef, Program, Role,
};
use crate::syntax::child;
use crate::types::{Argument, Detail, GenericKind, Scalar, Signature, Type};
use crate::values::candidates::{Given, Wanted};
use crate::values::fit::{open, whole};
use crate::values::inference::unsaid;
use crate::values::operators::from_place;
use crate::values::{Candidate, Tables, Typer, tables};
use crate::{TestCandidateScope, codes, warning};

/// Look for a concrete call that leaves multiple candidates equally preferred.
pub(super) fn check(program: &Program, diagnostics: &mut Diagnostics, tests: bool) {
    let mut operators: BTreeMap<_, Vec<Candidate<'_>>> = BTreeMap::new();
    for (index, module) in program.modules.iter().enumerate() {
        for (position, (declaration, detail)) in
            module.declarations.iter().zip(&module.details).enumerate()
        {
            if let (Some((_, Some(operator))), Detail::Callable(signature)) =
                (declaration.kind.candidate(), detail)
                && (tests || declaration.test_owner.is_none())
            {
                operators
                    .entry((operator.module, operator.declaration))
                    .or_default()
                    .push((index, position, signature));
            }
        }
    }

    // TST-1: a test-only candidate can prove another operator's
    // requirement even when the operator being checked is all production.
    let test_scopes: Vec<_> = program
        .modules
        .iter()
        .filter(|_| tests)
        .filter_map(|module| TestCandidateScope::of(program, module))
        .map(|scope| (scope, test_body_uses(program, scope.module())))
        .collect();
    let core = program.core();

    for ((module, declaration), candidates) in operators {
        if candidates.len() < 2 {
            continue;
        }
        let unit = &program.modules[module];
        let operator = &unit.declarations[declaration];
        let Detail::Callable(public) = &unit.details[declaration] else {
            continue;
        };
        if !positional_shape(public) || declaration_has_error(operator, diagnostics) {
            continue;
        }
        let declared = |(module, declaration, _): Candidate<'_>| {
            &program.modules[module].declarations[declaration]
        };
        if candidates
            .iter()
            .any(|&candidate| declaration_has_error(declared(candidate), diagnostics))
        {
            continue;
        }

        let mut ties = BTreeMap::new();
        let reachable_scopes = test_scopes.iter().filter_map(|(scope, uses)| {
            (test_sees_operator(program, scope.module(), module, core, operator)
                || uses.contains(&(module, declaration)))
            .then_some(*scope)
        });
        for test_scope in std::iter::once(None).chain(reachable_scopes.map(Some)) {
            let scope = test_scope.map(TestCandidateScope::module);
            let visible: Vec<_> = candidates
                .iter()
                .copied()
                .filter(|&candidate| {
                    declared(candidate).test_owner.is_none() || scope == Some(candidate.0)
                })
                .collect();
            if visible.len() >= 2 {
                witness_ties(
                    program,
                    OperatorRef {
                        module,
                        declaration,
                    },
                    public,
                    &visible,
                    test_scope,
                    &mut ties,
                );
            }
        }

        for (candidate, others) in ties {
            let declaration = &program.modules[candidate.0].declarations[candidate.1];
            if crate::syntax::suppresses_warning(
                &declaration.syntax,
                codes::OPERATOR_CANDIDATES_TIE,
            ) {
                continue;
            }
            let mut report = warning(
                Category::Operator,
                codes::OPERATOR_CANDIDATES_TIE,
                declaration.at,
                format!(
                    "a call of `{}` matches this candidate and another equally",
                    operator.name
                ),
                "GEN-6",
            );
            for (module, candidate) in others {
                report = report.with_related(
                    program.modules[module].declarations[candidate].at,
                    "a candidate that ties with this one",
                );
            }
            diagnostics.emit(report);
        }
    }
}

/// A declaration witness can name the operator in its own module, through
/// an import or through the prelude (UNIT-2, UNIT-11, TST-1).
fn test_sees_operator(
    program: &Program,
    test: usize,
    owner: usize,
    core: Option<usize>,
    operator: &Declaration,
) -> bool {
    test == owner
        || (operator.exported
            && (core == Some(owner)
                || program.modules[test]
                    .files
                    .iter()
                    .flat_map(|file| &file.imports)
                    .any(|import| {
                        import.module == Some(owner)
                            && (import.kind == ImportKind::Alias || import.name == operator.name)
                    })))
}

/// TST-1: a test's candidate view also reaches operators in functions
/// called by that test, including private operators behind public wrappers.
fn test_body_uses(program: &Program, test_module: usize) -> BTreeSet<(usize, usize)> {
    let unit = &program.modules[test_module];
    let mut pending = Vec::new();
    for test in &unit.tests {
        let Some(block) = child(&test.syntax, SyntaxKind::Block) else {
            continue;
        };
        let Some(statement) = block
            .children()
            .find(|node| Item::cast(node.clone()).is_none())
        else {
            continue;
        };
        let span = Span::new(statement.span().start, block.span().end);
        pending.extend(declarations_used(unit, test.at.file, span));
    }

    let mut reached = BTreeSet::new();
    while let Some((module, declaration)) = pending.pop() {
        if !reached.insert((module, declaration)) {
            continue;
        }
        let unit = &program.modules[module];
        let declaration = &unit.declarations[declaration];
        if declaration.kind.binding_role().is_some() {
            pending.extend(declarations_used(
                unit,
                declaration.at.file,
                declaration.syntax.span(),
            ));
        }
    }
    reached
}

fn declarations_used(
    unit: &Module,
    file: FileId,
    span: Span,
) -> impl Iterator<Item = (usize, usize)> + '_ {
    unit.uses
        .iter()
        .filter(move |used| {
            used.at.file == file && (span.start..span.end).contains(&used.at.span.start)
        })
        .filter_map(|used| match &used.binding {
            Binding::Declarations {
                module,
                declarations,
            } => Some((*module, declarations)),
            Binding::Local { .. } => None,
        })
        .flat_map(|(module, declarations)| declarations.iter().map(move |&index| (module, index)))
}

type Ties = BTreeMap<(usize, usize), BTreeSet<(usize, usize)>>;

/// Check typed witnesses in an ordinary value function and in composition.
/// Value functions only consider value candidates, as they do for real calls.
fn witness_ties<'a>(
    program: &'a Program,
    operator: OperatorRef,
    public: &Signature,
    visible: &[Candidate<'a>],
    test_scope: Option<TestCandidateScope>,
    ties: &mut Ties,
) {
    let module = &program.modules[operator.module];
    let declaration = &module.declarations[operator.declaration];
    let (bindings, written, calls) = tables(module);
    let selecting = RefCell::new(Vec::new());
    let tables = Tables {
        bindings: &bindings,
        written: &written,
        calls: &calls,
        selecting: &selecting,
    };
    let empty = Signature {
        generics: Vec::new(),
        parameters: Vec::new(),
        result: None,
        reference: false,
    };
    let mut trial_diagnostics = Diagnostics::default();
    let mut typer = Typer::bare(
        program,
        module,
        declaration,
        &empty,
        tables,
        &mut trial_diagnostics,
    );
    // TST-1: requirements can call other operators with test-only
    // candidates. They must see the same view as the call being tried.
    typer.test_candidates = test_scope;
    let declared_role = |(module, candidate, _): Candidate<'_>| {
        program.modules[module].declarations[candidate].kind.role()
    };

    for role in [Role::Value, Role::Temporal] {
        typer.graph = role == Role::Temporal;
        let matching: Vec<_> = visible
            .iter()
            .copied()
            .filter(|&candidate| typer.graph || declared_role(candidate) == Some(Role::Value))
            .collect();
        let witnesses: Vec<_> = matching
            .iter()
            .copied()
            .filter(|&candidate| {
                declared_role(candidate) == Some(role) && positional_shape(candidate.2)
            })
            .collect();
        let mut tried = Vec::new();
        for (index, left) in witnesses.iter().enumerate() {
            for right in &witnesses[index + 1..] {
                let Some(expected) = common_output(left.2, right.2) else {
                    continue;
                };
                let maximum = left.2.parameters.len().min(right.2.parameters.len());
                // Admission checks whether omitted slots can take
                // defaults declared on either the candidate or its operator.
                for arity in 0..=maximum {
                    for fallback in [Scalar::I64, Scalar::F64, Scalar::Bool, Scalar::Str] {
                        let Some(types) = witness_types(left.2, right.2, arity, fallback) else {
                            break;
                        };
                        let witness = (types, expected);
                        if tried.contains(&witness) {
                            continue;
                        }
                        record_tied_call(&mut typer, operator, public, &matching, &witness, ties);
                        tried.push(witness);
                    }
                }
            }
        }
    }
}

/// Calls considered here use fixed positional slots and no special modes.
/// Declared type parameters and concrete scalars are supported below.
fn positional_shape(signature: &Signature) -> bool {
    signature.parameters.iter().all(|parameter| {
        !parameter.configuration
            && parameter.pack.is_none()
            && !parameter.constant
            && !parameter.var
            && !parameter.reference
    })
}

/// A witness asks for a result both declarations can be seen to provide.
/// A call that leaves its output unresolved is not such a witness.
fn common_output<'a>(left: &'a Signature, right: &'a Signature) -> Option<Option<&'a Type>> {
    match (&left.result, &right.result) {
        (None, None) => Some(None),
        (Some(a), Some(b)) if a == b && whole(a) && !open(a) && !names_parameter(a) => {
            Some(Some(a))
        }
        _ => None,
    }
}

/// Find a positional witness for the two shapes. Admission checks the
/// full signatures, including repeated type parameters and constraints.
fn witness_types(
    left: &Signature,
    right: &Signature,
    arity: usize,
    fallback: Scalar,
) -> Option<Vec<Type>> {
    left.parameters
        .iter()
        .zip(&right.parameters)
        .take(arity)
        .map(|(a, b)| match (&a.ty, &b.ty) {
            (Type::Parameter { .. }, Type::Parameter { .. }) => Some(Type::Scalar(fallback)),
            (Type::Scalar(a), Type::Scalar(b)) if a == b && *a != Scalar::Any => {
                Some(Type::Scalar(*a))
            }
            (Type::Scalar(scalar), Type::Parameter { .. })
            | (Type::Parameter { .. }, Type::Scalar(scalar))
                if *scalar != Scalar::Any =>
            {
                Some(Type::Scalar(*scalar))
            }
            _ => None,
        })
        .collect()
}

/// Ask the same request, admission and ranking used by a named operator call.
fn record_tied_call<'a>(
    typer: &mut Typer<'a>,
    operator: OperatorRef,
    public: &Signature,
    candidates: &[Candidate<'a>],
    witness: &(Vec<Type>, Option<&Type>),
    ties: &mut Ties,
) {
    let (types, expected) = witness;
    let declaration = typer.declaration;
    let category = if typer.graph {
        ValueCategory::Connection
    } else {
        ValueCategory::Value
    };
    typer.use_instance_locals(
        types
            .iter()
            .enumerate()
            .map(|(position, ty)| Local {
                name: format!("arg_{position}"),
                ty: ty.clone(),
                at: declaration.at,
                kind: LocalKind::Parameter,
                category,
            })
            .collect(),
    );
    let given: Vec<_> = types
        .iter()
        .enumerate()
        .map(|(position, ty)| {
            Given::read(
                position,
                declaration.syntax.clone(),
                Expr {
                    kind: ExprKind::Local(position),
                    ty: ty.clone(),
                    at: declaration.at,
                    category,
                },
            )
        })
        .collect();
    let mark = typer.mark();
    let mut found = vec![None; public.generics.len()];
    from_place(public, *expected, &mut found);
    let request = typer.argument_output_requested(operator, public, found.clone(), &given, false);
    let supplied: Vec<_> = types.iter().map(Some).collect();
    typer.infer_operator_inputs(public, &mut found, &supplied);
    let public_arguments: Vec<_> = public
        .generics
        .iter()
        .zip(found)
        .map(|(parameter, value)| {
            value.unwrap_or_else(|| match parameter.kind {
                GenericKind::Value(_) => Argument::Constant(unsaid()),
                _ => Argument::Type(Type::Unknown),
            })
        })
        .collect();
    let target = (operator.module, operator.declaration);
    // Ordinary admission can leave the public requirement to a later
    // report. A declaration warning needs a call the public signature admits.
    if request.unfit.is_some()
        || typer
            .requires_told_with_packs(target, &public_arguments, &request.type_packs, None)
            .is_some_and(|(_, holds, _)| holds != Some(true))
    {
        typer.take_back(mark, false);
        return;
    }
    let wanted = Wanted {
        place: request.place.as_ref().or(*expected),
        among: request.among,
        written: false,
        callback: None,
    };
    let (selected, unknown) = typer.admitted(operator, candidates, &given, &wanted);
    if !typer.take_back(mark, false) || unknown || selected.len() < 2 {
        return;
    }
    for candidate in &selected {
        let at = (candidate.module, candidate.declaration);
        ties.entry(at).or_default().extend(
            selected
                .iter()
                .map(|other| (other.module, other.declaration))
                .filter(|&other| other != at),
        );
    }
}
