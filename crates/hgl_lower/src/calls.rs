//! What a call gives what it does not write, as the language says it:
//! the default or the null a parameter left out takes, and a field's
//! default. Part of lowering: a transpiler writes each call with these
//! (UNIT-7, UNIT-8).

use std::collections::HashMap;
use std::ops::Deref;

use hgl_analyser::hir::{
    Argument, Body, Callee, DefaultExpr, Expr, ExprKind, Form, PackKey, Value, ValueCategory,
};
use hgl_analyser::{
    Argument as GenericArgument, ConstantValue, DeclarationKind, Detail, FieldRef, GenericKind,
    Parameter, Program, Role, Type,
};
use hgl_diagnostics::Location;

/// The evaluated arguments of a call in written order, keeping their
/// positions in the analysed argument list (EXPR-2). A function
/// argument is a compile-time binding and has no runtime value (FN-17).
/// Values supplied in configuration braces (FN-18, EXPR-10) use the
/// same parameter slots and keep their place in that order.
pub fn runtime_arguments(arguments: &[Argument]) -> impl Iterator<Item = (usize, &Argument)> {
    arguments
        .iter()
        .enumerate()
        .filter(|(_, argument)| !matches!(argument.value.kind, ExprKind::StaticCallable(_)))
}

/// The selected growing-list ordered reduction and the call of its RED-5 combiner.
pub struct OrderedReduction {
    /// The source positions of the list and the seed, in parameter order.
    pub inputs: [usize; 2],
    /// A call of `f` with the accumulator and member as two connection locals.
    pub combiner: Expr,
}

/// Lower a call of core's native RED-5 candidate using the checked callable binding.
pub fn ordered_list_reduction(program: &Program, value: &Expr) -> Option<OrderedReduction> {
    let ExprKind::Call {
        callee, arguments, ..
    } = &value.kind
    else {
        return None;
    };
    let (module, declaration) = callee.declaration()?;
    if Some(module) != program.core() {
        return None;
    }
    let owner = &program.modules[module];
    let declared = &owner.declarations[declaration];
    if declared.name != "reduce"
        || !matches!(
            declared.kind,
            DeclarationKind::NativeCandidate(Role::Temporal, _)
        )
    {
        return None;
    }
    let Detail::Callable(signature) = &owner.details[declaration] else {
        return None;
    };
    let [_, items, zero] = signature.parameters.as_slice() else {
        return None;
    };
    if !matches!(items.ty, Type::List { size: None, .. }) || matches!(zero.ty, Type::Nullable(_)) {
        return None;
    }
    let argument = |slot| {
        arguments
            .iter()
            .position(|argument| argument.parameter == Some(slot))
    };
    let callback = &arguments[argument(0)?].value;
    let (ExprKind::StaticCallable(target), Type::Function(function)) =
        (&callback.kind, &callback.ty)
    else {
        return None;
    };
    let [accumulator, member] = function.parameters.as_slice() else {
        return None;
    };
    let inputs = [argument(1)?, argument(2)?];
    let lifted = match target.as_ref() {
        Callee::Anonymous(function) => function.template.value,
        _ => {
            let (module, declaration) = target.declaration()?;
            program.modules[module].declarations[declaration]
                .kind
                .role()
                == Some(Role::Value)
        }
    };
    let parameters = [accumulator, member]
        .into_iter()
        .enumerate()
        .map(|(position, parameter)| Argument {
            parameter: Some(position),
            pack: None,
            value: Expr {
                kind: ExprKind::Local(position),
                ty: parameter.ty().clone(),
                at: callback.at,
                category: ValueCategory::Connection,
            },
        })
        .collect();
    let combiner = Expr {
        kind: ExprKind::Call {
            callee: (**target).clone(),
            arguments: parameters,
            form: Form::Named,
            text: None,
            braced: Vec::new(),
            lifted,
        },
        ty: value.ty.clone(),
        at: callback.at,
        category: ValueCategory::Connection,
    };
    Some(OrderedReduction { inputs, combiner })
}

/// A checked result-bearing map with one split dictionary (MAP-1, MAP-2).
pub struct KeyedMap {
    /// The written lifecycle key set, or the split dictionary's keys.
    pub keys: Option<usize>,
    /// The callback applied to this child's bindings.
    pub callback: Expr,
    /// Each bound argument's source, in the callback call's argument order.
    pub inputs: Vec<KeyedMapInput>,
}

