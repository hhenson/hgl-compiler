//! Static operand types for `Type` constants read outside a body and
//! declaration-level deferred `type(...)` generic arguments (TYPE-18, GEN-13).
//!
//! The ordinary expression reader checks the operand; its HIR and effects
//! are discarded, so no second expression semantics is maintained here.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};

use hgl_diagnostics::{Category, Diagnostics, Location, Severity};
use hgl_parser::{HasSpan, SyntaxKind, SyntaxNode};
use hgl_source::FileId;

use super::{Tables, Typer, tables};
use crate::constant::{
    Constants, Fault, QueryType, Scope, has_deferred_type_query, type_has_deferred_query,
};
use crate::declared::substitute_within;
use crate::program::{Binding, LocalKind};
use crate::types::{Argument, Constant, ConstantValue, GenericKind};
use crate::{Declaration, Detail, Program, Scalar, Signature, Type, codes, error};

/// Collects every outermost `type(...)` query inside `node` (queries nested
/// inside another `type(...)` are typed as part of the outer operand).
pub(crate) fn outermost_type_queries(node: &SyntaxNode, found: &mut Vec<SyntaxNode>) {
    if node.kind() == SyntaxKind::TypeOfExpr {
        found.push(node.clone());
        return;
    }
    for child in node.children() {
        outermost_type_queries(&child, found);
    }
}

/// Read a query in its declaration's lexical environment without emitting
/// diagnostics. Used when `crate::constant` evaluates a module constant
/// whose expression contains `type(...)` (TYPE-18).
pub(crate) fn query_type(program: &Program, file: FileId, query: &SyntaxNode) -> QueryType {
    query_type_with_bindings(program, file, query, &[], true, None)
}

/// Read a cold `type(e)` query in its declaration's lexical environment.
///
/// When `sink` is `Some`, operand diagnostics are emitted into it so ill-typed
/// operands retain their own errors rather than collapsing into a generic
/// constant diagnostic (TYPE-18, ERR-9).
pub(crate) fn query_type_into(
    program: &Program,
    file: FileId,
    query: &SyntaxNode,
    sink: Option<&mut Diagnostics>,
) -> QueryType {
    query_type_with_bindings(program, file, query, &[], false, sink)
}

