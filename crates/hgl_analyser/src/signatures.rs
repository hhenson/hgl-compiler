//! Checks the signatures of callables against rules that need their
//! types read: what `read` made of each generic parameter, and what
//! `declared` made of each enum.
//!
//! * a generic value parameter's default is of the parameter's type
//!   (GEN-13, GEN-2);
//! * an option's name is one kind across the candidates of an operator
//!   (GEN-6);
//! * a native implementation matches a declaration of its name and
//!   role once the parameter and result types are read (NATIVE-2);
//! * a `var` parameter is one of a value function or a value candidate
//!   (FN-15), and a `ref` mark one of a node (FN-16).

use std::collections::HashMap;

use hgl_diagnostics::{Category, Diagnostics, Location};
use hgl_parser::{HasSpan, SyntaxKind, SyntaxNode};
use hgl_source::FileId;

use crate::constant::{Constants, Fault, Scope};
use crate::declared::names_parameter;
use crate::program::{Binding, Declaration, DeclarationKind, Phase, Program, Role};
use crate::syntax::{child, has_token};
use crate::types::{Detail, GenericKind, NamedKind, Pack, Type};
use crate::values::generic_default;
use crate::{codes, error};

mod native;

/// Checks the signature of every callable of the program.
pub(crate) fn check(program: &Program, diagnostics: &mut Diagnostics, tests: bool) {
    constant_failures(program, diagnostics);
    generic_defaults(program, diagnostics);
    crate::laws::check_requirements(program, diagnostics);
    crate::laws::check_properties(program, diagnostics);
    option_kinds(program, diagnostics, tests);
    pack_declarations(program, diagnostics);
    configuration_shapes(program, diagnostics);
    crate::values::check_candidate_contracts(program, diagnostics);
    var_parameters(program, diagnostics);
    ref_marks(program, diagnostics);
    native::check(program, diagnostics);
}

/// FN-8 permits one pack of each kind. GEN-6 requires an operator's
/// pack slot to stay a pack in each candidate, even for one argument.
fn pack_declarations(program: &Program, diagnostics: &mut Diagnostics) {
    for module in &program.modules {
        for (declaration, detail) in module.declarations.iter().zip(&module.details) {
            let Detail::Callable(signature) = detail else {
                continue;
            };
            let (mut positional, mut named) = (None, None);
            for parameter in &signature.parameters {
                let (first, kind) = match parameter.pack {
                    Some(Pack::Positional) => (&mut positional, "positional"),
                    Some(Pack::Named) => (&mut named, "named"),
                    None => continue,
                };
                if let Some(at) = *first {
                    diagnostics.emit(
                        error(
                            Category::Type,
                            codes::PACK_DUPLICATE,
                            parameter.at,
                            format!("`{}` is a second {kind} pack", parameter.name),
                            "FN-8",
                        )
                        .with_related(at, "the first pack of this kind"),
                    );
                } else {
                    *first = Some(parameter.at);
                }
            }

            let Some((_, Some(operator))) = declaration.kind.candidate() else {
                continue;
            };
            let owner = &program.modules[operator.module];
            let Some(Detail::Callable(public)) = owner.details.get(operator.declaration) else {
                continue;
            };
            for (parameter, index) in public
                .parameters
                .iter()
                .zip(public.candidate_parameters(signature))
            {
                let Some(index) = index.filter(|_| parameter.pack.is_some()) else {
                    continue;
                };
                let candidate = &signature.parameters[index];
                if candidate.pack.is_none() {
                    diagnostics.emit(
                        error(
                            Category::Operator,
                            codes::IMPL_PACK,
                            candidate.at,
                            format!(
                                "`{}` is a fixed parameter where the operator declares a pack",
                                candidate.name
                            ),
                            "GEN-6",
                        )
                        .with_related(parameter.at, "the operator's pack"),
                    );
                }
            }
        }
    }
}

