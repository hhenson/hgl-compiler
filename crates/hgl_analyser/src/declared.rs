//! Checks the types a program declares: its structs and its enums.
//!
//! `read` says what each written type means. This pass reads the
//! declarations against one another:
//!
//! * a struct's parents are abstract structs (TYPE-3), and no struct
//!   inherits from itself (TYPE-6);
//! * the fields a struct has from its parents and its own agree in type
//!   and optionality, and `name = value` overrides a field a parent
//!   declares (TYPE-4);
//! * a struct that refers back to itself does so through an optional
//!   `atomic<...>` field defaulted to null (TYPE-6);
//! * an enum's members are numbered, each number an `i64` no other
//!   member has (TYPE-7).
//!
//! A type that is not known, or a parent already reported, is left out
//! of what follows from it (ERR-9).

use std::collections::HashMap;

use hgl_diagnostics::{Category, Diagnostics, Location};
use hgl_parser::{HasSpan, SyntaxKind, SyntaxNode};
use hgl_source::FileId;
use petgraph::algo::kosaraju_scc;
use petgraph::graph::{DiGraph, NodeIndex};

use crate::constant::fold;
use crate::program::{Declaration, DeclarationKind, Program};
use crate::read::{child, has_token, is_type, tokens};
use crate::types::{
    Argument, Constant, ConstantValue, Detail, EnumMember, EnumType, StructType, Type,
};
use crate::{codes, local_error};

/// A declaration, by the position of its module in `Program::modules`
/// and its own in that module's `declarations`.
type Id = (usize, usize);

/// Checks every struct and enum of the program, and fills in the
/// [`Detail::Enum`] of each enum.
pub(crate) fn check(program: &mut Program, diagnostics: &mut Diagnostics) {
    for module in &mut program.modules {
        for (index, declaration) in module.declarations.iter().enumerate() {
            if declaration.kind == DeclarationKind::Enum {
                let file = module.files[declaration.file].file;
                module.details[index] =
                    Detail::Enum(enum_members(&declaration.syntax, file, diagnostics));
            }
        }
    }

    let mut structs = Structs {
        program,
        diagnostics,
        parents: HashMap::new(),
        fields: HashMap::new(),
    };
    let ids = structs.ids();
    for &id in &ids {
        let parents = structs.parents_of(id);
        structs.parents.insert(id, parents);
    }
    structs.inheritance_cycles(&ids);
    let mut fields = HashMap::new();
    for &id in &ids {
        structs.collect_fields(id, &mut fields, &mut Vec::new());
    }
    structs.fields = fields;
    for &id in &ids {
        structs.fields(id);
    }
    structs.recursion(&ids);
}

// ---- Enums (TYPE-7) ----