fn query_type_with_bindings(
    program: &Program,
    file: FileId,
    query: &SyntaxNode,
    instance: &[(Location, Argument)],
    declaration_pass: bool,
    sink: Option<&mut Diagnostics>,
) -> QueryType {
    let Some(module) = program
        .modules
        .iter()
        .find(|module| module.files.iter().any(|source| source.file == file))
    else {
        return QueryType::Unresolved;
    };
    let found = module
        .declarations
        .iter()
        .enumerate()
        .find(|(_, declaration)| {
            let span = declaration.syntax.span();
            declaration.at.file == file
                && span.start <= query.span().start
                && query.span().end <= span.end
        });
    let Some(file_index) = module.files.iter().position(|source| source.file == file) else {
        return QueryType::Unresolved;
    };
    let stand_in = Declaration::stand_in(
        "type query",
        Location::new(file, query.span()),
        file_index,
        query,
        false,
    );
    let declaration = found.map_or(&stand_in, |(_, declaration)| declaration);
    let empty = Signature {
        generics: Vec::new(),
        parameters: Vec::new(),
        result: None,
        reference: false,
    };
    let of_struct;
    let signature = match found.and_then(|(position, _)| module.details.get(position)) {
        Some(Detail::Callable(signature)) => signature,
        Some(Detail::Struct(read)) => {
            of_struct = Signature::of_struct(read);
            &of_struct
        }
        _ => &empty,
    };
    let (bindings, mut written, mut calls) = tables(module);
    let map: Vec<_> = instance.iter().map(|(at, arg)| (*at, arg)).collect();
    let contains = |position: &(FileId, u32)| {
        position.0 == file && query.span().start <= position.1 && position.1 < query.span().end
    };
    let projected_types: Vec<_> = written
        .iter()
        .filter(|(at, _)| !map.is_empty() && contains(at))
        .map(|(&at, ty)| (at, program.specialize_type(ty, &map)))
        .collect();
    for (at, ty) in &projected_types {
        written.insert(*at, ty);
    }
    let projected_calls: Vec<_> = calls
        .iter()
        .filter(|(at, _)| !map.is_empty() && contains(at))
        .map(|(&at, call)| {
            let mut call = (**call).clone();
            for (_, arg) in &mut call.arguments {
                *arg = crate::instance::placed(arg, &map);
            }
            (at, call)
        })
        .collect();
    for (at, call) in &projected_calls {
        calls.insert(*at, call);
    }
    let selecting = RefCell::new(Vec::new());
    let mut discard = Diagnostics::new();
    let diagnostics = sink.unwrap_or(&mut discard);
    let before = diagnostics.len();
    // Use `Typer::bare` (skipping `bindings()` and `applications()`) so a
    // `type(...)` query inside a `requires` clause or declaration header cannot
    // recurse back into `query_type_into`.
    let mut typer = Typer::bare(
        program,
        module,
        declaration,
        signature,
        Tables {
            bindings: &bindings,
            written: &written,
            calls: &calls,
            selecting: &selecting,
        },
        diagnostics,
    );
    typer.instance = instance.to_vec();
    for parameter in &signature.parameters {
        let ty = match parameter.pack {
            Some(_) => Type::Unknown,
            None => program.specialize_type(&parameter.ty, &map),
        };
        let kind = if parameter.var {
            LocalKind::Var
        } else {
            LocalKind::Parameter
        };
        typer.declare(&parameter.name, parameter.at, kind, ty, true);
    }
    typer.type_query = true;
    let Some(operand) = query.children().next() else {
        return QueryType::Unresolved;
    };
    let mut ty = typer.expression(&operand, None).ty;
    if !map.is_empty() {
        // A nested constructor can keep ordinary value expressions such as
        // N*2+1 in its nominal arguments. The enclosing Type describes the
        // concrete application after those arguments have also been placed.
        ty = program.specialize_type(&ty, &map);
    }
    let new_errors = typer.diagnostics.as_slice()[before..]
        .iter()
        .any(|report| report.severity == Severity::Error);
    let has_unknown = ty.any_part(&|part| matches!(part, Type::Unknown), &|_| false);
    if new_errors {
        QueryType::Reported
    // A selected generic fallback can already have a definite result, yet
    // a more-specific overload can replace it when the instance is made.
    } else if declaration_pass
        && !signature.generics.is_empty()
        && !type_has_deferred_query(&ty)
        && (!selecting.borrow().is_empty() || typer.nested_query_selects)
    {
        // Capture only parameters used by the operand. The argument taking
        // this default is not an input unless the default actually names it.
        let used: HashSet<_> = module
            .uses
            .iter()
            .filter_map(|used| {
                if used.at.file != file
                    || used.at.span.start < query.span().start
                    || query.span().end < used.at.span.end
                {
                    return None;
                }
                match &used.binding {
                    Binding::Local { at, .. } => Some(*at),
                    _ => None,
                }
            })
            .collect();
        let visible = signature.generics.iter().filter(|generic| {
            used.contains(&generic.at)
                || signature.parameters.iter().any(|parameter| {
                    used.contains(&parameter.at)
                        && super::inference::mentions(&parameter.ty, &[generic.at])
                })
        });
        let bindings = visible
            .map(|g| {
                let arg = if matches!(g.kind, GenericKind::Value(_)) {
                    Argument::Constant(Constant {
                        text: g.name.clone(),
                        value: ConstantValue::Parameter {
                            name: g.name.clone(),
                            at: g.at,
                        },
                    })
                } else {
                    Argument::Type(Type::Parameter {
                        name: g.name.clone(),
                        at: g.at,
                    })
                };
                (g.at, arg)
            })
            .collect();
        QueryType::InstanceDependent(bindings)
    } else if has_unknown || type_has_deferred_query(&ty) {
        QueryType::Unresolved
    } else {
        QueryType::Resolved(ty)
    }
}

