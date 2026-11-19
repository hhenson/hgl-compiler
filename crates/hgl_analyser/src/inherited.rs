//! Compares the defaults a struct has for one field from its parents
//! (UNIT-9).
//!
//! UNIT-9: "Compare inherited defaults by bound expression and
//! canonical type after substitution, before folding. Declaration
//! identity matters; source location and alias spelling do not.
//! Conflicting inherited defaults require an explicit child override."
//!
//! Each struct has one effective default for each field, worked out
//! from the top down, parents first. Where the struct declares the
//! field itself, with a default or without one, that declaration
//! decides (TYPE-4: "A typed field introduction fixes its type and
//! optionality"; "An untyped inherited override changes only the
//! construction default"). Otherwise the field has what its direct
//! parents' effective defaults give it, each with the arguments the
//! struct gives that parent in place. A conflict belongs to the struct
//! where two of those provably differ: it is reported there, and a
//! struct below sees only that the field's default is in conflict, so
//! it is not reported again, and a struct that resolves it ends it.
//!
//! The pass rejects only what it can tell: two defaults that are each a
//! constant value of the field's type, the type the struct has the field
//! with, and that are two values (see [`Known`]). Defaults it cannot
//! read so, a call, an expression of operators, a generic value
//! parameter left as one, a date or time, are not judged where they
//! stand; they are kept, and a struct below, whose arguments may make
//! them values, judges them again. It runs after `values`, which reads
//! each default.

use std::collections::{HashMap, HashSet};

use hgl_diagnostics::{Category, Code, Diagnostic, Diagnostics, Location};
use hgl_lexer::literal::{decode_float, decode_string};
use hgl_parser::{HasSpan, SyntaxKind};

use crate::codes;
use crate::declared::{agree, bare_literal, substitute_within};
use crate::hir::{Callee, Expr, ExprKind, Form, UnaryOperator, Value};
use crate::program::{DeclarationKind, Program};
use crate::read::{child, has_token, is_type};
use crate::types::{
    Argument, Constant, ConstantValue, Detail, FieldRef, GenericKind, Scalar, StructType, Type,
};

/// A declaration, by the position of its module in `Program::modules`
/// and its own in that module's `declarations`.
type Id = (usize, usize);

/// One default a field may take: the field that writes it, and what the
/// generic parameters of the struct that writes it stand for in the
/// struct being worked out, each by where it is declared.
#[derive(Debug, Clone, PartialEq)]
struct Inherited {
    /// The field that writes it, whose value is in `Module::defaults`.
    written: FieldRef,
    /// The arguments of its path.
    arguments: Vec<(Location, Argument)>,
}

/// What a struct's field takes when a construction leaves it out.
#[derive(Debug, Clone)]
enum Effective {
    /// No default: the field is given, or is null where it is optional.
    Nothing,
    /// The defaults it may take, each once: one, or several that this
    /// pass could not tell apart where they met, which a struct below
    /// judges again with its own arguments in place.
    Defaults(Vec<Inherited>),
    /// Two defaults that differ, reported where they met.
    Conflict,
}

/// How many constants a default's value is followed through: one that
/// names more is not judged.
const DEPTH: usize = 32;

/// Works out the effective default of every field of every struct,
/// parents first, and reports each conflict where it stands.
///
/// The parents are reached with a list of structs still to do, kept
/// here, and not by this function calling itself: a chain of parents is
/// as long as the source makes it, and the call stack is not. A struct
/// whose parent is still waiting is in a cycle of inheritance, which
/// has been reported, and that parent is passed over.
pub(crate) fn check(program: &Program, diagnostics: &mut Diagnostics) {
    let mut done: HashMap<Id, HashMap<String, Effective>> = HashMap::new();
    let mut waiting: HashSet<Id> = HashSet::new();
    for start in structs(program) {
        let mut to_do = vec![start];
        // `last` looks at the struct on top of the list without taking
        // it off.
        while let Some(&id) = to_do.last() {
            if done.contains_key(&id) {
                to_do.pop();
                continue;
            }
            waiting.insert(id);
            let next = parents(program, id)
                .into_iter()
                .map(|(parent, _, _)| parent)
                .find(|parent| !done.contains_key(parent) && !waiting.contains(parent));
            match next {
                Some(parent) => to_do.push(parent),
                None => {
                    let fields = effective(program, id, &done, diagnostics);
                    done.insert(id, fields);
                    waiting.remove(&id);
                    to_do.pop();
                }
            }
        }
    }
}