/// FN-16: "A node (FN-2, NATIVE-3) may mark a temporal parameter `ref`,
/// `ref low: f64`, and its result, `-> ref f64` ... A value function, a
/// graph, and an operator mark none, and a mark there rejects: an
/// operator gives the parameters every candidate takes, and a candidate
/// that is a node marks its own ... Proposed: ... a mark on what is no
/// node with `ref.position`." Each mark is reported at its `ref`.
///
/// A native temporal declaration, which has no body, is a node or a
/// graph by the implementation that completes it (NATIVE-3: "`{}`
/// denotes a graph; `{ when; }` denotes a node"), and takes the mark.
fn ref_marks(program: &Program, diagnostics: &mut Diagnostics) {
    for module in &program.modules {
        for (index, declaration) in module.declarations.iter().enumerate() {
            let marks = ref_tokens(&declaration.syntax);
            if marks.is_empty() {
                continue;
            }
            let phase = module.phases.get(index).copied().flatten();
            let what = match declaration.kind {
                DeclarationKind::Operator => Some("an operator"),
                DeclarationKind::Function(Role::Value)
                | DeclarationKind::NativeFunction(Role::Value)
                | DeclarationKind::NativeImplementation(Role::Value)
                | DeclarationKind::NativeCandidateImplementation(Role::Value) => {
                    Some("a value function")
                }
                kind if matches!(kind.candidate(), Some((Role::Value, _))) => {
                    Some("a value candidate of an operator")
                }
                DeclarationKind::Function(Role::Temporal)
                | DeclarationKind::Implementation(Role::Temporal, _) => {
                    (phase == Some(Phase::Graph)).then_some("a graph")
                }
                DeclarationKind::NativeImplementation(Role::Temporal)
                | DeclarationKind::NativeCandidateImplementation(Role::Temporal) => {
                    (!native_node(declaration)).then_some("a graph")
                }
                _ => None,
            };
            let file = module.files[declaration.file].file;
            let Some(what) = what else {
                signal_marks(module, index, file, diagnostics);
                continue;
            };
            for mark in marks {
                diagnostics.emit(
                    error(
                        Category::FunctionKind,
                        codes::REF_POSITION,
                        Location::new(file, mark.span()),
                        format!(
                            "`{}` is {what}, and only a node marks a parameter or its result `ref`",
                            declaration.name
                        ),
                        "FN-16",
                    )
                    .with_note(
                        "a node holds `state`, `cache`, `inject`, `start`, `when` or `stop`; \
                         a graph gives a connection to a `ref` parameter as it is",
                    ),
                );
            }
        }
    }
}

/// FN-16: "A `signal` parameter is not marked either, and a mark on one
/// rejects: a signal has no payload to designate (TYPE-12) ...
/// Proposed: ... a mark on what is no node, or on a `signal` parameter,
/// with `ref.position`." TYPE-12: a `signal` input "is not marked `ref`
/// (FN-16)". Each such mark of the declaration at `index` of `module`,
/// one that may take a mark, is reported at its `ref`; a `signal?` that
/// may be left out is a `signal` parameter too.
///
/// "Nor does `ref` stand on a window: a `ref` parameter or a `ref` result
/// whose type is a `rolling_window` rejects ... A mark on a window
/// rejects with `ref.position` too."
fn signal_marks(
    module: &crate::program::Module,
    index: usize,
    file: FileId,
    diagnostics: &mut Diagnostics,
) {
    let Some(Detail::Callable(signature)) = module.details.get(index) else {
        return;
    };
    let syntax = &module.declarations[index].syntax;
    let mark_of = |node: &hgl_parser::SyntaxNode| {
        crate::syntax::tokens(node).find(|token| token.kind() == hgl_parser::SyntaxKind::KwRef)
    };
    let window = |ty: &Type| matches!(ty.omission_removed(), Type::RollingWindow { .. });
    let mut marks = Vec::new();
    let written = crate::read::parameters(syntax);
    for (parameter, node) in signature.parameters.iter().zip(&written) {
        if !parameter.reference || parameter.configuration {
            continue;
        }
        let message = if *parameter.ty.omission_removed() == Type::Signal {
            (
                format!(
                    "`{}` is a `signal`, which has no payload to designate, and is not marked \
                     `ref`",
                    parameter.name
                ),
                "a `signal` parameter is given a connection as it is, and ticks",
            )
        } else if window(&parameter.ty) {
            (
                format!(
                    "`{}` is a window, which is not marked `ref`",
                    parameter.name
                ),
                "a window parameter is given the window's connection as it is",
            )
        } else {
            continue;
        };
        if let Some(mark) = mark_of(node) {
            marks.push((mark, message));
        }
    }
    if signature.reference
        && signature.result.as_ref().is_some_and(window)
        && let Some(mark) = crate::syntax::child(syntax, hgl_parser::SyntaxKind::ReturnType)
            .as_ref()
            .and_then(mark_of)
    {
        marks.push((
            mark,
            (
                "the result is a window, which is not marked `ref`".to_string(),
                "a window result publishes the values that arrive",
            ),
        ));
    }
    for (mark, (message, note)) in marks {
        diagnostics.emit(
            error(
                Category::FunctionKind,
                codes::REF_POSITION,
                Location::new(file, mark.span()),
                message,
                "FN-16",
            )
            .with_note(note),
        );
    }
}