/// How a checked map argument is delivered to one child.
#[derive(Clone, Copy)]
pub enum KeyedMapInput {
    /// The key that the engine publishes when the child starts (MAP-9).
    Key,
    /// The entry of this child in the written dictionary argument.
    Split(usize),
    /// The entire connection supplied by the written argument.
    Whole(usize),
    /// A value passed to a compile-time parameter.
    Configuration(usize),
}

/// A checked keyed map form that the single-dictionary adapter cannot bind.
pub enum KeyedMapProblem {
    /// The key is a connection, but this callback takes a constant.
    ConstantKey(Location),
    /// A dependent argument supplies the child with a static callable.
    FunctionArgument(Location),
    /// More than one dictionary needs to be split.
    MultipleSplitDictionaries,
}

// MAP-4: an untyped graph local has no result place to reveal the
// collection. A known split dictionary argument can still identify it.
fn deferred_map_splits_dictionary(
    arguments: &[Argument],
    parameters: &[hgl_analyser::FunctionParameter],
) -> bool {
    let split = |argument: &Argument| match &argument.value.ty {
        Type::Dict { value, .. } | Type::List { element: value, .. }
            if parameters
                .iter()
                .any(|parameter| parameter.ty() == value.as_ref())
                && parameters
                    .iter()
                    .all(|parameter| parameter.ty() != &argument.value.ty) =>
        {
            Some(matches!(argument.value.ty, Type::Dict { .. }))
        }
        _ => None,
    };
    arguments.iter().filter_map(split).reduce(|a, b| a && b) == Some(true)
}

/// Bind the checked dependent pack to the selected callback's own slots.
/// A missing callback argument stays omitted, so it keeps its own default.
pub fn keyed_map_call(
    program: &Program,
    value: &Expr,
) -> Option<Result<KeyedMap, KeyedMapProblem>> {
    let ExprKind::Call {
        callee, arguments, ..
    } = &value.kind
    else {
        return None;
    };
    if let Callee::DeferredOperator(call) = callee
        && Some(call.identity.module) == program.core()
        && program.modules[call.identity.module].declarations[call.identity.declaration].name
            == "map"
        && arguments
            .iter()
            .any(|argument| matches!(argument.value.kind, ExprKind::Unread))
        && let Some((callback, function)) = arguments.iter().find_map(|argument| {
            let Type::Function(function) = &argument.value.ty else {
                return None;
            };
            function
                .parameters
                .iter()
                .any(|parameter| matches!(parameter.ty(), Type::Function(_)))
                .then_some((argument, function))
        })
        && (matches!(call.expected, Some(Type::Dict { .. }))
            || matches!(call.expected, None | Some(Type::Unknown))
                && deferred_map_splits_dictionary(arguments, &function.parameters))
    {
        // The checker can defer a forwarded function argument before the
        // native candidate and its dependent pack slots are recorded.
        return Some(Err(KeyedMapProblem::FunctionArgument(callback.value.at)));
    }
    // MAP-4: the selected result carries the split dictionary's key type.
    let Type::Dict { key, value: result } = &value.ty else {
        return None;
    };
    let (module, declaration) = callee.declaration()?;
    if Some(module) != program.core() {
        return None;
    }
    let owner = &program.modules[module];
    let declared = &owner.declarations[declaration];
    if declared.name != "map"
        || !matches!(
            declared.kind,
            DeclarationKind::NativeCandidate(Role::Temporal, _)
        )
    {
        return None;
    }
    let Detail::Callable(signature) = &owner.details[declaration] else {
        return None;
    };
    let position = |name: &str| {
        signature
            .parameters
            .iter()
            .position(|item| item.name == name)
    };
    let keys_parameter = position("keys")?;
    let callback_parameter = position("f")?;
    let pack_parameter = position("args")?;
    let selected = signature
        .generics
        .iter()
        .position(|item| item.name == "key")?;
    let GenericArgument::Constant(selected) = callee.generics().get(selected)? else {
        return None;
    };
    let ConstantValue::Str(key_name) = &selected.value else {
        return None;
    };
    let callback = &arguments
        .iter()
        .find(|arg| arg.parameter == Some(callback_parameter))?
        .value;
    let (ExprKind::StaticCallable(target), Type::Function(function)) =
        (&callback.kind, &callback.ty)
    else {
        return None;
    };
    let lifted = match target.as_ref() {
        Callee::Anonymous(function) => function.template.value,
        _ => {
            let (module, declaration) = target.declaration()?;
            program.modules[module].declarations[declaration]
                .kind
                .role()
                == Some(Role::Value)
        }
    };
    let keys = arguments.iter().position(|arg| {
        arg.parameter == Some(keys_parameter)
            && !matches!(arg.value.kind, ExprKind::Literal(Value::Null))
    });
    let supplied: Vec<_> = arguments
        .iter()
        .enumerate()
        .filter(|(_, arg)| arg.parameter == Some(pack_parameter))
        .collect();
    if let Some((_, argument)) = supplied
        .iter()
        .find(|(_, argument)| matches!(argument.value.ty, Type::Function(_)))
    {
        return Some(Err(KeyedMapProblem::FunctionArgument(argument.value.at)));
    }
    let parameters = function.parameters.iter().enumerate().filter(|(_, item)| {
        item.declared()
            .is_none_or(|declared| !declared.configuration)
    });
    let mut inputs = Vec::new();
    let mut bound = Vec::new();
    let mut matched = 0;
    let mut splits = 0;
    for (ordinal, (parameter, item)) in parameters.enumerate() {
        let ty = item.ty();
        let name = item.name();
        let arg_key = name.map_or(PackKey::Position(ordinal), |name| {
            PackKey::Name(name.to_string())
        });
        let (input, expr) = if name == Some(key_name.as_str()) {
            if item.declared().is_some_and(|parameter| parameter.constant) {
                return Some(Err(KeyedMapProblem::ConstantKey(callback.at)));
            }
            (KeyedMapInput::Key, None)
        } else {
            let Some(&(source, arg)) = supplied
                .iter()
                .find(|(_, arg)| arg.pack.as_ref() == Some(&arg_key))
            else {
                continue;
            };
            matched += 1;
            if item.declared().is_some_and(|parameter| parameter.constant) {
                (
                    KeyedMapInput::Configuration(source),
                    Some(arg.value.clone()),
                )
            } else if &arg.value.ty == ty {
                (KeyedMapInput::Whole(source), None)
            } else if matches!(&arg.value.ty, Type::Dict { key: actual_key, value: member }
                if actual_key == key && member.as_ref() == ty)
            {
                splits += 1;
                (KeyedMapInput::Split(source), None)
            } else {
                return None;
            }
        };
        let local = inputs.len();
        inputs.push(input);
        bound.push(Argument {
            parameter: Some(parameter),
            pack: None,
            value: expr.unwrap_or_else(|| Expr {
                kind: ExprKind::Local(local),
                ty: ty.clone(),
                at: callback.at,
                category: ValueCategory::Connection,
            }),
        });
    }
    if splits > 1 {
        return Some(Err(KeyedMapProblem::MultipleSplitDictionaries));
    }
    if splits != 1 || supplied.len() != matched {
        return None;
    }
    let callback = Expr {
        kind: ExprKind::Call {
            callee: (**target).clone(),
            arguments: bound,
            form: Form::Named,
            text: None,
            braced: Vec::new(),
            lifted,
        },
        ty: result.as_ref().clone(),
        at: callback.at,
        category: ValueCategory::Connection,
    };
    Some(Ok(KeyedMap {
        keys,
        callback,
        inputs,
    }))
}

