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

use std::collections::{HashMap, HashSet};

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
        children: HashMap::new(),
    };
    let ids = structs.ids();
    for &id in &ids {
        let parents = structs.parents_of(id);
        for parent in &parents {
            structs
                .children
                .entry(parent.id)
                .or_default()
                .push((id, parent.arguments.clone()));
        }
        structs.parents.insert(id, parents);
    }
    structs.inheritance_cycles(&ids);
    structs.fields = structs.collect_fields(&ids);
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

/// A field of a struct, its own or one from a parent, whose type names
/// structs.
struct FieldEdge {
    /// The struct that has the field.
    owner: Id,
    /// The field's name.
    name: String,
    /// The struct that declares the field, and the field's position
    /// among that struct's members.
    origin: (Id, usize),
    /// Where the field is declared.
    at: Location,
    /// The position, among the owner's parents, of the parent the field
    /// came through; meaningless for a field of the owner's own.
    via: usize,
    /// The field's type, with the arguments of the path it came by in
    /// place.
    ty: Type,
    /// True when the field has the form TYPE-6 gives a recursive one:
    /// an `atomic<...>` with a default of null.
    in_form: bool,
    /// The structs a value of the field's type holds, each with whether
    /// it is reached through the elements of a collection.
    named: Vec<Held>,
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
    /// The structs that inherit from each abstract struct, each with
    /// the arguments it gives that parent.
    children: HashMap<Id, Vec<(Id, Vec<Argument>)>>,
}

/// A struct a value of some type holds.
#[derive(Debug, Clone, Copy)]
struct Held {
    /// The struct.
    id: Id,
    /// True when it is reached through the elements of a collection
    /// outside an `atomic<...>`.
    in_collection: bool,
    /// The struct the type names that leads to it: itself, or the
    /// abstract struct it inherits from.
    written: Id,
}

/// How many fields of generic structs are followed for one field's
/// type. A type that needs more is not followed further, so that the
/// check ends whatever it is given.
const EXPANSIONS: u32 = 10_000;

/// How many generic structs, one inside another, are followed for one
/// field's type.
const DEPTH: usize = 64;

