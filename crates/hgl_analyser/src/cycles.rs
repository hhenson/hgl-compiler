//! Constants and defaults that depend on themselves (UNIT-8, UNIT-13).
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
//!   call leaves out (UNIT-8);
//! * of a callable parameter, the static function the caller supplied
//!   where the body invokes it (FN-17);
//! * of a struct's field defaults, the default selected for each field
//!   the constructor leaves out, using the concrete keys of a forwarded
//!   argument pack (TYPE-5, FN-19).
//!
//! Each such thing is a node of a graph, with an edge to every other it
//! reads, and a constant depends on itself when it is in a cycle of that
//! graph. A default that needs itself without relying on a constant
//! is reported at its expression. The graph is of the whole program.
//! Modules do not import one another (UNIT-3), and a value may still
//! be worked out through one that does not import it back. An operator's
//! candidates are found wherever they are declared (GEN-5), so a function
//! of one module may select a candidate of a module that imports it.
//!
//! A constant is also read where the typed program has only its value:
//! in a type, as a size or a generic value argument, given in angle
//! brackets or by name. So every name that is bound to a constant is an
//! edge to it too, from the part it is written in.
//!
//! An anonymous argument contributes its call dependencies where a body
//! invokes it. Its written constant names are also read by UNIT-13.

mod callbacks;

use std::collections::{HashMap, HashSet, VecDeque};

use hgl_diagnostics::{Category, Code, Diagnostics};
use hgl_parser::HasSpan;
use hgl_source::{FileId, Span};
use petgraph::algo::kosaraju_scc;
use petgraph::graph::{DiGraph, NodeIndex};
use petgraph::visit::NodeFiltered;

use crate::hir::{Callee, Expr, ExprKind, walk_statements};
use crate::program::{Binding, DeclarationKind, Program, Role};
use crate::read::{default_of, parameters};
use crate::types::{Detail, Type};
use crate::values::{ForwardingInstance, has_forwarded_calls};
use crate::{codes, error};
use callbacks::CallbackUses;