/// Each `ref` written before a parameter of `declaration` or before its
/// result type, in the order written (FN-16).
fn ref_tokens(declaration: &hgl_parser::SyntaxNode) -> Vec<hgl_parser::SyntaxToken> {
    let result = crate::syntax::child(declaration, hgl_parser::SyntaxKind::ReturnType);
    crate::read::parameters(declaration)
        .iter()
        .filter(|node| {
            node.parent()
                .is_some_and(|list| list.kind() == SyntaxKind::ParamList)
        })
        .chain(result.iter())
        .filter_map(|node| {
            crate::syntax::tokens(node).find(|token| token.kind() == hgl_parser::SyntaxKind::KwRef)
        })
        .collect()
}

/// NATIVE-3: true where a native temporal implementation part is a node,
/// "`{ when; }` denotes a node", and not a graph, "`{}` denotes a
/// graph".
fn native_node(declaration: &Declaration) -> bool {
    crate::syntax::child(
        &declaration.syntax,
        hgl_parser::SyntaxKind::NativeContractBody,
    )
    .into_iter()
    .flat_map(|body| body.children())
    .filter(|hook| hook.kind() == hgl_parser::SyntaxKind::NativeHook)
    .filter_map(|hook| hook.first_token())
    .any(|keyword| keyword.kind() == hgl_parser::SyntaxKind::KwWhen)
}

/// FN-15: "A value function, a `const fn` or a native one, may mark a
/// parameter `var` ... So may a value candidate of an operator (GEN-6).
/// Its type is an ordinary value type, and no capability's (TIME-8) or
/// `endpoint<T>` (TIME-9), which no place holds ... A temporal `fn`, a
/// temporal candidate, and an operator declare none (`parameter.var`)".
/// Each is reported at the `var`.
fn var_parameters(program: &Program, diagnostics: &mut Diagnostics) {
    for module in &program.modules {
        for (index, declaration) in module.declarations.iter().enumerate() {
            let Some(Detail::Callable(signature)) = module.details.get(index) else {
                continue;
            };
            if !signature.parameters.iter().any(|parameter| parameter.var) {
                continue;
            }
            let file = module.files[declaration.file].file;
            // What the declaration is, where it may declare none.
            let declares_none = match declaration.kind {
                DeclarationKind::Function(Role::Value)
                | DeclarationKind::NativeFunction(Role::Value)
                | DeclarationKind::NativeImplementation(Role::Value)
                | DeclarationKind::NativeCandidateImplementation(Role::Value) => None,
                // "an operator gives the parameters every candidate
                // takes, and each candidate says which of its own is
                // `var`".
                kind if matches!(kind.candidate(), Some((Role::Value, _))) => None,
                DeclarationKind::Operator => Some("an operator"),
                kind if kind.candidate().is_some() => Some("a temporal candidate of an operator"),
                DeclarationKind::NativeCandidateImplementation(_) => {
                    Some("a temporal candidate of an operator")
                }
                _ => Some("a temporal function"),
            };
            let written = crate::read::parameters(&declaration.syntax);
            for (parameter, node) in signature.parameters.iter().zip(&written) {
                if !parameter.var || parameter.configuration {
                    continue;
                }
                let Some(var) = crate::syntax::tokens(node)
                    .find(|token| token.kind() == hgl_parser::SyntaxKind::KwVar)
                else {
                    continue;
                };
                let at = Location::new(file, var.span());
                let mut inner = &parameter.ty;
                while let Type::Nullable(of) = inner {
                    inner = of;
                }
                let no_place = match inner {
                    Type::Named {
                        kind: NamedKind::Native,
                        module: of,
                        ..
                    } if Some(*of) == program.core() => Some("a capability"),
                    Type::Endpoint(_) => Some("an endpoint"),
                    _ => None,
                };
                let message = match (declares_none, no_place) {
                    (Some(what), _) => format!(
                        "`{}` is a `var` parameter of {what}: only a `const fn`, a native value function or a value candidate declares one",
                        parameter.name
                    ),
                    (None, Some(what)) => format!(
                        "`{}` is a `var` parameter of the type of {what}, which no place holds",
                        parameter.name
                    ),
                    (None, None) => continue,
                };
                diagnostics.emit(error(
                    Category::FunctionKind,
                    codes::PARAMETER_VAR,
                    at,
                    message,
                    "FN-15",
                ));
            }
        }
    }
}