/// Every struct of the program, in order of module and declaration.
fn structs(program: &Program) -> Vec<Id> {
    let mut ids = Vec::new();
    for (module, declared) in program.modules.iter().enumerate() {
        for (index, declaration) in declared.declarations.iter().enumerate() {
            if declaration.kind == DeclarationKind::Struct {
                ids.push((module, index));
            }
        }
    }
    ids
}

/// What was read of the struct `id`.
fn struct_type(program: &Program, id: Id) -> Option<&StructType> {
    match program.modules[id.0].details.get(id.1) {
        Some(Detail::Struct(read)) => Some(read),
        _ => None,
    }
}

/// The parents of the struct `id` that are abstract structs, each with
/// the generic arguments it is given and where it is written. Any other
/// parent has been reported (TYPE-3).
fn parents(program: &Program, id: Id) -> Vec<(Id, Vec<Argument>, Location)> {
    let Some(read) = struct_type(program, id) else {
        return Vec::new();
    };
    let module = &program.modules[id.0];
    let declaration = &module.declarations[id.1];
    let file = module.files[declaration.file].file;
    let written = child(&declaration.syntax, SyntaxKind::StructBases)
        .into_iter()
        .flat_map(|bases| bases.children())
        .filter(|parent| is_type(parent.kind()))
        .map(|parent| Location::new(file, parent.span()));
    read.parents
        .iter()
        .zip(written)
        .filter_map(|(ty, at)| match ty {
            Type::Named {
                module,
                declaration,
                arguments,
                ..
            } => {
                let parent = &program.modules[*module].declarations[*declaration];
                let abstract_struct = parent.kind == DeclarationKind::Struct
                    && has_token(&parent.syntax, SyntaxKind::KwAbstract);
                abstract_struct.then(|| ((*module, *declaration), arguments.clone(), at))
            }
            _ => None,
        })
        .collect()
}

/// The effective default of each field of the struct `id`, its
/// parents' in `done`. A conflict that first stands here is reported.
fn effective(
    program: &Program,
    id: Id,
    done: &HashMap<Id, HashMap<String, Effective>>,
    diagnostics: &mut Diagnostics,
) -> HashMap<String, Effective> {
    let mut fields = HashMap::new();
    let Some(read) = struct_type(program, id) else {
        return fields;
    };
    let parents = parents(program, id);
    // The struct's type packs: an argument that passes one on to a
    // parent leaves which parameter each argument is given unknown.
    let packs: Vec<Location> = read
        .generics
        .iter()
        .filter(|generic| matches!(generic.kind, GenericKind::Pack { .. }))
        .map(|generic| generic.at)
        .collect();
    for member in &read.members {
        let name = &member.name;
        // The struct's own declaration decides: a default it writes,
        // or none.
        if let Some(index) = read.fields.iter().position(|field| field.name == *name) {
            let own = if read.fields[index].has_default {
                Effective::Defaults(vec![Inherited {
                    written: FieldRef {
                        module: id.0,
                        declaration: id.1,
                        field: index,
                    },
                    arguments: own_parameters(read),
                }])
            } else {
                Effective::Nothing
            };
            fields.insert(name.clone(), own);
            continue;
        }
        // What its direct parents' effective defaults give it, each
        // with the arguments of the parent it comes through in place,
        // and where that parent is written.
        let mut given: Vec<(Inherited, Location)> = Vec::new();
        let mut conflict = false;
        for (parent, arguments, at) in &parents {
            // TYPE-4: a field two parents give two types is a
            // `field.conflict`, and its defaults are not judged too.
            if let Some(ty) = parent_field_type(program, *parent, arguments, name)
                && !matches!(ty, Type::Unknown)
                && !matches!(member.ty, Type::Unknown)
                && !agree(&ty, &member.ty)
            {
                conflict = true;
            }
            match done.get(parent).and_then(|fields| fields.get(name)) {
                Some(Effective::Defaults(defaults)) => {
                    for default in defaults {
                        let default = carried(program, default, *parent, arguments, &packs);
                        if !given.iter().any(|(known, _)| *known == default) {
                            given.push((default, *at));
                        }
                    }
                }
                Some(Effective::Conflict) => conflict = true,
                Some(Effective::Nothing) | None => {}
            }
        }
        let field = if conflict {
            // Reported where it first stood, or a conflict of types.
            Effective::Conflict
        } else if given.is_empty() {
            Effective::Nothing
        } else if differing(program, id, name, &member.ty, &given, diagnostics) {
            Effective::Conflict
        } else {
            Effective::Defaults(given.into_iter().map(|(default, _)| default).collect())
        };
        fields.insert(name.clone(), field);
    }
    fields
}