/// Reports cyclic constants at their names and cyclic defaults at their
/// expressions. It reads the typed program, after the pass that types it.
pub(crate) fn check(program: &Program, diagnostics: &mut Diagnostics) {
    let has_constant = program.modules.iter().any(|module| {
        module
            .declarations
            .iter()
            .any(|declaration| declaration.kind == DeclarationKind::Constant)
    });
    let has_default = program
        .modules
        .iter()
        .any(|module| module.defaults.iter().flatten().any(Option::is_some));
    if has_constant || has_default {
        cycles(program, diagnostics, has_constant, has_default);
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
    /// The body of a function under one caller's concrete pack shape.
    PackBody(Declared, usize),
    /// The body of a function with its called arguments' dependencies.
    BoundBody(Declared, usize),
    /// The default of a parameter of a function: the function, and the
    /// parameter's position.
    Default(Declared, usize),
    /// A struct's declaration, including the types of its fields and parents.
    Struct(Declared),
    /// The default of a field a struct writes, by its declared position.
    FieldDefault(Declared, usize),
}

impl Part {
    /// The declaration the part belongs to.
    fn declaration(self) -> Declared {
        match self {
            Part::Constant(declaration)
            | Part::Body(declaration)
            | Part::PackBody(declaration, _)
            | Part::BoundBody(declaration, _)
            | Part::Default(declaration, _)
            | Part::Struct(declaration)
            | Part::FieldDefault(declaration, _) => declaration,
        }
    }
}

/// Value bodies that can reach a call awaiting concrete pack inputs.
/// Follow callers in reverse so an ordinary generic wrapper is included.
fn forwarding_bodies(program: &Program) -> HashSet<Declared> {
    let value_bodies = program
        .modules
        .iter()
        .enumerate()
        .flat_map(|(module, unit)| {
            unit.bodies
                .iter()
                .enumerate()
                .filter_map(move |(declaration, body)| {
                    let body = body.as_ref()?;
                    (unit.declarations[declaration].kind.role() == Some(Role::Value))
                        .then_some(((module, declaration), body))
                })
        });
    let mut forwarding: HashSet<_> = value_bodies
        .clone()
        .filter_map(|(owner, body)| has_forwarded_calls(body).then_some(owner))
        .collect();
    if forwarding.is_empty() {
        return forwarding;
    }
    let mut callers = HashMap::<Declared, Vec<Declared>>::new();
    for (owner, body) in value_bodies {
        walk_statements(&body.statements, &mut |value| {
            if let ExprKind::Call { callee, .. } = &value.kind
                && let Some(called) = callee.declaration()
            {
                callers.entry(called).or_default().push(owner);
            }
        });
    }
    let mut waiting: Vec<_> = forwarding.iter().copied().collect();
    while let Some(called) = waiting.pop() {
        for &owner in callers.get(&called).into_iter().flatten() {
            if forwarding.insert(owner) {
                waiting.push(owner);
            }
        }
    }
    forwarding
}

/// Concrete bodies are separate graph nodes. A default read by one pack
/// shape must not become a dependency of another shape of the same call.
#[derive(Default)]
struct PackBodies {
    forwarding: HashSet<Declared>,
    instances: HashMap<Declared, Vec<ForwardingInstance>>,
    waiting: VecDeque<(Declared, usize)>,
    bound: HashMap<Declared, Vec<Vec<Part>>>,
}

impl PackBodies {
    fn body_of(&mut self, program: &Program, called: Declared, expression: &Expr) -> Part {
        if !self.forwarding.contains(&called) {
            return Part::Body(called);
        }
        let Some(instance) = ForwardingInstance::of(program, expression) else {
            return Part::Body(called);
        };
        let instances = self.instances.entry(called).or_default();
        if let Some(index) = instances.iter().position(|known| known == &instance) {
            return Part::PackBody(called, index);
        }
        let index = instances.len();
        instances.push(instance);
        self.waiting.push_back((called, index));
        Part::PackBody(called, index)
    }

    fn bound_body(&mut self, called: Declared, body: Part, mut reads: Vec<Part>) -> Part {
        if reads.is_empty() {
            return body;
        }
        reads.push(body);
        reads.sort_unstable();
        reads.dedup();
        let bodies = self.bound.entry(called).or_default();
        if let Some(index) = bodies.iter().position(|known| *known == reads) {
            return Part::BoundBody(called, index);
        }
        let index = bodies.len();
        bodies.push(reads);
        Part::BoundBody(called, index)
    }

    fn add_parts(
        &mut self,
        program: &Program,
        callbacks: &CallbackUses,
        parts: &mut Vec<(Part, Vec<Part>)>,
    ) {
        while let Some((declaration, index)) = self.waiting.pop_front() {
            // Keep the declaration's folded constant references, and use
            // the checked body for edges that depend on the pack shape.
            let mut reads = vec![Part::Body(declaration)];
            if let Some(body) = self.instances[&declaration][index].body(program) {
                walk_statements(&body.statements, &mut |visited| {
                    reads_of(program, visited, callbacks, self, &mut reads);
                });
            }
            parts.push((Part::PackBody(declaration, index), reads));
        }
        for (declaration, bodies) in self.bound.drain() {
            for (index, reads) in bodies.into_iter().enumerate() {
                parts.push((Part::BoundBody(declaration, index), reads));
            }
        }
    }
}

/// Adds to `reads` each part of `program` that working out the
/// expression `visited` reads directly.
fn reads_of(
    program: &Program,
    visited: &Expr,
    callbacks: &CallbackUses,
    packs: &mut PackBodies,
    reads: &mut Vec<Part>,
) {
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
            let body = packs.body_of(program, called, visited);
            let mut callback_reads = Vec::new();
            for (target, supplied) in callbacks.of_call(called, arguments) {
                if let Some(target) = target.declaration() {
                    callback_reads.push(Part::Body(target));
                    read_parameter_defaults(
                        program,
                        target,
                        |position| supplied.contains(&position),
                        &mut callback_reads,
                    );
                } else if let Callee::Anonymous(function) = target {
                    walk_statements(&function.body.statements, &mut |value| {
                        reads_of(program, value, callbacks, packs, &mut callback_reads);
                    });
                }
            }
            reads.push(packs.bound_body(called, body, callback_reads));
            read_parameter_defaults(
                program,
                called,
                |position| {
                    arguments
                        .iter()
                        .any(|argument| argument.parameter == Some(position))
                },
                reads,
            );
        }
        ExprKind::Call {
            callee: Callee::Anonymous(function),
            ..
        } => {
            walk_statements(&function.body.statements, &mut |value| {
                reads_of(program, value, callbacks, packs, reads);
            });
        }
        ExprKind::Construct {
            module,
            declaration,
            arguments,
            ..
        } => {
            reads.push(Part::Struct((*module, *declaration)));
            let Some(Detail::Struct(structure)) =
                program.modules[*module].details.get(*declaration)
            else {
                return;
            };
            // TYPE-5: a constructor reads the selected default for each
            // omitted field, including a default it takes from a parent.
            for (position, member) in structure.members.iter().enumerate() {
                if let Some(default) = member.default
                    && !arguments
                        .iter()
                        .any(|argument| argument.parameter == Some(position))
                {
                    reads.push(Part::FieldDefault(
                        (default.module, default.declaration),
                        default.field,
                    ));
                }
            }
        }
        _ => {}
    }
}