/// GEN-13: "An explicit `const N: V` declares a value parameter of
/// ordinary type V", and the default a callable gives it (GEN-2) is a
/// V. A default is read as a value of V (`crate::constant`); one whose
/// type names another generic parameter, `<T, const fallback: T = 2>`,
/// is checked where a call resolves `T`.
///
/// GEN-6: a candidate "states in `requires` the values of a generic
/// value parameter it is for, by comparing it with constants". Each
/// constant a `requires` compares a generic value parameter with is a
/// value of that parameter's type too (EXPR-5): `requires mode == 1` of
/// an enum's `mode` rejects.
/// Reports each module constant whose evaluation fails where the
/// compiler works it out: `const BIG: i64 = 9223372036854775807 + 1`
/// (NUM-2), `const DIV: i64 = 1 // 0` (NUM-12). UNIT-13: "a failure in
/// evaluating it is a required-constant failure (ERR-9)", and ERR-9:
/// "Required constant failures reject where specified". Where it is
/// read, it says nothing more.
fn constant_failures(program: &Program, diagnostics: &mut Diagnostics) {
    let everywhere: HashMap<(FileId, u32), &Binding> = program
        .modules
        .iter()
        .flat_map(|module| &module.uses)
        .map(|used| ((used.at.file, used.at.span.start), &used.binding))
        .collect();
    let binding = |file: FileId, offset: u32| everywhere.get(&(file, offset)).copied();
    let constants = Constants::default();
    for (index, module) in program.modules.iter().enumerate() {
        for (position, declaration) in module.declarations.iter().enumerate() {
            if declaration.kind != DeclarationKind::Constant {
                continue;
            }
            let file = module.files[declaration.file].file;
            let scope = Scope {
                program,
                file,
                binding: &binding,
                parameter: &|_| None,
                arguments: &[],
                constants: &constants,
            };
            if !crate::constant::fails_to_evaluate(&scope, (index, position)) {
                continue;
            }
            // The expression: the child that is not the written type.
            let written = crate::read::type_child(&declaration.syntax);
            let Some(expression) = declaration
                .syntax
                .children()
                .find(|child| Some(child) != written.as_ref())
            else {
                continue;
            };
            diagnostics.emit(crate::error(
                Category::Type,
                codes::CONSTANT_FAILURE,
                Location::new(file, expression.span()),
                format!(
                    "evaluating `{}` fails: an operation in it is out of its type's range, \
                     or divides by zero",
                    declaration.name
                ),
                "UNIT-13",
            ));
        }
    }
}

