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
//! Modules do not import one another (UNIT-3), so a cycle is within one
//! module, and each module is looked at alone.

use std::collections::HashMap;

use hgl_diagnostics::{Category, Diagnostics};
use hgl_parser::HasSpan;
use hgl_source::FileId;
use petgraph::algo::kosaraju_scc;
use petgraph::graph::{DiGraph, NodeIndex};

use crate::program::{Binding, DeclarationKind, Module, Program};
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
    // The declaration a name at `at` of `file` is written in.
    let owner = |file: FileId, at: u32| -> Option<usize> {
        let declarations = written.get(&file)?;
        // `partition_point` gives how many start at or before `at`: the
        // last of them is the only one that can hold it.
        let before = declarations.partition_point(|&(start, _, _)| start <= at);
        let &(_, end, position) = declarations.get(before.checked_sub(1)?)?;
        (at < end).then_some(position)
    };

    // A node for each declaration, and an edge to each declaration of
    // this module it names.
    let mut graph: DiGraph<usize, ()> = DiGraph::new();
    let nodes: Vec<NodeIndex> = (0..module.declarations.len())
        .map(|position| graph.add_node(position))
        .collect();
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
        for &to in declarations {
            // `update_edge` adds the edge once however often it is named.
            graph.update_edge(nodes[from], nodes[to], ());
        }
    }

    // Declarations that name one another are a strongly connected group
    // of the graph.
    for group in kosaraju_scc(&graph) {
        let mut members: Vec<usize> = group.iter().map(|&node| graph[node]).collect();
        members.sort_unstable();
        // One declaration is a cycle only when it names itself.
        let cyclic = members.len() > 1 || graph.contains_edge(group[0], group[0]);
        if !cyclic {
            continue;
        }
        for &member in &members {
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