/// Numbers the members of an enum.
///
/// TYPE-7: "Unnumbered numbering starts at zero and increments the
/// previous member; explicit numbers reset the next value. Values fit
/// signed `i64`; duplicates and overflow reject."
fn enum_members(node: &SyntaxNode, file: FileId, diagnostics: &mut Diagnostics) -> EnumType {
    let mut members: Vec<EnumMember> = Vec::new();
    // The number of the member before: `None` when there is none yet,
    // `Some(None)` when it is not known.
    let mut previous: Option<Option<i64>> = None;
    for member in node
        .children()
        .filter(|member| member.kind() == SyntaxKind::EnumMember)
    {
        // A member's name is its first token. It may be a reserved word
        // (LEX-9).
        let Some(name) = tokens(&member).next() else {
            continue;
        };
        let at = Location::new(file, name.span());
        if let Some(first) = members.iter().find(|first| first.name == name.text()) {
            diagnostics.emit(
                local_error(
                    Category::Type,
                    codes::local::ENUM_DUPLICATE_MEMBER,
                    at,
                    format!("this enum has two members named `{}`", name.text()),
                    "TYPE-7",
                )
                .with_related(first.at, "first declared here"),
            );
        }

        let value = match member.children().next() {
            // `buy = 1`: the number written. LEX-7: "Enum numbering
            // admits the full signed minimum", which `fold` reads as a
            // minus before 2^63.
            Some(number) => match fold(&number, &|_| ConstantValue::Unevaluated) {
                ConstantValue::Int(value) => Some(value),
                ConstantValue::Duration(_) => {
                    not_an_integer(&number, file, "a duration", diagnostics);
                    None
                }
                ConstantValue::Other(what) => {
                    not_an_integer(&number, file, what, diagnostics);
                    None
                }
                // An expression the pass that evaluates constants reads.
                _ => None,
            },
            None => match previous {
                None => Some(0),
                Some(None) => None,
                // `checked_add` gives `None` when the sum does not fit.
                Some(Some(before)) => {
                    let next = before.checked_add(1);
                    if next.is_none() {
                        diagnostics.emit(local_error(
                            Category::Type,
                            codes::local::ENUM_OVERFLOW,
                            at,
                            format!(
                                "`{}` would be numbered one more than the largest i64",
                                name.text()
                            ),
                            "TYPE-7",
                        ));
                    }
                    next
                }
            },
        };
        if let Some(number) = value
            && let Some(first) = members.iter().find(|first| first.value == Some(number))
        {
            diagnostics.emit(
                local_error(
                    Category::Type,
                    codes::local::ENUM_DUPLICATE_VALUE,
                    at,
                    format!(
                        "`{}` has the number {number}, which `{}` has",
                        name.text(),
                        first.name
                    ),
                    "TYPE-7",
                )
                .with_related(first.at, "the member that has it"),
            );
        }
        previous = Some(value);
        members.push(EnumMember {
            name: name.text().to_string(),
            at,
            value,
        });
    }
    EnumType { members }
}

fn not_an_integer(number: &SyntaxNode, file: FileId, what: &str, diagnostics: &mut Diagnostics) {
    diagnostics.emit(local_error(
        Category::Type,
        codes::local::ENUM_VALUE,
        Location::new(file, number.span()),
        format!("an enum member's number is an integer, and this is {what}"),
        "TYPE-7",
    ));
}

// ---- Structs ----

/// One parent of a struct that is an abstract struct.
#[derive(Debug, Clone)]
struct Parent {
    /// The parent.
    id: Id,
    /// The generic arguments it is given.
    arguments: Vec<Argument>,
    /// Where it is written, after the `:`.
    at: Location,
}

/// One field a struct has: its own, or one from a parent.
#[derive(Debug, Clone)]
struct Introduced {
    /// The field's name.
    name: String,
    /// Its type, with the generic arguments of the path it came by put
    /// in place of the parameters.
    ty: Type,
    /// True when its default is null (TYPE-4).
    optional: bool,
    /// The struct that declares it, and the field's position among that
    /// struct's members.
    origin: (Id, usize),
    /// Where it is declared.
    at: Location,
    /// The position, among the parents of the struct being checked, of
    /// the parent it came through.
    via: usize,
}

/// A field whose type names structs.
struct FieldEdge {
    /// The struct that declares the field.
    owner: Id,
    /// The field's position among that struct's members.
    field: usize,
    /// The structs its type names, each with whether it is reached
    /// through the elements of a collection.
    named: Vec<(Id, bool)>,
}

/// The fields one struct has.
#[derive(Debug, Clone, Default)]
struct Fields {
    /// Those it has from its parents.
    inherited: Vec<Introduced>,
    /// Those, then its own.
    all: Vec<Introduced>,
}

struct Structs<'a> {
    program: &'a Program,
    diagnostics: &'a mut Diagnostics,
    /// The abstract-struct parents of each struct.
    parents: HashMap<Id, Vec<Parent>>,
    /// The fields of each struct, worked out once for each.
    fields: HashMap<Id, Fields>,
}