/// True when two of `given`, the defaults the field `name` of the
/// struct `id` has from its parents, are constant values of the field's
/// type `ty` that differ; the first two such are reported, at the later
/// one's parent.
fn differing(
    program: &Program,
    id: Id,
    name: &str,
    ty: &Type,
    given: &[(Inherited, Location)],
    diagnostics: &mut Diagnostics,
) -> bool {
    if given.len() < 2 || !concrete(ty) {
        return false;
    }
    let values: Vec<Option<Known>> = given
        .iter()
        .map(|(default, _)| value_of(program, default, ty))
        .collect();
    for (later, value) in values.iter().enumerate() {
        let Some(value) = value else {
            continue;
        };
        let earlier = values[..later]
            .iter()
            .position(|other| other.as_ref().is_some_and(|other| other != value));
        let Some(earlier) = earlier else {
            continue;
        };
        let owner = &program.modules[id.0].declarations[id.1].name;
        let message = format!(
            "`{owner}` has two different defaults for `{name}` from its parents, and gives it \
             none of its own: write `{name} = ...` in `{owner}`"
        );
        let mut diagnostic = Diagnostic::error(
            Category::Type,
            Code::Local(codes::local::DEFAULT_CONFLICT),
            given[later].1,
            message,
        )
        .with_rule("UNIT-9");
        for (note, index) in [("one default", earlier), ("the other", later)] {
            if let Some(at) = written_at(program, given[index].0.written) {
                diagnostic = diagnostic.with_related(at, note);
            }
        }
        diagnostics.emit(diagnostic);
        return true;
    }
    false
}

/// The type the parent `parent` has its field `name` with, `arguments`
/// in place of its generic parameters; `None` where it has no such
/// field.
fn parent_field_type(
    program: &Program,
    parent: Id,
    arguments: &[Argument],
    name: &str,
) -> Option<Type> {
    let read = struct_type(program, parent)?;
    let member = read.members.iter().find(|member| member.name == name)?;
    if read
        .generics
        .iter()
        .any(|generic| matches!(generic.kind, GenericKind::Pack { .. }))
    {
        return Some(Type::Unknown);
    }
    let map: Vec<(Location, &Argument)> = read
        .generics
        .iter()
        .map(|generic| generic.at)
        .zip(arguments)
        .collect();
    Some(substitute_within(&member.ty, &map))
}

/// Where the field `written` is written.
fn written_at(program: &Program, written: FieldRef) -> Option<Location> {
    let read = struct_type(program, (written.module, written.declaration))?;
    Some(read.fields.get(written.field)?.at)
}

/// The generic parameters of a struct, each standing for itself: a type
/// parameter as that type, a value parameter as that constant.
fn own_parameters(read: &StructType) -> Vec<(Location, Argument)> {
    read.generics
        .iter()
        .map(|generic| {
            let argument = match generic.kind {
                GenericKind::Value(_) => Argument::Constant(Constant {
                    text: generic.name.clone(),
                    value: ConstantValue::Parameter {
                        name: generic.name.clone(),
                        at: generic.at,
                    },
                }),
                _ => Argument::Type(Type::Parameter {
                    name: generic.name.clone(),
                    at: generic.at,
                }),
            };
            (generic.at, argument)
        })
        .collect()
}