/// Resolves declaration-level `ConstantValue::DeferredTypeQuery` entries left
/// by `read::read` once all modules' signatures and initial struct members are
/// populated (TYPE-18, GEN-13).
///
/// Body-level, test-level, and default/constant-initializer deferred queries
/// are left intact so `values::check` resolves them with body locals in scope
/// and `reread_instance_body` can specialize them per generic instance.
///
/// Returns `true` if any `Detail::Struct` changed (requiring struct parents
/// and members to be recomputed before field-conflict and recursion checks).
pub(crate) fn resolve_declared_type_queries(
    program: &mut Program,
    diagnostics: &mut Diagnostics,
) -> bool {
    if !has_any_declared_deferred(program) {
        return false;
    }
    let mut any_struct_changed = false;
    // Each update replaces a deferred query with a resolved constant. A chain
    // may cross any number of declarations; a cycle makes no further progress.
    loop {
        let mut pass = QueryPass::default();
        let (updates, struct_changed) = resolve_pass(program, None, &mut pass);
        program.has_dependent_type_queries |= pass.instance_dependent;
        if !pass.resolved || updates.is_empty() {
            break;
        }
        apply_updates(program, updates);
        if struct_changed {
            any_struct_changed = true;
            crate::declared::refresh_members(program);
        }
    }
    if has_any_declared_deferred(program) {
        let mut pass = QueryPass::default();
        let (updates, struct_changed) = resolve_pass(program, Some(diagnostics), &mut pass);
        program.has_dependent_type_queries |= pass.instance_dependent;
        if !updates.is_empty() {
            apply_updates(program, updates);
            if struct_changed {
                any_struct_changed = true;
                crate::declared::refresh_members(program);
            }
        }
    }
    any_struct_changed
}

/// Identifies a declaration-level `Type` slot to update after a pass.
enum SlotUpdate {
    Constant(usize, usize, Type),
    StructGeneric(usize, usize, usize, Type),
    StructParent(usize, usize, usize, Type),
    StructField(usize, usize, usize, Type),
    CallableGeneric(usize, usize, usize, Type),
    CallableParam(usize, usize, usize, Type),
    CallableResult(usize, usize, Type),
    Written(usize, usize, Type),
    DeltaInput(usize, usize, Type),
}

fn has_any_declared_deferred(program: &Program) -> bool {
    for (module_index, module) in program.modules.iter().enumerate() {
        for detail in &module.details {
            let has = match detail {
                Detail::Constant(ty) => type_has_deferred_query(ty),
                Detail::Struct(read) => {
                    read.generics.iter().any(|generic| {
                        matches!(&generic.kind, GenericKind::Value(ty) if type_has_deferred_query(ty))
                    }) || read.parents.iter().any(type_has_deferred_query)
                        || read
                            .fields
                            .iter()
                            .any(|field| field.ty.as_ref().is_some_and(type_has_deferred_query))
                }
                Detail::Callable(signature) => {
                    signature.generics.iter().any(|generic| {
                        matches!(&generic.kind, GenericKind::Value(ty) if type_has_deferred_query(ty))
                    }) || signature
                        .parameters
                        .iter()
                        .any(|parameter| type_has_deferred_query(&parameter.ty))
                        || signature
                            .result
                            .as_ref()
                            .is_some_and(type_has_deferred_query)
                }
                Detail::Enum(_) | Detail::Native(_) | Detail::Plain => false,
            };
            if has {
                return true;
            }
        }
        if module.written.iter().any(|written| {
            type_has_deferred_query(&written.ty)
                && is_declaration_level(program, module_index, written.at)
        }) {
            return true;
        }
        if module.delta_inputs.iter().any(|input| {
            type_has_deferred_query(&input.payload)
                && is_declaration_level(program, module_index, input.at)
        }) {
            return true;
        }
    }
    false
}

/// True when `at` lies in a declaration header (signature, struct parents or
/// field types, constant type, `requires` clause, or generic parameter
/// default), rather than inside a body, anonymous function, test, run/store
/// statement, or default/constant initializer expression.
fn is_declaration_level(program: &Program, module_index: usize, at: Location) -> bool {
    let module = &program.modules[module_index];
    let Some(declaration) = module.declarations.iter().find(|declaration| {
        let span = declaration.syntax.span();
        declaration.at.file == at.file && span.start <= at.span.start && at.span.end <= span.end
    }) else {
        return false;
    };
    let Some(node) = declaration
        .syntax
        .descendants()
        .find(|node| node.span() == at.span)
    else {
        return false;
    };
    let mut current = Some(node);
    while let Some(node) = current {
        if node.span() == declaration.syntax.span() && node.kind() == declaration.syntax.kind() {
            return true;
        }
        if matches!(
            node.kind(),
            SyntaxKind::Block | SyntaxKind::ArrowBody | SyntaxKind::FnExpr
        ) {
            return false;
        }
        if let Some(parent) = node.parent()
            && matches!(
                parent.kind(),
                SyntaxKind::Param | SyntaxKind::StructMember | SyntaxKind::ConstantDecl
            )
            && !crate::read::is_type(node.kind())
        {
            return false;
        }
        current = node.parent();
    }
    true
}

