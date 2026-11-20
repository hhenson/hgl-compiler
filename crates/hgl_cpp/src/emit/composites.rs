//! Writes HGL's composite values beside the list and the tuple as C++:
//! structs and abstract structs (TYPE-3 to TYPE-6), sets and dictionaries
//! (TYPE-14, EXPR-3, ERR-6, CTRL-12), deltas as values (TYPE-16), an
//! enum's member made from a value (TYPE-7), and an equality of a `T?`
//! with a `T` (OPS-2, TYPE-14). Part of the transpiler, the last stage.
//!
//! | HGL | C++ |
//! | --- | --- |
//! | `struct Point { x: f64 }` | An aggregate `struct Point { ::hgraph::Float x; }` with `==` of each field (OPS-2), in the module's types header; `hglrt::fields<Point>` naming its members, `std::hash<Point>`, and its delta, `d_::Point` |
//! | `struct Box<T> { value: T }` | A class template, `template <class T_> struct Box`; `Box<i64>` is `Box<::hgraph::Int>` |
//! | `abstract struct Shape { id: i64 }` | A class holding any struct that inherits from it, with a function of each field's name, `id()`; `hglrt::parents<Round>` lists what `Round` inherits from |
//! | `Point(y: 2.0, x: 1.0)` | `Point{1.0, 2.0}`, its fields in their order; each held first, in the order written, where that order shows (TYPE-5, EXPR-2) |
//! | `point.x`, `shape.id`, `change.added` | `(point).x`, `(shape).id()`, `(change).added` |
//! | `set<K>`, `dict<K, V>` | `::hglrt::set<K>`, `::hglrt::dict<K, V>`: hash containers, in no order (CTRL-12) |
//! | `[1, 2]` as a set, `["a": 1]` | `::hglrt::set<K>{1, 2}`, `::hglrt::dict_of<K, V>({{"a", 1}})` |
//! | `book[key]`, `book[key] = value`, `book[key] += value` | `::hglrt::entry_at(book, key)`, which fails with `value.missing_key` (ERR-6); `::hglrt::entry(book, key) = value`, which adds the key (EXPR-3); `::hglrt::entry_at(book, key) = ...` |
//! | `for x in members`, `for k, v in book` | `for (const K& x : members)`, `for (const auto& [k, v] : book)` |
//! | `delta<set<i64>>(added: [1])`, of a dictionary, a list, a nullable, a struct | `::hglrt::set_delta<::hgraph::Int>{...}`, `::hglrt::dict_delta`, `::hglrt::list_delta_of<L>(...)`, `std::optional`, `d_::Point{...}` |
//! | `Side(1)`, `Side("buy")` | `::hglrt::enum_member<Side, ::hgraph::Int>(1, {{0, Side::buy}, ...})` |
//! | `maybe == 1` of an `i64?` | `(maybe == 1)`: an optional and a value are equal where the optional holds that value |
//!
//! A struct that holds itself (TYPE-6), a field of an abstract struct
//! assigned, a dictionary's entry given to a `var` parameter, and a pack
//! are not written yet.

use std::collections::BTreeSet;

use hgl_analyser::hir::{Argument, Body, Expr, ExprKind, Stmt, Value};
use hgl_analyser::{
    Argument as GenericArgument, ConstantValue, Detail, GenericKind, GenericParameter, NamedKind,
    Program, Type,
};
use hgl_diagnostics::Location;
use hgl_lower::calls::field_default;
use hgl_lower::types::inherited;

use super::Emitter;
use super::calls::{changes, may_fail};
use super::expressions::{held_read, integer_literal};
use super::names::{self, identifier, module_namespace};
use super::statements::{Scope, refers};
use super::types::{cpp_type, cpp_type_in, enum_name};
use super::values::INDENT;
use crate::runtime;

/// What the types header of a module holds of one struct it declares:
/// what goes where, as `assemble` puts the header together. A struct may
/// name a type declared after it (UNIT-6), and its definition needs the
/// definition of each it holds before it, so the definitions are put in
/// the order of what they hold.
#[derive(Debug, Default)]
pub(crate) struct Layout {
    /// The struct's position among the module's declarations.
    pub(crate) declaration: usize,
    /// Its forward declaration, in the module's namespace, before every
    /// definition.
    pub(crate) forward: String,
    /// Its delta's, in the namespace `d_` of the module's.
    pub(crate) delta_forward: String,
    /// What is said of it at the global namespace before the definitions:
    /// its delta (`hglrt::delta_of`) and the structs it inherits from
    /// (`hglrt::parents`).
    pub(crate) parents: String,
    /// Its definition and its delta's, in the module's namespace.
    pub(crate) definition: String,
    /// What is said of it at the global namespace after every
    /// definition: its fields (`hglrt::fields`) and its hash
    /// (`std::hash`).
    pub(crate) traits: String,
    /// The module's structs its definition needs defined before it: each
    /// it holds, and for an abstract one each it inherits from.
    pub(crate) holds: BTreeSet<usize>,
}

/// The C++ type of the application of the struct `(module, declaration)`
/// to `arguments` (TYPE-3): its name, and the arguments of its template.
/// `generic` is as `types::cpp_type_in` has it. `None` where an argument
/// is not written yet.
pub(super) fn struct_type(
    program: &Program,
    (module, declaration): (usize, usize),
    arguments: &[GenericArgument],
    generic: bool,
) -> Option<String> {
    let name = identifier(&program.modules[module].declarations[declaration].name);
    let namespace = module_namespace(&program.modules[module].path);
    Some(format!(
        "{namespace}::{name}{}",
        template_arguments(program, (module, declaration), arguments, generic)?
    ))
}

/// The C++ type of the delta of the struct `(module, declaration)`
/// applied to `arguments` (TYPE-16): the struct of that name in the
/// namespace `d_` of its module's, which no HGL name gives.
fn struct_delta_type(
    program: &Program,
    (module, declaration): (usize, usize),
    arguments: &[GenericArgument],
    generic: bool,
) -> Option<String> {
    let name = identifier(&program.modules[module].declarations[declaration].name);
    let namespace = module_namespace(&program.modules[module].path);
    Some(format!(
        "{namespace}::d_::{name}{}",
        template_arguments(program, (module, declaration), arguments, generic)?
    ))
}

/// `<A, B>` of a struct's generic `arguments`, or nothing for a struct
/// with no generic parameters. `None` where one is not written yet, and
/// where the struct takes a pack.
fn template_arguments(
    program: &Program,
    struct_of: (usize, usize),
    arguments: &[GenericArgument],
    generic: bool,
) -> Option<String> {
    let Some(Detail::Struct(read)) = program.modules[struct_of.0].details.get(struct_of.1) else {
        return None;
    };
    if read.generics.is_empty() {
        return Some(String::new());
    }
    if read.generics.len() != arguments.len()
        || read
            .generics
            .iter()
            .any(|generic| matches!(generic.kind, GenericKind::Pack { .. }))
    {
        return None;
    }
    let written: Option<Vec<String>> = arguments
        .iter()
        .map(|argument| match argument {
            GenericArgument::Type(ty) => cpp_type_in(program, ty, generic),
            GenericArgument::Constant(constant) => {
                constant_argument(program, &constant.value, generic)
            }
        })
        .collect();
    Some(format!("<{}>", written?.join(", ")))
}

