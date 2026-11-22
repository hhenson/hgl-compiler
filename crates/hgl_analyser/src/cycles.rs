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
//! graph. The graph is of the whole program. Modules do not import one
//! another (UNIT-3), and a value may still be worked out through one
//! that does not import it back: an operator's candidates are found
//! wherever they are declared (GEN-5), so a function of one module may
//! select a candidate of a module that imports it.
//!
//! A constant is also read where the typed program has only its value:
//! in a type, as a size or a generic value argument, given in angle
//! brackets or by name. So every name that is bound to a constant is an
//! edge to it too, from the part it is written in.
//!
//! One approximation is kept, on the safe side: a construction of a
//! struct counts every default of the struct's fields and of its
//! parents', given or not. What the typed program does not hold, the
//! body of an anonymous function, is followed only to the constants it
//! names.

use std::collections::HashMap;

use hgl_diagnostics::{Category, Diagnostics};
use hgl_parser::HasSpan;
use hgl_source::{FileId, Span};
use petgraph::algo::kosaraju_scc;
use petgraph::graph::{DiGraph, NodeIndex};

use crate::hir::{Callee, Expr, ExprKind, walk_statements};
use crate::program::{Binding, DeclarationKind, Program};
use crate::read::{default_of, parameters};
use crate::types::{Detail, Type};
use crate::{codes, error};

/// Reports each constant whose value depends on itself
/// (`constant.cycle`), at its name. It reads the typed program, so it
/// runs after the pass that types it.
pub(crate) fn check(program: &Program, diagnostics: &mut Diagnostics) {
    let has_constant = program.modules.iter().any(|module| {
        module
            .declarations
            .iter()
            .any(|declaration| declaration.kind == DeclarationKind::Constant)
    });
    if has_constant {
        cycles(program, diagnostics);
    }
}

/// A declaration of the program: the position of its module, and its
/// position among that module's declarations.
type Declared = (usize, usize);

/// A node of the graph: something a value may be worked out through.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
enum Part {
    /// The value of a constant.
    Constant(Declared),
    /// The body of a function.
    Body(Declared),
    /// The default of a parameter of a function: the function, and the
    /// parameter's position.
    Default(Declared, usize),
    /// The defaults of a struct's fields, and of its parents'.
    Fields(Declared),
}

impl Part {
    /// The declaration the part belongs to.
    fn declaration(self) -> Declared {
        match self {
            Part::Constant(declaration)
            | Part::Body(declaration)
            | Part::Default(declaration, _)
            | Part::Fields(declaration) => declaration,
        }
    }
}

/// Adds to `reads` each part of `program` that working out the
/// expression `visited` reads directly.
fn reads_of(program: &Program, visited: &Expr, reads: &mut Vec<Part>) {
    match &visited.kind {
        ExprKind::Constant {
            module,
            declaration,
        } => reads.push(Part::Constant((*module, *declaration))),
        ExprKind::Call {
            callee:
                Callee::Function {
                    module,
                    declaration,
                    ..
                }
                | Callee::Candidate {
                    module,
                    declaration,
                    ..
                },
            arguments,
            ..
        } => {
            let called = (*module, *declaration);
            reads.push(Part::Body(called));
            // UNIT-8: the call reads the default of each parameter it
            // leaves out. An argument says which parameter it is given
            // to.
            let defaults = program.modules[*module]
                .defaults
                .get(*declaration)
                .into_iter()
                .flatten();
            for (position, default) in defaults.enumerate() {
                let given = arguments
                    .iter()
                    .any(|argument| argument.parameter == Some(position));
                if default.is_some() && !given {
                    reads.push(Part::Default(called, position));
                }
            }
        }
        ExprKind::Construct {
            module,
            declaration,
            ..
        } => reads.push(Part::Fields((*module, *declaration))),
        _ => {}
    }
}