/// An unresolved constant compares unequal even to itself. Only replacing
/// a query with a known value counts as progress through declaration dependencies.
#[derive(Default)]
struct QueryPass {
    instance_dependent: bool,
    resolved: bool,
    diagnosed: HashSet<Location>,
}

fn resolve_pass(
    program: &Program,
    mut sink: Option<&mut Diagnostics>,
    pass: &mut QueryPass,
) -> (Vec<SlotUpdate>, bool) {
    let everywhere: HashMap<(FileId, u32), &Binding> = program
        .modules
        .iter()
        .flat_map(|module| &module.uses)
        .map(|used| ((used.at.file, used.at.span.start), &used.binding))
        .collect();
    let mut updates = Vec::new();
    let mut struct_changed = false;
    for (module_index, module) in program.modules.iter().enumerate() {
        for (decl_index, detail) in module.details.iter().enumerate() {
            match detail {
                Detail::Constant(ty) if type_has_deferred_query(ty) => {
                    let next =
                        resolve_type_declared(program, &everywhere, ty, sink.as_deref_mut(), pass);
                    if &next != ty {
                        updates.push(SlotUpdate::Constant(module_index, decl_index, next));
                    }
                }
                Detail::Struct(read) => {
                    for (slot, generic) in read.generics.iter().enumerate() {
                        if let GenericKind::Value(ty) = &generic.kind
                            && type_has_deferred_query(ty)
                        {
                            let next = resolve_type_declared(
                                program,
                                &everywhere,
                                ty,
                                sink.as_deref_mut(),
                                pass,
                            );
                            if &next != ty {
                                struct_changed = true;
                                updates.push(SlotUpdate::StructGeneric(
                                    module_index,
                                    decl_index,
                                    slot,
                                    next,
                                ));
                            }
                        }
                    }
                    for (slot, parent) in read.parents.iter().enumerate() {
                        if type_has_deferred_query(parent) {
                            let next = resolve_type_declared(
                                program,
                                &everywhere,
                                parent,
                                sink.as_deref_mut(),
                                pass,
                            );
                            if &next != parent {
                                struct_changed = true;
                                updates.push(SlotUpdate::StructParent(
                                    module_index,
                                    decl_index,
                                    slot,
                                    next,
                                ));
                            }
                        }
                    }
                    for (slot, field) in read.fields.iter().enumerate() {
                        if let Some(ty) = &field.ty
                            && type_has_deferred_query(ty)
                        {
                            let next = resolve_type_declared(
                                program,
                                &everywhere,
                                ty,
                                sink.as_deref_mut(),
                                pass,
                            );
                            if &next != ty {
                                struct_changed = true;
                                updates.push(SlotUpdate::StructField(
                                    module_index,
                                    decl_index,
                                    slot,
                                    next,
                                ));
                            }
                        }
                    }
                }
                Detail::Callable(signature) => {
                    for (slot, generic) in signature.generics.iter().enumerate() {
                        if let GenericKind::Value(ty) = &generic.kind
                            && type_has_deferred_query(ty)
                        {
                            let next = resolve_type_declared(
                                program,
                                &everywhere,
                                ty,
                                sink.as_deref_mut(),
                                pass,
                            );
                            if &next != ty {
                                updates.push(SlotUpdate::CallableGeneric(
                                    module_index,
                                    decl_index,
                                    slot,
                                    next,
                                ));
                            }
                        }
                    }
                    for (slot, parameter) in signature.parameters.iter().enumerate() {
                        if type_has_deferred_query(&parameter.ty) {
                            let next = resolve_type_declared(
                                program,
                                &everywhere,
                                &parameter.ty,
                                sink.as_deref_mut(),
                                pass,
                            );
                            if next != parameter.ty {
                                updates.push(SlotUpdate::CallableParam(
                                    module_index,
                                    decl_index,
                                    slot,
                                    next,
                                ));
                            }
                        }
                    }
                    if let Some(result) = &signature.result
                        && type_has_deferred_query(result)
                    {
                        let next = resolve_type_declared(
                            program,
                            &everywhere,
                            result,
                            sink.as_deref_mut(),
                            pass,
                        );
                        if &next != result {
                            updates.push(SlotUpdate::CallableResult(
                                module_index,
                                decl_index,
                                next,
                            ));
                        }
                    }
                }
                _ => {}
            }
        }
        for (slot, written) in module.written.iter().enumerate() {
            if type_has_deferred_query(&written.ty)
                && is_declaration_level(program, module_index, written.at)
            {
                let next = resolve_type_declared(
                    program,
                    &everywhere,
                    &written.ty,
                    sink.as_deref_mut(),
                    pass,
                );
                if next != written.ty {
                    updates.push(SlotUpdate::Written(module_index, slot, next));
                }
            }
        }
        for (slot, input) in module.delta_inputs.iter().enumerate() {
            if type_has_deferred_query(&input.payload)
                && is_declaration_level(program, module_index, input.at)
            {
                let next = resolve_type_declared(
                    program,
                    &everywhere,
                    &input.payload,
                    sink.as_deref_mut(),
                    pass,
                );
                if next != input.payload {
                    updates.push(SlotUpdate::DeltaInput(module_index, slot, next));
                }
            }
        }
    }
    (updates, struct_changed)
}

