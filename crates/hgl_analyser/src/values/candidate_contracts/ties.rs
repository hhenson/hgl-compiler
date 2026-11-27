//! GEN-6 declaration warnings for candidates with a demonstrably tied call.
//! Construct typed positional and configuration inputs, then use the normal call matcher
//! so a more specific candidate or an unsettled requirement can prevent a warning.

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};

use hgl_diagnostics::{Category, Diagnostics, Location};
use hgl_parser::{HasSpan, SyntaxKind, SyntaxNode, ast::Item};
use hgl_source::{FileId, Span};

use super::declaration_has_error;
use crate::declared::names_parameter;
use crate::hir::{Expr, ExprKind, Local, ValueCategory};
use crate::program::{
    Binding, Declaration, ImportKind, LocalKind, Module, OperatorRef, Program, Role,
};
use crate::syntax::child;
use crate::types::{ConstantValue, Detail, Scalar, Signature, Type};
use crate::values::candidates::{Chosen, Given, Key, Wanted};
use crate::values::constants::default_at;
use crate::values::expressions::written_null;
use crate::values::fit::{open, whole};
use crate::values::operators::{from_place, public_inputs};
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
        if !witness_shape(public) || declaration_has_error(operator, diagnostics) {
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

#[derive(PartialEq)]
struct Witness<'a> {
    positional: Vec<Type>,
    configuration: Vec<(&'a str, Type)>,
    output: Option<&'a Type>,
}

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
                declared_role(candidate) == Some(role) && witness_shape(candidate.2)
            })
            .collect();
        let mut tried = Vec::new();
        for (index, left) in witnesses.iter().enumerate() {
            for right in &witnesses[index + 1..] {
                let Some(expected) = common_output(left.2, right.2) else {
                    continue;
                };
                let generics: Vec<_> = left
                    .2
                    .generics
                    .iter()
                    .chain(&right.2.generics)
                    .map(|generic| generic.at)
                    .collect();
                // Admission checks whether omitted slots can take
                // defaults declared on either the candidate or its operator.
                for fallback in [Scalar::I64, Scalar::F64, Scalar::Bool, Scalar::Str] {
                    let positional: Vec<_> = left
                        .2
                        .positional_parameters()
                        .zip(right.2.positional_parameters())
                        .map_while(|((_, left), (_, right))| {
                            witness_type(&left.ty, &right.ty, fallback, &generics)
                        })
                        .collect();
                    let configurations = configuration_witnesses(
                        public,
                        &declaration.syntax,
                        left.2,
                        right.2,
                        fallback,
                        &generics,
                    );
                    for arity in 0..=positional.len() {
                        for configuration in &configurations {
                            let witness = Witness {
                                positional: positional[..arity].to_vec(),
                                configuration: configuration.clone(),
                                output: expected,
                            };
                            if tried.contains(&witness) {
                                continue;
                            }
                            record_tied_call(
                                &mut typer, operator, public, &matching, &witness, ties,
                            );
                            tried.push(witness);
                        }
                    }
                }
            }
        }
    }
}