fn generic_defaults(program: &Program, diagnostics: &mut Diagnostics) {
    let everywhere: HashMap<(FileId, u32), &Binding> = program
        .modules
        .iter()
        .flat_map(|module| &module.uses)
        .map(|used| ((used.at.file, used.at.span.start), &used.binding))
        .collect();
    let binding = |file: FileId, offset: u32| everywhere.get(&(file, offset)).copied();
    let constants = Constants::default();
    for module in &program.modules {
        for (index, declaration) in module.declarations.iter().enumerate() {
            // A function's, a candidate's and an operator's, and a
            // struct's (GEN-2).
            let generics = match module.details.get(index) {
                Some(Detail::Callable(signature)) => &signature.generics,
                Some(Detail::Struct(read)) => &read.generics,
                _ => continue,
            };
            let file = module.files[declaration.file].file;
            let scope = Scope {
                program,
                file,
                binding: &binding,
                parameter: &|_| None,
                arguments: &[],
                constants: &constants,
            };
            // What is wrong with the constant `node` as a value of `ty`,
            // `place` named for the message.
            let check =
                |node: &SyntaxNode, ty: &Type, place: &str, diagnostics: &mut Diagnostics| {
                    let at = Location::new(file, node.span());
                    let read = match crate::constant::typed(node, Some(ty), &scope) {
                        Ok(read) => read,
                        Err(fault) => {
                            if let Some(report) = fault.report(at, place, ty) {
                                diagnostics.emit(report);
                            }
                            return;
                        }
                    };
                    match crate::constant::fit(read.clone(), Some(ty), program) {
                        Err(Fault::Mismatch(_)) => diagnostics.emit(error(
                            Category::Type,
                            codes::TYPE_MISMATCH,
                            at,
                            format!(
                                "this is {}, and {place} is `{ty}`",
                                crate::constant::described(&read)
                            ),
                            "GEN-13",
                        )),
                        Err(fault) => {
                            if let Some(report) = fault.report(at, place, ty) {
                                diagnostics.emit(report);
                            }
                        }
                        Ok(_) => {}
                    }
                };
            for generic in generics {
                let GenericKind::Value(ty) = &generic.kind else {
                    continue;
                };
                if names_parameter(ty) {
                    continue;
                }
                let Some(default) = generic_default(&declaration.syntax, generic) else {
                    continue;
                };
                let place = format!("the generic value parameter `{}`", generic.name);
                if let Some(Detail::Callable(signature)) = module.details.get(index)
                    && crate::laws::predicate(&default).is_some()
                {
                    if crate::laws::default_parameter(signature, &default).is_none() {
                        diagnostics.emit(error(
                            Category::Type,
                            codes::TYPE_MISMATCH,
                            Location::new(file, default.span()),
                            "a law default must query a formal parameter that takes a function"
                                .to_string(),
                            "GEN-18",
                        ));
                        continue;
                    }
                    // GEN-18, UNIT-8: this is a compile-time bool query
                    // of a function supplied at the call, not a cold constant.
                    if *ty != Type::Scalar(crate::types::Scalar::Bool) {
                        diagnostics.emit(error(
                            Category::Type,
                            codes::TYPE_MISMATCH,
                            Location::new(file, default.span()),
                            format!("a law predicate gives `bool`, and {place} is `{ty}`"),
                            "GEN-18",
                        ));
                    }
                    continue;
                }
                check(&default, ty, &place, diagnostics);
            }
            let condition = child(&declaration.syntax, SyntaxKind::RequiresClause)
                .and_then(|clause| clause.children().next());
            for term in condition
                .iter()
                .flat_map(|condition| condition.descendants())
                .filter(|node| node.kind() == SyntaxKind::ConstraintValue)
            {
                let Some(name) = crate::syntax::name_token(&term) else {
                    continue;
                };
                let generic = generics.iter().find(|generic| generic.name == name.text());
                let ordinary_parameter = matches!(module.details.get(index), Some(Detail::Callable(signature))
                    if signature.parameters.iter().any(|parameter| parameter.name == name.text()));
                if ordinary_parameter
                    || generic.is_some_and(|generic| {
                        !matches!(
                            &generic.kind,
                            GenericKind::Value(Type::Scalar(crate::types::Scalar::Bool))
                        )
                    })
                {
                    diagnostics.emit(error(
                        Category::Type,
                        codes::TYPE_MISMATCH,
                        Location::new(file, term.span()),
                        "a bare requirement term must name a bool generic value parameter"
                            .to_string(),
                        "GEN-3",
                    ));
                }
            }
            for relation in condition
                .iter()
                .flat_map(|condition| condition.descendants())
                .filter(|node| node.kind() == SyntaxKind::ConstraintRelation)
            {
                let operands: Vec<SyntaxNode> = relation.children().collect();
                let [left, right] = operands.as_slice() else {
                    continue;
                };
                // The generic value parameter a lone name is.
                let option = |operand: &SyntaxNode| {
                    (operand.kind() == SyntaxKind::NamedType)
                        .then(|| operand.text().to_string())
                        .and_then(|name| {
                            generics.iter().find(|generic| {
                                generic.name == name.trim()
                                    && matches!(generic.kind, GenericKind::Value(_))
                            })
                        })
                };
                let (generic, compared) = match (option(left), option(right)) {
                    (Some(generic), None) => (generic, right),
                    (None, Some(generic)) => (generic, left),
                    _ => continue,
                };
                let GenericKind::Value(ty) = &generic.kind else {
                    continue;
                };
                if names_parameter(ty) {
                    continue;
                }
                let place = format!("`{}`", generic.name);
                if compared.kind() == SyntaxKind::ConstraintSet {
                    for member in compared.children() {
                        check(&member, ty, &place, diagnostics);
                    }
                } else if has_token(&relation, SyntaxKind::EqEq) {
                    check(compared, ty, &place, diagnostics);
                }
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
fn option_kinds(program: &Program, diagnostics: &mut Diagnostics, tests: bool) {
    // The candidates of the program, each with its operator, by the
    // operator's module and declaration.
    let candidates = || {
        program
            .modules
            .iter()
            .enumerate()
            .flat_map(|(index, module)| {
                module
                    .declarations
                    .iter()
                    .enumerate()
                    .filter_map(move |(position, declaration)| {
                        let (_, operator) = declaration.kind.candidate()?;
                        let operator = operator?;
                        let scope = declaration.test_owner.map(|_| index);
                        if scope.is_some() && !tests {
                            return None;
                        }
                        match module.details.get(position) {
                            Some(Detail::Callable(signature)) => {
                                Some(((operator.module, operator.declaration), scope, signature))
                            }
                            _ => None,
                        }
                    })
            })
    };
    struct OptionDeclaration {
        /// The module whose tests see this candidate, when private to testing.
        scope: Option<usize>,
        at: Location,
    }
    // For each operator: the options declared as generic value parameters
    // and the scopes where their candidates are visible (GEN-6, TST-1).
    let mut selecting: HashMap<(usize, usize), HashMap<&str, Vec<OptionDeclaration>>> =
        HashMap::new();
    for (operator, scope, signature) in candidates() {
        for generic in &signature.generics {
            if matches!(generic.kind, GenericKind::Value(_)) {
                selecting
                    .entry(operator)
                    .or_default()
                    .entry(generic.name.as_str())
                    .or_default()
                    .push(OptionDeclaration {
                        scope,
                        at: generic.at,
                    });
            }
        }
    }
    if selecting.is_empty() {
        return;
    }
    // A second pass, in the order the candidates are declared, so that
    // what is reported comes in one order on every run.
    for (operator, scope, signature) in candidates() {
        let Some(names) = selecting.get(&operator) else {
            continue;
        };
        let public = &program.modules[operator.0].declarations[operator.1];
        let Some(Detail::Callable(signature_of_operator)) =
            program.modules[operator.0].details.get(operator.1)
        else {
            continue;
        };
        let own: std::collections::HashSet<_> = signature_of_operator
            .candidate_parameters(signature)
            .flatten()
            .collect();
        for (index, parameter) in signature.parameters.iter().enumerate() {
            if own.contains(&index) {
                continue;
            }
            let Some(other) = names.get(parameter.name.as_str()).and_then(|declared| {
                declared
                    .iter()
                    .find(|other| scope.is_none() || other.scope.is_none() || scope == other.scope)
            }) else {
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
                .with_related(other.at, "a generic value parameter here"),
            );
        }
    }
}

/// GEN-6 and FN-18: a candidate must declare its operator's
/// configuration parameters in braces. A temporal candidate cannot
/// make a declared const configuration into a temporal input.
fn configuration_shapes(program: &Program, diagnostics: &mut Diagnostics) {
    for module in &program.modules {
        for (index, candidate) in module.declarations.iter().enumerate() {
            let (Some((role, Some(operator))), Some(Detail::Callable(signature))) =
                (candidate.kind.candidate(), module.details.get(index))
            else {
                continue;
            };
            let Some(Detail::Callable(public)) = program.modules[operator.module]
                .details
                .get(operator.declaration)
            else {
                continue;
            };
            for (parameter, place) in public
                .parameters
                .iter()
                .zip(public.candidate_parameters(signature))
            {
                if !parameter.configuration {
                    continue;
                }
                let (at, message) = match place {
                    None => (
                        candidate.at,
                        format!(
                            "this candidate must declare `{}` in its configuration",
                            parameter.name
                        ),
                    ),
                    Some(index)
                        if role == Role::Temporal
                            && parameter.constant
                            && !signature.parameters[index].constant =>
                    {
                        (
                            signature.parameters[index].at,
                            format!(
                                "this candidate must declare configuration parameter `{}` as `const`",
                                parameter.name
                            ),
                        )
                    }
                    Some(_) => continue,
                };
                diagnostics.emit(
                    crate::local_error(
                        Category::Operator,
                        codes::local::IMPL_CONFIGURATION,
                        at,
                        message,
                        "GEN-6",
                    )
                    .with_related(parameter.at, "the operator's configuration parameter"),
                );
            }
        }
    }
}