/// A constant generic argument of a struct (GEN-8) as a C++ template
/// argument: an integer, a `bool`, an enum's member, an `f64` (C++ tells
/// `-0.0` from `0.0` as a template argument, as TYPE-3 does an argument
/// by its value, UNIT-13), or a generic value parameter of the struct
/// being declared. `None` for any other.
fn constant_argument(program: &Program, value: &ConstantValue, generic: bool) -> Option<String> {
    match value {
        ConstantValue::Int(number) => Some(integer_literal(*number)),
        ConstantValue::Bool(flag) => Some(flag.to_string()),
        ConstantValue::Float(number) if number.is_finite() => Some(format!("({number:?})")),
        ConstantValue::Member(member) => {
            let (module, declaration) = member.of.as_ref()?.declared;
            Some(format!(
                "{}::{}",
                enum_name(program, module, declaration),
                identifier(&member.name)
            ))
        }
        ConstantValue::Parameter { name, .. } if generic => Some(identifier(name)),
        _ => None,
    }
}

/// The C++ type of a set, a dictionary, a delta or an atomic value (the
/// arms of `types::cpp_type_in` this module writes). `None` for one not
/// written yet.
pub(super) fn composite_type(program: &Program, ty: &Type, generic: bool) -> Option<String> {
    Some(match ty {
        // TYPE-14: "A set's element and a dictionary's key are never
        // null".
        Type::Set(element) => format!(
            "::{}::set<{}>",
            runtime::NAMESPACE,
            cpp_type_in(program, element, generic)?
        ),
        Type::Dict { key, value } => format!(
            "::{}::dict<{}, {}>",
            runtime::NAMESPACE,
            cpp_type_in(program, key, generic)?,
            cpp_type_in(program, value, generic)?
        ),
        Type::Delta(of) => delta_type(program, of, generic)?,
        // TYPE-9: "`atomic<V>` stops expansion at a complete ordinary V":
        // as a value, a struct's field of it holds the V.
        Type::Atomic(payload) => cpp_type_in(program, payload, generic)?,
        _ => return None,
    })
}

/// The C++ type of `delta<of>` where it is a type of its own (TYPE-16):
/// a set's, a dictionary's and a list's of the library, a nullable's an
/// optional of its payload's delta, a struct's the struct's delta.
/// "In a generic declaration it stays as written until T is known": in a
/// generic struct, `hglrt::delta_t` of the parameter.
fn delta_type(program: &Program, of: &Type, generic: bool) -> Option<String> {
    Some(match of {
        Type::Set(element) => format!(
            "::{}::set_delta<{}>",
            runtime::NAMESPACE,
            cpp_type_in(program, element, generic)?
        ),
        Type::Dict { key, value } => format!(
            "::{}::dict_delta<{}, {}>",
            runtime::NAMESPACE,
            cpp_type_in(program, key, generic)?,
            cpp_type_in(program, value, generic)?
        ),
        Type::List { .. } => format!(
            "::{}::list_delta<{}>",
            runtime::NAMESPACE,
            cpp_type_in(program, of, generic)?
        ),
        // "A nullable's delta is an optional: it holds what was
        // published, a null among them".
        Type::Nullable(payload) => format!(
            "std::optional<{}>",
            cpp_type_in(program, &Type::delta((**payload).clone()), generic)?
        ),
        Type::Named {
            module,
            declaration,
            arguments,
            kind: NamedKind::Struct,
            ..
        } => struct_delta_type(program, (*module, *declaration), arguments, generic)?,
        Type::Parameter { name, .. } if generic => {
            format!("::{}::delta_t<{}>", runtime::NAMESPACE, identifier(name))
        }
        // A delta that is no type of its own is the type it is.
        other if Type::delta(other.clone()) != Type::Delta(Box::new(other.clone())) => {
            cpp_type_in(program, &Type::delta(other.clone()), generic)?
        }
        _ => return None,
    })
}

/// The type of the property of a struct's delta for a field of type
/// `field` (TYPE-16): "of type `delta<F>?`: null where the field did not
/// change", one `?` where the delta is a nullable already, as the
/// analyser types the property.
fn property_type(field: &Type) -> Type {
    match Type::delta(field.clone()) {
        nullable @ Type::Nullable(_) => nullable,
        delta => Type::Nullable(Box::new(delta)),
    }
}

/// True where the struct `of` holds itself (TYPE-6): a field of it, or
/// of a struct it holds, at any depth, names it.
fn holds_itself(program: &Program, of: (usize, usize)) -> bool {
    let mut seen = BTreeSet::new();
    let mut next: Vec<(usize, usize)> = vec![of];
    while let Some(at) = next.pop() {
        let Some(Detail::Struct(read)) = program.modules[at.0].details.get(at.1) else {
            continue;
        };
        for member in &read.members {
            for named in structs_in(&member.ty) {
                if named == of {
                    return true;
                }
                if seen.insert(named) {
                    next.push(named);
                }
            }
        }
    }
    false
}

/// The structs `ty` names at any depth, each as its module and its
/// declaration.
fn structs_in(ty: &Type) -> Vec<(usize, usize)> {
    let mut found = Vec::new();
    collect_structs(ty, &mut found);
    found
}

/// Adds to `found` each struct `ty` names at any depth.
fn collect_structs(ty: &Type, found: &mut Vec<(usize, usize)>) {
    match ty {
        Type::Named {
            module,
            declaration,
            arguments,
            kind,
            ..
        } => {
            if *kind == NamedKind::Struct {
                found.push((*module, *declaration));
            }
            for argument in arguments {
                if let GenericArgument::Type(argument) = argument {
                    collect_structs(argument, found);
                }
            }
        }
        Type::Nullable(inner)
        | Type::List { element: inner, .. }
        | Type::Set(inner)
        | Type::Delta(inner)
        | Type::Atomic(inner) => collect_structs(inner, found),
        Type::Dict { key, value } => {
            collect_structs(key, found);
            collect_structs(value, found);
        }
        Type::Tuple(positions) => {
            for position in positions {
                collect_structs(position, found);
            }
        }
        Type::Result { value, error } => {
            collect_structs(value, found);
            collect_structs(error, found);
        }
        _ => {}
    }
}

/// `ty` with each generic parameter of `generics` replaced by the
/// argument of the same place in `arguments` (GEN-2), a parameter being
/// known by where it is declared.
fn substituted(ty: &Type, generics: &[GenericParameter], arguments: &[GenericArgument]) -> Type {
    let argument_of = |at: &Location| {
        generics
            .iter()
            .position(|generic| generic.at == *at)
            .and_then(|position| arguments.get(position))
    };
    let again = |inner: &Type| Box::new(substituted(inner, generics, arguments));
    match ty {
        Type::Parameter { at, .. } => match argument_of(at) {
            Some(GenericArgument::Type(argument)) => argument.clone(),
            _ => ty.clone(),
        },
        Type::Named {
            name,
            module,
            declaration,
            arguments: own,
            kind,
        } => Type::Named {
            name: name.clone(),
            module: *module,
            declaration: *declaration,
            arguments: own
                .iter()
                .map(|argument| match argument {
                    GenericArgument::Type(argument) => {
                        GenericArgument::Type(substituted(argument, generics, arguments))
                    }
                    GenericArgument::Constant(constant) => match &constant.value {
                        ConstantValue::Parameter { at, .. } => match argument_of(at) {
                            Some(given @ GenericArgument::Constant(_)) => given.clone(),
                            _ => argument.clone(),
                        },
                        _ => argument.clone(),
                    },
                })
                .collect(),
            kind: *kind,
        },
        Type::Nullable(inner) => Type::Nullable(again(inner)),
        Type::Set(inner) => Type::Set(again(inner)),
        Type::Delta(inner) => Type::Delta(again(inner)),
        Type::Atomic(inner) => Type::Atomic(again(inner)),
        Type::List { element, size } => Type::List {
            element: again(element),
            size: size.clone(),
        },
        Type::Dict { key, value } => Type::Dict {
            key: again(key),
            value: again(value),
        },
        Type::Tuple(positions) => Type::Tuple(
            positions
                .iter()
                .map(|position| substituted(position, generics, arguments))
                .collect(),
        ),
        Type::Result { value, error } => Type::Result {
            value: again(value),
            error: again(error),
        },
        other => other.clone(),
    }
}