/// `default`, a default of the parent `parent`, as the struct below has
/// it: the parent's generic parameters, wherever its arguments name
/// them, given `arguments`. Where the parent or the arguments have a
/// type pack, which argument a parameter is given is not worked out:
/// each argument is then not known, and the default is not judged.
/// `packs` are the type packs of the struct below.
fn carried(
    program: &Program,
    default: &Inherited,
    parent: Id,
    arguments: &[Argument],
    packs: &[Location],
) -> Inherited {
    let Some(read) = struct_type(program, parent) else {
        return default.clone();
    };
    let forwards = arguments.iter().any(|argument| {
        matches!(argument, Argument::Type(Type::Parameter { at, .. }) if packs.contains(at))
    });
    let packed = forwards
        || read
            .generics
            .iter()
            .any(|generic| matches!(generic.kind, GenericKind::Pack { .. }));
    let map: Vec<(Location, &Argument)> = read
        .generics
        .iter()
        .map(|generic| generic.at)
        .zip(arguments)
        .collect();
    let arguments = default
        .arguments
        .iter()
        .map(|(at, argument)| {
            let argument = match argument {
                _ if packed => Argument::Type(Type::Unknown),
                Argument::Type(ty) => Argument::Type(substitute_within(ty, &map)),
                Argument::Constant(constant) => match &constant.value {
                    ConstantValue::Parameter { at, .. } => {
                        match map.iter().find(|(declared, _)| declared == at) {
                            Some((_, Argument::Constant(given))) => {
                                Argument::Constant(given.clone())
                            }
                            Some((_, Argument::Type(_))) => Argument::Type(Type::Unknown),
                            None => argument.clone(),
                        }
                    }
                    _ => argument.clone(),
                },
            };
            (*at, argument)
        })
        .collect();
    Inherited {
        written: default.written,
        arguments,
    }
}

/// A default as the constant value it is, of the field's type.
///
/// It is a literal; `null`; a member of an enum; a module's constant,
/// as its own value is; or a list or a tuple of those. A minus before a
/// number, by symbol or as core's `neg_`, is that negative number, and
/// an integer where an `f64` is wanted is that `f64` (EXPR-5). Two
/// values are compared as values: `(true)` is `true`, `1_000` is
/// `1000`. A date or time literal is not judged: two written
/// differently may be one instant.
#[derive(Debug, Clone, PartialEq)]
enum Known {
    /// A literal or `null`, of the type it is in its place.
    Scalar(Value),
    /// A member of an enum: the enum's module and declaration, and the
    /// member's position.
    Member(usize, usize, usize),
    /// The elements of a list, or the positions of a tuple, in order.
    Items(Vec<Known>),
}

/// The value `default` gives a field of type `ty`, with the arguments of
/// its path in place of the generic parameters of the struct that
/// writes it. `None` where it is no constant value of that type.
fn value_of(program: &Program, default: &Inherited, ty: &Type) -> Option<Known> {
    let written = default.written;
    let mut value = program.modules[written.module]
        .defaults
        .get(written.declaration)?
        .get(written.field)?
        .clone()?;
    // `walk_mut` visits each expression before those inside it, and
    // then those inside what the visit made of it.
    value.walk_mut(&mut |expression| {
        // A generic value parameter of the struct that writes the
        // default: the value its path gives it, where that is held.
        if let ExprKind::GenericValue { at, .. } = &expression.kind
            && let Some((_, Argument::Constant(constant))) = default
                .arguments
                .iter()
                .find(|(declared, _)| declared == at)
            && let Some(kind) = given_value(program, constant, &expression.ty)
                .or_else(|| given_value(program, constant, ty))
        {
            expression.kind = kind;
        }
    });
    numbers(program, &mut value);
    known(program, &value, ty, 0)
}

