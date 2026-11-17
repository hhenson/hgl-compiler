//! Constants whose value depends on itself (UNIT-13).
//!
//! > It is referenced forward as any module declaration is (UNIT-6), and
//! > its value depends on no constant that depends on it.
//!
//! What a constant's value depends on is what its expression names, and
//! what each constant, function or struct named there names in turn: a
//! constant read in the body of a function the expression calls is one
//! the value depends on. So the question is asked of the module's
//! declarations as a whole. Each is a node of a graph, with an edge to
//! every declaration of the module it names, and a constant depends on
//! itself when it is in a cycle of that graph.
//!
//! A function's parameter defaults are nodes of their own. A call reads
//! the default of a parameter only where it leaves that parameter out
//! (UNIT-8), so `const answer: i64 = choose(1)` does not depend on what
//! the default of `choose`'s parameter names.
//!
//! Modules do not import one another (UNIT-3), so a cycle is within one
//! module, and each module is looked at alone.

use std::collections::HashMap;

use hgl_diagnostics::{Category, Diagnostics};
use hgl_parser::{HasSpan, SyntaxKind, SyntaxNode};
use hgl_source::{FileId, Span};
use petgraph::algo::kosaraju_scc;
use petgraph::graph::{DiGraph, NodeIndex};

use crate::program::{Binding, DeclarationKind, Module, Program, Role};
use crate::read::{child, default_of, has_token, parameters, tokens};
use crate::{codes, error};

/// Reports each constant whose value depends on itself
/// (`constant.cycle`), at its name.
pub(crate) fn check(program: &Program, diagnostics: &mut Diagnostics) {
    for (index, module) in program.modules.iter().enumerate() {
        let has_constant = module
            .declarations
            .iter()
            .any(|declaration| declaration.kind == DeclarationKind::Constant);
        if has_constant {
            cycles(index, module, diagnostics);
        }
    }
}

/// A node of the graph: something a value may be worked out through.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
enum Part {
    /// The declaration at this position: all of it, but the defaults of
    /// its parameters where it is a function.
    Declaration(usize),
    /// The default of a parameter of a function: the function's
    /// position, and the parameter's.
    Default(usize, usize),
}

/// A parameter of a function, as far as a call's use of its default
/// goes.
struct Parameter {
    /// Its name.
    name: Option<String>,
    /// True for a pack, `...T` (FN-8), which takes every argument the
    /// parameters before it leave.
    pack: bool,
    /// Where its default is written, when it has one.
    default: Option<Span>,
}

/// The parameters of the function `declaration` is; empty for a
/// declaration that is no function.
fn parameters_of(declaration: &SyntaxNode) -> Vec<Parameter> {
    if !matches!(
        declaration.kind(),
        SyntaxKind::FnDecl | SyntaxKind::NativeFnDecl | SyntaxKind::OperatorDecl
    ) {
        return Vec::new();
    }
    parameters(declaration)
        .iter()
        .map(|parameter| Parameter {
            // The name is the last identifier before the `:`; `const`
            // may stand before it.
            name: tokens(parameter)
                .take_while(|token| token.kind() != SyntaxKind::Colon)
                .filter(|token| token.kind() == SyntaxKind::Ident)
                .last()
                .map(|name| name.text().to_string()),
            pack: has_token(parameter, SyntaxKind::Ellipsis),
            default: default_of(parameter).map(|default| default.span()),
        })
        .collect()
}

/// What a call gives: how many arguments by position, and which by
/// name.
struct Given {
    positional: usize,
    named: Vec<String>,
}

/// What the call whose callee is the name at `at` gives, in the
/// declaration `within`; `None` when the name is not the callee of a
/// call written out, and so may be used any way.
fn call_at(within: &SyntaxNode, at: u32) -> Option<Given> {
    let name = within
        .descendants_with_tokens()
        .filter_map(|element| element.into_token())
        .find(|token| token.kind() == SyntaxKind::Ident && token.span().start == at)?;
    let mut holder = name.parent()?;
    if holder.kind() == SyntaxKind::QualifiedName {
        holder = holder.parent()?;
    }
    let list = match holder.kind() {
        // `name(...)`: the name is what the call calls.
        SyntaxKind::NameExpr => {
            let call = holder.parent()?;
            let callee = call.children().next()?;
            (call.kind() == SyntaxKind::CallExpr && callee == holder)
                .then(|| child(&call, SyntaxKind::ArgList))
                .flatten()?
        }
        // `name<...>(...)`: the name stands in the call itself.
        SyntaxKind::TypedConstructor | SyntaxKind::UndecidedConstructor => {
            child(&holder, SyntaxKind::ArgList)?
        }
        _ => return None,
    };
    let mut given = Given {
        positional: 0,
        named: Vec::new(),
    };
    for argument in list
        .children()
        .filter(|argument| argument.kind() == SyntaxKind::Arg)
    {
        // `name: value`: the name is the first token (LEX-9).
        match tokens(&argument).next() {
            Some(label) if has_token(&argument, SyntaxKind::Colon) => {
                given.named.push(label.text().to_string());
            }
            _ => given.positional += 1,
        }
    }
    Some(given)
}