/// Each name of module `index` of `program` that is bound to a
/// constant, of that module or of one it imports: the part it is
/// written in, and the constant.
///
/// A name read as a value is in the typed program already. This adds
/// the ones the typed program has folded to their values: a size, and a
/// generic value argument (GEN-8).
fn named_constants(program: &Program, index: usize) -> Vec<(Part, Vec<Part>)> {
    let module = &program.modules[index];
    // Within each file, order source ranges separately for ordinary
    // declarations and example-local declarations. An example's code
    // shares the positions of its owner's comment (SDOC-6).
    let mut written = HashMap::new();
    for (position, declaration) in module.declarations.iter().enumerate() {
        let in_example = declaration
            .test_owner
            .is_some_and(|test| module.tests[test].documentation_example);
        written
            .entry((module.files[declaration.file].file, in_example))
            .or_insert_with(Vec::new)
            .push((declaration.syntax.span(), position));
    }
    for declarations in written.values_mut() {
        declarations.sort_unstable();
    }
    // Where each function writes the default of each of its parameters.
    let defaults: Vec<Vec<Option<Span>>> = module
        .declarations
        .iter()
        .map(|declaration| {
            parameters(&declaration.syntax)
                .iter()
                .map(|parameter| default_of(parameter).map(|default| default.span()))
                .collect()
        })
        .collect();
    // The part a name at `at` of `file` is written in.
    let owner = |file: FileId, at: u32, in_example: bool| -> Option<Part> {
        let declarations = written.get(&(file, in_example))?;
        // `partition_point` gives how many start at or before `at`: the
        // last of them is the only one that can hold it.
        let before = declarations.partition_point(|&(span, _)| span.start <= at);
        let &(span, position) = declarations.get(before.checked_sub(1)?)?;
        if at >= span.end {
            return None;
        }
        let declared = (index, position);
        if module.declarations[position].kind == DeclarationKind::Constant {
            return Some(Part::Constant(declared));
        }
        if matches!(module.details.get(position), Some(Detail::Struct(_))) {
            return Some(Part::Fields(declared));
        }
        let default = defaults[position]
            .iter()
            .position(|default| default.is_some_and(|span| span.start <= at && at < span.end));
        Some(match default {
            Some(parameter) => Part::Default(declared, parameter),
            None => Part::Body(declared),
        })
    };

    let mut found = Vec::new();
    for used in &module.uses {
        let Binding::Declarations {
            module: bound,
            declarations,
        } = &used.binding
        else {
            continue;
        };
        let constants: Vec<Part> = declarations
            .iter()
            .filter(|&&declaration| {
                program.modules[*bound].declarations[declaration].kind == DeclarationKind::Constant
            })
            .map(|&declaration| Part::Constant((*bound, declaration)))
            .collect();
        if constants.is_empty() {
            continue;
        }
        if let Some(part) = owner(used.at.file, used.at.span.start, used.documentation_example) {
            found.push((part, constants));
        }
    }
    found
}

