//! Writes struct declarations, templates, layouts and their C++ traits.

use std::collections::BTreeSet;

use hgl_analyser::{
    Argument as GenericArgument, ConstantValue, Detail, GenericKind, GenericParameter, NamedKind,
    Program, Type,
};
use hgl_diagnostics::Location;
use hgl_lower::types::inherited;

use crate::emit::Emitter;
use crate::emit::expressions::integer_literal;
use crate::emit::names::{self, identifier, module_namespace};
use crate::emit::types::{cpp_type, cpp_type_in, enum_name};
use crate::emit::values::INDENT;
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
pub(in crate::emit) fn struct_type(
    program: &Program,
    (module, declaration): (usize, usize),
    arguments: &[GenericArgument],
    generic: bool,
) -> Option<String> {
    let name = names::declared_type(
        &program.modules[module].declarations[declaration],
        declaration,
    );
    let namespace = module_namespace(&program.modules[module].path);
    Some(format!(
        "{namespace}::{name}{}",
        template_arguments(program, (module, declaration), arguments, generic)?
    ))
}

/// The C++ type of the delta of the struct `(module, declaration)`
/// applied to `arguments` (TYPE-16): the struct of that name in the
/// namespace `d_` of its module's, which no HGL name gives.
pub(super) fn struct_delta_type(
    program: &Program,
    (module, declaration): (usize, usize),
    arguments: &[GenericArgument],
    generic: bool,
) -> Option<String> {
    let name = names::declared_type(
        &program.modules[module].declarations[declaration],
        declaration,
    );
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
    if !template_keeps(program, struct_of, arguments, &mut Vec::new()) {
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

/// True where the C++ template of a generic struct, applied to the C++
/// types of `arguments`, is the struct HGL's application is. Two cases
/// are not, and such an application is not written yet:
///
/// * An argument that holds an atomic, `atomic<list<i64>>`, which is
///   the value it holds in C++: the two applications would be one C++
///   struct, though TYPE-3 tells them apart by their arguments, and an
///   atomic's delta is the whole value (TYPE-16), which the template's
///   would not be.
/// * A field written `T?`, or one that holds `T` under an `atomic<...>`,
///   whose `T` is given a nullable, `i64?`: HGL keeps one nullable, the
///   field's `i64?` (TYPE-14: the suffix makes no nullable of a
///   nullable), and its delta's property, the atomic's whole value
///   (TYPE-16), where the template's would be an optional of an optional.
///
/// A field that names another generic struct is checked as that struct's
/// application, its arguments given ours: `seen` holds the applications
/// on the way, which are not checked again.
fn template_keeps(
    program: &Program,
    struct_of: (usize, usize),
    arguments: &[GenericArgument],
    seen: &mut Vec<((usize, usize), Vec<GenericArgument>)>,
) -> bool {
    let Some(Detail::Struct(read)) = program.modules[struct_of.0].details.get(struct_of.1) else {
        return true;
    };
    let (generics, members) = (&read.generics, &read.members);
    if seen
        .iter()
        .any(|(at, given)| *at == struct_of && given.as_slice() == arguments)
    {
        return true;
    }
    seen.push((struct_of, arguments.to_vec()));
    fn erased_in_cpp(ty: &Type) -> bool {
        match ty {
            Type::Atomic(_) => true,
            Type::Delta(of) => erased_in_cpp(of),
            Type::Named { arguments, .. } => arguments.iter().any(
                |argument| matches!(argument, GenericArgument::Type(inner) if erased_in_cpp(inner)),
            ),
            Type::Nullable(inner) | Type::List { element: inner, .. } | Type::Set(inner) => {
                erased_in_cpp(inner)
            }
            Type::Dict { key, value } => erased_in_cpp(key) || erased_in_cpp(value),
            Type::Tuple(positions) => positions.iter().any(erased_in_cpp),
            Type::Result { value, error } => erased_in_cpp(value) || erased_in_cpp(error),
            _ => false,
        }
    }
    /// The generic parameters `ty` writes `T?` of, at any depth, and each
    /// under an `atomic<...>`, whose delta is its payload whole (TYPE-16):
    /// a nullable given either is one HGL keeps one nullable, where the
    /// template would hold an optional of an optional.
    fn made_nullable(ty: &Type, found: &mut Vec<Location>) {
        match ty {
            Type::Nullable(inner) => {
                if let Type::Parameter { at, .. } = &**inner {
                    found.push(*at);
                }
                made_nullable(inner, found);
            }
            Type::Atomic(inner) => {
                let mut every = |part: &Type| {
                    if let Type::Parameter { at, .. } = part {
                        found.push(*at);
                    }
                };
                walk_types(inner, &mut every);
                made_nullable(inner, found);
            }
            Type::Named { arguments, .. } => {
                for argument in arguments {
                    if let GenericArgument::Type(inner) = argument {
                        made_nullable(inner, found);
                    }
                }
            }
            Type::List { element: inner, .. } | Type::Set(inner) | Type::Delta(inner) => {
                made_nullable(inner, found)
            }
            Type::Dict { key, value }
            | Type::Result {
                value: key,
                error: value,
            } => {
                made_nullable(key, found);
                made_nullable(value, found);
            }
            Type::Tuple(positions) => {
                for position in positions {
                    made_nullable(position, found);
                }
            }
            _ => {}
        }
    }
    if arguments
        .iter()
        .any(|argument| matches!(argument, GenericArgument::Type(ty) if erased_in_cpp(ty)))
    {
        return false;
    }
    // A struct's parents are written in its template too: `Mid<T> :
    // Base<T?>` converts to `Base<T?>`.
    let mut nullable_of = Vec::new();
    for ty in members.iter().map(|member| &member.ty).chain(&read.parents) {
        made_nullable(ty, &mut nullable_of);
    }
    let flattened = nullable_of.iter().any(|at| {
        generics
            .iter()
            .position(|generic| generic.at == *at)
            .and_then(|position| arguments.get(position))
            .is_some_and(|argument| matches!(argument, GenericArgument::Type(Type::Nullable(_))))
    });
    if flattened {
        return false;
    }
    // Each generic struct a field names, applied to what our arguments
    // make of its own.
    let mut nested = Vec::new();
    for ty in members.iter().map(|member| &member.ty).chain(&read.parents) {
        walk_types(ty, &mut |part| {
            if let Type::Named {
                module,
                declaration,
                arguments: given,
                kind: NamedKind::Struct,
                ..
            } = part
                && !given.is_empty()
            {
                nested.push(((*module, *declaration), given.clone()));
            }
        });
    }
    nested.into_iter().all(|(of, given)| {
        let given: Vec<GenericArgument> = given
            .iter()
            .map(|argument| match argument {
                GenericArgument::Type(ty) => {
                    GenericArgument::Type(substituted(ty, generics, arguments))
                }
                GenericArgument::Constant(_) => argument.clone(),
            })
            .collect();
        template_keeps(program, of, &given, seen)
    })
}

/// Calls `visit` with `ty` and each type inside it, at any depth.
fn walk_types(ty: &Type, visit: &mut dyn FnMut(&Type)) {
    visit(ty);
    match ty {
        Type::Named { arguments, .. } => {
            for argument in arguments {
                if let GenericArgument::Type(inner) = argument {
                    walk_types(inner, visit);
                }
            }
        }
        Type::Nullable(inner)
        | Type::List { element: inner, .. }
        | Type::Set(inner)
        | Type::Delta(inner)
        | Type::Atomic(inner) => walk_types(inner, visit),
        Type::Dict { key, value }
        | Type::Result {
            value: key,
            error: value,
        } => {
            walk_types(key, visit);
            walk_types(value, visit);
        }
        Type::Tuple(positions) => {
            for position in positions {
                walk_types(position, visit);
            }
        }
        _ => {}
    }
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
        ConstantValue::Parameter { name, .. } if generic => Some(names::template_parameter(name)),
        _ => None,
    }
}

/// The type of the property of a struct's delta for a field of type
/// `field` (TYPE-16): "of type `delta<F>?`: null where the field did not
/// change", one `?` where the delta is a nullable already, as the
/// analyser types the property.
pub(super) fn property_type(field: &Type) -> Type {
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
        // As the analyser substitutes (`declared::substitute`): TYPE-14,
        // "A `T?` whose T is given a nullable type is that type"; an
        // atomic and a delta formed again with what they now hold
        // (TYPE-9, TYPE-16).
        Type::Nullable(inner) => match substituted(inner, generics, arguments) {
            inner @ Type::Nullable(_) => inner,
            inner => Type::Nullable(Box::new(inner)),
        },
        Type::Set(inner) => Type::Set(again(inner)),
        Type::Delta(inner) => Type::delta(substituted(inner, generics, arguments)),
        Type::Atomic(inner) => Type::atomic(substituted(inner, generics, arguments)),
        // A size that is a generic value parameter of `generics` is the
        // constant its argument gives (GEN-13).
        Type::List { element, size } => Type::List {
            element: again(element),
            size: size.as_ref().map(|size| match &size.value {
                ConstantValue::Parameter { at, .. } => match argument_of(at) {
                    Some(GenericArgument::Constant(given)) => given.clone(),
                    _ => size.clone(),
                },
                _ => size.clone(),
            }),
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
        let name = names::template_parameter(&generic.name);
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

impl Emitter<'_> {
    /// Notes the struct `(module, declaration)` as one the piece names:
    /// a piece that names it needs its declaration written (it is one of
    /// the piece's `uses`), and a header that names a struct of another
    /// module includes that module's types header.
    pub(in crate::emit) fn note_struct(&mut self, module: usize, declaration: usize) {
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
    pub(in crate::emit) fn struct_declaration(&mut self, index: usize) {
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
        // A generic value parameter's type, an enum of another module's
        // among them, is named where the struct is declared.
        for generic in &read.generics {
            if let GenericKind::Value(ty) = &generic.kind {
                self.note_types(ty);
            }
        }
        let name = names::declared_type(declaration, index);
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
            // An abstract struct converts to each struct it inherits
            // from, whose definition its own needs; the arguments it gives
            // that struct are named, and need none.
            if is_abstract
                && let Type::Named {
                    module,
                    declaration,
                    ..
                } = ancestor
                && *module == self.module
                && *declaration != index
            {
                holds.insert(*declaration);
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
                    "{specialized}struct {}::parents<{qualified}>\n{{\n{INDENT}using type = ::std::tuple<{}>;\n}};\n",
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
                "{specialized}struct {}::fields<{of}>\n{{\n{INDENT}static constexpr auto members = ::std::tuple{{{}}};\n}};\n",
                runtime::NAMESPACE,
                pointers.join(", ")
            )
        };
        // Written in the global namespace, where `std` is the standard
        // library's, and a specialization's name takes no leading `::`.
        let hash_of = |of: &str| -> String {
            format!(
                "{specialized}struct std::hash<{of}>\n{{\n{INDENT}::std::size_t operator()(const {of}& value) const {{ return ::{}::key_hash_of(value); }}\n}};\n",
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
        // TYPE-9: what hgraph needs to hold it as a temporal value
        // (`structs`).
        traits.push_str(&self.bundle_traits(index, &qualified, &delta));
        layout.traits = traits;
        self.piece.is_type = true;
        self.piece.layout = Some(Box::new(layout));
    }
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
    // The struct named as a class, which a field of its name does not
    // hide (UNIT-12: "A member name is not a binding and may match any
    // name").
    text.push_str(&format!(
        "{INDENT}friend bool operator==(const struct {name}&, const struct {name}&) = default;\n}};\n"
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
         {INDENT}{name}(S value) : held_{{::std::make_shared<const as_<S>>(::std::move(value))}}\n{INDENT}{{\n{INDENT}}}\n\n\
         {read}\
         {INDENT}friend bool operator==(const {name}& lhs, const {name}& rhs) {{ return lhs.held_->equal_(*rhs.held_); }}\n\n\
         {INDENT}::std::shared_ptr<const of_> held_;\n}};\n"
    )
}
