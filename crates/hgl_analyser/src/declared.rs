//! Checks the types a program declares: its structs and its enums.
//!
//! `read` says what each written type means. This pass reads the
//! declarations against one another:
//!
//! * a struct's parents are abstract structs (TYPE-3), and no struct
//!   inherits from itself (TYPE-6);
//! * the fields a struct has from its parents and its own agree in
//!   type, `name = value` overrides a field a parent declares, and only
//!   an optional field's default is null (TYPE-4);
//! * a struct that refers back to itself does so through an optional
//!   field, `T?` or `atomic<T?>` (TYPE-6);
//! * an enum's members are numbered, each number an `i64` no other
//!   member has (TYPE-7).
//!
//! A type that is not known, or a parent already reported, is left out
//! of what follows from it (ERR-9).

use std::collections::{HashMap, HashSet};

use hgl_diagnostics::{Category, Diagnostics, Location};
use hgl_lexer::literal::{decode_float, decode_string};
use hgl_parser::{HasSpan, SyntaxKind, SyntaxNode};
use hgl_source::FileId;
use petgraph::algo::kosaraju_scc;
use petgraph::graph::{DiGraph, NodeIndex};

use crate::constant::fold;
use crate::program::{Declaration, DeclarationKind, Program};
use crate::read::is_type;
use crate::syntax::{child, has_token, tokens};
use crate::types::{
    Argument, Constant, ConstantValue, Detail, EnumMember, EnumType, FieldRef, GenericKind, Member,
    StructType, Type,
};
use crate::{codes, error};

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
        packs: HashMap::new(),
    };
    let ids = structs.ids();
    for &id in &ids {
        let packs: Vec<(Location, Cardinality)> = structs
            .struct_type(id)
            .into_iter()
            .flat_map(|read| &read.generics)
            .filter_map(|generic| match generic.kind {
                GenericKind::Pack { minimum, maximum } => Some((generic.at, (minimum, maximum))),
                _ => None,
            })
            .collect();
        structs.packs.extend(packs);
    }
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
    // What each struct has, for the passes that read constructors and
    // member accesses.
    let members: Vec<(Id, Vec<Member>)> = ids.iter().map(|&id| (id, structs.members(id))).collect();
    for ((module, declaration), members) in members {
        if let Detail::Struct(read) = &mut program.modules[module].details[declaration] {
            read.members = members;
        }
    }
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
                error(
                    Category::Type,
                    codes::ENUM_DUPLICATE_MEMBER,
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
                        diagnostics.emit(error(
                            Category::Type,
                            codes::ENUM_OVERFLOW,
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
                error(
                    Category::Type,
                    codes::ENUM_DUPLICATE_VALUE,
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
    diagnostics.emit(error(
        Category::Type,
        codes::ENUM_VALUE,
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
    /// True when one of those arguments passes on a type pack of the
    /// struct that names the parent. How many arguments it stands for,
    /// and so which parameter each of the others is given to, is then
    /// not worked out.
    forwards_pack: bool,
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
    /// True when a construction that omits it has a default to take:
    /// the one it is declared with, or one a struct on the way down
    /// gives it.
    has_default: bool,
    /// True when the default it is declared with is null.
    declared_null: bool,
    /// True when the default a construction takes is null: the one it
    /// is declared with, or the last a struct on the way down gives it
    /// (TYPE-4).
    null_default: bool,
    /// True when that default is the empty collection `[]` (TYPE-6).
    empty_default: bool,
    /// Where the field that gives that default is written: the
    /// declaration, or the last `name = value` on the way down.
    default_from: Option<Location>,
    /// That field: the struct that writes it, and its position among
    /// that struct's own fields.
    default_of: Option<(Id, usize)>,
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
    /// True when the field is optional (TYPE-4), as TYPE-6 has a
    /// recursive one.
    in_form: bool,
    /// False when the field is declared with a null default and a
    /// struct on the way down has given it another.
    kept: bool,
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
    /// The type packs the structs declare.
    packs: Packs,
}

/// A struct a value of some type holds.
#[derive(Debug, Clone, Copy)]
struct Held {
    /// The struct.
    id: Id,
    /// True when it is reached through the elements of a collection
    /// outside an `atomic<...>`.
    in_collection: bool,
    /// True when it is reached through the elements of a collection
    /// that may be empty: an unsized list, a list of size zero, a set
    /// or a dictionary (TYPE-6).
    may_be_empty: bool,
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

/// How many applications of one struct have their children looked at
/// when the descendants of an abstract struct are found.
const APPLICATIONS: usize = 16;

/// True when `ty` names a generic parameter, itself or inside it.
pub(crate) fn names_parameter(ty: &Type) -> bool {
    ty.any_part(&|part| matches!(part, Type::Parameter { .. }), &|_| false)
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
            kind,
        } => Type::Named {
            name: name.clone(),
            module: *module,
            declaration: *declaration,
            kind: *kind,
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
        Type::RollingWindow {
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
            Type::RollingWindow {
                value: part(value),
                maximum,
                minimum,
            }
        }
        Type::Ref(inner) => Type::Ref(part(inner)),
        Type::Endpoint(inner) => Type::Endpoint(part(inner)),
        // TYPE-9: formed again with what its payload now is, so that
        // `atomic<T>` with `T` an `i64` is an `i64`.
        Type::Atomic(inner) => Type::atomic(*part(inner)),
        Type::Result { value, error } => Type::Result {
            value: part(value),
            error: part(error),
        },
        // TYPE-16: "In a generic declaration it stays as written until
        // T is known", and is then what the table gives for that T.
        Type::Delta(inner) => Type::delta(substitute(inner, map)),
        Type::Trace { cell, style } => Type::Trace {
            cell: part(cell),
            style: *style,
        },
        // TYPE-14: "either a T or null". A `T?` whose T is given a
        // nullable type is that type.
        Type::Nullable(inner) => match substitute(inner, map) {
            inner @ Type::Nullable(_) => inner,
            inner => Type::Nullable(Box::new(inner)),
        },
        Type::Scalar(_) | Type::Signal | Type::Error | Type::ErrorContext | Type::Unknown => {
            ty.clone()
        }
    }
}

/// True when `ty` names one of the generic parameters declared at
/// `declared`, itself or inside it.
pub(crate) fn names_one_of(ty: &Type, declared: &[Location]) -> bool {
    if declared.is_empty() {
        return false;
    }
    ty.any_part(
        &|part| matches!(part, Type::Parameter { at, .. } if declared.contains(at)),
        &|_| false,
    )
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
        | Type::Endpoint(inner)
        | Type::Atomic(inner)
        | Type::Delta(inner)
        | Type::Trace { cell: inner, .. }
        | Type::Nullable(inner) => weight(inner),
        Type::Dict { key, value } => weight(key) + weight(value),
        Type::RollingWindow { value, .. } => weight(value),
        Type::Result { value, error } => weight(value) + weight(error),
        Type::Scalar(_)
        | Type::Parameter { .. }
        | Type::Signal
        | Type::Error
        | Type::ErrorContext
        | Type::Unknown => 0,
    }
}

/// [`substitute`], unless the result could weigh more than [`WEIGHT`]:
/// then the type is not known.
///
/// Each argument may stand in a type many times, and a chain of generic
/// parents that each give the next `tuple<T, T>` doubles a field's type
/// at every link. The weight is bounded before anything is built: no
/// more than the type's own, times the heaviest argument's.
pub(crate) fn substitute_within(ty: &Type, map: &[(Location, &Argument)]) -> Type {
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

/// TYPE-4: "A field is optional when its type is nullable, `T?`, or an
/// `atomic<...>` of a nullable type, `atomic<T?>`."
///
/// `None` when the type, or what an `atomic<...>` holds, is not known.
pub(crate) fn is_optional(ty: &Type) -> Option<bool> {
    match ty {
        Type::Unknown => None,
        Type::Nullable(_) => Some(true),
        Type::Atomic(inner) => match inner.as_ref() {
            Type::Unknown => None,
            inner => Some(matches!(inner, Type::Nullable(_))),
        },
        _ => Some(false),
    }
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
                    ..
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
                                    codes::STRUCT_DUPLICATE_PARENT,
                                    at,
                                    format!("`{name}` is listed twice among the parents"),
                                    Some((first.at, "first listed here")),
                                ));
                            } else {
                                let forwards_pack = arguments.iter().any(|argument| {
                                    matches!(argument, Argument::Type(Type::Parameter { at, .. })
                                    if read.generics.iter().any(|generic| {
                                        generic.at == *at
                                            && matches!(generic.kind, GenericKind::Pack { .. })
                                    }))
                                });
                                parents.push(Parent {
                                    id: parent,
                                    arguments: arguments.clone(),
                                    forwards_pack,
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
                        codes::STRUCT_PARENT,
                        at,
                        format!(
                            "`{name}` is {what} and cannot be a parent: only an abstract \
                             struct may be"
                        ),
                        Some((declared.at, "declared here")),
                    ));
                }
                other => problems.push((
                    codes::STRUCT_PARENT,
                    at,
                    format!("`{other}` cannot be a parent: only an abstract struct may be"),
                    None,
                )),
            }
        }
        for (code, at, message, related) in problems {
            let mut diagnostic = error(Category::Type, code, at, message, "TYPE-3");
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
            self.diagnostics.emit(error(
                Category::Type,
                codes::STRUCT_INHERITANCE_CYCLE,
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
            for field in &fields.all {
                // With a pack passed on among the arguments, which one
                // a parameter is given is not known: a field's type
                // that names a parameter is not known either.
                let ty = if parent.forwards_pack && names_parameter(&field.ty) {
                    Type::Unknown
                } else {
                    self.in_place(&field.ty, parent.id, &parent.arguments)
                };
                let field = Introduced {
                    ty,
                    via,
                    ..field.clone()
                };
                // The same field by another path is one field.
                let listed = inherited
                    .iter()
                    .any(|other| other.origin == field.origin && other.ty == field.ty);
                if !listed {
                    inherited.push(field);
                }
            }
        }
        let mut all = inherited.clone();
        // A default this struct gives a field a parent has, by
        // `name = value` or by declaring the field again, is the one a
        // construction takes, for this struct and those below it.
        if let Some(read) = self.struct_type(id) {
            let written = read.fields.iter().enumerate();
            for (index, field) in written.filter(|(_, field)| field.has_default) {
                for inherited in all.iter_mut().filter(|parent| parent.name == field.name) {
                    inherited.has_default = true;
                    inherited.null_default = field.null_default;
                    inherited.empty_default = field.empty_default;
                    inherited.default_from = Some(field.at);
                    inherited.default_of = Some((id, index));
                }
            }
        }
        if let Some(read) = self.struct_type(id) {
            for (index, field) in read.fields.iter().enumerate() {
                // `name = value` introduces no field.
                if let Some(ty) = &field.ty {
                    all.push(Introduced {
                        name: field.name.clone(),
                        ty: ty.clone(),
                        has_default: field.has_default,
                        declared_null: field.null_default,
                        null_default: field.null_default,
                        empty_default: field.empty_default,
                        default_from: field.has_default.then_some(field.at),
                        default_of: field.has_default.then_some((id, index)),
                        origin: (id, index),
                        at: field.at,
                        via: 0,
                    });
                }
            }
        }
        Fields { inherited, all }
    }

    /// A field's type `ty`, of the generic struct `id`, with `arguments`
    /// in place of that struct's parameters.
    ///
    /// Each parameter before a type pack takes the argument in its
    /// place. A pack takes a sequence of them (GEN-2), which this pass
    /// does not put in place: a type that names the pack is not known.
    /// A wrong count of arguments has been reported, and leaves the
    /// parameters past it as they are.
    fn in_place(&self, ty: &Type, id: Id, arguments: &[Argument]) -> Type {
        let Some(read) = self.struct_type(id) else {
            return ty.clone();
        };
        let fixed = read
            .generics
            .iter()
            .take_while(|generic| !matches!(generic.kind, GenericKind::Pack { .. }));
        let map: Vec<(Location, &Argument)> =
            fixed.map(|generic| generic.at).zip(arguments).collect();
        let packs: Vec<Location> = read
            .generics
            .iter()
            .filter(|generic| matches!(generic.kind, GenericKind::Pack { .. }))
            .map(|generic| generic.at)
            .collect();
        if names_one_of(ty, &packs) {
            return Type::Unknown;
        }
        substitute_within(ty, &map)
    }

    /// Every field the struct `id` has, its parents' first. A field
    /// the struct declares again is listed once, as the struct has it.
    fn members(&self, id: Id) -> Vec<Member> {
        let mut members: Vec<Member> = Vec::new();
        let Some(fields) = self.fields.get(&id) else {
            return members;
        };
        for field in &fields.all {
            let member = Member {
                name: field.name.clone(),
                at: field.at,
                ty: field.ty.clone(),
                has_default: field.has_default,
                default: field
                    .default_of
                    .map(|((module, declaration), field)| FieldRef {
                        module,
                        declaration,
                        field,
                    }),
            };
            match members.iter_mut().find(|known| known.name == member.name) {
                Some(known) => *known = member,
                None => members.push(member),
            }
        }
        members
    }

    /// The struct `id` and every struct it inherits from.
    fn line(&self, id: Id) -> HashSet<Id> {
        let mut line: HashSet<Id> = HashSet::new();
        let mut to_do = vec![id];
        // Each struct is added once, so a cycle of inheritance ends.
        while let Some(current) = to_do.pop() {
            if line.insert(current) {
                to_do.extend(self.parents[&current].iter().map(|parent| parent.id));
            }
        }
        line
    }

    /// The fields the struct `id` has from its parents.
    fn inherited(&self, id: Id) -> Vec<Introduced> {
        self.fields
            .get(&id)
            .map(|fields| fields.inherited.clone())
            .unwrap_or_default()
    }

    /// TYPE-4: "A typed field introduction fixes its type and
    /// optionality. A field is optional when its type is nullable ...
    /// and only an optional field's default may be null. An untyped
    /// inherited override changes only the construction default of a
    /// field a parent has ... a conflict is an error."
    ///
    /// Optionality follows from the type, so two fields that agree in
    /// type agree in it.
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
                    && differs(earlier, &later.ty)
            });
            if let Some(earlier) = conflict {
                problems.push((
                    codes::FIELD_CONFLICT,
                    self.parents[&id][later.via].at,
                    format!(
                        "`{name}` has `{}` from two parents: `{}` and `{}`",
                        later.name, earlier.ty, later.ty
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
                    codes::FIELD_DUPLICATE,
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
            // "Only an optional field's default may be null." The type
            // is the member's own, or for `name = null` the one each
            // parent gives the field, this struct's arguments in place.
            if field.null_default {
                let required = match &field.ty {
                    Some(ty) => (is_optional(ty) == Some(false)).then(|| (ty.clone(), field.at)),
                    None => inherited
                        .iter()
                        .filter(|parent| parent.name == field.name)
                        .find(|parent| is_optional(&parent.ty) == Some(false))
                        .map(|parent| (parent.ty.clone(), parent.at)),
                };
                if let Some((ty, declared)) = required {
                    problems.push((
                        codes::FIELD_NULL_DEFAULT,
                        field.default_at.unwrap_or(field.at),
                        format!(
                            "`{}` is `{ty}`, which is not optional, and its default is \
                             null: an optional field's type is nullable, `T?`, or an \
                             `atomic<T?>`",
                            field.name
                        ),
                        declared,
                    ));
                }
            }
            match (&field.ty, inherits) {
                // A field of its own.
                (Some(_), false) => {}
                (Some(ty), true) => {
                    // The first of them this one is not. One whose type
                    // is not known is passed over: another may be.
                    let conflict = from_parents.find(|parent| differs(parent, ty));
                    if let Some(parent) = conflict {
                        problems.push((
                            codes::FIELD_CONFLICT,
                            field.at,
                            format!(
                                "`{}` is `{ty}` here and `{}` in `{}`",
                                field.name,
                                parent.ty,
                                self.declaration(parent.origin.0).name
                            ),
                            parent.at,
                        ));
                    }
                }
                (None, true) => {}
                (None, false) => problems.push((
                    codes::FIELD_NOT_INHERITED,
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
                codes::FIELD_NOT_INHERITED => "the struct",
                codes::FIELD_DUPLICATE => "first declared here",
                codes::FIELD_NULL_DEFAULT => "the field",
                _ => "the other declaration",
            };
            self.diagnostics.emit(
                error(Category::Type, code, at, message, "TYPE-4").with_related(related, note),
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
        self.walk(ty, false, false, &mut found, &mut Vec::new(), &mut {
            EXPANSIONS
        });
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
        may_be_empty: bool,
        found: &mut Vec<Held>,
    ) {
        // The structs found, each added once.
        let mut held: HashSet<Id> = HashSet::new();
        // What each struct was applied to when its children were looked
        // at. A struct reached by two lines of parents is a descendant
        // by either, each with what that line gives it: its children
        // are looked at again for an application not yet followed.
        let mut followed: HashMap<Id, Vec<Vec<Option<Argument>>>> = HashMap::new();
        // The structs whose children are still to look at, each with
        // what is known of the arguments it is applied to: one for each
        // of its generic parameters, or `None` where it is not known.
        let known: Vec<Option<Argument>> = arguments.iter().cloned().map(Some).collect();
        let mut to_do: Vec<(Id, Vec<Option<Argument>>)> = vec![(id, known)];
        while let Some((parent, given)) = to_do.pop() {
            let Some(children) = self.children.get(&parent) else {
                continue;
            };
            for (child, applied) in children {
                // The child is this application of its parent when each
                // argument it gives the parent may be the one known,
                // its own parameters standing for one thing throughout.
                // A wrong count has been reported; what is there is
                // compared.
                let mut matching = Matching {
                    bound: Vec::new(),
                    packs: &self.packs,
                    open: false,
                };
                // What is known of the parent's arguments is in the
                // places of what the child writes, but where the parent
                // has a type pack of its own: one entry then stands
                // for a run of arguments.
                let unplaced = given.iter().any(Option::is_none)
                    && self.struct_type(parent).is_some_and(|read| {
                        read.generics
                            .iter()
                            .any(|generic| matches!(generic.kind, GenericKind::Pack { .. }))
                    });
                let packs = &self.packs;
                let pack_of = |argument: &Argument| match argument {
                    Argument::Type(ty) => pack_of(ty, packs),
                    Argument::Constant(_) => None,
                };
                // An argument not known may be anything.
                let mut each = |given: &Option<Argument>, applied: &Argument| {
                    given
                        .as_ref()
                        .is_none_or(|given| may_be(given, applied, &mut matching))
                };
                let fits = if !unplaced {
                    in_sequence(
                        &given,
                        applied,
                        &|given| given.as_ref().is_some_and(|given| pack_of(given).is_some()),
                        &pack_of,
                        &mut each,
                    )
                } else if applied.iter().any(|applied| pack_of(applied).is_some()) {
                    // A pack passed on to a parent that has one: the
                    // places are not known.
                    true
                } else {
                    given
                        .iter()
                        .zip(applied)
                        .all(|(given, applied)| each(given, applied))
                };
                let bound = matching.bound;
                if !fits {
                    continue;
                }
                // `insert` is false for a struct already there.
                if held.insert(*child) {
                    found.push(Held {
                        id: *child,
                        in_collection,
                        may_be_empty,
                        written: id,
                    });
                }
                // What the child is applied to, for its own children:
                // what was found for each of its parameters.
                let passed_on: Vec<Option<Argument>> = self
                    .struct_type(*child)
                    .map(|read| {
                        read.generics
                            .iter()
                            .map(|generic| {
                                bound
                                    .iter()
                                    .find(|(at, _)| *at == generic.at)
                                    .map(|(_, argument)| argument.clone())
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                // An application already followed finds nothing new,
                // and a struct is followed a bounded number of times,
                // so that structs that inherit from one another, which
                // have been reported, end.
                let applications = followed.entry(*child).or_default();
                if applications.contains(&passed_on) || applications.len() >= APPLICATIONS {
                    continue;
                }
                applications.push(passed_on.clone());
                to_do.push((*child, passed_on));
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
        may_be_empty: bool,
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
                    may_be_empty,
                    written: id,
                });
                // TYPE-6: "A field whose type is an abstract struct
                // holds every struct that inherits from it."
                self.descendants(id, arguments, in_collection, may_be_empty, found);
                for argument in arguments {
                    if let Argument::Type(argument) = argument {
                        self.walk(argument, in_collection, may_be_empty, found, path, budget);
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
                    let given = self.in_place(&field.ty, id, arguments);
                    self.walk(&given, in_collection, may_be_empty, found, path, budget);
                }
                path.pop();
            }
            Type::Tuple(elements) => {
                for element in elements {
                    self.walk(element, in_collection, may_be_empty, found, path, budget);
                }
            }
            // TYPE-6: "a collection that may be empty, an unsized list, a
            // list of size zero, a set, or a dictionary".
            Type::List { element, size } => {
                let empty = match size {
                    None => true,
                    Some(size) => {
                        matches!(size.value, ConstantValue::Int(0) | ConstantValue::Unbounded)
                    }
                };
                self.walk(element, true, may_be_empty || empty, found, path, budget);
            }
            Type::Set(element) => self.walk(element, true, true, found, path, budget),
            Type::Dict { key, value } => {
                self.walk(key, true, true, found, path, budget);
                self.walk(value, true, true, found, path, budget);
            }
            Type::RollingWindow { value, .. } => {
                self.walk(value, true, may_be_empty, found, path, budget);
            }
            // Inside an `atomic<...>` a collection is part of one
            // snapshot. Only a collection outside it is on the path.
            Type::Atomic(inner) => {
                let mut inside = Vec::new();
                self.walk(inner, false, may_be_empty, &mut inside, path, budget);
                found.extend(inside.into_iter().map(|held| Held {
                    in_collection,
                    ..held
                }));
            }
            Type::Ref(inner)
            | Type::Endpoint(inner)
            | Type::Delta(inner)
            | Type::Trace { cell: inner, .. }
            | Type::Nullable(inner) => {
                self.walk(inner, in_collection, may_be_empty, found, path, budget);
            }
            Type::Result { value, error } => {
                self.walk(value, in_collection, may_be_empty, found, path, budget);
                self.walk(error, in_collection, may_be_empty, found, path, budget);
            }
            Type::Scalar(_)
            | Type::Parameter { .. }
            | Type::Signal
            | Type::Error
            | Type::ErrorContext
            | Type::Unknown => {}
        }
    }

    /// TYPE-6: "Recursive fields stay within their defining module. So
    /// that the recursion can end, a recursive field has a value that
    /// holds no struct of its cycle. It is optional (TYPE-4) ... or,
    /// inside an `atomic<...>`, it holds the struct only as elements of
    /// a collection that may be empty ... A recursive field's default,
    /// its own or a descendant's, is such a value: `null`, or the empty
    /// collection `[]`. Descendants cannot remove a null default it has
    /// ... Cycles through ... collection elements outside an
    /// `atomic<...>`" are rejected.
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
                        // TYPE-6: optional, so that the recursion can
                        // end. The type is the one this struct has the
                        // field with, its parents' arguments in place.
                        in_form: is_optional(&field.ty) != Some(false),
                        kept: !field.declared_null || field.null_default,
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
        let mut recursive: Vec<(Id, (Id, usize))> = Vec::new();
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
            // Recursive for the struct that has it: a parent's field of
            // type `atomic<T>` is one in `Node : Base<Node>` and not in
            // `Value : Base<i64>`.
            recursive.push((edge.owner, edge.origin));
            // "Recursive fields stay within their defining module." A
            // struct of another module is reached back through an
            // abstract struct declared there.
            let abroad = back.iter().find(|held| held.written.0 != edge.owner.0);
            // "Cycles through ... collection elements outside the
            // `atomic<...>`" are rejected; inside it a collection is
            // part of one snapshot.
            let through_collection = back.iter().any(|held| held.in_collection);
            // A struct that itself gives the field a default other than
            // null is reported for that, below, where it writes it.
            let removed_here = self.struct_type(edge.owner).is_some_and(|read| {
                read.fields.iter().any(|field| {
                    field.has_default && !field.null_default && field.name == edge.name
                })
            });
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
            } else if !edge.in_form && !back.iter().all(|held| held.may_be_empty) {
                // Optional, and so able to be null; or holding the
                // struct only as elements of a collection that may be
                // empty.
                faulty.push((
                    edge,
                    ": a recursive field is optional, its type nullable or an `atomic<...>` \
                     of a nullable type, or holds its struct only as elements of a \
                     collection that may be empty"
                        .to_string(),
                ));
            } else if !edge.kept && !removed_here {
                // "Descendants cannot remove a null default it has."
                faulty.push((
                    edge,
                    ": a struct between has removed its null default".to_string(),
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

        // "A recursive field's default, its own or a descendant's, is
        // such a value: `null`, or the empty collection `[]`.
        // Descendants cannot remove a null default it has, not for an
        // empty collection either."
        let faulty_fields: HashSet<(Id, usize)> =
            faulty.iter().map(|(edge, _)| edge.origin).collect();
        for &id in ids {
            let Some(read) = self.struct_type(id) else {
                continue;
            };
            // Only a struct that gives a field a default other than
            // null has anything to check.
            if !read
                .fields
                .iter()
                .any(|field| field.has_default && !field.null_default)
            {
                continue;
            }
            let inherited = self.inherited(id);
            // The struct and every struct it inherits from: a field is
            // recursive here when it is in any of them.
            let line = self.line(id);
            let is_recursive = |origin: (Id, usize)| {
                recursive
                    .iter()
                    .any(|(owner, declared)| *declared == origin && line.contains(owner))
            };
            for field in &read.fields {
                if !field.has_default || field.null_default {
                    continue;
                }
                // The field as a parent declares it, when this struct
                // gives a parent's field its default.
                let from_parents: Vec<&Introduced> = inherited
                    .iter()
                    .filter(|parent| parent.name == field.name && is_recursive(parent.origin))
                    .collect();
                if from_parents.iter().any(|parent| parent.declared_null) {
                    // Only a field declared with a null default has one
                    // to keep.
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
        // The default each struct has for a field that is recursive in
        // it: the one the field is declared with, or the last a struct
        // on the way down gave it. A field may be recursive only in a
        // struct below the one that writes its default, so the default
        // is judged where the field is recursive and reported where it
        // is written. One reported above for its type is not reported
        // for its default as well.
        for (owner, origin) in &recursive {
            if faulty_fields.contains(origin) {
                continue;
            }
            let Some(fields) = self.fields.get(owner) else {
                continue;
            };
            for field in fields.all.iter().filter(|field| field.origin == *origin) {
                // A null default that a struct below has replaced is
                // reported as that.
                let ends = !field.has_default || field.null_default || field.empty_default;
                if ends || field.declared_null {
                    continue;
                }
                // A place reported already, for a null default removed
                // there, is not reported again.
                if let Some(at) = field.default_from
                    && !reports.iter().any(|(reported, _)| *reported == at)
                {
                    reports.push((
                        at,
                        format!(
                            "`{}` is a recursive field, and its default is `null` or the \
                             empty collection `[]`",
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
            self.diagnostics.emit(error(
                Category::Type,
                codes::FIELD_RECURSIVE,
                at,
                message,
                "TYPE-6",
            ));
        }
    }
}

/// What a struct's generic parameters stand for when it is taken to be
/// one application of its parent: each parameter, by where it is
/// declared, with the argument found for it.
type Bindings = Vec<(Location, Argument)>;

/// The fewest and the most types a type pack holds (GEN-10); `None`
/// for no most.
type Cardinality = (u64, Option<u64>);

/// Every type pack the program's structs declare, by where it is
/// declared, with its cardinality.
type Packs = HashMap<Location, Cardinality>;

/// What matching a struct against one application of its parent has
/// found, and what it reads.
struct Matching<'a> {
    /// What has been found for the struct's parameters.
    bound: Bindings,
    /// The type packs of the program's structs.
    packs: &'a Packs,
    /// True when both sides are written by the struct that has the
    /// field: a parameter on either side may then be anything, and
    /// nothing is bound.
    open: bool,
}

/// True when a constant is one this pass cannot compare: it names a
/// generic parameter, or is an expression not evaluated.
fn is_open(constant: &Constant) -> bool {
    matches!(
        constant.value,
        ConstantValue::Parameter { .. } | ConstantValue::Unevaluated
    )
}

/// True when the argument a struct gives its parent, `applied`, may be
/// the argument `given` that a type writes for that parent. `bound` is
/// what has been found for the struct's parameters, and gains what
/// this argument finds.
///
/// A parameter of the struct stands for whatever is given in its
/// place, and for one thing wherever it is written: with
/// `struct Child<U> : Base<U, U>`, a `Base<i64, str>` is no `Child`. One
/// inside a type is matched there: with `Base<list<U>>`, a `Base<str>`
/// is no `Child`. What is not known, and a parameter of the struct that
/// writes `given`, may be anything.
fn may_be(given: &Argument, applied: &Argument, found: &mut Matching<'_>) -> bool {
    match (given, applied) {
        (Argument::Type(given), Argument::Type(applied)) => type_may_be(given, applied, found),
        (Argument::Constant(given), Argument::Constant(applied)) => {
            constant_may_be(given, applied, found)
        }
        // A type against a constant has been reported as the wrong
        // kind of argument.
        _ => true,
    }
}

/// The cardinality of the type pack `ty` names, when it names one.
fn pack_of(ty: &Type, packs: &Packs) -> Option<Cardinality> {
    match ty {
        Type::Parameter { at, .. } => packs.get(at).copied(),
        _ => None,
    }
}

/// [`may_be`], for the arguments one generic type is given.
fn arguments_may_be(given: &[Argument], applied: &[Argument], found: &mut Matching<'_>) -> bool {
    let packs = found.packs;
    let pack_of = |argument: &Argument| match argument {
        Argument::Type(ty) => pack_of(ty, packs),
        Argument::Constant(_) => None,
    };
    in_sequence(
        given,
        applied,
        &|given| pack_of(given).is_some(),
        &pack_of,
        &mut |given, applied| may_be(given, applied, found),
    )
}

/// True when the sequence a struct writes, `applied`, may be the
/// sequence `given`: the arguments of a generic type, or the elements
/// of a tuple. `passes_on` says whether an item of `given` passes on a
/// type pack, `pack_of` gives the cardinality of one of `applied` that
/// does, and `each` matches one item with another.
///
/// A type pack passed on stands for a run of items (GEN-10), as many
/// as the items before and after it leave, which is a number the pack
/// admits. With `struct Child<...Us> : Base<Row<Us, i64>>`, a
/// `Base<Row<str, f64, i64>>` may be a `Child`, `Us` holding `str` and
/// `f64`; a `Base<Row<str, f64>>` is none. Which types a pack holds is
/// not recorded. Where `given` passes on a pack, or `applied` two, the
/// places are not known and the sequences may be one.
fn in_sequence<G, A>(
    given: &[G],
    applied: &[A],
    passes_on: &dyn Fn(&G) -> bool,
    pack_of: &dyn Fn(&A) -> Option<Cardinality>,
    each: &mut dyn FnMut(&G, &A) -> bool,
) -> bool {
    if given.iter().any(passes_on) {
        return true;
    }
    let mut passed_on = applied
        .iter()
        .enumerate()
        .filter_map(|(index, item)| pack_of(item).map(|cardinality| (index, cardinality)));
    let Some((index, (minimum, maximum))) = passed_on.next() else {
        // No pack: item for item.
        return given.len() == applied.len()
            && given
                .iter()
                .zip(applied)
                .all(|(given, applied)| each(given, applied));
    };
    if passed_on.next().is_some() {
        return true;
    }
    // The items after the pack are the last of `given`.
    let after = applied.len() - index - 1;
    let Some(held) = given.len().checked_sub(index + after) else {
        return false;
    };
    let held = held as u64;
    if held < minimum || maximum.is_some_and(|maximum| held > maximum) {
        return false;
    }
    let before = given[..index].iter().zip(&applied[..index]);
    let last = given[given.len() - after..]
        .iter()
        .zip(&applied[index + 1..]);
    before
        .chain(last)
        .all(|(given, applied)| each(given, applied))
}

/// [`may_be`], for a type.
fn type_may_be(given: &Type, applied: &Type, found: &mut Matching<'_>) -> bool {
    match (given, applied) {
        // In what a struct gives its parent, a parameter is the
        // struct's own.
        (_, Type::Parameter { at, .. }) => {
            found.open || bind(*at, Argument::Type(given.clone()), found)
        }
        (Type::Unknown | Type::Parameter { .. }, _) | (_, Type::Unknown) => true,
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
                && arguments_may_be(arguments, other_arguments, found)
        }
        (Type::Tuple(given), Type::Tuple(applied)) => {
            let packs = found.packs;
            in_sequence(
                given,
                applied,
                &|ty| pack_of(ty, packs).is_some(),
                &|ty| pack_of(ty, packs),
                &mut |given, applied| type_may_be(given, applied, found),
            )
        }
        (
            Type::List { element, size },
            Type::List {
                element: other_element,
                size: other_size,
            },
        ) => {
            let sizes = match (size, other_size) {
                (Some(size), Some(other_size)) => constant_may_be(size, other_size, found),
                (size, other_size) => size.is_none() == other_size.is_none(),
            };
            sizes && type_may_be(element, other_element, found)
        }
        (Type::Set(given), Type::Set(applied))
        | (Type::Ref(given), Type::Ref(applied))
        | (Type::Endpoint(given), Type::Endpoint(applied))
        | (Type::Atomic(given), Type::Atomic(applied))
        | (Type::Delta(given), Type::Delta(applied))
        | (Type::Nullable(given), Type::Nullable(applied)) => type_may_be(given, applied, found),
        (
            Type::Dict { key, value },
            Type::Dict {
                key: other_key,
                value: other_value,
            },
        ) => type_may_be(key, other_key, found) && type_may_be(value, other_value, found),
        (
            Type::RollingWindow {
                value,
                maximum,
                minimum,
            },
            Type::RollingWindow {
                value: other_value,
                maximum: other_maximum,
                minimum: other_minimum,
            },
        ) => {
            // TYPE-11: "Omitted minimum equals maximum." A window with
            // none written has its maximum for one.
            let minimum = minimum.as_ref().unwrap_or(maximum);
            let other_minimum = other_minimum.as_ref().unwrap_or(other_maximum);
            constant_may_be(maximum, other_maximum, found)
                && constant_may_be(minimum, other_minimum, found)
                && type_may_be(value, other_value, found)
        }
        (
            Type::Result { value, error },
            Type::Result {
                value: other_value,
                error: other_error,
            },
        ) => type_may_be(value, other_value, found) && type_may_be(error, other_error, found),
        // Two scalars, or two types of different forms.
        (given, applied) => given == applied,
    }
}

/// [`may_be`], for a constant.
fn constant_may_be(given: &Constant, applied: &Constant, found: &mut Matching<'_>) -> bool {
    if let ConstantValue::Parameter { at, .. } = &applied.value {
        return found.open || bind(*at, Argument::Constant(given.clone()), found);
    }
    may_be_one(given, applied)
}

/// True when two constants may be one value.
///
/// One that names a generic parameter, or is an expression not
/// evaluated, may be anything. Two literals whose values this pass does
/// not hold are read through the parentheses and the minus around them
/// (EXPR-1): `(true)` is `true`. Two floats are compared as the numbers
/// they denote (LEX-20) and two strings as their contents (LEX-2), so
/// `1.0` is `1.00`; a bool or `null` is as it is written; and two dates
/// or times written differently may be one.
fn may_be_one(left: &Constant, right: &Constant) -> bool {
    if is_open(left) || is_open(right) || left == right {
        return true;
    }
    let (ConstantValue::Other(kind), ConstantValue::Other(other_kind)) =
        (&left.value, &right.value)
    else {
        // Two values this pass holds, or one it holds and one it does
        // not: they differ.
        return false;
    };
    if kind != other_kind {
        return false;
    }
    let (negative, text) = bare_literal(&left.text);
    let (other_negative, other_text) = bare_literal(&right.text);
    match *kind {
        // A literal the lexer has reported is not read here: it may be
        // the other.
        "a float" => match (decode_float(text), decode_float(other_text)) {
            (Ok(value), Ok(other)) => {
                let signed = |value: f64, negative: bool| if negative { -value } else { value };
                signed(value, negative) == signed(other, other_negative)
            }
            _ => true,
        },
        "a string" => match (decode_string(text), decode_string(other_text)) {
            (Ok(value), Ok(other)) => value == other,
            _ => true,
        },
        "a date or time" => true,
        // `true`, `false` and `null` are as they are written.
        _ => negative == other_negative && text == other_text,
    }
}

/// The literal inside a constant that is one literal in parentheses or
/// under a minus, with whether an odd number of minuses stand before
/// it: `-(1.0)` gives `1.0`, negative.
fn bare_literal(text: &str) -> (bool, &str) {
    let mut negative = false;
    let mut rest = text.trim();
    loop {
        // `strip_prefix` gives what follows the prefix, or `None` when
        // the text does not start with it.
        if let Some(inner) = rest.strip_prefix('-') {
            negative = !negative;
            rest = inner.trim();
        } else if let Some(inner) = rest
            .strip_prefix('(')
            .and_then(|inner| inner.strip_suffix(')'))
        {
            rest = inner.trim();
        } else {
            return (negative, rest);
        }
    }
}

/// Records that the parameter declared at `at` stands for `argument`.
/// False when it already stands for something that cannot be the same.
fn bind(at: Location, argument: Argument, found: &mut Matching<'_>) -> bool {
    let Some((_, earlier)) = found.bound.iter().find(|(known, _)| *known == at) else {
        found.bound.push((at, argument));
        return true;
    };
    // Both are written by the struct that has the field. They are
    // matched as an argument is, a parameter of that struct on either
    // side being anything.
    let mut both = Matching {
        bound: Vec::new(),
        packs: found.packs,
        open: true,
    };
    may_be(earlier, &argument, &mut both)
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
        | Type::Endpoint(inner)
        | Type::Atomic(inner)
        | Type::Delta(inner)
        | Type::Trace { cell: inner, .. }
        | Type::Nullable(inner) => grown(inner, in_group),
        Type::Dict { key, value } => grown(key, in_group).or_else(|| grown(value, in_group)),
        Type::RollingWindow { value, .. } => grown(value, in_group),
        Type::Result { value, error } => grown(value, in_group).or_else(|| grown(error, in_group)),
        Type::Scalar(_)
        | Type::Parameter { .. }
        | Type::Signal
        | Type::Error
        | Type::ErrorContext
        | Type::Unknown => None,
    }
}

/// True when a field of type `ty` is not the field `parent` introduces.
/// A type that is not known, or a part of one that is not, differs from
/// nothing: it has been reported, and what is known of the rest is
/// still compared (ERR-9).
fn differs(parent: &Introduced, ty: &Type) -> bool {
    if matches!(parent.ty, Type::Unknown) || matches!(ty, Type::Unknown) {
        return false;
    }
    !agree(&parent.ty, ty)
}

/// True when two types are the same as far as each is known.
pub(crate) fn agree(left: &Type, right: &Type) -> bool {
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
                        (Argument::Constant(left), Argument::Constant(right)) => {
                            constants_agree(left, right)
                        }
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
        ) => {
            let sizes = match (size, other_size) {
                (Some(size), Some(other_size)) => constants_agree(size, other_size),
                (size, other_size) => size == other_size,
            };
            sizes && agree(element, other_element)
        }
        (Type::Set(left), Type::Set(right))
        | (Type::Ref(left), Type::Ref(right))
        | (Type::Endpoint(left), Type::Endpoint(right))
        | (Type::Atomic(left), Type::Atomic(right))
        | (Type::Delta(left), Type::Delta(right))
        | (Type::Nullable(left), Type::Nullable(right))
        // A trace's style is judged where two traces meet (TST-7), and
        // is no part of what the trace is of.
        | (Type::Trace { cell: left, .. }, Type::Trace { cell: right, .. }) => agree(left, right),
        (
            Type::Dict { key, value },
            Type::Dict {
                key: other_key,
                value: other_value,
            },
        ) => agree(key, other_key) && agree(value, other_value),
        (
            Type::RollingWindow {
                value,
                maximum,
                minimum,
            },
            Type::RollingWindow {
                value: other_value,
                maximum: other_maximum,
                minimum: other_minimum,
            },
        ) => {
            let minimums = match (minimum, other_minimum) {
                (Some(minimum), Some(other_minimum)) => constants_agree(minimum, other_minimum),
                // One written and one not: the same when the one
                // written may yet come to the maximum.
                (Some(written), None) | (None, Some(written)) => {
                    matches!(written.value, ConstantValue::Unevaluated)
                }
                (None, None) => true,
            };
            constants_agree(maximum, other_maximum) && minimums && agree(value, other_value)
        }
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

/// True when two constants are the same as far as each is known. One
/// this pass has not evaluated, such as `N + 1` with an argument put in
/// place of `N`, differs from nothing: the pass that evaluates constants
/// compares it.
fn constants_agree(left: &Constant, right: &Constant) -> bool {
    // An expression not evaluated, and a literal whose value is not
    // held: `1.0` and `1.00` are one float, written two ways.
    let open = |constant: &Constant| {
        matches!(
            constant.value,
            ConstantValue::Unevaluated | ConstantValue::Other(_)
        )
    };
    open(left) || open(right) || left == right
}