/// What a call that leaves a parameter out gives it (UNIT-8).
pub enum LeftOut<'p> {
    /// Its default, in the declaration’s enclosing scope (UNIT-7).
    Default(DefaultValue<'p>),
    /// `null`. UNIT-8: "A parameter whose type is written `T?` needs no
    /// default to be omitted: it is taken to have a default of `null`".
    Null,
}

/// A checked default, with the arguments to its enclosing declaration in place.
pub enum DefaultValue<'p> {
    /// The original checked expression.
    Declared(&'p DefaultExpr),
    /// An expression specialized for this call (GEN-6, UNIT-7).
    Specialized(Box<DefaultExpr>),
}

impl Deref for DefaultValue<'_> {
    type Target = Expr;

    fn deref(&self) -> &Expr {
        &self.checked().value
    }
}

impl DefaultValue<'_> {
    /// The expression and its local bindings as an isolated body (UNIT-7).
    pub fn body(&self) -> Body {
        self.checked().clone().into_body()
    }

    fn checked(&self) -> &DefaultExpr {
        match self {
            Self::Declared(default) => default,
            Self::Specialized(default) => default,
        }
    }
}

/// What a call gives parameter `index` when it leaves it out (UNIT-8).
/// A named `callee` may declare a default. A `T?` parameter without a
/// declared default is given `null`, including in an anonymous function.
pub fn left_out<'p>(
    program: &'p Program,
    callee: &Callee,
    index: usize,
    parameter: &Parameter,
) -> Option<LeftOut<'p>> {
    if parameter.configuration
        && let Callee::Candidate {
            operator,
            operator_generics,
            operator_type_packs,
            ..
        } = callee
        && let Some(default) = program.inherited_configuration_default(callee, index)
    {
        let mut diagnostics = hgl_diagnostics::Diagnostics::new();
        let default = program.specialize_operator_default(
            default,
            *operator,
            operator_generics,
            operator_type_packs,
            parameter,
            &mut diagnostics,
        );
        return (!diagnostics.has_errors())
            .then(|| LeftOut::Default(DefaultValue::Specialized(Box::new(default))));
    }
    let default = callee.declaration().and_then(|(module, declaration)| {
        program.modules[module]
            .defaults
            .get(declaration)
            .and_then(|defaults| defaults.get(index))
            .and_then(Option::as_ref)
    });
    if let Some(default) = default {
        if default.needs_type_specialization()
            && !callee.generics().is_empty()
            && let Some(owner) = callee.declaration()
        {
            let mut diagnostics = hgl_diagnostics::Diagnostics::new();
            let default = program.specialize_parameter_default(
                default,
                owner,
                callee.generics(),
                callee.type_packs(),
                parameter,
                &mut diagnostics,
            );
            return (!diagnostics.has_errors())
                .then(|| LeftOut::Default(DefaultValue::Specialized(Box::new(default))));
        }
        Some(LeftOut::Default(DefaultValue::Declared(default)))
    } else if matches!(parameter.ty, Type::Nullable(_)) {
        Some(LeftOut::Null)
    } else {
        None
    }
}