/// UNIT-8: a call reads the default of each parameter it leaves out.
fn read_parameter_defaults(
    program: &Program,
    called: Declared,
    supplied: impl Fn(usize) -> bool,
    reads: &mut Vec<Part>,
) {
    let defaults = program.modules[called.0]
        .defaults
        .get(called.1)
        .into_iter()
        .flatten();
    for (position, default) in defaults.enumerate() {
        if default.is_some() && !supplied(position) {
            reads.push(Part::Default(called, position));
        }
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
        if let Some(Detail::Struct(structure)) = module.details.get(position) {
            let field = structure.fields.iter().position(|field| {
                field
                    .default_at
                    .is_some_and(|default| default.span.start <= at && at < default.span.end)
            });
            return Some(match field {
                Some(field) => Part::FieldDefault(declared, field),
                None => Part::Struct(declared),
            });
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
fn parts_of(
    program: &Program,
    index: usize,
    callbacks: &CallbackUses,
    packs: &mut PackBodies,
    parts: &mut Vec<(Part, Vec<Part>)>,
) {
    let module = &program.modules[index];
    // What each expression of a part reads, gathered by walking it.
    let gather = |expressions: &[&Expr], packs: &mut PackBodies| {
        let mut reads = Vec::new();
        for expression in expressions {
            expression
                .walk(&mut |visited| reads_of(program, visited, callbacks, packs, &mut reads));
        }
        reads
    };
    for position in 0..module.declarations.len() {
        let declared = (index, position);
        if let Some(Some(value)) = module.constants.get(position) {
            parts.push((Part::Constant(declared), gather(&[&value.value], packs)));
        }
        if let Some(Some(body)) = module.bodies.get(position) {
            let mut reads = Vec::new();
            walk_statements(&body.statements, &mut |visited| {
                reads_of(program, visited, callbacks, packs, &mut reads);
            });
            parts.push((Part::Body(declared), reads));
        }
        let defaults = module.defaults.get(position).map_or(&[][..], Vec::as_slice);
        match module.details.get(position) {
            Some(Detail::Struct(read)) => {
                // The struct still needs its parents' types (TYPE-4).
                let parents = read
                    .parents
                    .iter()
                    .filter_map(|parent| match parent {
                        Type::Named {
                            module,
                            declaration,
                            ..
                        } => Some(Part::Struct((*module, *declaration))),
                        _ => None,
                    })
                    .collect();
                parts.push((Part::Struct(declared), parents));
                for (field, default) in defaults.iter().enumerate() {
                    if let Some(default) = default {
                        parts.push((
                            Part::FieldDefault(declared, field),
                            gather(&[&default.value], packs),
                        ));
                    }
                }
            }
            _ => {
                for (parameter, default) in defaults.iter().enumerate() {
                    if let Some(default) = default {
                        parts.push((
                            Part::Default(declared, parameter),
                            gather(&[&default.value], packs),
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
fn cycles(program: &Program, diagnostics: &mut Diagnostics, has_constant: bool, has_default: bool) {
    let mut packs = PackBodies::default();
    // The value pass has already bounded this instantiation walk. Do not
    // repeat a walk that it rejected for exceeding the instance limit.
    if !diagnostics
        .as_slice()
        .iter()
        .any(|d| d.code == Code::Catalogued(codes::INSTANCE_LIMIT))
    {
        packs.forwarding = forwarding_bodies(program);
    }
    // Each part of each module, with the parts it reads.
    let callbacks = CallbackUses::of(program);
    let mut parts: Vec<(Part, Vec<Part>)> = Vec::new();
    for index in 0..program.modules.len() {
        parts_of(program, index, &callbacks, &mut packs, &mut parts);
    }
    packs.add_parts(program, &callbacks, &mut parts);

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

    if has_constant {
        report_constants(program, &graph, diagnostics);
    }
    if has_default {
        report_defaults(program, &graph, diagnostics);
    }
}

/// A one-node component cycles only when it has an edge to itself.
fn cyclic(graph: &DiGraph<Part, ()>, group: &[NodeIndex]) -> bool {
    group.len() > 1 || graph.contains_edge(group[0], group[0])
}

/// Each declaration on a cycle, in declaration order.
fn declarations_on(graph: &DiGraph<Part, ()>, group: &[NodeIndex]) -> Vec<Declared> {
    let mut declarations: Vec<_> = group
        .iter()
        .map(|&part| graph[part].declaration())
        .collect();
    declarations.sort_unstable();
    declarations.dedup();
    declarations
}

/// Name the other declarations on the path. Give an imported
/// declaration's module when its name is outside the subject's module.
fn cycle_message(
    program: &Program,
    subject: String,
    own: Declared,
    declarations: &[Declared],
) -> String {
    let through: Vec<_> = declarations
        .iter()
        .filter(|&&other| other != own)
        .map(|&(module, declaration)| {
            let other = &program.modules[module];
            let name = &other.declarations[declaration].name;
            if module == own.0 {
                format!("`{name}`")
            } else {
                format!("`{name}` of `{}`", other.path)
            }
        })
        .collect();
    if through.is_empty() {
        format!("{subject} depends on itself")
    } else {
        format!(
            "{subject} depends on itself, through {}",
            through.join(" and ")
        )
    }
}

/// UNIT-13: a constant may not depend on itself, even through a default.
fn report_constants(program: &Program, graph: &DiGraph<Part, ()>, diagnostics: &mut Diagnostics) {
    // Parts that read one another are a strongly connected group of the
    // graph. The findings are gathered and then given in the order the
    // constants are declared, whatever order the groups are found in.
    let mut findings: Vec<(Declared, String)> = Vec::new();
    for group in kosaraju_scc(graph) {
        if !cyclic(graph, &group) {
            continue;
        }
        let declarations = declarations_on(graph, &group);
        for &part in &group {
            let Part::Constant(member) = graph[part] else {
                continue;
            };
            let name = &program.modules[member.0].declarations[member.1].name;
            let message = cycle_message(
                program,
                format!("the value of `{name}`"),
                member,
                &declarations,
            );
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

/// UNIT-8: a default may not require itself. Follow the default's
/// dependencies independently of named-constant cycles (UNIT-13).
fn report_defaults(program: &Program, graph: &DiGraph<Part, ()>, diagnostics: &mut Diagnostics) {
    let defaults = NodeFiltered::from_fn(graph, |part| !matches!(graph[part], Part::Constant(_)));
    let mut findings = Vec::new();
    for group in kosaraju_scc(&defaults) {
        if !cyclic(graph, &group) {
            continue;
        }
        let declarations = declarations_on(graph, &group);
        for part in group {
            let (member, position) = match graph[part] {
                Part::Default(member, position) | Part::FieldDefault(member, position) => {
                    (member, position)
                }
                _ => continue,
            };
            let module = &program.modules[member.0];
            let Some(default) = module
                .defaults
                .get(member.1)
                .and_then(|defaults| defaults.get(position))
                .and_then(Option::as_ref)
            else {
                continue;
            };
            let name = match module.details.get(member.1) {
                Some(Detail::Callable(signature)) => signature
                    .parameters
                    .get(position)
                    .map(|parameter| &parameter.name),
                Some(Detail::Struct(structure)) => {
                    structure.fields.get(position).map(|field| &field.name)
                }
                _ => None,
            };
            let Some(name) = name else { continue };
            let owner = &module.declarations[member.1].name;
            let subject = format!("the default of `{name}` in `{owner}`");
            let message = cycle_message(program, subject, member, &declarations);
            findings.push((member, position, default.value.at, message));
        }
    }
    findings.sort_by_key(|&(member, position, ..)| (member, position));
    for (_, _, at, message) in findings {
        diagnostics.emit(error(
            Category::Name,
            codes::DEFAULT_CYCLE,
            at,
            message,
            "UNIT-8",
        ));
    }
}