/// The abstract structs the struct `(module, declaration)` inherits
/// from, at any depth, each with the arguments the struct gives it, in
/// its own generic parameters (TYPE-3: "through any number of parents,
/// each with the arguments the struct below gives it"), each once.
fn ancestors(program: &Program, (module, declaration): (usize, usize)) -> Vec<Type> {
    let mut found: Vec<Type> = Vec::new();
    let Some(Detail::Struct(read)) = program.modules[module].details.get(declaration) else {
        return found;
    };
    let mut next: Vec<Type> = read.parents.clone();
    while let Some(parent) = next.pop() {
        let Type::Named {
            module: parent_module,
            declaration: parent_declaration,
            arguments,
            ..
        } = &parent
        else {
            continue;
        };
        if found.contains(&parent) {
            continue;
        }
        if let Some(Detail::Struct(above)) = program.modules[*parent_module]
            .details
            .get(*parent_declaration)
        {
            for grand in &above.parents {
                next.push(substituted(grand, &above.generics, arguments));
            }
        }
        found.push(parent);
    }
    found
}

/// The head of the template of a struct with `generics`, `template <class
/// T_, ::hgraph::Int N_>` and a new line, and the parameters' names, in
/// order; nothing for a struct with none. `None` for a generic parameter
/// not written yet: a pack, a value of a type no template takes.
fn template_head(
    program: &Program,
    generics: &[GenericParameter],
) -> Option<(String, Vec<String>)> {
    if generics.is_empty() {
        return Some((String::new(), Vec::new()));
    }
    let mut parameters = Vec::new();
    let mut named = Vec::new();
    for generic in generics {
        let name = identifier(&generic.name);
        let parameter = match &generic.kind {
            GenericKind::Plain | GenericKind::Type => format!("class {name}"),
            // A value written with no type is a size, or has its kind once
            // resolved (GEN-13): the template takes it as it comes.
            GenericKind::Value(Type::Unknown) => format!("auto {name}"),
            GenericKind::Value(
                ty @ Type::Scalar(
                    hgl_analyser::Scalar::I64
                    | hgl_analyser::Scalar::Bool
                    | hgl_analyser::Scalar::F64,
                ),
            ) => format!("{} {name}", cpp_type(program, ty)?),
            GenericKind::Value(
                ty @ Type::Named {
                    kind: NamedKind::Enum,
                    ..
                },
            ) => format!("{} {name}", cpp_type(program, ty)?),
            _ => return None,
        };
        parameters.push(parameter);
        named.push(name);
    }
    Some((format!("template <{}>\n", parameters.join(", ")), named))
}

/// One value a constructor of a struct or a delta is given, in the
/// order HGL evaluates it (TYPE-5, EXPR-2).
struct Given {
    /// The position it fills among what the C++ is made of.
    slot: usize,
    /// The C++ that makes it.
    text: String,
    /// True where it may fail or do anything ([`may_fail`]).
    fails: bool,
    /// True where it runs statements of its own ([`changes`]).
    changes: bool,
    /// True where it reads what a local or a constant holds ([`refers`]).
    refers: bool,
    /// The C++ type it is held as where it is held: its own, never `auto`
    /// of what was read, which for an element of a `list<bool>` is the
    /// vector's reference to its bit.
    ty: String,
}