fn apply_updates(program: &mut Program, updates: Vec<SlotUpdate>) {
    for update in updates {
        match update {
            SlotUpdate::Constant(module, decl, ty) => {
                if let Detail::Constant(slot) = &mut program.modules[module].details[decl] {
                    *slot = ty;
                }
            }
            SlotUpdate::StructGeneric(module, decl, slot, ty) => {
                if let Detail::Struct(read) = &mut program.modules[module].details[decl]
                    && let GenericKind::Value(target) = &mut read.generics[slot].kind
                {
                    *target = ty;
                }
            }
            SlotUpdate::StructParent(module, decl, slot, ty) => {
                if let Detail::Struct(read) = &mut program.modules[module].details[decl] {
                    read.parents[slot] = ty;
                }
            }
            SlotUpdate::StructField(module, decl, slot, ty) => {
                if let Detail::Struct(read) = &mut program.modules[module].details[decl] {
                    read.fields[slot].ty = Some(ty);
                }
            }
            SlotUpdate::CallableGeneric(module, decl, slot, ty) => {
                if let Detail::Callable(signature) = &mut program.modules[module].details[decl]
                    && let GenericKind::Value(target) = &mut signature.generics[slot].kind
                {
                    *target = ty;
                }
            }
            SlotUpdate::CallableParam(module, decl, slot, ty) => {
                if let Detail::Callable(signature) = &mut program.modules[module].details[decl] {
                    signature.parameters[slot].ty = ty;
                }
            }
            SlotUpdate::CallableResult(module, decl, ty) => {
                if let Detail::Callable(signature) = &mut program.modules[module].details[decl] {
                    signature.result = Some(ty);
                }
            }
            SlotUpdate::Written(module, slot, ty) => {
                program.modules[module].written[slot].ty = ty;
            }
            SlotUpdate::DeltaInput(module, slot, ty) => {
                program.modules[module].delta_inputs[slot].payload = ty;
            }
        }
    }
}