/// The types `ty` names, itself and inside it, each with whether it is
/// reached through the elements of a collection: a list, a set, a
/// dictionary or a window.
fn mentions(ty: &Type, in_collection: bool, found: &mut Vec<(Id, bool)>) {
    match ty {
        Type::Named {
            module,
            declaration,
            arguments,
            ..
        } => {
            found.push(((*module, *declaration), in_collection));
            for argument in arguments {
                if let Argument::Type(argument) = argument {
                    mentions(argument, in_collection, found);
                }
            }
        }
        Type::Tuple(elements) => {
            for element in elements {
                mentions(element, in_collection, found);
            }
        }
        Type::List { element, .. } => mentions(element, true, found),
        Type::Set(element) => mentions(element, true, found),
        Type::Dict { key, value } => {
            mentions(key, true, found);
            mentions(value, true, found);
        }
        Type::Rolling { value, .. } => mentions(value, true, found),
        Type::Ref(inner) | Type::Atomic(inner) | Type::Delta(inner) | Type::Nullable(inner) => {
            mentions(inner, in_collection, found);
        }
        Type::Result { value, error } => {
            mentions(value, in_collection, found);
            mentions(error, in_collection, found);
        }
        Type::Scalar(_) | Type::Parameter { .. } | Type::Signal | Type::Error | Type::Unknown => {}
    }
}

/// `ty` with each generic parameter in `map` replaced by the argument
/// given for it. `map` pairs where a parameter is declared with its
/// argument.
fn substitute(ty: &Type, map: &[(Location, &Argument)]) -> Type {
    // `Box::new` puts the substituted part where a `Box<Type>` points.
    let part = |inner: &Type| Box::new(substitute(inner, map));
    let constant = |constant: &Constant| match &constant.value {
        ConstantValue::Parameter { at, .. } => map
            .iter()
            .find_map(|(declared, argument)| match argument {
                Argument::Constant(given) if declared == at => Some(given.clone()),
                _ => None,
            })
            .unwrap_or_else(|| constant.clone()),
        _ => constant.clone(),
    };
    match ty {
        Type::Parameter { at, .. } => map
            .iter()
            .find_map(|(declared, argument)| match argument {
                Argument::Type(given) if declared == at => Some(given.clone()),
                _ => None,
            })
            .unwrap_or_else(|| ty.clone()),
        Type::Named {
            name,
            module,
            declaration,
            arguments,
        } => Type::Named {
            name: name.clone(),
            module: *module,
            declaration: *declaration,
            arguments: arguments
                .iter()
                .map(|argument| match argument {
                    Argument::Type(argument) => Argument::Type(substitute(argument, map)),
                    Argument::Constant(argument) => Argument::Constant(constant(argument)),
                })
                .collect(),
        },
        Type::Tuple(elements) => Type::Tuple(
            elements
                .iter()
                .map(|element| substitute(element, map))
                .collect(),
        ),
        Type::List { element, size } => Type::List {
            element: part(element),
            size: size.as_ref().map(constant),
        },
        Type::Set(inner) => Type::Set(part(inner)),
        Type::Dict { key, value } => Type::Dict {
            key: part(key),
            value: part(value),
        },
        Type::Rolling {
            value,
            maximum,
            minimum,
        } => Type::Rolling {
            value: part(value),
            maximum: constant(maximum),
            minimum: minimum.as_ref().map(constant),
        },
        Type::Ref(inner) => Type::Ref(part(inner)),
        Type::Atomic(inner) => Type::Atomic(part(inner)),
        Type::Result { value, error } => Type::Result {
            value: part(value),
            error: part(error),
        },
        Type::Delta(inner) => Type::Delta(part(inner)),
        Type::Nullable(inner) => Type::Nullable(part(inner)),
        Type::Scalar(_) | Type::Signal | Type::Error | Type::Unknown => ty.clone(),
    }
}