impl Emitter<'_> {
    /// Notes the struct `(module, declaration)` as one the piece names:
    /// a piece that names it needs its declaration written (it is one of
    /// the piece's `uses`), and a header that names a struct of another
    /// module includes that module's types header.
    pub(super) fn note_struct(&mut self, module: usize, declaration: usize) {
        self.piece.reach.uses.insert((module, declaration, None));
        if module != self.module {
            self.piece.types.insert((module, declaration));
        }
    }

    /// Writes the declaration of struct `index` of the module (TYPE-3 to
    /// TYPE-6): a C++ aggregate of its fields, a parent's first, with
    /// `==` of each field (OPS-2), or, for an abstract struct, a class
    /// that holds any struct that inherits from it; and its delta
    /// (TYPE-16), and what it tells the library (`Layout`).
    pub(super) fn struct_declaration(&mut self, index: usize) {
        let program = self.program;
        let declared = &program.modules[self.module];
        let declaration = &declared.declarations[index];
        let at = declaration.at;
        let Some(Detail::Struct(read)) = declared.details.get(index) else {
            self.unsupported(at, "a struct");
            return;
        };
        // TYPE-6: a struct that holds itself needs a value that holds it
        // through what C++ cannot hold as it is.
        if holds_itself(program, (self.module, index)) {
            self.unsupported(at, "a struct that holds itself");
            return;
        }
        let Some((template, parameters)) = template_head(program, &read.generics) else {
            self.unsupported(at, "a generic parameter of this kind of a struct");
            return;
        };
        let name = identifier(&declaration.name);
        let namespace = module_namespace(&declared.path);
        let arguments = if parameters.is_empty() {
            String::new()
        } else {
            format!("<{}>", parameters.join(", "))
        };
        let qualified = format!("{namespace}::{name}{arguments}");
        let delta = format!("{namespace}::d_::{name}{arguments}");
        // Each member: its name, its C++ type, and its delta's property's.
        let mut members: Vec<(String, String, String)> = Vec::new();
        let mut holds = BTreeSet::new();
        for member in &read.members {
            let Some(ty) = cpp_type_in(program, &member.ty, true) else {
                self.unsupported(member.at, &format!("the type `{}`", member.ty));
                return;
            };
            let Some(property) = cpp_type_in(program, &property_type(&member.ty), true) else {
                self.unsupported(member.at, &format!("the type `delta<{}>`", member.ty));
                return;
            };
            self.note_types(&member.ty);
            for (module, held) in structs_in(&member.ty) {
                if module == self.module {
                    holds.insert(held);
                }
            }
            members.push((identifier(&member.name), ty, property));
        }
        let is_abstract = inherited(program, self.module, index);
        // An abstract struct reads each field through a function of its
        // name, which C++ does not let a class's own name be.
        if is_abstract && members.iter().any(|(member, _, _)| *member == name) {
            self.unsupported(at, "an abstract struct with a field of its own name");
            return;
        }
        let place = self.place_of(index);
        let kind = if is_abstract {
            "abstract struct"
        } else {
            "struct"
        };
        let comment = format!("// {kind} {} (TYPE-3), {place}\n", declaration.name);
        let specialized = if parameters.is_empty() {
            "template <>\n".to_string()
        } else {
            template.clone()
        };
        let mut layout = Layout {
            declaration: index,
            ..Layout::default()
        };
        let class = if is_abstract { "class" } else { "struct" };
        layout.forward = format!("{template}{class} {name};\n");
        layout.delta_forward = format!("{template}struct {name};\n");
        let ancestors = ancestors(program, (self.module, index));
        let mut parents = Vec::new();
        for ancestor in &ancestors {
            self.note_types(ancestor);
            let Some(written) = cpp_type_in(program, ancestor, true) else {
                self.unsupported(at, &format!("the type `{ancestor}`"));
                return;
            };
            if is_abstract {
                for (module, held) in structs_in(ancestor) {
                    if module == self.module {
                        holds.insert(held);
                    }
                }
            }
            parents.push(written);
        }
        layout.holds = holds;
        // A struct's delta is named before any definition: the delta of a
        // list of it is a member of a delta defined before its own.
        layout.parents.push_str(&format!(
            "{specialized}struct {}::delta_of<{qualified}>\n{{\n{INDENT}using type = {delta};\n}};\n",
            runtime::NAMESPACE
        ));
        let mut definition = comment;
        if is_abstract {
            definition.push_str(&abstract_class(&template, &name, &members, &parents));
            // An abstract struct is no struct that inherits from another
            // here: a value of it is given where one of a parent is
            // expected through its conversion, and so holds the struct it
            // holds, and no value of itself.
        } else {
            definition.push_str(&aggregate(&template, &name, &members, |(_, ty, _)| ty));
            if !parents.is_empty() {
                layout.parents += &format!(
                    "{specialized}struct {}::parents<{qualified}>\n{{\n{INDENT}using type = std::tuple<{}>;\n}};\n",
                    runtime::NAMESPACE,
                    parents.join(", ")
                );
            }
        }
        definition.push_str(&format!(
            "\nnamespace d_\n{{\n// delta<{}> (TYPE-16)\n{}}}  // namespace d_\n",
            declaration.name,
            aggregate(&template, &name, &members, |(_, _, property)| property)
        ));
        layout.definition = definition;
        let members_of = |of: &str| -> String {
            let pointers: Vec<String> = members
                .iter()
                .map(|(member, _, _)| format!("&{of}::{member}"))
                .collect();
            format!(
                "{specialized}struct {}::fields<{of}>\n{{\n{INDENT}static constexpr auto members = std::tuple{{{}}};\n}};\n",
                runtime::NAMESPACE,
                pointers.join(", ")
            )
        };
        let hash_of = |of: &str| -> String {
            format!(
                "{specialized}struct std::hash<{of}>\n{{\n{INDENT}std::size_t operator()(const {of}& value) const {{ return ::{}::key_hash_of(value); }}\n}};\n",
                runtime::NAMESPACE
            )
        };
        let mut traits = String::new();
        if !is_abstract {
            traits.push_str(&members_of(&qualified));
        }
        traits.push_str(&members_of(&delta));
        traits.push_str(&hash_of(&qualified));
        traits.push_str(&hash_of(&delta));
        layout.traits = traits;
        self.piece.is_type = true;
        self.piece.layout = Some(Box::new(layout));
    }

    /// `Struct(field: value, ...)` (TYPE-5): "Its arguments are named,
    /// each for a field the struct has ... A field with a default ... may
    /// be left out, and then has that default. An optional field with no
    /// default may be left out too, and is then null ... Written arguments
    /// evaluate once in source order; failure prevents later arguments.
    /// Defaults are evaluated only when omitted."
    pub(super) fn construct(
        &mut self,
        scope: &mut Scope,
        value: &Expr,
        arguments: &[Argument],
    ) -> String {
        let program = self.program;
        let Type::Named {
            module,
            declaration,
            arguments: generics,
            kind: NamedKind::Struct,
            ..
        } = &value.ty
        else {
            return self.missing(value.at, "this construction");
        };
        let Some(Detail::Struct(read)) = program.modules[*module].details.get(*declaration) else {
            return self.missing(value.at, "this construction");
        };
        let Some(ty) = self.written_type(&value.ty, value.at) else {
            return "0".to_string();
        };
        let Some(field_types) = program.field_types(&value.ty) else {
            return self.missing(value.at, "this construction");
        };
        let mut given = Vec::new();
        for argument in arguments {
            let Some(slot) = argument.parameter.filter(|&slot| slot < read.members.len()) else {
                return self.missing(argument.value.at, "an argument given to no field");
            };
            let Some(written) = self.given(scope, slot, &argument.value) else {
                return "0".to_string();
            };
            given.push(written);
        }
        // Each field left out, in the order of the fields: its default, or
        // null.
        for (slot, member) in read.members.iter().enumerate() {
            if given.iter().any(|written| written.slot == slot) {
                continue;
            }
            let field_type = &field_types[slot];
            let Some(slot_type) = self.written_type(field_type, value.at) else {
                return "0".to_string();
            };
            let default = member
                .default
                .and_then(|default| field_default(program, default));
            let text = match default {
                Some(default) => {
                    let key = (*module, *declaration, usize::MAX - slot);
                    if self.expanding.contains(&key) {
                        return self.missing(default.at, "a default that leaves itself out");
                    }
                    self.expanding.push(key);
                    let made = self.field_default(default, field_type, generics.is_empty());
                    self.expanding.pop();
                    let Some(made) = made else {
                        return "0".to_string();
                    };
                    Given {
                        slot,
                        text: made,
                        fails: may_fail(default),
                        changes: false,
                        refers: false,
                        ty: slot_type,
                    }
                }
                None if hgl_lower::types::is_nullable(field_type)
                    || matches!(field_type, Type::Atomic(payload) if hgl_lower::types::is_nullable(payload)) =>
                {
                    Given {
                        slot,
                        text: "std::nullopt".to_string(),
                        fails: false,
                        changes: false,
                        refers: false,
                        ty: slot_type,
                    }
                }
                None => {
                    return self.missing(
                        value.at,
                        &format!("a construction that leaves out `{}`", member.name),
                    );
                }
            };
            given.push(text);
        }
        self.made_in_order(scope, &ty, read.members.len(), given, ("{", "}"))
    }

    /// What `argument`, given to `slot`, is written as; `None` where its
    /// type is not written yet, which is reported.
    fn given(&mut self, scope: &mut Scope, slot: usize, argument: &Expr) -> Option<Given> {
        let text = self.expression(scope, argument);
        let ty = self.written_type(&argument.ty, argument.at)?;
        Some(Given {
            slot,
            text,
            fails: may_fail(argument),
            changes: changes(argument),
            refers: refers(argument),
            ty,
        })
    }

    /// The default `default` of a field of type `field_type` (TYPE-5),
    /// read with nothing in scope (UNIT-7). A default of a field of a
    /// generic struct is of the struct's own parameters: where it is the
    /// empty collection or null (TYPE-6), it is made of the type the
    /// application gives the field, and any other is not written yet
    /// unless `concrete` says the struct has none. `None` where it is not
    /// written, which is reported.
    fn field_default(
        &mut self,
        default: &Expr,
        field_type: &Type,
        concrete: bool,
    ) -> Option<String> {
        let empty = matches!(&default.kind, ExprKind::Sequence(entries) if entries.is_empty())
            || matches!(&default.kind, ExprKind::Pairs(entries) if entries.is_empty());
        if empty {
            let ty = self.written_type(field_type, default.at)?;
            return Some(format!("{ty}{{}}"));
        }
        if matches!(default.kind, ExprKind::Literal(Value::Null)) {
            return Some("std::nullopt".to_string());
        }
        if !concrete && default.ty != *field_type {
            self.unsupported(default.at, "a default of a generic struct's field");
            return None;
        }
        // A default is read with no local in scope (UNIT-7), and its
        // value is written over an empty body: one that runs statements
        // of its own, which have locals of their own, is not written yet.
        if changes(default) {
            self.unsupported(default.at, "a default that runs statements");
            return None;
        }
        let cold = Body {
            locals: Vec::new(),
            statements: Vec::new(),
        };
        let mut defaults = Scope::cold(self.program, &cold);
        let text = self.expression(&mut defaults, default);
        if !defaults.before.is_empty() {
            self.unsupported(default.at, "a default that propagates a failure");
            return None;
        }
        Some(text)
    }

    /// `ty` made of `slots` values, `open` and `close` around them, each
    /// given by one of `given`, which are in the order HGL evaluates them.
    /// Where more than one of them may fail, or one runs statements, which
    /// may change what another reads, each is first held in that order, as
    /// a value of its own where one runs statements: C++ evaluates them in
    /// the order of the slots.
    fn made_in_order(
        &mut self,
        scope: &mut Scope,
        ty: &str,
        slots: usize,
        given: Vec<Given>,
        (open, close): (&str, &str),
    ) -> String {
        let changing = given.iter().any(|value| value.changes);
        let failing = given.iter().filter(|value| value.fails).count();
        let mut filled = vec![String::new(); slots];
        if !changing && failing <= 1 {
            for value in given {
                filled[value.slot] = value.text;
            }
            return format!("{ty}{open}{}{close}", filled.join(", "));
        }
        let mut steps = String::new();
        for value in given {
            let name = names::temporary("field", self.temporary());
            let held = if value.refers && !changing {
                format!("const {}&", value.ty)
            } else {
                format!("const {}", value.ty)
            };
            steps.push_str(&format!("{held} {name} = {}; ", value.text));
            filled[value.slot] = name;
        }
        scope.acted = true;
        format!(
            "[&]() -> {ty} {{ {steps}return {ty}{open}{}{close}; }}()",
            filled.join(", ")
        )
    }

    /// `base.member`: a field of a struct (TYPE-3), of an abstract one
    /// through the function of its name, or a property of a delta that
    /// is a type of its own (TYPE-16): a set's `added` and `removed`, a
    /// dictionary's `updated` and `removed`, a list's `updated`,
    /// `removed` and `size_modified`, a nullable's `value`, which is the
    /// optional itself, and a struct's of the field's name.
    pub(super) fn field(
        &mut self,
        scope: &mut Scope,
        value: &Expr,
        base: &Expr,
        member: usize,
    ) -> String {
        let program = self.program;
        match &base.ty {
            Type::Named {
                module,
                declaration,
                kind: NamedKind::Struct,
                ..
            } => {
                let Some(field) = hgl_lower::types::field_of(program, &base.ty, member) else {
                    return self.missing(value.at, "this field");
                };
                let name = identifier(&field.name);
                let abstract_ = inherited(program, *module, *declaration);
                let base = self.expression(scope, base);
                if abstract_ {
                    format!("({base}).{name}()")
                } else {
                    format!("({base}).{name}")
                }
            }
            Type::Delta(of) => {
                let ns = runtime::NAMESPACE;
                // A struct's delta's property is of its field's name.
                let field = match &**of {
                    Type::Named {
                        kind: NamedKind::Struct,
                        ..
                    } => match hgl_lower::types::field_of(program, of, member) {
                        Some(field) => Some(identifier(&field.name)),
                        None => return self.missing(value.at, "this property of a delta"),
                    },
                    _ => None,
                };
                let written = self.expression(scope, base);
                match (&**of, member) {
                    (Type::Set(_), 0) => format!("({written}).added"),
                    (Type::Set(_) | Type::Dict { .. }, 1) => format!("({written}).removed"),
                    (Type::Dict { .. }, 0) => format!("({written}).updated"),
                    // A list's delta keeps its changes in order of their
                    // positions; HGL's `updated` and `removed` are a
                    // dictionary and a set, made where they are read.
                    (Type::List { .. }, 0) => format!("::{ns}::updated_of({written})"),
                    (Type::List { .. }, 1) => format!("::{ns}::removed_of({written})"),
                    (Type::List { .. }, 2) => format!("({written}).size_modified"),
                    // "the property `value`, a `delta<V>?`": the optional
                    // the delta is.
                    (Type::Nullable(_), 0) => format!("({written})"),
                    _ => match field {
                        Some(field) => format!("({written}).{field}"),
                        None => self.missing(value.at, "this property of a delta"),
                    },
                }
            }
            _ => self.missing(value.at, "this property"),
        }
    }

    /// `delta<T>(property: value, ...)` (TYPE-16): "A property left out is
    /// empty, or null for a struct's field, a nullable's `value`, and a
    /// list's `size_modified`." The properties are evaluated once, in the
    /// order written (EXPR-2).
    pub(super) fn delta(
        &mut self,
        scope: &mut Scope,
        value: &Expr,
        arguments: &[Argument],
    ) -> String {
        let program = self.program;
        let Type::Delta(of) = &value.ty else {
            return self.missing(value.at, "this delta");
        };
        let Some(ty) = self.written_type(&value.ty, value.at) else {
            return "0".to_string();
        };
        // Each property's type, and what it is when it is left out.
        let empty: Vec<(String, String)> = match &**of {
            Type::Set(element) => {
                let Some(set) = self.written_type(&Type::Set(element.clone()), value.at) else {
                    return "0".to_string();
                };
                vec![
                    (set.clone(), format!("{set}{{}}")),
                    (set.clone(), format!("{set}{{}}")),
                ]
            }
            Type::Dict { key, value: held } => {
                let updated = Type::Dict {
                    key: key.clone(),
                    value: Box::new(Type::delta((**held).clone())),
                };
                let (Some(updated), Some(removed)) = (
                    self.written_type(&updated, value.at),
                    self.written_type(&Type::Set(key.clone()), value.at),
                ) else {
                    return "0".to_string();
                };
                vec![
                    (updated.clone(), format!("{updated}{{}}")),
                    (removed.clone(), format!("{removed}{{}}")),
                ]
            }
            Type::List { element, .. } => {
                let position = Box::new(Type::Scalar(hgl_analyser::Scalar::I64));
                let updated = Type::Dict {
                    key: position.clone(),
                    value: Box::new(Type::delta((**element).clone())),
                };
                let (Some(updated), Some(removed)) = (
                    self.written_type(&updated, value.at),
                    self.written_type(&Type::Set(position), value.at),
                ) else {
                    return "0".to_string();
                };
                vec![
                    (updated.clone(), format!("{updated}{{}}")),
                    (removed.clone(), format!("{removed}{{}}")),
                    (
                        "std::optional<::hgraph::Int>".to_string(),
                        "std::nullopt".to_string(),
                    ),
                ]
            }
            Type::Nullable(_) => vec![(ty.clone(), String::new())],
            Type::Named {
                kind: NamedKind::Struct,
                ..
            } => {
                let Some(fields) = program.field_types(of) else {
                    return self.missing(value.at, "this delta");
                };
                let mut properties = Vec::new();
                for field in &fields {
                    let Some(property) = self.written_type(&property_type(field), value.at) else {
                        return "0".to_string();
                    };
                    properties.push((property, "std::nullopt".to_string()));
                }
                properties
            }
            _ => return self.missing(value.at, "this delta"),
        };
        let mut given = Vec::new();
        for argument in arguments {
            let Some(slot) = argument.parameter.filter(|&slot| slot < empty.len()) else {
                return self.missing(argument.value.at, "an argument given to no property");
            };
            let Some(written) = self.given(scope, slot, &argument.value) else {
                return "0".to_string();
            };
            given.push(written);
        }
        for (slot, (property, left_out)) in empty.iter().enumerate() {
            if !given.iter().any(|written| written.slot == slot) {
                given.push(Given {
                    slot,
                    text: left_out.clone(),
                    fails: false,
                    changes: false,
                    refers: false,
                    ty: property.clone(),
                });
            }
        }
        match &**of {
            // `delta<i64?>(value: 7)` holds 7, and `delta<i64?>()` is a
            // published null.
            Type::Nullable(_) => {
                let text = given.pop().map(|value| value.text).unwrap_or_default();
                format!("{ty}{{{text}}}")
            }
            Type::List { .. } => {
                let made = format!(
                    "::{}::list_delta_of<{}>",
                    runtime::NAMESPACE,
                    match self.written_type(of, value.at) {
                        Some(list) => list,
                        None => return "0".to_string(),
                    }
                );
                let slots = empty.len();
                self.made_in_order(scope, &made, slots, given, ("(", ")"))
            }
            _ => {
                let slots = empty.len();
                self.made_in_order(scope, &ty, slots, given, ("{", "}"))
            }
        }
    }

    /// `[a, b]` where a set is expected (EXPR-9), and `[]` where a set or
    /// a dictionary is: each entry evaluated in the order written.
    pub(super) fn set_literal(
        &mut self,
        scope: &mut Scope,
        value: &Expr,
        entries: &[Expr],
    ) -> String {
        let Some(ty) = self.written_type(&value.ty, value.at) else {
            return "0".to_string();
        };
        match &value.ty {
            Type::Set(_) => self.constructed(scope, &ty, entries),
            Type::Dict { .. } if entries.is_empty() => format!("{ty}{{}}"),
            _ => self.missing(value.at, &format!("the type `{}`", value.ty)),
        }
    }

    /// `[key: value, ...]` where a dictionary is expected (EXPR-9): each
    /// key and each value evaluated in the order written. Where an entry
    /// after the first runs statements, which may change what an earlier
    /// one read, each is first held as a value of its own. The pinned text
    /// does not say what a dictionary written with one key twice holds, so
    /// one of more than one entry is written only where its keys are
    /// constants that are not `==` ([`distinct_keys`]), and reported
    /// otherwise.
    pub(super) fn dict_literal(
        &mut self,
        scope: &mut Scope,
        value: &Expr,
        entries: &[(Expr, Expr)],
    ) -> String {
        let Type::Dict { key, value: held } = &value.ty else {
            return self.missing(value.at, &format!("the type `{}`", value.ty));
        };
        let (Some(key_type), Some(value_type)) = (
            self.written_type(key, value.at),
            self.written_type(held, value.at),
        ) else {
            return "0".to_string();
        };
        if entries.is_empty() {
            return format!(
                "::{}::dict<{key_type}, {value_type}>{{}}",
                runtime::NAMESPACE
            );
        }
        let made = format!(
            "::{}::dict_of<{key_type}, {value_type}>",
            runtime::NAMESPACE
        );
        if !distinct_keys(entries) {
            return self.missing(value.at, "a dictionary written with keys that may be one");
        }
        let flattened: Vec<&Expr> = entries
            .iter()
            .flat_map(|(key, value)| [key, value])
            .collect();
        let ordered = flattened.iter().skip(1).any(|entry| changes(entry));
        if !ordered {
            let pairs: Vec<String> = entries
                .iter()
                .map(|(key, value)| {
                    let key = self.expression(scope, key);
                    let value = self.expression(scope, value);
                    format!("{{{key}, {value}}}")
                })
                .collect();
            return format!("{made}({{{}}})", pairs.join(", "));
        }
        let mut steps = String::new();
        let mut pairs = Vec::new();
        for (key, held) in entries {
            let key = self.expression(scope, key);
            let held = self.expression(scope, held);
            let key_name = names::temporary("key", self.temporary());
            let value_name = names::temporary("value", self.temporary());
            steps.push_str(&format!(
                "const {key_type} {key_name} = {key}; const {value_type} {value_name} = {held}; "
            ));
            pairs.push(format!("{{{key_name}, {value_name}}}"));
        }
        scope.acted = true;
        let Some(ty) = self.written_type(&value.ty, value.at) else {
            return "0".to_string();
        };
        format!(
            "[&]() -> {ty} {{ {steps}return {made}({{{}}}); }}()",
            pairs.join(", ")
        )
    }

    /// `book[key]` of a dictionary (ERR-6, EXPR-7): "brackets and `at`
    /// are strict point-in-time reads; absence fails", with
    /// `value.missing_key`. The dictionary, then the key (EXPR-2): where
    /// they are held (`expressions::held_read`), the dictionary is held
    /// first and the value read is a value of its own.
    pub(super) fn dict_read(
        &mut self,
        scope: &mut Scope,
        value: &Expr,
        base: &Expr,
        index: &Expr,
    ) -> String {
        if held_read(base, index) {
            let Some(ty) = self.written_type(&value.ty, value.at) else {
                return "0".to_string();
            };
            let (Some(book_type), Some(key_type)) = (
                self.written_type(&base.ty, base.at),
                self.written_type(&index.ty, index.at),
            ) else {
                return "0".to_string();
            };
            let held = if refers(base) && !changes(index) {
                format!("const {book_type}&")
            } else {
                format!("const {book_type}")
            };
            let base = self.expression(scope, base);
            let index = self.expression(scope, index);
            scope.acted = true;
            let book = names::temporary("book", self.temporary());
            let key = names::temporary("key", self.temporary());
            return format!(
                "[&]() -> {ty} {{ {held} {book} = {base}; const {key_type} {key} = {index}; return ::{}::entry_at({book}, {key}); }}()",
                runtime::NAMESPACE
            );
        }
        let base = self.expression(scope, base);
        let index = self.expression(scope, index);
        scope.acted = true;
        format!("::{}::entry_at({base}, {index})", runtime::NAMESPACE)
    }

    /// `book[key]` as a place (EXPR-3): "A key the dictionary has not is
    /// added" where `adds`, the place the assignment writes whole;
    /// otherwise the key must be there, as a compound assignment reads
    /// the place first. `base` is the dictionary's place, written; the key
    /// is held before the statement where `holds` is true.
    pub(super) fn dict_place(
        &mut self,
        scope: &mut Scope,
        base: String,
        index: &Expr,
        holds: bool,
        adds: bool,
    ) -> Option<String> {
        let written = self.expression(scope, index);
        let key = if holds
            && !matches!(
                index.kind,
                ExprKind::Literal(_) | ExprKind::EnumMember { .. }
            ) {
            let ty = self.named_type(&index.ty)?;
            let held = names::temporary("key", self.temporary());
            scope.before.push(format!("const {ty} {held} = {written};"));
            held
        } else {
            written
        };
        let function = if adds { "entry" } else { "entry_at" };
        Some(format!(
            "::{}::{function}({base}, {key})",
            runtime::NAMESPACE
        ))
    }

    /// The C++ place a field of the struct `base` is (EXPR-3), `base`
    /// being its place written: `point.x`. A field of an abstract struct
    /// is not written yet as a place.
    pub(super) fn field_place(
        &self,
        base_type: &Type,
        base: String,
        member: usize,
    ) -> Option<String> {
        let Type::Named {
            module,
            declaration,
            kind: NamedKind::Struct,
            ..
        } = base_type
        else {
            return None;
        };
        if inherited(self.program, *module, *declaration) {
            return None;
        }
        let field = hgl_lower::types::field_of(self.program, base_type, member)?;
        Some(format!("{base}.{}", identifier(&field.name)))
    }

    /// `for item in members` over a set, and `for key, value in entries`
    /// over a dictionary (CTRL-12), in no order. Each name is bound to a
    /// member, or a key and its value, of what the set or the dictionary
    /// held when the loop began: what is looped over is held whole where
    /// it could change while the loop runs (the copy rule) and where it
    /// is a value made here, which a range over a part of it would not
    /// keep.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn composite_for(
        &mut self,
        scope: &mut Scope,
        bound: &[usize],
        sequence: &Expr,
        statements: &[Stmt],
        at: Location,
        depth: usize,
        text: &mut String,
    ) {
        let indent = INDENT.repeat(depth);
        let body = scope.body;
        let Some(whole) = self.written_type(&sequence.ty, at) else {
            return;
        };
        let names: Vec<String> = bound
            .iter()
            .map(|local| identifier(&body.locals[*local].name))
            .collect();
        let binding = match (&sequence.ty, names.as_slice()) {
            (Type::Set(element), [name]) => {
                let Some(element) = self.written_type(element, at) else {
                    return;
                };
                format!("[[maybe_unused]] const {element}& {name}")
            }
            (Type::Dict { .. }, [key, value]) => {
                format!("[[maybe_unused]] const auto& [{key}, {value}]")
            }
            _ => {
                self.unsupported(at, "a `for` loop over this");
                return;
            }
        };
        let items = self.expression(scope, sequence);
        let copied = bound.iter().any(|local| scope.copies.copied(*local));
        let over = if copied || !refers(sequence) {
            let held = names::temporary("items", self.temporary());
            text.push_str(&format!("{indent}const {whole} {held} = {items};\n"));
            held
        } else {
            items
        };
        text.push_str(&format!("{indent}for ({binding} : {over})\n{indent}{{\n"));
        self.statements(scope, statements, depth + 1, text);
        text.push_str(&format!("{indent}}}\n"));
    }

    /// `Kind(value)` (TYPE-7): "`Side(1)` is the member whose number is 1,
    /// and `Side("buy")` the member of that exact name ... a value that is
    /// none fails where it is evaluated". The catalogue has no code for
    /// that failure.
    pub(super) fn enum_from(
        &mut self,
        scope: &mut Scope,
        value: &Expr,
        (module, declaration): (usize, usize),
        from: &Expr,
    ) -> String {
        let Some(Detail::Enum(read)) = self.program.modules[module].details.get(declaration) else {
            return self.missing(value.at, "an enum's member made from a value");
        };
        self.note_enum(module, declaration);
        let kind = enum_name(self.program, module, declaration);
        let by_name = from.ty == Type::Scalar(hgl_analyser::Scalar::Str);
        let mut table = Vec::new();
        for member in &read.members {
            let member_name = format!("{kind}::{}", identifier(&member.name));
            if by_name {
                table.push(format!(
                    "{{{}, {member_name}}}",
                    names::string(&member.name)
                ));
            } else {
                let Some(number) = member.value else {
                    return self.missing(value.at, "an enum's member made from a value");
                };
                table.push(format!("{{{}, {member_name}}}", integer_literal(number)));
            }
        }
        let given = if by_name {
            "::hgraph::Str"
        } else {
            "::hgraph::Int"
        };
        let written = self.expression(scope, from);
        scope.acted = true;
        format!(
            "::{}::enum_member<{kind}, {given}>({written}, {{{}}})",
            runtime::NAMESPACE,
            table.join(", ")
        )
    }

    /// `left == right` or `left != right` where one is a `T?` and the
    /// other a `T` or a `T?` (TYPE-14: "A T is a value where a `T?` is
    /// expected"; OPS-2: "`==` and `!=` compare two values of one type"):
    /// two nullables are equal where both are null or both hold equal
    /// values. C++'s comparison of optionals is that. The two are
    /// evaluated in the order written (EXPR-2). `None` for operands of any
    /// other types.
    pub(super) fn nullable_equality(
        &mut self,
        scope: &mut Scope,
        equal: bool,
        left: &Expr,
        right: &Expr,
    ) -> Option<String> {
        let nullable_of = |nullable: &Type, other: &Type| matches!(nullable, Type::Nullable(payload) if **payload == *other || *nullable == *other);
        if !nullable_of(&left.ty, &right.ty) && !nullable_of(&right.ty, &left.ty) {
            return None;
        }
        let symbol = if equal { "==" } else { "!=" };
        let ordered = changes(right) || (may_fail(left) && may_fail(right));
        let left_text = self.expression(scope, left);
        let right_text = self.expression(scope, right);
        if !ordered {
            return Some(format!("({left_text} {symbol} {right_text})"));
        }
        let left_type = self.written_type(&left.ty, left.at)?;
        let right_type = self.written_type(&right.ty, right.at)?;
        let first = names::temporary("left", self.temporary());
        let second = names::temporary("right", self.temporary());
        scope.acted = true;
        Some(format!(
            "[&] {{ const {left_type} {first} = {left_text}; const {right_type} {second} = {right_text}; return {first} {symbol} {second}; }}()"
        ))
    }

    /// `convert<list<B>>(values)` (core's OPS-6): a list of structs taken
    /// as a list of an abstract struct `B` they inherit from, "the same
    /// elements".
    pub(super) fn parent_list(&mut self, scope: &mut Scope, value: &Expr, inner: &Expr) -> String {
        let Type::List { element, .. } = &value.ty else {
            return self.missing(value.at, "this conversion");
        };
        let Some(element) = self.written_type(element, value.at) else {
            return "0".to_string();
        };
        let written = self.expression(scope, inner);
        format!(
            "::{}::parent_list<{element}>({written})",
            runtime::NAMESPACE
        )
    }

    /// The place `place` given to a `var` parameter, as the steps that
    /// hold each list index it has, in order, and the place written with
    /// the indices held (FN-15, EXPR-2): a call with an argument that runs
    /// statements takes the place where it is called, the list it names
    /// as it is then, and each index as it was when its argument was
    /// evaluated. `None` for a place not written yet this way, a
    /// dictionary's entry among them.
    pub(super) fn held_place(
        &mut self,
        scope: &mut Scope,
        place: &Expr,
    ) -> Option<(String, String)> {
        match &place.kind {
            ExprKind::Local(local) => {
                scope.read = true;
                Some((String::new(), identifier(&scope.body.locals[*local].name)))
            }
            ExprKind::Field { base, member } => {
                let (steps, written) = self.held_place(scope, base)?;
                Some((steps, self.field_place(&base.ty, written, *member)?))
            }
            ExprKind::Index { base, index } => match &base.ty {
                Type::Tuple(_) => {
                    let ExprKind::Literal(Value::Int(position)) = index.kind else {
                        return None;
                    };
                    let (steps, written) = self.held_place(scope, base)?;
                    Some((steps, format!("std::get<{position}>({written})")))
                }
                Type::List { .. } => {
                    let (mut steps, written) = self.held_place(scope, base)?;
                    let at = self.expression(scope, index);
                    let held = names::temporary("index", self.temporary());
                    steps.push_str(&format!("const ::hgraph::Int {held} = {at}; "));
                    Some((
                        steps,
                        format!("::{}::item({written}, {held})", runtime::NAMESPACE),
                    ))
                }
                _ => None,
            },
            _ => None,
        }
    }
}