fn resolve_type_declared(
    program: &Program,
    everywhere: &HashMap<(FileId, u32), &Binding>,
    ty: &Type,
    mut sink: Option<&mut Diagnostics>,
    pass: &mut QueryPass,
) -> Type {
    if !type_has_deferred_query(ty) {
        return ty.clone();
    }
    let part = |inner: &Type, sink: Option<&mut Diagnostics>, pass: &mut QueryPass| {
        Box::new(resolve_type_declared(
            program, everywhere, inner, sink, pass,
        ))
    };
    match ty {
        Type::Named {
            name,
            module,
            declaration,
            arguments,
            kind,
        } => {
            let generics = match program.modules[*module].details.get(*declaration) {
                Some(Detail::Struct(read)) => read.generics.as_slice(),
                _ => &[],
            };
            let mut resolved_args: Vec<Argument> = Vec::with_capacity(arguments.len());
            for (slot, argument) in arguments.iter().enumerate() {
                let next = match argument {
                    Argument::Type(inner) => Argument::Type(resolve_type_declared(
                        program,
                        everywhere,
                        inner,
                        sink.as_deref_mut(),
                        pass,
                    )),
                    Argument::NoResult => Argument::NoResult,
                    Argument::Constant(constant) if has_deferred_type_query(&constant.value) => {
                        let (expected, place) = match generics.get(slot) {
                            Some(generic) => {
                                let expected = match &generic.kind {
                                    GenericKind::Value(pattern) => {
                                        let map: Vec<_> = generics
                                            .iter()
                                            .map(|g| g.at)
                                            .zip(&resolved_args)
                                            .collect();
                                        let substituted = substitute_within(pattern, &map);
                                        (!crate::declared::names_parameter(&substituted))
                                            .then_some(substituted)
                                    }
                                    _ => None,
                                };
                                (expected, Some(format!("`{}` of `{name}`", generic.name)))
                            }
                            None => (None, None),
                        };
                        let value = if let ConstantValue::DeferredDefaultQuery(at) = constant.value
                        {
                            let instance = application_arguments(generics, &resolved_args);
                            evaluate_deferred_declared(
                                program,
                                everywhere,
                                (at, Some(&instance)),
                                expected.as_ref(),
                                place.as_deref(),
                                sink.as_deref_mut(),
                                pass,
                            )
                        } else {
                            resolve_constant_declared(
                                program,
                                everywhere,
                                &constant.value,
                                expected.as_ref(),
                                place.as_deref(),
                                sink.as_deref_mut(),
                                pass,
                            )
                        };
                        Argument::Constant(Constant {
                            text: constant.text.clone(),
                            value,
                        })
                    }
                    Argument::Constant(_) => argument.clone(),
                };
                resolved_args.push(next);
            }
            Type::Named {
                name: name.clone(),
                module: *module,
                declaration: *declaration,
                arguments: resolved_args,
                kind: *kind,
            }
        }
        Type::List { element, size } => Type::List {
            element: part(element, sink.as_deref_mut(), pass),
            size: size.as_ref().map(|size| {
                if has_deferred_type_query(&size.value) {
                    let integer = Type::Scalar(Scalar::I64);
                    Constant {
                        text: size.text.clone(),
                        value: resolve_constant_declared(
                            program,
                            everywhere,
                            &size.value,
                            Some(&integer),
                            Some("a list's size"),
                            sink.as_deref_mut(),
                            pass,
                        ),
                    }
                } else {
                    size.clone()
                }
            }),
        },
        Type::RollingWindow {
            value,
            maximum,
            minimum,
        } => Type::RollingWindow {
            value: part(value, sink.as_deref_mut(), pass),
            maximum: if has_deferred_type_query(&maximum.value) {
                Constant {
                    text: maximum.text.clone(),
                    value: resolve_constant_declared(
                        program,
                        everywhere,
                        &maximum.value,
                        None,
                        Some("a window's size"),
                        sink.as_deref_mut(),
                        pass,
                    ),
                }
            } else {
                maximum.clone()
            },
            minimum: minimum.as_ref().map(|minimum| {
                if has_deferred_type_query(&minimum.value) {
                    Constant {
                        text: minimum.text.clone(),
                        value: resolve_constant_declared(
                            program,
                            everywhere,
                            &minimum.value,
                            None,
                            Some("a window's minimum size"),
                            sink.as_deref_mut(),
                            pass,
                        ),
                    }
                } else {
                    minimum.clone()
                }
            }),
        },
        Type::PackMember { pack, name, key } => Type::PackMember {
            pack: *pack,
            name: name.clone(),
            key: if has_deferred_type_query(&key.value) {
                Constant {
                    text: key.text.clone(),
                    value: resolve_constant_declared(
                        program,
                        everywhere,
                        &key.value,
                        None,
                        None,
                        sink.as_deref_mut(),
                        pass,
                    ),
                }
            } else {
                key.clone()
            },
        },
        Type::Tuple(elements) => Type::Tuple(
            elements
                .iter()
                .map(|element| {
                    resolve_type_declared(program, everywhere, element, sink.as_deref_mut(), pass)
                })
                .collect(),
        ),
        Type::Set(inner) => Type::Set(part(inner, sink, pass)),
        Type::Dict { key, value } => Type::Dict {
            key: part(key, sink.as_deref_mut(), pass),
            value: part(value, sink, pass),
        },
        Type::Endpoint(inner) => Type::Endpoint(part(inner, sink, pass)),
        Type::Atomic(inner) => Type::atomic(*part(inner, sink, pass)),
        Type::Delta(inner) => Type::delta(*part(inner, sink, pass)),
        Type::Result { value, error } => Type::Result {
            value: part(value, sink.as_deref_mut(), pass),
            error: part(error, sink, pass),
        },
        Type::Nullable(inner) => Type::Nullable(part(inner, sink, pass)),
        Type::Trace { cell, style } => Type::Trace {
            cell: part(cell, sink, pass),
            style: *style,
        },
        Type::Function(function) => {
            let mut function = function.as_ref().clone();
            for parameter in &mut function.parameters {
                let inner = parameter.ty_mut();
                *inner =
                    resolve_type_declared(program, everywhere, inner, sink.as_deref_mut(), pass);
            }
            function.result = function
                .result
                .as_ref()
                .map(|result| resolve_type_declared(program, everywhere, result, sink, pass));
            Type::Function(Box::new(function))
        }
        _ => ty.clone(),
    }
}