/// What `value`, its numbers read by [`numbers`], is as a value of
/// `ty`; `None` where it is no constant value of that type. `depth`
/// counts the constants followed to reach it.
fn known(program: &Program, value: &Expr, ty: &Type, depth: usize) -> Option<Known> {
    match ty {
        Type::Nullable(inner) => {
            return match value.kind {
                ExprKind::Literal(Value::Null) => Some(Known::Scalar(Value::Null)),
                _ => known(program, value, inner, depth),
            };
        }
        // TYPE-9: an `atomic<V>` holds a complete V.
        Type::Atomic(inner) => return known(program, value, inner, depth),
        _ => {}
    }
    match &value.kind {
        ExprKind::Literal(literal) => fits(literal, ty).map(Known::Scalar),
        ExprKind::EnumMember {
            module,
            declaration,
            member,
        } => match ty {
            Type::Named {
                module: enum_module,
                declaration: enum_declaration,
                ..
            } if (enum_module, enum_declaration) == (module, declaration) => {
                Some(Known::Member(*module, *declaration, *member))
            }
            _ => None,
        },
        // UNIT-13: "The constant has that one value wherever it is
        // read". Its value is followed, as far as `DEPTH`: a constant
        // that depends on itself is reported as that. A constant is of
        // the type it is declared with, and is not widened (EXPR-5: "A
        // value that has a type is not converted by its place"): one of
        // another type than the field's is not judged.
        ExprKind::Constant {
            module,
            declaration,
        } if depth < DEPTH => {
            let unit = &program.modules[*module];
            match unit.details.get(*declaration) {
                Some(Detail::Constant(declared)) if declared == ty => {}
                _ => return None,
            }
            let mut value = unit.constants.get(*declaration)?.clone()?;
            numbers(program, &mut value);
            known(program, &value, ty, depth + 1)
        }
        ExprKind::Sequence(items) => match ty {
            Type::List { element, .. } => items
                .iter()
                .map(|item| known(program, item, element, depth))
                .collect::<Option<Vec<_>>>()
                .map(Known::Items),
            // A set's and a dictionary's entries are no order of their
            // own; they are not judged.
            _ => None,
        },
        ExprKind::Tuple(items) => match ty {
            Type::Tuple(types) if types.len() == items.len() => items
                .iter()
                .zip(types)
                .map(|(item, ty)| known(program, item, ty, depth))
                .collect::<Option<Vec<_>>>()
                .map(Known::Items),
            _ => None,
        },
        _ => None,
    }
}

/// The literal `literal` as a value of the scalar type `ty`; `None`
/// where it is none. EXPR-5: "An integer literal is an `f64` where one
/// is expected".
fn fits(literal: &Value, ty: &Type) -> Option<Value> {
    let Type::Scalar(scalar) = ty else {
        return None;
    };
    match (literal, scalar) {
        // `as` gives the nearest `f64`, as EXPR-5's widening does.
        (Value::Int(whole), Scalar::F64) => Some(Value::Float(*whole as f64)),
        (Value::Int(_), Scalar::I64)
        | (Value::Float(_), Scalar::F64)
        | (Value::Bool(_), Scalar::Bool)
        | (Value::Str(_), Scalar::Str)
        | (Value::Duration(_), Scalar::Duration)
        | (Value::Tenor(_), Scalar::Tenor) => Some(literal.clone()),
        _ => None,
    }
}

/// Reads each number `value` writes as the number it is: a minus before
/// a number literal, by symbol or as core's `neg_` (EXPR-10), is the
/// negative number, as a generic argument `-3` is (LEX-7 reads the least
/// `i64` so); and an integer widened to an `f64` is that `f64` (EXPR-5).
/// Each change takes a node away, so the passes end; one exposes the
/// next, as in `-1` where an `f64` is wanted.
fn numbers(program: &Program, value: &mut Expr) {
    loop {
        let mut changed = false;
        value.walk_mut(&mut |expression| {
            let literal = match &expression.kind {
                ExprKind::Widen(inner) => match inner.kind {
                    ExprKind::Literal(Value::Int(whole)) => Some(Value::Float(whole as f64)),
                    _ => None,
                },
                kind => negated(program, kind),
            };
            if let Some(literal) = literal {
                expression.kind = ExprKind::Literal(literal);
                changed = true;
            }
        });
        if !changed {
            break;
        }
    }
}

/// A constant as the value it is: what this pass holds of it, or for a
/// literal it holds as written (a float, a string, a bool, `null`), the
/// literal read through the parentheses and the minus around it
/// (EXPR-1). `None` for one it does not hold: an expression not
/// evaluated, a date or time, a tenor.
fn held(constant: &Constant) -> Option<Value> {
    Some(match &constant.value {
        ConstantValue::Int(value) => Value::Int(*value),
        ConstantValue::Duration(value) => Value::Duration(*value),
        ConstantValue::Other(kind) => {
            let (negative, text) = bare_literal(&constant.text);
            match *kind {
                "a float" => {
                    let value = decode_float(text).ok()?;
                    Value::Float(if negative { -value } else { value })
                }
                "a string" if !negative => Value::Str(decode_string(text).ok()?),
                "a bool" if !negative => match text {
                    "true" => Value::Bool(true),
                    "false" => Value::Bool(false),
                    _ => return None,
                },
                "null" if !negative => Value::Null,
                _ => return None,
            }
        }
        _ => return None,
    })
}