/// The default of the field `default` of a struct, which a construction
/// that leaves the field out gives it (TYPE-5), read with no local in
/// scope (UNIT-7). `None` where the analysed program has none for it.
pub fn field_default(program: &Program, default: FieldRef) -> Option<&DefaultExpr> {
    program.modules[default.module]
        .defaults
        .get(default.declaration)
        .and_then(|defaults| defaults.get(default.field))
        .and_then(Option::as_ref)
}

/// TYPE-5: the checked default of an omitted field at an applied struct type.
/// Its own defining declaration supplies the lexical and generic scope (UNIT-7).
pub fn applied_field_default<'p>(
    program: &'p Program,
    field: FieldRef,
    structure: &Type,
    expected: &Type,
) -> Option<DefaultValue<'p>> {
    let default = field_default(program, field)?;
    let Detail::Struct(owner) = program
        .modules
        .get(field.module)?
        .details
        .get(field.declaration)?
    else {
        return None;
    };
    if owner.generics.is_empty()
        || matches!(&default.value.kind, ExprKind::Literal(Value::Null))
        || matches!(&default.value.kind,
        ExprKind::Sequence(entries) if entries.is_empty())
        || matches!(&default.value.kind,
        ExprKind::Pairs(entries) if entries.is_empty())
    {
        return Some(DefaultValue::Declared(default));
    }
    let arguments = field_arguments(program, field, structure)?;
    program
        .specialize_field_default(field, &arguments, expected)
        .map(|value| DefaultValue::Specialized(Box::new(value)))
}

/// The arguments of the struct that writes this default, following only
/// parents that supply it. A repeated path to the same application is one.
fn field_arguments(
    program: &Program,
    field: FieldRef,
    structure: &Type,
) -> Option<Vec<hgl_analyser::Argument>> {
    let mut pending = vec![structure.clone()];
    let mut visited: HashMap<_, Vec<Vec<_>>> = HashMap::new();
    let mut budget = hgl_analyser::INSTANCES;
    let mut found = None;
    while let Some(current) = pending.pop() {
        let Type::Named {
            module,
            declaration,
            arguments,
            ..
        } = current
        else {
            continue;
        };
        if (module, declaration) == (field.module, field.declaration) {
            if found
                .as_ref()
                .is_some_and(|previous| previous != &arguments)
            {
                return None;
            }
            found = Some(arguments);
            continue;
        }
        let Some(Detail::Struct(read)) = program.modules[module].details.get(declaration) else {
            continue;
        };
        if !read
            .members
            .iter()
            .any(|member| member.default == Some(field))
        {
            continue;
        }
        let applications = visited.entry((module, declaration)).or_default();
        if applications.contains(&arguments) {
            continue;
        }
        budget = budget.checked_sub(1)?;
        let bindings: Vec<_> = read
            .generics
            .iter()
            .take_while(|generic| !matches!(generic.kind, GenericKind::Pack { .. }))
            .map(|generic| generic.at)
            .zip(&arguments)
            .collect();
        pending.extend(
            read.parents
                .iter()
                .map(|parent| program.checked_layout_type(parent, &bindings)),
        );
        applications.push(arguments);
    }
    found
}