/// The cycles of module `index`, which is `module`.
fn cycles(index: usize, module: &Module, diagnostics: &mut Diagnostics) {
    // Where each declaration is written: for each file, the start, the
    // end and the position of each of its declarations, in order of
    // start. No declaration is written inside another.
    let mut written: HashMap<FileId, Vec<(u32, u32, usize)>> = HashMap::new();
    for (position, declaration) in module.declarations.iter().enumerate() {
        let span = declaration.syntax.span();
        written
            .entry(module.files[declaration.file].file)
            .or_default()
            .push((span.start, span.end, position));
    }
    for declarations in written.values_mut() {
        declarations.sort_unstable();
    }
    let parameters: Vec<Vec<Parameter>> = module
        .declarations
        .iter()
        .map(|declaration| parameters_of(&declaration.syntax))
        .collect();
    // The part a name at `at` of `file` is written in: a declaration,
    // or the default of one of its parameters.
    let owner = |file: FileId, at: u32| -> Option<Part> {
        let declarations = written.get(&file)?;
        // `partition_point` gives how many start at or before `at`: the
        // last of them is the only one that can hold it.
        let before = declarations.partition_point(|&(start, _, _)| start <= at);
        let &(_, end, position) = declarations.get(before.checked_sub(1)?)?;
        if at >= end {
            return None;
        }
        let default = parameters[position].iter().position(|parameter| {
            parameter
                .default
                .is_some_and(|default| default.start <= at && at < default.end)
        });
        Some(match default {
            Some(parameter) => Part::Default(position, parameter),
            None => Part::Declaration(position),
        })
    };

    // A node for each part, made when it is first named.
    let mut graph: DiGraph<Part, ()> = DiGraph::new();
    let mut nodes: HashMap<Part, NodeIndex> = HashMap::new();
    let mut node = |graph: &mut DiGraph<Part, ()>, part: Part| -> NodeIndex {
        *nodes.entry(part).or_insert_with(|| graph.add_node(part))
    };
    for position in 0..module.declarations.len() {
        node(&mut graph, Part::Declaration(position));
    }
    for used in &module.uses {
        let Binding::Declarations {
            module: found,
            declarations,
        } = &used.binding
        else {
            continue;
        };
        if *found != index {
            continue;
        }
        let Some(from) = owner(used.at.file, used.at.span.start) else {
            continue;
        };
        let within = match from {
            Part::Declaration(position) | Part::Default(position, _) => position,
        };
        // FN-4: a name may have a temporal and a value definition, and
        // "runtime/value evaluation chooses the value definition". What
        // a constant's value is worked out through is value evaluation:
        // a constant, a value function, a default. There the temporal
        // definition of such a pair is not the one named.
        let value_context = module.declarations[within].kind.role() != Some(Role::Temporal);
        let paired = declarations.len() > 1;
        let from = node(&mut graph, from);
        for &to in declarations {
            if paired
                && value_context
                && module.declarations[to].kind.role() == Some(Role::Temporal)
            {
                continue;
            }
            // `update_edge` adds the edge once however often it is named.
            let target = node(&mut graph, Part::Declaration(to));
            graph.update_edge(from, target, ());
            // UNIT-8: a call reads the default of each parameter it
            // leaves out, and of no other.
            let declared = &parameters[to];
            if declared.iter().all(|parameter| parameter.default.is_none()) {
                continue;
            }
            // A pack takes what the parameters before it leave, so what
            // a call gives by position says nothing of them; and a name
            // that is not called may be used any way. Every default is
            // then taken to be read.
            let given = call_at(&module.declarations[within].syntax, used.at.span.start)
                .filter(|_| declared.iter().all(|parameter| !parameter.pack));
            for (position, parameter) in declared.iter().enumerate() {
                if parameter.default.is_none() {
                    continue;
                }
                let supplied = given.as_ref().is_some_and(|given| {
                    position < given.positional
                        || parameter
                            .name
                            .as_ref()
                            .is_some_and(|name| given.named.contains(name))
                });
                if !supplied {
                    let default = node(&mut graph, Part::Default(to, position));
                    graph.update_edge(from, default, ());
                }
            }
        }
    }

    // Parts that name one another are a strongly connected group of the
    // graph.
    for group in kosaraju_scc(&graph) {
        // One part is a cycle only when it names itself.
        let cyclic = group.len() > 1 || graph.contains_edge(group[0], group[0]);
        if !cyclic {
            continue;
        }
        // The declarations the group's parts belong to.
        let mut members: Vec<usize> = group
            .iter()
            .map(|&part| match graph[part] {
                Part::Declaration(position) | Part::Default(position, _) => position,
            })
            .collect();
        members.sort_unstable();
        members.dedup();
        for &part in &group {
            let Part::Declaration(member) = graph[part] else {
                continue;
            };
            let declaration = &module.declarations[member];
            if declaration.kind != DeclarationKind::Constant {
                continue;
            }
            let through: Vec<String> = members
                .iter()
                .filter(|&&other| other != member)
                .map(|&other| format!("`{}`", module.declarations[other].name))
                .collect();
            let message = if through.is_empty() {
                format!("the value of `{}` depends on itself", declaration.name)
            } else {
                format!(
                    "the value of `{}` depends on itself, through {}",
                    declaration.name,
                    through.join(" and ")
                )
            };
            diagnostics.emit(error(
                Category::Name,
                codes::CONSTANT_CYCLE,
                declaration.at,
                message,
                "UNIT-13",
            ));
        }
    }
}