/// What a generic value parameter is, given `constant`, in a default of
/// a field of type `ty`: the literal of its value, or the member of the
/// field's enum it names. `None` where the value is not held: the
/// parameter is then left as it is, and the default is not judged.
fn given_value(program: &Program, constant: &Constant, ty: &Type) -> Option<ExprKind> {
    match &constant.value {
        ConstantValue::Member(name) => member(program, ty, name),
        _ => held(constant).map(ExprKind::Literal),
    }
}

/// The member `name` of the enum `ty`, or of the enum a `ty?` holds, as
/// an expression.
fn member(program: &Program, ty: &Type, name: &str) -> Option<ExprKind> {
    let ty = match ty {
        Type::Nullable(inner) => inner,
        other => other,
    };
    let Type::Named {
        module,
        declaration,
        ..
    } = ty
    else {
        return None;
    };
    let Some(Detail::Enum(read)) = program.modules[*module].details.get(*declaration) else {
        return None;
    };
    let member = read.members.iter().position(|member| member.name == name)?;
    Some(ExprKind::EnumMember {
        module: *module,
        declaration: *declaration,
        member,
    })
}

/// The negative number `kind` writes, where it is core's negation of a
/// number literal: `-0.5`, read as a call of `neg_` or not read as one,
/// and `neg_(0.5)`, which binds the same operator (EXPR-10). `None` for
/// any other, and for the negation of the least `i64`, which is no
/// `i64`.
fn negated(program: &Program, kind: &ExprKind) -> Option<Value> {
    // Core's `neg_`, the operator `-` before an operand stands for.
    let negation = |(module, declaration): (usize, usize)| {
        Some(module) == program.core()
            && program.modules[module].declarations[declaration].name
                == UnaryOperator::Negate.identity()
    };
    let operand = match kind {
        ExprKind::Call {
            form: Form::Unary(UnaryOperator::Negate),
            arguments,
            ..
        } if arguments.len() == 1 => &arguments[0].value,
        ExprKind::Call {
            callee: Callee::Candidate { operator, .. },
            arguments,
            ..
        } if arguments.len() == 1 && negation(*operator) => &arguments[0].value,
        ExprKind::Unary {
            operator: UnaryOperator::Negate,
            operand,
        } => operand,
        _ => return None,
    };
    match &operand.kind {
        ExprKind::Literal(Value::Int(value)) => value.checked_neg().map(Value::Int),
        ExprKind::Literal(Value::Float(value)) => Some(Value::Float(-value)),
        ExprKind::Literal(Value::Duration(value)) => value.checked_neg().map(Value::Duration),
        _ => None,
    }
}

/// True when `ty` is concrete: no generic parameter is in it, and
/// nothing that is not known.
fn concrete(ty: &Type) -> bool {
    let argument = |argument: &Argument| match argument {
        Argument::Type(ty) => concrete(ty),
        Argument::Constant(constant) => !matches!(
            constant.value,
            ConstantValue::Parameter { .. } | ConstantValue::Unevaluated
        ),
    };
    match ty {
        Type::Unknown | Type::Parameter { .. } => false,
        Type::Named { arguments, .. } => arguments.iter().all(argument),
        Type::Tuple(elements) => elements.iter().all(concrete),
        Type::List { element, size } => {
            concrete(element)
                && size
                    .as_ref()
                    .is_none_or(|size| argument(&Argument::Constant(size.clone())))
        }
        Type::Set(inner)
        | Type::Ref(inner)
        | Type::Endpoint(inner)
        | Type::Atomic(inner)
        | Type::Delta(inner)
        | Type::Trace { cell: inner, .. }
        | Type::Nullable(inner) => concrete(inner),
        Type::Dict { key, value } => concrete(key) && concrete(value),
        Type::RollingWindow { value, .. } => concrete(value),
        Type::Result { value, error } => concrete(value) && concrete(error),
        Type::Scalar(_) | Type::Signal | Type::Error | Type::ErrorContext => true,
    }
}