/// Adds to `parts` each part of module `index` of `program`, with the
/// parts it reads.
fn parts_of(program: &Program, index: usize, parts: &mut Vec<(Part, Vec<Part>)>) {
    let module = &program.modules[index];
    // What each expression of a part reads, gathered by walking it.
    let gather = |expressions: &[&Expr]| {
        let mut reads = Vec::new();
        for expression in expressions {
            expression.walk(&mut |visited| reads_of(program, visited, &mut reads));
        }
        reads
    };
    for position in 0..module.declarations.len() {
        let declared = (index, position);
        if let Some(Some(value)) = module.constants.get(position) {
            parts.push((Part::Constant(declared), gather(&[&value.value])));
        }
        if let Some(Some(body)) = module.bodies.get(position) {
            let mut reads = Vec::new();
            walk_statements(&body.statements, &mut |visited| {
                reads_of(program, visited, &mut reads);
            });
            parts.push((Part::Body(declared), reads));
        }
        let defaults = module.defaults.get(position).map_or(&[][..], Vec::as_slice);
        match module.details.get(position) {
            Some(Detail::Struct(read)) => {
                // A struct's field defaults are one part, which reads
                // its parents' too (TYPE-4).
                let written: Vec<&Expr> = defaults
                    .iter()
                    .flatten()
                    .map(|default| &default.value)
                    .collect();
                let mut reads = gather(&written);
                for parent in &read.parents {
                    if let Type::Named {
                        module,
                        declaration,
                        ..
                    } = parent
                    {
                        reads.push(Part::Fields((*module, *declaration)));
                    }
                }
                parts.push((Part::Fields(declared), reads));
            }
            _ => {
                for (parameter, default) in defaults.iter().enumerate() {
                    if let Some(default) = default {
                        parts.push((
                            Part::Default(declared, parameter),
                            gather(&[&default.value]),
                        ));
                    }
                }
            }
        }
    }

    // A constant named where the typed program keeps only its value: in
    // a type, as a size or a generic value argument.
    parts.extend(named_constants(program, index));
}

/// The cycles of `program`.
fn cycles(program: &Program, diagnostics: &mut Diagnostics) {
    // Each part of each module, with the parts it reads.
    let mut parts: Vec<(Part, Vec<Part>)> = Vec::new();
    for index in 0..program.modules.len() {
        parts_of(program, index, &mut parts);
    }

    // A node for each part, made when it is first met: a part nothing
    // was gathered for, a function with no body say, reads nothing.
    let mut graph: DiGraph<Part, ()> = DiGraph::new();
    let mut nodes: HashMap<Part, NodeIndex> = HashMap::new();
    let mut node = |graph: &mut DiGraph<Part, ()>, part: Part| -> NodeIndex {
        *nodes.entry(part).or_insert_with(|| graph.add_node(part))
    };
    for (part, reads) in &parts {
        let from = node(&mut graph, *part);
        for read in reads {
            let target = node(&mut graph, *read);
            // `update_edge` adds the edge once however often it is read.
            graph.update_edge(from, target, ());
        }
    }

    // Parts that read one another are a strongly connected group of the
    // graph. The findings are gathered and then given in the order the
    // constants are declared, whatever order the groups are found in.
    let mut findings: Vec<(Declared, String)> = Vec::new();
    for group in kosaraju_scc(&graph) {
        // One part is a cycle only when it reads itself.
        let cyclic = group.len() > 1 || graph.contains_edge(group[0], group[0]);
        if !cyclic {
            continue;
        }
        // The declarations the group's parts belong to.
        let mut members: Vec<Declared> = group
            .iter()
            .map(|&part| graph[part].declaration())
            .collect();
        members.sort_unstable();
        members.dedup();
        for &part in &group {
            let Part::Constant(member) = graph[part] else {
                continue;
            };
            let name = &program.modules[member.0].declarations[member.1].name;
            // Another declaration of the constant's module is given by
            // its name, and one of another module with that module's.
            let through: Vec<String> = members
                .iter()
                .filter(|&&other| other != member)
                .map(|&(module, declaration)| {
                    let other = &program.modules[module];
                    let name = &other.declarations[declaration].name;
                    if module == member.0 {
                        format!("`{name}`")
                    } else {
                        format!("`{name}` of `{}`", other.path)
                    }
                })
                .collect();
            let message = if through.is_empty() {
                format!("the value of `{name}` depends on itself")
            } else {
                format!(
                    "the value of `{name}` depends on itself, through {}",
                    through.join(" and ")
                )
            };
            findings.push((member, message));
        }
    }
    findings.sort();
    for ((module, declaration), message) in findings {
        diagnostics.emit(error(
            Category::Name,
            codes::CONSTANT_CYCLE,
            program.modules[module].declarations[declaration].at,
            message,
            "UNIT-13",
        ));
    }
}