/// This witness looks for calls with fixed ordinary inputs. A static callback,
/// place, endpoint designation or compile-time value would need its own proof.
fn witness_shape(signature: &Signature) -> bool {
    signature.parameters.iter().all(|parameter| {
        parameter.pack.is_none() && !parameter.constant && !parameter.var && !parameter.reference
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

/// Take a concrete input from the two declarations or use a common scalar
/// for two bare parameters. Admission checks the remaining constraints.
fn witness_type(
    left: &Type,
    right: &Type,
    fallback: Scalar,
    generics: &[Location],
) -> Option<Type> {
    let concrete = |ty: &Type| {
        matches!(
            ty,
            Type::Scalar(_)
                | Type::Named { .. }
                | Type::List { .. }
                | Type::Set(_)
                | Type::Dict { .. }
                | Type::Tuple(_)
        ) && !ty.any_part(
            &|part| {
                open(part)
                    || matches!(
                        part,
                        Type::Parameter { .. } | Type::PackMember { .. } | Type::Function(_)
                    )
            },
            &|constant| {
                matches!(constant.value, ConstantValue::Unknown)
                    || !crate::constant::worked_out(&constant.value)
                    || constant.value.names_one_of(generics)
            },
        )
    };
    match (left, right) {
        (Type::Parameter { .. }, Type::Parameter { .. }) => Some(Type::Scalar(fallback)),
        (Type::Parameter { .. }, ty) | (ty, Type::Parameter { .. }) if concrete(ty) => {
            Some(ty.clone())
        }
        (left, right) if left == right && concrete(left) => Some(left.clone()),
        _ => None,
    }
}

/// Configuration uses names, even where a candidate's declaration changes
/// their order. Try omission, required shared slots and all compatible shared
/// slots, keeping the number of trials independent of the number of subsets.
fn configuration_witnesses<'a>(
    public: &Signature,
    syntax: &SyntaxNode,
    left: &'a Signature,
    right: &Signature,
    fallback: Scalar,
    generics: &[Location],
) -> Vec<Vec<(&'a str, Type)>> {
    let mut required = Vec::new();
    let mut shared = Vec::new();
    for left in left
        .parameters
        .iter()
        .filter(|parameter| parameter.configuration)
    {
        let Some(right) = right
            .parameters
            .iter()
            .find(|parameter| parameter.configuration && parameter.name == left.name)
        else {
            continue;
        };
        let Some(ty) = witness_type(&left.ty, &right.ty, fallback, generics) else {
            continue;
        };
        let public_option = public
            .parameters
            .iter()
            .find(|parameter| parameter.configuration && parameter.name == left.name);
        // FN-18: both candidates may default a slot the operator still requires.
        // GEN-6: a written public null cannot fill a narrowed required slot.
        let needs_argument = match public_option {
            Some(parameter) if !matches!(parameter.ty, Type::Nullable(_)) => !parameter.has_default,
            Some(parameter)
                if parameter.has_default
                    && default_at(syntax, parameter)
                        .is_none_or(|default| !written_null(&default)) =>
            {
                false
            }
            _ => !left.has_default || !right.has_default,
        };
        let name = left.name.as_str();
        if needs_argument {
            required.push((name, ty.clone()));
        }
        shared.push((name, ty));
    }
    required.sort_unstable_by_key(|&(name, _)| name);
    shared.sort_unstable_by_key(|&(name, _)| name);
    let mut witnesses = vec![Vec::new()];
    if !required.is_empty() && required != shared {
        witnesses.push(required);
    }
    if !shared.is_empty() {
        witnesses.push(shared);
    }
    witnesses
}

/// Ask the same request, admission and ranking used by a named operator call.
fn record_tied_call<'a>(
    typer: &mut Typer<'a>,
    operator: OperatorRef,
    public: &Signature,
    candidates: &[Candidate<'a>],
    witness: &Witness<'_>,
    ties: &mut Ties,
) {
    let declaration = typer.declaration;
    let category = if typer.graph {
        ValueCategory::Connection
    } else {
        ValueCategory::Value
    };
    let inputs = witness
        .positional
        .iter()
        .enumerate()
        .map(|(position, ty)| (Key::Position(position), ty))
        .chain(
            witness
                .configuration
                .iter()
                .map(|(name, ty)| (Key::Configuration((*name).to_owned()), ty)),
        );
    let (locals, given): (Vec<_>, Vec<_>) = inputs
        .enumerate()
        .map(|(position, (key, ty))| {
            let local = Local {
                name: format!("arg_{position}"),
                ty: ty.clone(),
                at: declaration.at,
                kind: LocalKind::Parameter,
                category,
            };
            let mut input = Given::read(
                position,
                declaration.syntax.clone(),
                Expr {
                    kind: ExprKind::Local(position),
                    ty: ty.clone(),
                    at: declaration.at,
                    category,
                },
            );
            input.key = key;
            (local, input)
        })
        .unzip();
    typer.use_instance_locals(locals);
    let mark = typer.mark();
    if typer.missing_configuration(public, &given, &declaration.name, declaration.at) {
        typer.take_back(mark, false);
        return;
    }
    let mut found = vec![None; public.generics.len()];
    from_place(public, witness.output, &mut found);
    let request = typer.argument_output_requested(operator, public, found, &given, false);
    if request.unfit.is_some() {
        typer.take_back(mark, false);
        return;
    }
    let wanted = Wanted {
        place: request.place.as_ref().or(witness.output),
        among: request.among,
        written: false,
        callback: None,
    };
    let (mut selected, unknown) = typer.admitted(operator, candidates, &given, &wanted);
    if !typer.take_back(mark, false) || unknown || selected.len() < 2 {
        return;
    }
    if child(&declaration.syntax, SyntaxKind::RequiresClause).is_some() {
        let supplied = public_inputs(public, &given);
        selected.retain(|candidate| {
            public_requirement_holds(
                typer,
                operator,
                public,
                &supplied,
                witness.output,
                candidate,
            )
        });
        if selected.len() < 2 {
            return;
        }
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

/// An omitted argument takes the instantiated type of the admitted
/// candidate's default, which can differ across the same call (GEN-6).
fn public_requirement_holds(
    typer: &mut Typer<'_>,
    operator: OperatorRef,
    public: &Signature,
    supplied: &[Option<&Type>],
    expected: Option<&Type>,
    candidate: &Chosen<'_>,
) -> bool {
    let signature = candidate.signature();
    let supplied: Vec<_> = public
        .candidate_parameters(signature)
        .zip(supplied)
        .map(|(slot, supplied)| {
            supplied.or_else(|| slot.map(|slot| &signature.parameters[slot].ty))
        })
        .collect();
    let mut found = vec![None; public.generics.len()];
    from_place(public, expected, &mut found);
    let Some(arguments) = typer.public_witness_arguments(operator, public, found, &supplied) else {
        return false;
    };
    let mark = typer.mark();
    let holds = typer
        .requires_told_with_packs(
            (operator.module, operator.declaration),
            &arguments,
            &[],
            None,
        )
        .is_none_or(|(_, holds, _)| holds == Some(true));
    typer.take_back(mark, false) && holds
}