fn resolve_constant_declared(
    program: &Program,
    everywhere: &HashMap<(FileId, u32), &Binding>,
    value: &ConstantValue,
    expected: Option<&Type>,
    place: Option<&str>,
    sink: Option<&mut Diagnostics>,
    pass: &mut QueryPass,
) -> ConstantValue {
    match value {
        ConstantValue::DeferredTypeQuery(expr_at) => evaluate_deferred_declared(
            program,
            everywhere,
            (*expr_at, None),
            expected,
            place,
            sink,
            pass,
        ),
        ConstantValue::Type(inner) if type_has_deferred_query(inner) => {
            ConstantValue::Type(Box::new(resolve_type_declared(
                program, everywhere, inner, sink, pass,
            )))
        }
        other => other.clone(),
    }
}

fn application_arguments(
    generics: &[crate::GenericParameter],
    arguments: &[Argument],
) -> Vec<(Location, Argument)> {
    generics
        .iter()
        .map(|g| g.at)
        .zip(arguments.iter().cloned())
        .collect()
}

/// A bare struct type in a body reads its defaults in their owner's scope,
/// with earlier applied values, just as a field or signature does.
pub(super) fn applied_default(
    program: &Program,
    at: Location,
    parameters: (&[crate::GenericParameter], &[Argument]),
    expected: Option<&Type>,
    place: Option<&str>,
    diagnostics: &mut Diagnostics,
) -> ConstantValue {
    let instance = application_arguments(parameters.0, parameters.1);
    let everywhere = program
        .modules
        .iter()
        .flat_map(|module| &module.uses)
        .map(|used| ((used.at.file, used.at.span.start), &used.binding))
        .collect();
    evaluate_deferred_declared(
        program,
        &everywhere,
        (at, Some(&instance)),
        expected,
        place,
        Some(diagnostics),
        &mut QueryPass::default(),
    )
}