/// True where the keys of `entries`, a dictionary written out, are known
/// to be distinct: there is at most one, or each is a literal or an enum's
/// member and no two are `==` (OPS-2: the two zeros are one).
fn distinct_keys(entries: &[(Expr, Expr)]) -> bool {
    /// A key as a constant, where it is one that is told apart here.
    #[derive(PartialEq)]
    enum Key<'e> {
        Value(&'e Value),
        Float(f64),
        Member(usize, usize, usize),
    }
    fn key(expr: &Expr) -> Option<Key<'_>> {
        match &expr.kind {
            ExprKind::Literal(Value::Float(number)) => Some(Key::Float(*number)),
            ExprKind::Literal(
                value @ (Value::Int(_) | Value::Str(_) | Value::Bool(_) | Value::Duration(_)),
            ) => Some(Key::Value(value)),
            // An integer literal where an `f64` is expected (EXPR-5).
            ExprKind::Widen(inner) => match inner.kind {
                ExprKind::Literal(Value::Int(number)) => Some(Key::Float(number as f64)),
                _ => None,
            },
            ExprKind::EnumMember {
                module,
                declaration,
                member,
            } => Some(Key::Member(*module, *declaration, *member)),
            _ => None,
        }
    }
    if entries.len() <= 1 {
        return true;
    }
    let keys: Option<Vec<Key>> = entries.iter().map(|(written, _)| key(written)).collect();
    let Some(keys) = keys else {
        return false;
    };
    keys.iter()
        .enumerate()
        .all(|(at, one)| keys[at + 1..].iter().all(|other| other != one))
}