/// True when `ty` names a generic parameter, itself or inside it.
fn names_parameter(ty: &Type) -> bool {
    match ty {
        Type::Parameter { .. } => true,
        Type::Named { arguments, .. } => arguments.iter().any(|argument| match argument {
            Argument::Type(argument) => names_parameter(argument),
            Argument::Constant(_) => false,
        }),
        Type::Tuple(elements) => elements.iter().any(names_parameter),
        Type::List { element, .. } => names_parameter(element),
        Type::Set(inner)
        | Type::Ref(inner)
        | Type::Atomic(inner)
        | Type::Delta(inner)
        | Type::Nullable(inner) => names_parameter(inner),
        Type::Dict { key, value } => names_parameter(key) || names_parameter(value),
        Type::Rolling { value, .. } => names_parameter(value),
        Type::Result { value, error } => names_parameter(value) || names_parameter(error),
        Type::Scalar(_) | Type::Signal | Type::Error | Type::Unknown => false,
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
        } => {
            let maximum = constant(maximum);
            // TYPE-11: "Omitted minimum equals maximum." A minimum that
            // an argument makes equal to the maximum is held as none,
            // as one written equal is.
            let minimum = minimum
                .as_ref()
                .map(constant)
                .filter(|minimum| *minimum != maximum);
            Type::Rolling {
                value: part(value),
                maximum,
                minimum,
            }
        }
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

/// The most a type may weigh once arguments stand for its parameters.
/// A type weighs one for itself and for each type inside it.
const WEIGHT: usize = 10_000;

/// How much a type weighs: one for itself and for each type inside it.
fn weight(ty: &Type) -> usize {
    1 + match ty {
        Type::Named { arguments, .. } => arguments
            .iter()
            .map(|argument| match argument {
                Argument::Type(argument) => weight(argument),
                Argument::Constant(_) => 1,
            })
            .sum(),
        Type::Tuple(elements) => elements.iter().map(weight).sum(),
        Type::List { element, .. } => weight(element),
        Type::Set(inner)
        | Type::Ref(inner)
        | Type::Atomic(inner)
        | Type::Delta(inner)
        | Type::Nullable(inner) => weight(inner),
        Type::Dict { key, value } => weight(key) + weight(value),
        Type::Rolling { value, .. } => weight(value),
        Type::Result { value, error } => weight(value) + weight(error),
        Type::Scalar(_) | Type::Parameter { .. } | Type::Signal | Type::Error | Type::Unknown => 0,
    }
}

/// [`substitute`], unless the result could weigh more than [`WEIGHT`]:
/// then the type is not known.
///
/// Each argument may stand in a type many times, and a chain of generic
/// parents that each give the next `tuple<T, T>` doubles a field's type
/// at every link. The weight is bounded before anything is built: no
/// more than the type's own, times the heaviest argument's.
fn substitute_within(ty: &Type, map: &[(Location, &Argument)]) -> Type {
    let heaviest = map
        .iter()
        .map(|(_, argument)| match argument {
            Argument::Type(argument) => weight(argument),
            Argument::Constant(_) => 1,
        })
        .max()
        .unwrap_or(1);
    if weight(ty).saturating_mul(heaviest) > WEIGHT {
        return Type::Unknown;
    }
    substitute(ty, map)
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
                            // TYPE-3: a parent is listed once.
                            let listed =
                                parents.iter().find(|listed: &&Parent| listed.id == parent);
                            if let Some(first) = listed {
                                problems.push((
                                    codes::local::STRUCT_DUPLICATE_PARENT,
                                    at,
                                    format!("`{name}` is listed twice among the parents"),
                                    Some((first.at, "first listed here")),
                                ));
                            } else {
                                parents.push(Parent {
                                    id: parent,
                                    arguments: arguments.clone(),
                                    at,
                                });
                            }
                            continue;
                        }
                        DeclarationKind::Struct => "a concrete struct".to_string(),
                        DeclarationKind::Enum => "an enum".to_string(),
                        kind => format!("a {}", kind.describe()),
                    };
                    problems.push((
                        codes::local::STRUCT_PARENT,
                        at,
                        format!(
                            "`{name}` is {what} and cannot be a parent: only an abstract \
                             struct may be"
                        ),
                        Some((declared.at, "declared here")),
                    ));
                }
                other => problems.push((
                    codes::local::STRUCT_PARENT,
                    at,
                    format!("`{other}` cannot be a parent: only an abstract struct may be"),
                    None,
                )),
            }
        }
        for (code, at, message, related) in problems {
            let mut diagnostic = local_error(Category::Type, code, at, message, "TYPE-3");
            if let Some((place, note)) = related {
                diagnostic = diagnostic.with_related(place, note);
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

    /// Works out the fields of every struct: those of its parents, each
    /// with the parent's generic arguments in place of its parameters,
    /// then its own.
    ///
    /// Each struct's fields are worked out once and kept, and a field
    /// that reaches a struct by two paths is listed once. Otherwise a
    /// ladder of structs that each inherit from the two before would
    /// list the first one's field twice as often at every step.
    ///
    /// A struct's fields need its parents' first. The parents are
    /// reached with a list of structs still to do, kept here, and not
    /// by this function calling itself: a chain of parents is as long
    /// as the source makes it, and the call stack is not.
    fn collect_fields(&self, ids: &[Id]) -> HashMap<Id, Fields> {
        let mut known: HashMap<Id, Fields> = HashMap::new();
        // The structs whose parents are being worked out, so that a
        // cycle of inheritance ends.
        let mut waiting: HashSet<Id> = HashSet::new();
        for &start in ids {
            let mut to_do = vec![start];
            // `last` looks at the struct on top of the list without
            // taking it off.
            while let Some(&id) = to_do.last() {
                if known.contains_key(&id) {
                    to_do.pop();
                    continue;
                }
                waiting.insert(id);
                // A parent that is waiting is above this struct: a cycle
                // of inheritance, which has been reported.
                let parent = self.parents[&id]
                    .iter()
                    .map(|parent| parent.id)
                    .find(|parent| !known.contains_key(parent) && !waiting.contains(parent));
                match parent {
                    Some(parent) => to_do.push(parent),
                    None => {
                        let fields = self.fields_of(id, &known);
                        known.insert(id, fields);
                        waiting.remove(&id);
                        to_do.pop();
                    }
                }
            }
        }
        known
    }

    /// The fields of the struct `id`, from those of its parents in
    /// `known`.
    fn fields_of(&self, id: Id, known: &HashMap<Id, Fields>) -> Fields {
        let mut inherited: Vec<Introduced> = Vec::new();
        for (via, parent) in self.parents[&id].iter().enumerate() {
            // A parent with no fields is part of a cycle of
            // inheritance, which has been reported.
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
                    ty: substitute_within(&field.ty, &map),
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
        Fields { inherited, all }
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
            // Every field of this name the struct has from a parent: two
            // parents may each give one.
            let mut from_parents = inherited
                .iter()
                .filter(|parent| parent.name == field.name)
                .peekable();
            let inherits = from_parents.peek().is_some();
            match (&field.ty, inherits) {
                // A field of its own.
                (Some(_), false) => {}
                (Some(ty), true) => {
                    // The first of them this one is not. One whose type
                    // is not known is passed over: another may be.
                    let conflict =
                        from_parents.find(|parent| differs(parent, ty, field.null_default));
                    if let Some(parent) = conflict {
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
                (None, true) => {}
                (None, false) => problems.push((
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

    /// The structs a value of type `ty` holds, each with whether it is
    /// reached through the elements of a collection: a list, a set, a
    /// dictionary or a window.
    ///
    /// A generic struct holds what its fields hold once its arguments
    /// stand for its parameters: with `struct Boxed<T> { values: list<T> }`,
    /// a `Boxed<Node>` holds `Node`s as the elements of a list. So the
    /// fields of each generic struct named are followed, with its
    /// arguments in place.
    fn holds(&self, ty: &Type) -> Vec<Held> {
        let mut found = Vec::new();
        self.walk(ty, false, &mut found, &mut Vec::new(), &mut { EXPANSIONS });
        found
    }

    /// Adds to `found` the structs that inherit from the abstract struct
    /// `id` and may be the application of it that `arguments` write: a
    /// value of that type is a value of one of them.
    ///
    /// `struct Child : Base<i64>` is a `Base<i64>` and no `Base<str>`. A
    /// struct that gives its parent one of its own parameters,
    /// `struct Child<U> : Base<U>`, may be any application. What a
    /// struct's own parameters are is not worked out, so every struct
    /// that inherits from one found may be one too.
    fn descendants(
        &self,
        id: Id,
        arguments: &[Argument],
        in_collection: bool,
        found: &mut Vec<Held>,
    ) {
        let mut seen: HashSet<Id> = HashSet::new();
        // The structs whose children are still to look at, each with
        // the arguments it is known to be applied to, when it is known.
        let mut to_do: Vec<(Id, Option<&[Argument]>)> = vec![(id, Some(arguments))];
        while let Some((parent, given)) = to_do.pop() {
            let Some(children) = self.children.get(&parent) else {
                continue;
            };
            for (child, applied) in children {
                let fits = given.is_none_or(|given| {
                    given.len() == applied.len()
                        && given
                            .iter()
                            .zip(applied)
                            .all(|(given, applied)| may_be(given, applied))
                });
                // `insert` is false for a struct already seen.
                if fits && seen.insert(*child) {
                    found.push(Held {
                        id: *child,
                        in_collection,
                        written: id,
                    });
                    to_do.push((*child, None));
                }
            }
        }
    }

    /// Adds to `found` what a value of type `ty` holds. `path` is the
    /// generic structs whose fields are being followed, none of which is
    /// followed again inside itself; `budget` is how many more fields
    /// may be followed.
    fn walk(
        &self,
        ty: &Type,
        in_collection: bool,
        found: &mut Vec<Held>,
        path: &mut Vec<Id>,
        budget: &mut u32,
    ) {
        match ty {
            Type::Named {
                module,
                declaration,
                arguments,
                ..
            } => {
                let id = (*module, *declaration);
                found.push(Held {
                    id,
                    in_collection,
                    written: id,
                });
                // TYPE-6: "A field whose type is an abstract struct
                // holds every struct that inherits from it."
                self.descendants(id, arguments, in_collection, found);
                for argument in arguments {
                    if let Argument::Type(argument) = argument {
                        self.walk(argument, in_collection, found, path, budget);
                    }
                }
                // Only a struct given arguments has fields that depend
                // on them.
                let (Some(fields), false) = (self.fields.get(&id), arguments.is_empty()) else {
                    return;
                };
                // One generic struct inside another is followed as deep
                // as `DEPTH`, so that this function, which calls itself
                // for each, keeps little on the call stack.
                if path.contains(&id) || path.len() >= DEPTH {
                    return;
                }
                let map: Vec<(Location, &Argument)> = self
                    .struct_type(id)
                    .map(|read| {
                        read.generics
                            .iter()
                            .map(|generic| generic.at)
                            .zip(arguments)
                            .collect()
                    })
                    .unwrap_or_default();
                path.push(id);
                for field in &fields.all {
                    // A field that names no parameter holds the same
                    // whatever the arguments, and is its struct's own
                    // edge in the graph.
                    if !names_parameter(&field.ty) {
                        continue;
                    }
                    if *budget == 0 {
                        break;
                    }
                    *budget -= 1;
                    let given = substitute_within(&field.ty, &map);
                    self.walk(&given, in_collection, found, path, budget);
                }
                path.pop();
            }
            Type::Tuple(elements) => {
                for element in elements {
                    self.walk(element, in_collection, found, path, budget);
                }
            }
            Type::List { element, .. } => self.walk(element, true, found, path, budget),
            Type::Set(element) => self.walk(element, true, found, path, budget),
            Type::Dict { key, value } => {
                self.walk(key, true, found, path, budget);
                self.walk(value, true, found, path, budget);
            }
            Type::Rolling { value, .. } => self.walk(value, true, found, path, budget),
            // Inside an `atomic<...>` a collection is part of one
            // snapshot. Only a collection outside it is on the path.
            Type::Atomic(inner) => {
                let mut inside = Vec::new();
                self.walk(inner, false, &mut inside, path, budget);
                found.extend(inside.into_iter().map(|held| Held {
                    in_collection,
                    ..held
                }));
            }
            Type::Ref(inner) | Type::Delta(inner) | Type::Nullable(inner) => {
                self.walk(inner, in_collection, found, path, budget);
            }
            Type::Result { value, error } => {
                self.walk(value, in_collection, found, path, budget);
                self.walk(error, in_collection, found, path, budget);
            }
            Type::Scalar(_)
            | Type::Parameter { .. }
            | Type::Signal
            | Type::Error
            | Type::Unknown => {}
        }
    }

    /// TYPE-6: "Recursive struct edges stay within their defining
    /// module, use an optional `atomic<...>` field defaulted to null ...
    /// Descendants cannot remove that null default. Cycles through ...
    /// collection elements are rejected."
    fn recursion(&mut self, ids: &[Id]) {
        // A graph with a node for each struct, and an edge from each to
        // every struct it holds: the structs its fields hold, those it
        // has from its parents included, with the parents' arguments in
        // place. A value of an abstract struct's type is a value of one
        // of the structs that inherit from it, so an abstract struct
        // has an edge to each of those. A struct that holds itself,
        // directly or through others, is in a strongly connected group.
        let mut graph: DiGraph<Id, ()> = DiGraph::new();
        let nodes: HashMap<Id, NodeIndex> =
            ids.iter().map(|&id| (id, graph.add_node(id))).collect();
        // Each field that names a struct.
        let mut edges: Vec<FieldEdge> = Vec::new();
        for &id in ids {
            let Some(fields) = self.fields.get(&id) else {
                continue;
            };
            for field in &fields.all {
                let mut named = self.holds(&field.ty);
                named.retain(|held| nodes.contains_key(&held.id));
                for held in &named {
                    graph.add_edge(nodes[&id], nodes[&held.id], ());
                }
                if !named.is_empty() {
                    edges.push(FieldEdge {
                        owner: id,
                        name: field.name.clone(),
                        origin: field.origin,
                        at: field.at,
                        via: field.via,
                        ty: field.ty.clone(),
                        // TYPE-6: an `atomic<...>` defaulted to null,
                        // or a nullable type, so that the recursion can
                        // end.
                        in_form: (matches!(field.ty, Type::Atomic(_)) && field.optional)
                            || matches!(field.ty, Type::Nullable(_)),
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

        // The fields that are recursive edges: they hold a struct of
        // the group their own struct is in. `recursive` has where each
        // is declared; `faulty` those that are not as TYPE-6 has them,
        // with what is wrong.
        let mut recursive: Vec<(Id, usize)> = Vec::new();
        let mut faulty: Vec<(&FieldEdge, String)> = Vec::new();
        for edge in &edges {
            let group = group_of[&edge.owner];
            let back: Vec<&Held> = edge
                .named
                .iter()
                .filter(|held| group_of[&held.id] == group)
                .collect();
            if back.is_empty() {
                continue;
            }
            recursive.push(edge.origin);
            // "Recursive fields stay within their defining module." A
            // struct of another module is reached back through an
            // abstract struct declared there.
            let abroad = back.iter().find(|held| held.written.0 != edge.owner.0);
            // "Cycles through ... collection elements outside the
            // `atomic<...>`" are rejected; inside it a collection is
            // part of one snapshot.
            let through_collection = back.iter().any(|held| held.in_collection);
            if let Some(held) = abroad {
                faulty.push((
                    edge,
                    format!(
                        " through `{}` of module `{}`: a recursive field stays within its \
                         module",
                        self.declaration(held.written).name,
                        self.program.modules[held.written.0].path
                    ),
                ));
            } else if through_collection {
                faulty.push((
                    edge,
                    " through the elements of a collection outside an `atomic<...>`".to_string(),
                ));
            } else if !edge.in_form {
                faulty.push((
                    edge,
                    ": a recursive field is an `atomic<...>` with a default of null, or nullable"
                        .to_string(),
                ));
            } else if let Some(grown) = grown(&edge.ty, &|id| group_of.get(&id) == Some(&group)) {
                // "Such a field gives a struct of its cycle each
                // generic parameter whole ... and no type built from
                // one."
                faulty.push((
                    edge,
                    format!(
                        ": `{grown}` is given a type built from a generic parameter, a new \
                         type at every step"
                    ),
                ));
            }
        }

        let mut reports = Vec::new();
        for (edge, fault) in &faulty {
            let owner = &self.declaration(edge.owner).name;
            // A field from a parent is at fault where it is declared
            // when its type there holds this struct whatever the
            // arguments; otherwise the fault is in what this struct
            // gives the parent, and is reported where it names it.
            let declared_so = edge.origin.0 == edge.owner
                || self
                    .struct_type(edge.origin.0)
                    .and_then(|read| read.fields.get(edge.origin.1))
                    .and_then(|field| field.ty.as_ref())
                    .is_some_and(|ty| {
                        self.holds(ty)
                            .iter()
                            .any(|held| group_of.get(&held.id) == group_of.get(&edge.owner))
                    });
            if declared_so {
                reports.push((
                    edge.at,
                    format!("`{}` makes `{owner}` hold itself{fault}", edge.name),
                ));
                continue;
            }
            let Some(parent) = self.parents[&edge.owner].get(edge.via) else {
                continue;
            };
            reports.push((
                parent.at,
                format!(
                    "`{owner}` holds itself through `{}`, a field of `{}`{fault}",
                    edge.name,
                    self.declaration(edge.origin.0).name
                ),
            ));
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
                        parent.name == field.name && recursive.contains(&parent.origin)
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
        // One report for one place: two parents' paths may find the
        // same fault.
        reports.sort_by_key(|(at, _)| (at.file.index(), at.span.start));
        reports.dedup();
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

/// True when the argument a struct gives its parent, `applied`, may be
/// the argument `given` that a type writes for that parent. One that
/// names a generic parameter may be anything.
fn may_be(given: &Argument, applied: &Argument) -> bool {
    match (given, applied) {
        (Argument::Type(given), Argument::Type(applied)) => {
            names_parameter(given) || names_parameter(applied) || agree(given, applied)
        }
        (Argument::Constant(given), Argument::Constant(applied)) => {
            let open = |constant: &Constant| {
                matches!(
                    constant.value,
                    ConstantValue::Parameter { .. } | ConstantValue::Unevaluated
                )
            };
            open(given) || open(applied) || given == applied
        }
        // A type against a constant has been reported as the wrong
        // kind of argument.
        _ => true,
    }
}

/// The first type inside `ty` that gives a struct of the group an
/// argument built from a generic parameter, as HGL writes it:
/// `Node<list<T>>`. A parameter passed on whole, `Node<T>`, and a type
/// that names none, `Node<i64>`, are not such arguments.
fn grown(ty: &Type, in_group: &dyn Fn(Id) -> bool) -> Option<String> {
    match ty {
        Type::Named {
            module,
            declaration,
            arguments,
            ..
        } => {
            let built = arguments.iter().any(|argument| match argument {
                Argument::Type(argument) => {
                    !matches!(argument, Type::Parameter { .. }) && names_parameter(argument)
                }
                Argument::Constant(_) => false,
            });
            if built && in_group((*module, *declaration)) {
                return Some(ty.to_string());
            }
            arguments.iter().find_map(|argument| match argument {
                Argument::Type(argument) => grown(argument, in_group),
                Argument::Constant(_) => None,
            })
        }
        Type::Tuple(elements) => elements.iter().find_map(|element| grown(element, in_group)),
        Type::List { element, .. } => grown(element, in_group),
        Type::Set(inner)
        | Type::Ref(inner)
        | Type::Atomic(inner)
        | Type::Delta(inner)
        | Type::Nullable(inner) => grown(inner, in_group),
        Type::Dict { key, value } => grown(key, in_group).or_else(|| grown(value, in_group)),
        Type::Rolling { value, .. } => grown(value, in_group),
        Type::Result { value, error } => grown(value, in_group).or_else(|| grown(error, in_group)),
        Type::Scalar(_) | Type::Parameter { .. } | Type::Signal | Type::Error | Type::Unknown => {
            None
        }
    }
}

/// True when a field of type `ty`, optional or not, is not the field
/// `parent` introduces. A type that is not known, or a part of one that
/// is not, differs from nothing: it has been reported, and what is known
/// of the rest is still compared (ERR-9).
fn differs(parent: &Introduced, ty: &Type, optional: bool) -> bool {
    if matches!(parent.ty, Type::Unknown) || matches!(ty, Type::Unknown) {
        return false;
    }
    !agree(&parent.ty, ty) || parent.optional != optional
}

/// True when two types are the same as far as each is known.
fn agree(left: &Type, right: &Type) -> bool {
    // Two lists of types agree when they are as long as each other and
    // agree place by place.
    let all = |left: &[Type], right: &[Type]| {
        left.len() == right.len()
            && left
                .iter()
                .zip(right)
                .all(|(left, right)| agree(left, right))
    };
    match (left, right) {
        (Type::Unknown, _) | (_, Type::Unknown) => true,
        (
            Type::Named {
                module,
                declaration,
                arguments,
                ..
            },
            Type::Named {
                module: other_module,
                declaration: other_declaration,
                arguments: other_arguments,
                ..
            },
        ) => {
            module == other_module
                && declaration == other_declaration
                && arguments.len() == other_arguments.len()
                && arguments
                    .iter()
                    .zip(other_arguments)
                    .all(|pair| match pair {
                        (Argument::Type(left), Argument::Type(right)) => agree(left, right),
                        (left, right) => left == right,
                    })
        }
        (Type::Tuple(left), Type::Tuple(right)) => all(left, right),
        (
            Type::List { element, size },
            Type::List {
                element: other_element,
                size: other_size,
            },
        ) => size == other_size && agree(element, other_element),
        (Type::Set(left), Type::Set(right))
        | (Type::Ref(left), Type::Ref(right))
        | (Type::Atomic(left), Type::Atomic(right))
        | (Type::Delta(left), Type::Delta(right))
        | (Type::Nullable(left), Type::Nullable(right)) => agree(left, right),
        (
            Type::Dict { key, value },
            Type::Dict {
                key: other_key,
                value: other_value,
            },
        ) => agree(key, other_key) && agree(value, other_value),
        (
            Type::Rolling {
                value,
                maximum,
                minimum,
            },
            Type::Rolling {
                value: other_value,
                maximum: other_maximum,
                minimum: other_minimum,
            },
        ) => maximum == other_maximum && minimum == other_minimum && agree(value, other_value),
        (
            Type::Result { value, error },
            Type::Result {
                value: other_value,
                error: other_error,
            },
        ) => agree(value, other_value) && agree(error, other_error),
        (left, right) => left == right,
    }
}