fn evaluate_deferred_declared(
    program: &Program,
    everywhere: &HashMap<(FileId, u32), &Binding>,
    (expr_at, instance): (Location, Option<&[(Location, Argument)]>),
    expected: Option<&Type>,
    place: Option<&str>,
    mut sink: Option<&mut Diagnostics>,
    pass: &mut QueryPass,
) -> ConstantValue {
    let final_pass = sink.is_some();
    let pending = if instance.is_some() {
        ConstantValue::DeferredDefaultQuery(expr_at)
    } else {
        ConstantValue::DeferredTypeQuery(expr_at)
    };
    let arguments: Vec<_> = instance
        .unwrap_or_default()
        .iter()
        .map(|(at, arg)| (*at, arg))
        .collect();
    let Some((module, declaration, node)) = find_node(program, expr_at) else {
        return if final_pass {
            ConstantValue::Unknown
        } else {
            pending
        };
    };
    let should_report = final_pass && pass.diagnosed.insert(expr_at);
    let constants = Constants::default();
    let mut depends_on_instance = false;
    let mut queries = Vec::new();
    outermost_type_queries(&node, &mut queries);
    for query in &queries {
        let query_at = Location::new(expr_at.file, query.span());
        let result = query_type_with_bindings(
            program,
            expr_at.file,
            query,
            instance.unwrap_or_default(),
            instance.is_none(),
            if should_report {
                sink.as_deref_mut()
            } else {
                None
            },
        );
        depends_on_instance |= matches!(result, QueryType::InstanceDependent(_));
        constants.remember_type(query_at, result);
    }
    let generics = match declaration.and_then(|decl| module.details.get(decl)) {
        Some(Detail::Callable(signature)) => signature.generics.as_slice(),
        Some(Detail::Struct(read)) => read.generics.as_slice(),
        _ => &[],
    };
    let binding = |file: FileId, offset: u32| everywhere.get(&(file, offset)).copied();
    let parameter = |at: Location| {
        generics
            .iter()
            .find(|generic| generic.at == at)
            .and_then(|generic| match &generic.kind {
                GenericKind::Value(ty) => Some(ty.clone()),
                _ => None,
            })
    };
    let scope = Scope {
        program,
        file: expr_at.file,
        binding: &binding,
        parameter: &parameter,
        arguments: &arguments,
        type_packs: &[],
        constants: &constants,
    };
    let read = crate::constant::read(&node, expected, &scope);
    match read {
        Ok(value) if crate::constant::worked_out(&value) && !has_deferred_type_query(&value) => {
            pass.instance_dependent |= depends_on_instance;
            pass.resolved = true;
            value
        }
        _ if !final_pass => pending,
        Ok(_) => {
            if should_report && let (Some(sink), Some(place)) = (sink, place) {
                sink.emit(error(
                    Category::Type,
                    codes::GENERIC_ARGUMENT_CONSTANT,
                    expr_at,
                    format!(
                        "{place} is a generic value parameter, and this is no compile-time \
                         constant: this compiler does not work out its value"
                    ),
                    "GEN-8",
                ));
            }
            ConstantValue::Unknown
        }
        Err(Fault::NotConstant) => {
            if should_report && let (Some(sink), Some(place)) = (sink, place) {
                sink.emit(error(
                    Category::Type,
                    codes::GENERIC_ARGUMENT_CONSTANT,
                    expr_at,
                    format!(
                        "{place} is a generic value parameter, and this is no compile-time \
                         constant: it is known only when the program runs"
                    ),
                    "GEN-8",
                ));
            }
            ConstantValue::Unknown
        }
        Err(fault) => {
            if should_report
                && let (Some(sink), Some(place), Some(expected)) = (sink, place, expected)
                && let Some(report) = fault.report(expr_at, place, expected)
            {
                sink.emit(report);
            }
            ConstantValue::Unknown
        }
    }
}

/// Locates the syntax node at `at` across all modules of `program`.
pub(crate) fn find_node(
    program: &Program,
    at: Location,
) -> Option<(&crate::Module, Option<usize>, SyntaxNode)> {
    let module = program
        .modules
        .iter()
        .find(|module| module.files.iter().any(|source| source.file == at.file))?;
    if let Some((index, declaration)) =
        module
            .declarations
            .iter()
            .enumerate()
            .find(|(_, declaration)| {
                let span = declaration.syntax.span();
                declaration.at.file == at.file
                    && span.start <= at.span.start
                    && at.span.end <= span.end
            })
        && let Some(node) = declaration
            .syntax
            .descendants()
            .find(|node| node.span() == at.span)
    {
        return Some((module, Some(index), node));
    }
    None
}

impl Program {
    pub(crate) fn instance_query_type(
        &self,
        at: Location,
        bindings: &[(Location, Argument)],
    ) -> Option<Type> {
        if bindings.iter().any(|(_, arg)| match arg {
            Argument::Type(ty) => ty.identity().is_none(),
            Argument::Constant(c) => c.value.key().is_none(),
            Argument::NoResult => false,
        }) {
            return None;
        }
        // A declaration cycle cannot acquire an identity by reading itself.
        thread_local! {
            static READING: RefCell<Vec<Location>> = const { RefCell::new(Vec::new()) };
        }
        READING.with(|reading| {
            if reading.borrow().contains(&at) {
                return None;
            }
            let (_, _, query) = find_node(self, at)?;
            reading.borrow_mut().push(at);
            let answer = query_type_with_bindings(self, at.file, &query, bindings, false, None);
            reading.borrow_mut().pop();
            match answer {
                QueryType::Resolved(ty) => Some(ty),
                _ => None,
            }
        })
    }
}