/// `required` or `optional`.
fn optionality(optional: bool) -> &'static str {
    if optional { "optional" } else { "required" }
}

impl Structs<'_> {
    // ---- Reading the program ----

    fn declaration(&self, id: Id) -> &Declaration {
        &self.program.modules[id.0].declarations[id.1]
    }

    /// What was read of the struct `id`.
    fn struct_type(&self, id: Id) -> Option<&StructType> {
        match &self.program.modules[id.0].details[id.1] {
            Detail::Struct(read) => Some(read),
            _ => None,
        }
    }

    fn file(&self, id: Id) -> FileId {
        let module = &self.program.modules[id.0];
        module.files[module.declarations[id.1].file].file
    }

    /// Every struct of the program, in order of module and declaration.
    fn ids(&self) -> Vec<Id> {
        let mut ids = Vec::new();
        for (module, declared) in self.program.modules.iter().enumerate() {
            for (index, declaration) in declared.declarations.iter().enumerate() {
                if declaration.kind == DeclarationKind::Struct {
                    ids.push((module, index));
                }
            }
        }
        ids
    }

    // ---- Parents (TYPE-3) ----

    /// The parents of `id` that are abstract structs. Any other parent
    /// is reported.
    ///
    /// TYPE-3: "Only abstract structs may be parents ... Concrete
    /// structs are final."
    fn parents_of(&mut self, id: Id) -> Vec<Parent> {
        let Some(read) = self.struct_type(id) else {
            return Vec::new();
        };
        let file = self.file(id);
        // Where each parent is written, in the order `read.parents` has
        // them.
        let written: Vec<Location> = child(&self.declaration(id).syntax, SyntaxKind::StructBases)
            .into_iter()
            .flat_map(|bases| bases.children())
            .filter(|parent| is_type(parent.kind()))
            .map(|parent| Location::new(file, parent.span()))
            .collect();

        let mut parents = Vec::new();
        let mut problems = Vec::new();
        for (ty, &at) in read.parents.iter().zip(&written) {
            match ty {
                // Not known: left for the prelude, or reported.
                Type::Unknown => {}
                Type::Named {
                    name,
                    module,
                    declaration,
                    arguments,
                } => {
                    let parent = (*module, *declaration);
                    let declared = self.declaration(parent);
                    let what = match declared.kind {
                        DeclarationKind::Struct
                            if has_token(&declared.syntax, SyntaxKind::KwAbstract) =>
                        {
                            parents.push(Parent {
                                id: parent,
                                arguments: arguments.clone(),
                                at,
                            });
                            continue;
                        }
                        DeclarationKind::Struct => "a concrete struct".to_string(),
                        DeclarationKind::Enum => "an enum".to_string(),
                        kind => format!("a {}", kind.describe()),
                    };
                    problems.push((
                        at,
                        format!(
                            "`{name}` is {what} and cannot be a parent: only an abstract \
                             struct may be"
                        ),
                        Some(declared.at),
                    ));
                }
                other => problems.push((
                    at,
                    format!("`{other}` cannot be a parent: only an abstract struct may be"),
                    None,
                )),
            }
        }
        for (at, message, declared) in problems {
            let mut diagnostic = local_error(
                Category::Type,
                codes::local::STRUCT_PARENT,
                at,
                message,
                "TYPE-3",
            );
            if let Some(declared) = declared {
                diagnostic = diagnostic.with_related(declared, "declared here");
            }
            self.diagnostics.emit(diagnostic);
        }
        parents
    }

    /// TYPE-6: "Cycles through inheritance ... are rejected."
    fn inheritance_cycles(&mut self, ids: &[Id]) {
        // A graph with a node for each struct and an edge from each to
        // its parents. Structs that inherit from one another are a
        // strongly connected group of it.
        let mut graph: DiGraph<Id, ()> = DiGraph::new();
        let nodes: HashMap<Id, NodeIndex> =
            ids.iter().map(|&id| (id, graph.add_node(id))).collect();
        for &id in ids {
            for parent in &self.parents[&id] {
                graph.add_edge(nodes[&id], nodes[&parent.id], ());
            }
        }
        for group in kosaraju_scc(&graph) {
            let mut members: Vec<Id> = group.iter().map(|&node| graph[node]).collect();
            members.sort_unstable();
            let first = members[0];
            // One struct is a cycle only when it is its own parent.
            let Some(parent) = self.parents[&first]
                .iter()
                .find(|parent| members.contains(&parent.id))
                .filter(|_| members.len() > 1 || self.is_own_parent(first))
            else {
                continue;
            };
            let names: Vec<String> = members
                .iter()
                .map(|&member| format!("`{}`", self.declaration(member).name))
                .collect();
            let message = if members.len() == 1 {
                format!("{} is its own parent", names[0])
            } else {
                format!("{} inherit from one another", names.join(" and "))
            };
            self.diagnostics.emit(local_error(
                Category::Type,
                codes::local::STRUCT_INHERITANCE_CYCLE,
                parent.at,
                message,
                "TYPE-6",
            ));
        }
    }

    fn is_own_parent(&self, id: Id) -> bool {
        self.parents[&id].iter().any(|parent| parent.id == id)
    }

    // ---- Fields (TYPE-4) ----

    /// Works out the fields of the struct `id`, and of each ancestor of
    /// it that `known` does not have yet: those of its parents, each
    /// with the parent's generic arguments in place of its parameters,
    /// then its own.
    ///
    /// Each struct's fields are worked out once and kept, and a field
    /// that reaches a struct by two paths is listed once. Otherwise a
    /// ladder of structs that each inherit from the two before would
    /// list the first one's field twice as often at every step.
    ///
    /// `visiting` holds the structs being read above this one, so that
    /// a cycle of inheritance ends.
    fn collect_fields(&self, id: Id, known: &mut HashMap<Id, Fields>, visiting: &mut Vec<Id>) {
        if known.contains_key(&id) || visiting.contains(&id) {
            return;
        }
        visiting.push(id);
        let mut inherited: Vec<Introduced> = Vec::new();
        for (via, parent) in self.parents[&id].iter().enumerate() {
            self.collect_fields(parent.id, known, visiting);
            // A parent with no fields yet is being read above this
            // struct: a cycle of inheritance, which has been reported.
            let Some(fields) = known.get(&parent.id) else {
                continue;
            };
            // The parent's generic parameters, each with the argument
            // this struct gives it. `zip` stops at the shorter, so a
            // wrong count, which has been reported, leaves the rest as
            // they are.
            let map: Vec<(Location, &Argument)> = self
                .struct_type(parent.id)
                .map(|read| {
                    read.generics
                        .iter()
                        .map(|generic| generic.at)
                        .zip(&parent.arguments)
                        .collect()
                })
                .unwrap_or_default();
            for field in &fields.all {
                let field = Introduced {
                    ty: substitute(&field.ty, &map),
                    via,
                    ..field.clone()
                };
                // The same field by another path is one field.
                let listed = inherited.iter().any(|other| {
                    other.origin == field.origin
                        && other.optional == field.optional
                        && other.ty == field.ty
                });
                if !listed {
                    inherited.push(field);
                }
            }
        }
        let mut all = inherited.clone();
        if let Some(read) = self.struct_type(id) {
            for (index, field) in read.fields.iter().enumerate() {
                // `name = value` introduces no field.
                if let Some(ty) = &field.ty {
                    all.push(Introduced {
                        name: field.name.clone(),
                        ty: ty.clone(),
                        optional: field.null_default,
                        origin: (id, index),
                        at: field.at,
                        via: 0,
                    });
                }
            }
        }
        visiting.pop();
        known.insert(id, Fields { inherited, all });
    }

    /// The fields the struct `id` has from its parents.
    fn inherited(&self, id: Id) -> Vec<Introduced> {
        self.fields
            .get(&id)
            .map(|fields| fields.inherited.clone())
            .unwrap_or_default()
    }

    /// TYPE-4: "A typed field introduction fixes its type and
    /// optionality. A null default marks it optional. An untyped
    /// inherited override changes only its construction default ...
    /// Parent conflicts in type/optionality are errors."
    fn fields(&mut self, id: Id) {
        let Some(read) = self.struct_type(id) else {
            return;
        };
        let name = &self.declaration(id).name;
        let inherited = self.inherited(id);
        let mut problems: Vec<(&'static str, Location, String, Location)> = Vec::new();

        // Two parents that give one name two meanings. The same field
        // by two paths is one field.
        for (index, later) in inherited.iter().enumerate() {
            let conflict = inherited[..index].iter().find(|earlier| {
                earlier.name == later.name
                    && earlier.via != later.via
                    && differs(earlier, &later.ty, later.optional)
            });
            if let Some(earlier) = conflict {
                problems.push((
                    codes::local::FIELD_CONFLICT,
                    self.parents[&id][later.via].at,
                    format!(
                        "`{name}` has `{}` from two parents: {} `{}` and {} `{}`",
                        later.name,
                        optionality(earlier.optional),
                        earlier.ty,
                        optionality(later.optional),
                        later.ty
                    ),
                    earlier.at,
                ));
            }
        }

        for (index, field) in read.fields.iter().enumerate() {
            if let Some(first) = read.fields[..index]
                .iter()
                .find(|first| first.name == field.name)
            {
                problems.push((
                    codes::local::FIELD_DUPLICATE,
                    field.at,
                    format!("`{name}` has two members named `{}`", field.name),
                    first.at,
                ));
                continue;
            }
            let from_parent = inherited.iter().find(|parent| parent.name == field.name);
            match (&field.ty, from_parent) {
                // A field of its own.
                (Some(_), None) => {}
                (Some(ty), Some(parent)) => {
                    if differs(parent, ty, field.null_default) {
                        problems.push((
                            codes::local::FIELD_CONFLICT,
                            field.at,
                            format!(
                                "`{}` is {} `{ty}` here and {} `{}` in `{}`",
                                field.name,
                                optionality(field.null_default),
                                optionality(parent.optional),
                                parent.ty,
                                self.declaration(parent.origin.0).name
                            ),
                            parent.at,
                        ));
                    }
                }
                (None, Some(_)) => {}
                (None, None) => problems.push((
                    codes::local::FIELD_NOT_INHERITED,
                    field.at,
                    format!(
                        "`{} = ...` overrides the default of a parent's field, and `{name}` \
                         has no field `{}` from a parent",
                        field.name, field.name
                    ),
                    self.declaration(id).at,
                )),
            }
        }

        for (code, at, message, related) in problems {
            let note = match code {
                codes::local::FIELD_NOT_INHERITED => "the struct",
                codes::local::FIELD_DUPLICATE => "first declared here",
                _ => "the other declaration",
            };
            self.diagnostics.emit(
                local_error(Category::Type, code, at, message, "TYPE-4")
                    .with_related(related, note),
            );
        }
    }

    // ---- Recursion (TYPE-6) ----

    /// TYPE-6: "Recursive struct edges stay within their defining
    /// module, use an optional `atomic<...>` field defaulted to null ...
    /// Descendants cannot remove that null default. Cycles through ...
    /// collection elements are rejected."
    fn recursion(&mut self, ids: &[Id]) {
        // A graph with a node for each struct, and an edge from each to
        // every struct it holds: its parents, whose fields it has, and
        // the structs its own fields name. A struct that holds itself,
        // directly or through others, is in a strongly connected group.
        let mut graph: DiGraph<Id, ()> = DiGraph::new();
        let nodes: HashMap<Id, NodeIndex> =
            ids.iter().map(|&id| (id, graph.add_node(id))).collect();
        // Each field that names a struct.
        let mut edges: Vec<FieldEdge> = Vec::new();
        for &id in ids {
            for parent in &self.parents[&id] {
                graph.add_edge(nodes[&id], nodes[&parent.id], ());
            }
            let Some(read) = self.struct_type(id) else {
                continue;
            };
            for (index, field) in read.fields.iter().enumerate() {
                let mut named = Vec::new();
                if let Some(ty) = &field.ty {
                    mentions(ty, false, &mut named);
                }
                named.retain(|(target, _)| nodes.contains_key(target));
                for (target, _) in &named {
                    graph.add_edge(nodes[&id], nodes[target], ());
                }
                if !named.is_empty() {
                    edges.push(FieldEdge {
                        owner: id,
                        field: index,
                        named,
                    });
                }
            }
        }
        // The group each struct is in.
        let mut group_of: HashMap<Id, usize> = HashMap::new();
        for (group, members) in kosaraju_scc(&graph).into_iter().enumerate() {
            for node in members {
                group_of.insert(graph[node], group);
            }
        }

        // The fields that are recursive edges: they name a struct of
        // the group their own struct is in. With each, whether it
        // reaches such a struct through a collection's elements.
        let mut recursive: HashMap<(Id, usize), bool> = HashMap::new();
        for edge in &edges {
            let mut back = edge
                .named
                .iter()
                .filter(|(target, _)| group_of[target] == group_of[&edge.owner])
                .peekable();
            // `peek` looks at the next one without taking it.
            if back.peek().is_some() {
                let through_collection = back.any(|(_, in_collection)| *in_collection);
                recursive.insert((edge.owner, edge.field), through_collection);
            }
        }
        let mut reports = Vec::new();
        for (&(id, index), &through_collection) in &recursive {
            let Some(field) = self.struct_type(id).map(|read| &read.fields[index]) else {
                continue;
            };
            let name = &self.declaration(id).name;
            // "Cycles through ... collection elements are rejected",
            // inside an `atomic<...>` as outside one.
            if through_collection {
                reports.push((
                    field.at,
                    format!(
                        "`{}` makes `{name}` hold itself through the elements of a collection",
                        field.name
                    ),
                ));
                continue;
            }
            let atomic = matches!(field.ty, Some(Type::Atomic(_)));
            if !atomic || !field.null_default {
                reports.push((
                    field.at,
                    format!(
                        "`{}` makes `{name}` hold itself: a recursive field is an \
                         `atomic<...>` with a default of null",
                        field.name
                    ),
                ));
            }
        }

        // "Descendants cannot remove that null default."
        for &id in ids {
            let Some(read) = self.struct_type(id) else {
                continue;
            };
            let inherited = self.inherited(id);
            for field in &read.fields {
                let overrides_recursive = field.ty.is_none()
                    && !field.null_default
                    && inherited.iter().any(|parent| {
                        parent.name == field.name && recursive.contains_key(&parent.origin)
                    });
                if overrides_recursive {
                    reports.push((
                        field.at,
                        format!(
                            "`{}` is a recursive field, and its default stays null",
                            field.name
                        ),
                    ));
                }
            }
        }
        for (at, message) in reports {
            self.diagnostics.emit(local_error(
                Category::Type,
                codes::local::FIELD_RECURSIVE,
                at,
                message,
                "TYPE-6",
            ));
        }
    }
}

/// True when a field of type `ty`, optional or not, is not the field
/// `parent` introduces. A type that is not known differs from nothing.
fn differs(parent: &Introduced, ty: &Type, optional: bool) -> bool {
    if matches!(parent.ty, Type::Unknown) || matches!(ty, Type::Unknown) {
        return false;
    }
    parent.ty != *ty || parent.optional != optional
}