/// A struct of `members` (each a name, a type and a property's type),
/// named `name`, as a C++ aggregate with `==` of each member, the member's
/// type being what `ty` takes of it: the struct's own, or its delta's.
fn aggregate<F>(template: &str, name: &str, members: &[(String, String, String)], ty: F) -> String
where
    F: Fn(&(String, String, String)) -> &String,
{
    let mut text = format!("{template}struct {name}\n{{\n");
    for member in members {
        text.push_str(&format!("{INDENT}{} {};\n", ty(member), member.0));
    }
    if !members.is_empty() {
        text.push('\n');
    }
    text.push_str(&format!(
        "{INDENT}friend bool operator==(const {name}&, const {name}&) = default;\n}};\n"
    ));
    text
}

/// An abstract struct named `name` with `members`, which inherits from
/// `parents`, as a class that holds any struct that inherits from it
/// (TYPE-3): `of_`, what each struct it may hold is asked, its fields and
/// itself as each parent; `as_<S>`, the struct `S` held; a constructor
/// from each struct that inherits from it (`hglrt::inherits`); a function
/// of each field's name; a conversion to each parent; and `==`, of one
/// struct with the same generic arguments and each field equal (OPS-2).
/// A value of it holds the struct it is given, which nothing changes: a
/// copy shares it.
fn abstract_class(
    template: &str,
    name: &str,
    members: &[(String, String, String)],
    parents: &[String],
) -> String {
    let ns = runtime::NAMESPACE;
    let mut asked = String::new();
    let mut answered = String::new();
    let mut read = String::new();
    for (member, ty, _) in members {
        asked.push_str(&format!(
            "{INDENT}{INDENT}[[nodiscard]] virtual const {ty}& {member}() const = 0;\n"
        ));
        answered.push_str(&format!(
            "{INDENT}{INDENT}[[nodiscard]] const {ty}& {member}() const override {{ return this->held_.{member}; }}\n"
        ));
        read.push_str(&format!(
            "{INDENT}[[nodiscard]] const {ty}& {member}() const {{ return held_->{member}(); }}\n"
        ));
    }
    for (position, parent) in parents.iter().enumerate() {
        asked.push_str(&format!(
            "{INDENT}{INDENT}[[nodiscard]] virtual {parent} as_{position}_() const = 0;\n"
        ));
        answered.push_str(&format!(
            "{INDENT}{INDENT}[[nodiscard]] {parent} as_{position}_() const override {{ return {parent}(this->held_); }}\n"
        ));
        read.push_str(&format!(
            "{INDENT}operator {parent}() const {{ return held_->as_{position}_(); }}\n"
        ));
    }
    format!(
        "{template}class {name}\n{{\n  public:\n\
         {INDENT}struct of_ : ::{ns}::held_value\n{INDENT}{{\n{asked}{INDENT}}};\n\
         {INDENT}template <class S>\n{INDENT}struct as_ final : ::{ns}::held_as<of_, S>\n{INDENT}{{\n\
         {INDENT}{INDENT}using ::{ns}::held_as<of_, S>::held_as;\n{answered}{INDENT}}};\n\n\
         {INDENT}{name}() = default;\n\
         {INDENT}template <class S>\n{INDENT}{INDENT}requires ::{ns}::inherits<S, {name}>\n\
         {INDENT}{name}(S value) : held_{{std::make_shared<const as_<S>>(std::move(value))}}\n{INDENT}{{\n{INDENT}}}\n\n\
         {read}\
         {INDENT}friend bool operator==(const {name}& lhs, const {name}& rhs) {{ return lhs.held_->equal_(*rhs.held_); }}\n\n\
         {INDENT}std::shared_ptr<const of_> held_;\n}};\n"
    )
}

/// True where `ty` is a struct, or the delta of a set, a dictionary, a
/// nullable or a struct, whose field or property a read names in place,
/// so that a `let` of the read may refer to it where its base is one a
/// `let` may refer to (`statements::refers`). A list's delta's
/// `updated` and `removed` are made where they are read.
pub(super) fn field_refers(base: &Type, member: usize) -> bool {
    match base {
        Type::Named {
            kind: NamedKind::Struct,
            ..
        } => true,
        Type::Delta(of) => match &**of {
            Type::List { .. } => member == 2,
            Type::Set(_) | Type::Dict { .. } | Type::Nullable(_) => true,
            Type::Named {
                kind: NamedKind::Struct,
                ..
            } => true,
            _ => false,
        },
        _ => false,
    }
}
