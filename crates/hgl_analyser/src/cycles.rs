//! Constants whose value depends on itself (UNIT-13).
//!
//! > It is referenced forward as any module declaration is (UNIT-6), and
//! > its value depends on no constant that depends on it.
//!
//! What a constant's value depends on is what working it out reads: the
//! constants its expression names, the functions it calls, and what each
//! of those reads in turn. That is asked of the typed program, where
//! every call has the definition it selected:
//!
//! * of a temporal and a value definition of one name, the one the call
//!   chose (FN-4);
//! * of an operator's candidates, the one the operands selected, whether
//!   the operator is called by name or written as a symbol (OPS-2);
//! * of a function's parameter defaults, those of the parameters the
//!   call leaves out, and no other (UNIT-8).
//!
//! Each such thing is a node of a graph, with an edge to every other it
//! reads, and a constant depends on itself when it is in a cycle of that
//! graph. Modules do not import one another (UNIT-3), so a cycle is
//! within one module, and each module is looked at alone.
//!
//! One approximation is kept, on the safe side: a construction of a
//! struct counts every default of the struct's fields and of its
//! parents', given or not. What the typed program does not hold, the
//! body of an anonymous function, is not followed.

use std::collections::HashMap;

use hgl_diagnostics::{Category, Diagnostics};
use petgraph::algo::kosaraju_scc;
use petgraph::graph::{DiGraph, NodeIndex};

use crate::hir::{Callee, Expr, ExprKind, walk_statements};
use crate::program::{DeclarationKind, Module, Program};
use crate::types::{Detail, Type};
use crate::{codes, error};

/// Reports each constant whose value depends on itself
/// (`constant.cycle`), at its name. It reads the typed program, so it
/// runs after the pass that types it.
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
/// Each names a declaration by its position in the module.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
enum Part {
    /// The value of a constant.
    Constant(usize),
    /// The body of a function.
    Body(usize),
    /// The default of a parameter of a function: the function, and the
    /// parameter's position.
    Default(usize, usize),
    /// The defaults of a struct's fields, and of its parents'.
    Fields(usize),
}

impl Part {
    /// The declaration the part belongs to.
    fn declaration(self) -> usize {
        match self {
            Part::Constant(declaration)
            | Part::Body(declaration)
            | Part::Default(declaration, _)
            | Part::Fields(declaration) => declaration,
        }
    }
}

/// Adds to `reads` each part of module `index`, which is `module`, that
/// working out the expression `visited` reads directly.
fn reads_of(index: usize, module: &Module, visited: &Expr, reads: &mut Vec<Part>) {
    match &visited.kind {
        ExprKind::Constant {
            module: found,
            declaration,
        } if *found == index => reads.push(Part::Constant(*declaration)),
        ExprKind::Call {
            callee:
                Callee::Function {
                    module: found,
                    declaration,
                    ..
                }
                | Callee::Candidate {
                    module: found,
                    declaration,
                    ..
                },
            arguments,
            ..
        } if *found == index => {
            reads.push(Part::Body(*declaration));
            // UNIT-8: the call reads the default of each parameter it
            // leaves out. An argument says which parameter it is given
            // to.
            let defaults = module.defaults.get(*declaration).into_iter().flatten();
            for (position, default) in defaults.enumerate() {
                let given = arguments
                    .iter()
                    .any(|argument| argument.parameter == Some(position));
                if default.is_some() && !given {
                    reads.push(Part::Default(*declaration, position));
                }
            }
        }
        ExprKind::Construct {
            module: found,
            declaration,
            ..
        } if *found == index => reads.push(Part::Fields(*declaration)),
        _ => {}
    }
}

/// The cycles of module `index`, which is `module`.
fn cycles(index: usize, module: &Module, diagnostics: &mut Diagnostics) {
    // Each part, with the parts it reads.
    let mut parts: Vec<(Part, Vec<Part>)> = Vec::new();
    for position in 0..module.declarations.len() {
        // What each expression of a part reads, gathered by walking it.
        let gather = |expressions: &[&Expr]| {
            let mut reads = Vec::new();
            for expression in expressions {
                expression.walk(&mut |visited| reads_of(index, module, visited, &mut reads));
            }
            reads
        };
        if let Some(Some(value)) = module.constants.get(position) {
            parts.push((Part::Constant(position), gather(&[value])));
        }
        if let Some(Some(body)) = module.bodies.get(position) {
            let mut reads = Vec::new();
            walk_statements(&body.statements, &mut |visited| {
                reads_of(index, module, visited, &mut reads);
            });
            parts.push((Part::Body(position), reads));
        }
        let defaults = module.defaults.get(position).map_or(&[][..], Vec::as_slice);
        match module.details.get(position) {
            Some(Detail::Struct(read)) => {
                // A struct's field defaults are one part, which reads
                // its parents' too (TYPE-4).
                let written: Vec<&Expr> = defaults.iter().flatten().collect();
                let mut reads = gather(&written);
                for parent in &read.parents {
                    if let Type::Named {
                        module: found,
                        declaration,
                        ..
                    } = parent
                        && *found == index
                    {
                        reads.push(Part::Fields(*declaration));
                    }
                }
                parts.push((Part::Fields(position), reads));
            }
            _ => {
                for (parameter, default) in defaults.iter().enumerate() {
                    if let Some(default) = default {
                        parts.push((Part::Default(position, parameter), gather(&[default])));
                    }
                }
            }
        }
    }

    let mut graph: DiGraph<Part, ()> = DiGraph::new();
    let mut nodes: HashMap<Part, NodeIndex> = HashMap::new();
    for (part, _) in &parts {
        nodes.insert(*part, graph.add_node(*part));
    }
    for (part, reads) in &parts {
        for read in reads {
            // A part nothing was gathered for reads nothing: a function
            // with no body, say.
            if let Some(&target) = nodes.get(read) {
                // `update_edge` adds the edge once however often it is
                // read.
                graph.update_edge(nodes[part], target, ());
            }
        }
    }

    // Parts that read one another are a strongly connected group of the
    // graph.
    for group in kosaraju_scc(&graph) {
        // One part is a cycle only when it reads itself.
        let cyclic = group.len() > 1 || graph.contains_edge(group[0], group[0]);
        if !cyclic {
            continue;
        }
        // The declarations the group's parts belong to.
        let mut members: Vec<usize> = group
            .iter()
            .map(|&part| graph[part].declaration())
            .collect();
        members.sort_unstable();
        members.dedup();
        for &part in &group {
            let Part::Constant(member) = graph[part] else {
                continue;
            };
            let declaration = &module.declarations[member];
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
