//! Ordinary const parameters read by a compile-time check (FN-1, FMT-3).
//!
//! A const parameter can also be read at run time. Only a check that
//! needs its value during analysis adds its slot to the instance inputs.

use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};

use hgl_diagnostics::{Diagnostics, Location};

use crate::formatting;
use crate::hir::{
    Argument, BinaryOperator, Callee, Expr, ExprKind, Form, Stmt, Value, ValueCategory,
};
use crate::{
    Argument as GenericArgument, Binding, ConstantValue, Detail, Program, Role, Scalar, Type,
};

type Declaration = (usize, usize);
type Slot = (Declaration, usize);
pub(crate) type Demands = HashMap<Declaration, BTreeMap<usize, Demand>>;

/// A deferred operator can select a body that only uses this value at run time.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Demand {
    IfSelected,
    Always,
}

/// A known ordinary const input that participates in the instance identity.
#[derive(Clone, Debug)]
pub struct ConstInput {
    /// The parameter's position in its declaration, including configuration.
    pub slot: usize,
    /// The normalized value, independent of the caller's spelling or location.
    pub(crate) value: Expr,
    /// The identity of the value for both analyser and lowering keys.
    pub key: String,
}

/// The values analysis can use, and whether reading another hit its budget.
#[derive(Clone, Debug, Default)]
pub(crate) struct ReadInputs {
    pub(crate) values: Vec<ConstInput>,
    pub(crate) limit: Option<ConstInputFault>,
}

/// Why a call's compile-time inputs could not be read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConstInputFault {
    /// The compiler cannot evaluate a required value here.
    Unread,
    /// Reading a required value exceeded the reader's traversal budget.
    ReaderLimit,
    /// A required value's text exceeded the instance argument budget.
    TextLimit,
}

/// Match the size bound used for a growing generic value argument (GEN-13).
const INPUT_BUDGET: usize = 1 << 16;
const INPUT_DEPTH: usize = 64;

impl Program {
    /// Whether a compile-time check in this declaration needs const inputs.
    pub fn needs_const_inputs(&self, declaration: Declaration) -> bool {
        self.const_demands.contains_key(&declaration)
    }

    /// The const inputs a resolved call must have to check its instance.
    /// The ordinary defaults come from the selected declaration or its operator.
    pub fn call_const_inputs(
        &self,
        callee: &Callee,
        arguments: &[Argument],
    ) -> Result<Vec<ConstInput>, ConstInputFault> {
        let known = self.known_const_inputs(callee, arguments);
        if let Some(limit) = known.limit {
            return Err(limit);
        }
        let demands = callee
            .declaration()
            .and_then(|declaration| self.const_demands.get(&declaration));
        if demands.is_some_and(|demands| {
            demands.iter().any(|(&slot, &demand)| {
                demand == Demand::Always && !known.values.iter().any(|input| input.slot == slot)
            })
        }) {
            return Err(ConstInputFault::Unread);
        }
        // A deferred call only needs the conditional value where its
        // selected candidate reads it. That candidate checks its own inputs.
        Ok(known.values)
    }

    /// Analysis can check the known inputs even when another input remains
    /// unread. Lowering requires the complete set through `call_const_inputs`.
    pub(crate) fn known_const_inputs(&self, callee: &Callee, arguments: &[Argument]) -> ReadInputs {
        use ConstInputFault::{ReaderLimit, TextLimit, Unread};

        let Some((module, declaration)) = callee.declaration() else {
            return ReadInputs::default();
        };
        let Some(demand) = self.const_demands.get(&(module, declaration)) else {
            return ReadInputs::default();
        };
        let owner = &self.modules[module];
        let Some(Detail::Callable(signature)) = owner.details.get(declaration) else {
            return ReadInputs::default();
        };
        let mut reader = Reader {
            program: self,
            steps_left: INPUT_BUDGET,
            text_left: INPUT_BUDGET,
            reading: Vec::new(),
        };
        let mut known = ReadInputs::default();
        // Give unconditional inputs the first use of the read budget.
        let ordered = [Demand::Always, Demand::IfSelected]
            .into_iter()
            .flat_map(|mode| demand.iter().filter(move |(_, demand)| **demand == mode));
        for (&slot, &demand) in ordered {
            if demand == Demand::IfSelected && !self.instance_reads_const(callee, slot) {
                continue;
            }
            let text_before = reader.text_left;
            let input = (|| {
                let parameter = signature.parameters.get(slot).ok_or(Unread)?;
                let given = arguments
                    .iter()
                    .find(|argument| argument.parameter == Some(slot));
                let mut generics: Vec<_> = signature
                    .generics
                    .iter()
                    .zip(callee.generics())
                    .map(|(parameter, value)| (parameter.at, value))
                    .collect();
                let value = if let Some(given) = given {
                    reader.read(&given.value, INPUT_DEPTH, &[])?
                } else if let Some(default) = self.inherited_configuration_default(callee, slot) {
                    if let Callee::Candidate {
                        operator,
                        operator_generics,
                        ..
                    } = callee
                        && let Some(Detail::Callable(public)) =
                            self.modules[operator.0].details.get(operator.1)
                    {
                        generics = public
                            .generics
                            .iter()
                            .zip(operator_generics)
                            .map(|(parameter, value)| (parameter.at, value))
                            .collect();
                    }
                    reader.read(&default.value, INPUT_DEPTH, &generics)?
                } else if let Some(default) = owner
                    .defaults
                    .get(declaration)
                    .and_then(|defaults| defaults.get(slot))
                    .and_then(Option::as_ref)
                {
                    reader.read(&default.value, INPUT_DEPTH, &generics)?
                } else if matches!(parameter.ty, Type::Nullable(_)) {
                    Expr {
                        kind: ExprKind::Literal(Value::Null),
                        ty: parameter.ty.clone(),
                        at: parameter.at,
                        category: ValueCategory::Value,
                    }
                } else {
                    return Err(Unread);
                };
                let mut key = String::new();
                key_value(&value, &mut key)?;
                // A string already spent its text budget before key framing.
                // Keep a separate bound on composite and scalar key metadata.
                if key.len() > INPUT_BUDGET
                    && !matches!(value.kind, ExprKind::Literal(Value::Str(_)))
                {
                    return Err(ReaderLimit);
                }
                Ok(ConstInput { slot, value, key })
            })();
            match input {
                Ok(input) => known.values.push(input),
                Err(fault) => {
                    if demand == Demand::Always
                        && (fault == TextLimit || (fault == ReaderLimit && known.limit.is_none()))
                    {
                        known.limit = Some(fault);
                    }
                    reader.text_left = text_before;
                }
            }
        }
        known
    }

    /// Follow a possible demand with the instance's types in place and its
    /// ordinary consts still unread. A forwarded value only enters the key
    /// if a selected candidate reads it, or selection remains unresolved.
    fn instance_reads_const(&self, callee: &Callee, slot: usize) -> bool {
        let mut pending = VecDeque::from([(callee.clone(), slot)]);
        let mut seen = Vec::new();
        while let Some((callee, slot)) = pending.pop_front() {
            let Some((module, declaration)) = callee.declaration() else {
                return true;
            };
            match self
                .const_demands
                .get(&(module, declaration))
                .and_then(|slots| slots.get(&slot))
            {
                Some(Demand::Always) => return true,
                Some(Demand::IfSelected) => {}
                None => continue,
            }
            if seen
                .iter()
                .any(|(target, position)| target == &callee && *position == slot)
            {
                continue;
            }
            if seen.len() >= crate::INSTANCES {
                return true;
            }
            let unit = &self.modules[module];
            let Some(Detail::Callable(signature)) = unit.details.get(declaration) else {
                return true;
            };
            let Some(parameter) = signature.parameters.get(slot) else {
                return true;
            };
            let (Callee::Candidate { requirements, .. } | Callee::Function { requirements, .. }) =
                &callee
            else {
                return true;
            };
            let placement_reports = RefCell::new(Diagnostics::new());
            let mut body_reports = Diagnostics::new();
            let Some(instance) = self.checked_instance(
                (module, declaration),
                callee.generics(),
                requirements,
                crate::InstanceInputs {
                    type_packs: callee.type_packs(),
                    ..crate::InstanceInputs::default()
                },
                &placement_reports,
                &mut body_reports,
            ) else {
                return true;
            };
            if body_reports.has_errors() || placement_reports.borrow().has_errors() {
                return true;
            }
            seen.push((callee, slot));
            let bindings = local_bindings(unit);
            let slots = HashMap::from([(parameter.at, slot)]);
            let reads_parameter = |argument: &Expr| {
                let mut reads = false;
                slots_read(argument, &bindings, &slots, |_| reads = true);
                reads
            };
            let mut required = false;
            instance.walk_calls(&mut |call| {
                let ExprKind::Call {
                    callee, arguments, ..
                } = &call.kind
                else {
                    return;
                };
                if let Some(signature) = formatting::value_signature(self, callee) {
                    required |= formatting::argument(signature, arguments, "fmt_str")
                        .is_some_and(|pattern| reads_parameter(&pattern.value));
                    return;
                }
                if matches!(callee, Callee::DeferredPackCall) {
                    required = true;
                    return;
                }
                for argument in arguments
                    .iter()
                    .filter(|argument| reads_parameter(&argument.value))
                {
                    if let Some(slot) = argument
                        .parameter
                        .filter(|_| callee.declaration().is_some())
                    {
                        pending.push_back((callee.clone(), slot));
                    } else {
                        required = true;
                    }
                }
            });
            if required {
                return true;
            }
        }
        false
    }
}

struct Reader<'a> {
    program: &'a Program,
    steps_left: usize,
    text_left: usize,
    reading: Vec<(Declaration, Option<usize>)>,
}

impl Reader<'_> {
    fn read_declaration(
        &mut self,
        declaration: Declaration,
        field: Option<usize>,
        value: &Expr,
        depth: usize,
        generics: &[(Location, &GenericArgument)],
    ) -> Result<Expr, ConstInputFault> {
        let dependency = (declaration, field);
        if self.reading.contains(&dependency) {
            // UNIT-8 and UNIT-13 report these dependency cycles themselves.
            return Err(ConstInputFault::Unread);
        }
        self.reading.push(dependency);
        let read = self.read(value, depth, generics);
        self.reading.pop();
        read
    }

    fn read(
        &mut self,
        expression: &Expr,
        depth: usize,
        generics: &[(Location, &GenericArgument)],
    ) -> Result<Expr, ConstInputFault> {
        use ConstInputFault::{ReaderLimit, TextLimit, Unread};

        let next = depth.checked_sub(1).ok_or(ReaderLimit)?;
        self.steps_left = self.steps_left.checked_sub(1).ok_or(ReaderLimit)?;
        let kind = match &expression.kind {
            ExprKind::Literal(value) => {
                if let Value::Str(text) = value {
                    self.text_left = self.text_left.checked_sub(text.len()).ok_or(TextLimit)?;
                }
                ExprKind::Literal(value.clone())
            }
            ExprKind::EnumMember {
                module,
                declaration,
                member,
            } => ExprKind::EnumMember {
                module: *module,
                declaration: *declaration,
                member: *member,
            },
            ExprKind::GenericValue { at, .. } => {
                let (_, GenericArgument::Constant(given)) = generics
                    .iter()
                    .find(|(parameter, _)| parameter == at)
                    .ok_or(Unread)?
                else {
                    return Err(Unread);
                };
                let value = given.value.literal().ok_or(Unread)?;
                if let Value::Str(text) = &value {
                    self.text_left = self.text_left.checked_sub(text.len()).ok_or(TextLimit)?;
                }
                ExprKind::Literal(value)
            }
            ExprKind::Constant {
                module,
                declaration,
            } => {
                let constant = self.program.modules[*module]
                    .constants
                    .get(*declaration)
                    .and_then(Option::as_ref)
                    .ok_or(Unread)?;
                return self.read_declaration(
                    (*module, *declaration),
                    None,
                    &constant.value,
                    next,
                    &[],
                );
            }
            ExprKind::Call {
                callee,
                arguments,
                form: Form::Named,
                ..
            } if arguments.is_empty() && expression.ty == Type::Scalar(Scalar::Str) => {
                // Match the existing format reader's single-return string
                // helpers. A general compile-time call is tracked separately.
                let (module, declaration) = callee.declaration().ok_or(Unread)?;
                let owner = &self.program.modules[module];
                if owner.declarations[declaration].kind
                    != crate::DeclarationKind::Function(Role::Value)
                {
                    return Err(Unread);
                }
                let Some(Detail::Callable(signature)) = owner.details.get(declaration) else {
                    return Err(Unread);
                };
                if !signature.generics.is_empty() || !signature.parameters.is_empty() {
                    return Err(Unread);
                }
                let body = owner
                    .bodies
                    .get(declaration)
                    .and_then(Option::as_ref)
                    .ok_or(Unread)?;
                let [
                    Stmt::Return {
                        value: Some(returned),
                        ..
                    },
                ] = body.statements.as_slice()
                else {
                    return Err(Unread);
                };
                return self.read_declaration((module, declaration), None, returned, next, &[]);
            }
            ExprKind::Tuple(parts) | ExprKind::Sequence(parts)
                if matches!(expression.ty, Type::Tuple(_) | Type::List { .. }) =>
            {
                let parts = parts
                    .iter()
                    .map(|part| self.read(part, next, generics))
                    .collect::<Result<Vec<_>, _>>()?;
                if matches!(expression.kind, ExprKind::Tuple(_)) {
                    ExprKind::Tuple(parts)
                } else {
                    ExprKind::Sequence(parts)
                }
            }
            ExprKind::Construct {
                module,
                declaration,
                arguments,
                ..
            } => {
                let Some(Detail::Struct(structure)) =
                    self.program.modules[*module].details.get(*declaration)
                else {
                    return Err(Unread);
                };
                let mut fields = Vec::with_capacity(structure.members.len());
                for (position, member) in structure.members.iter().enumerate() {
                    let value = if let Some(given) =
                        arguments.iter().find(|arg| arg.parameter == Some(position))
                    {
                        self.read(&given.value, next, generics)?
                    } else {
                        let default = member.default.ok_or(Unread)?;
                        let value = &self.program.modules[default.module]
                            .defaults
                            .get(default.declaration)
                            .and_then(|fields| fields.get(default.field))
                            .and_then(Option::as_ref)
                            .ok_or(Unread)?
                            .value;
                        self.read_declaration(
                            (default.module, default.declaration),
                            Some(default.field),
                            value,
                            next,
                            generics,
                        )?
                    };
                    fields.push(Argument {
                        parameter: Some(position),
                        pack: None,
                        value,
                    });
                }
                ExprKind::Construct {
                    module: *module,
                    declaration: *declaration,
                    arguments: fields,
                    infer_types: false,
                }
            }
            ExprKind::Widen(inner) => match self.scalar(inner, next, generics)? {
                ConstantValue::Int(value) => ExprKind::Literal(Value::Float(value as f64)),
                _ => return Err(Unread),
            },
            ExprKind::Block {
                statements,
                value: Some(value),
            } if statements.is_empty() => {
                return self.read(value, next, generics);
            }
            ExprKind::If {
                condition,
                then,
                otherwise,
                ..
            } => {
                let branch = match self.scalar(condition, next, generics)? {
                    ConstantValue::Bool(true) => then,
                    ConstantValue::Bool(false) => otherwise.as_ref().ok_or(Unread)?,
                    _ => return Err(Unread),
                };
                return self.read(branch, next, generics);
            }
            _ => return self.operation(expression, next, generics),
        };
        Ok(Expr {
            kind,
            at: expression.at,
            ty: expression.ty.clone(),
            category: ValueCategory::Value,
        })
    }

    fn scalar(
        &mut self,
        value: &Expr,
        depth: usize,
        generics: &[(Location, &GenericArgument)],
    ) -> Result<ConstantValue, ConstInputFault> {
        match self.read(value, depth, generics)?.kind {
            ExprKind::Literal(literal) => Ok(literal.into()),
            _ => Err(ConstInputFault::Unread),
        }
    }

    fn operation(
        &mut self,
        value: &Expr,
        depth: usize,
        generics: &[(Location, &GenericArgument)],
    ) -> Result<Expr, ConstInputFault> {
        use ConstInputFault::Unread;

        if !self.builtin_operation(value) {
            return Err(Unread);
        }
        let calculated = if value.ty == Type::Scalar(Scalar::Str)
            && matches!(value.kind.binary(), Some((BinaryOperator::Add, _, _)))
        {
            ConstantValue::Str(self.concatenation(value, depth, generics)?)
        } else if let Some((operator, left, right)) = value.kind.binary() {
            let left = self.scalar(left, depth, generics)?;
            match (&left, operator) {
                (ConstantValue::Bool(false), BinaryOperator::And)
                | (ConstantValue::Bool(true), BinaryOperator::Or) => left,
                _ => crate::constant::combined(
                    operator,
                    &left,
                    &self.scalar(right, depth, generics)?,
                ),
            }
        } else if let Some((operator, operand)) = value.kind.unary() {
            crate::constant::negated_or_not(operator, &self.scalar(operand, depth, generics)?)
        } else {
            return Err(Unread);
        };
        Ok(Expr {
            kind: ExprKind::Literal(calculated.literal().ok_or(Unread)?),
            at: value.at,
            ty: value.ty.clone(),
            category: ValueCategory::Value,
        })
    }

    /// A user's named candidate can run value code. Fold only the checked
    /// builtin arithmetic, with no opaque configuration operands.
    fn builtin_operation(&self, value: &Expr) -> bool {
        match &value.kind {
            ExprKind::Binary { .. } | ExprKind::Unary { .. } => true,
            ExprKind::Call {
                callee,
                form,
                arguments,
                ..
            } => {
                let arity = match form {
                    Form::Binary(_) => 2,
                    Form::Unary(_) => 1,
                    Form::Named => return false,
                };
                arguments.len() == arity
                    && callee.declaration().is_some_and(|(module, declaration)| {
                        self.program.core() == Some(module)
                            && matches!(
                                self.program.modules[module].declarations[declaration].kind,
                                crate::DeclarationKind::NativeCandidate(Role::Value, _)
                            )
                    })
            }
            _ => false,
        }
    }

    /// The components of one joined pattern share the same depth budget.
    fn concatenation(
        &mut self,
        value: &Expr,
        depth: usize,
        generics: &[(Location, &GenericArgument)],
    ) -> Result<String, ConstInputFault> {
        let mut text = String::new();
        let mut pending = vec![value];
        while let Some(piece) = pending.pop() {
            if piece.ty == Type::Scalar(Scalar::Str)
                && self.builtin_operation(piece)
                && let Some((BinaryOperator::Add, left, right)) = piece.kind.binary()
            {
                self.steps_left = self
                    .steps_left
                    .checked_sub(1)
                    .ok_or(ConstInputFault::ReaderLimit)?;
                pending.extend([right, left]);
            } else {
                let ConstantValue::Str(piece) = self.scalar(piece, depth, generics)? else {
                    return Err(ConstInputFault::Unread);
                };
                text.push_str(&piece);
            }
        }
        Ok(text)
    }
}

/// Write keys from normalized values. Brackets frame lists, tuples and
/// named structures without repeatedly escaping a nested part's text.
fn key_value(value: &Expr, output: &mut String) -> Result<(), ConstInputFault> {
    use ConstInputFault::Unread;
    use std::fmt::Write;

    let parts = match &value.kind {
        ExprKind::Literal(Value::Str(text)) => {
            let _ = write!(output, "str:{}:", text.len());
            output.push_str(text);
            return Ok(());
        }
        ExprKind::Literal(Value::Float(value)) => {
            let _ = write!(output, "f64:{:016x}", value.to_bits());
            return Ok(());
        }
        ExprKind::Literal(value) => {
            output.push_str(&ConstantValue::from(value.clone()).key().ok_or(Unread)?);
            return Ok(());
        }
        ExprKind::EnumMember {
            module,
            declaration,
            member,
        } => {
            let _ = write!(output, "enum({module},{declaration},{member})");
            return Ok(());
        }
        ExprKind::Tuple(parts) => {
            output.push_str("tuple[");
            parts
        }
        ExprKind::Sequence(parts) => {
            output.push_str("list[");
            parts
        }
        ExprKind::Construct {
            module,
            declaration,
            arguments,
            ..
        } => {
            let _ = write!(output, "struct({module},{declaration})[");
            for argument in arguments {
                key_value(&argument.value, output)?;
                output.push(',');
            }
            output.push(']');
            return Ok(());
        }
        _ => return Err(Unread),
    };
    for part in parts {
        key_value(part, output)?;
        output.push(',');
    }
    output.push(']');
    Ok(())
}

/// Follow demand for a compile-time value back through its call arguments.
pub(crate) fn demands(program: &Program) -> Demands {
    let mut required = Demands::new();
    let mut uses: HashMap<Slot, Vec<(Slot, Demand)>> = HashMap::new();
    let mut pending = VecDeque::new();
    let mut candidates: HashMap<Declaration, Vec<Declaration>> = HashMap::new();
    for (module, unit) in program.modules.iter().enumerate() {
        for (declaration, declared) in unit.declarations.iter().enumerate() {
            if let Some((Role::Value, Some(operator))) = declared.kind.candidate() {
                candidates
                    .entry((operator.module, operator.declaration))
                    .or_default()
                    .push((module, declaration));
            }
        }
    }
    for (module_index, module) in program.modules.iter().enumerate() {
        let bindings = local_bindings(module);
        for (declaration, body) in module.bodies.iter().enumerate() {
            let (Some(body), Some(Detail::Callable(signature))) =
                (body, module.details.get(declaration))
            else {
                continue;
            };
            let slots: HashMap<Location, usize> = signature
                .parameters
                .iter()
                .enumerate()
                .filter(|(_, parameter)| parameter.constant)
                .map(|(position, parameter)| (parameter.at, position))
                .collect();
            if slots.is_empty() {
                continue;
            }
            let caller = (module_index, declaration);
            let mut read_pattern = |pattern: &Expr| {
                if crate::values::fixed_expression(pattern, &|_, at| {
                    bindings
                        .get(&(at.file, at.span.start))
                        .is_some_and(|at| slots.contains_key(at))
                }) {
                    slots_read(pattern, &bindings, &slots, |slot| {
                        pending.push_back(((caller, slot), Demand::Always));
                    });
                }
            };
            let mut deferred_at = HashSet::new();
            for selection in &module.selecting[declaration] {
                if selection.graph {
                    continue;
                }
                deferred_at.insert(selection.at);
                if let Some(pattern) = formatting::deferred_pattern(program, selection) {
                    read_pattern(pattern);
                    continue;
                }
                let operator = selection.operator;
                let Some(alternatives) = candidates.get(&(operator.module, operator.declaration))
                else {
                    continue;
                };
                for &(unit, member) in alternatives {
                    let candidate = &program.modules[unit];
                    if let Some(test) = candidate.declarations[member].test_owner
                        && (unit != module_index
                            || module.declarations[declaration].test_owner != Some(test))
                    {
                        continue;
                    }
                    let Some(Detail::Callable(parameters)) = candidate.details.get(member) else {
                        continue;
                    };
                    for (position, _) in parameters
                        .parameters
                        .iter()
                        .enumerate()
                        .filter(|(_, parameter)| parameter.constant)
                    {
                        if let Some(argument) = selection.argument_for(parameters, position) {
                            slots_read(argument, &bindings, &slots, |slot| {
                                uses.entry(((unit, member), position))
                                    .or_default()
                                    .push(((caller, slot), Demand::IfSelected));
                            });
                        }
                    }
                }
            }
            crate::values::walk_body_calls(body, &mut |call| {
                // A provisional candidate does not make its demands unconditional.
                if deferred_at.contains(&call.at) {
                    return;
                }
                let ExprKind::Call {
                    callee, arguments, ..
                } = &call.kind
                else {
                    return;
                };
                let Some(called) = callee.declaration() else {
                    return;
                };
                if let Some(signature) = formatting::value_signature(program, callee)
                    && let Some(pattern) = formatting::argument(signature, arguments, "fmt_str")
                {
                    read_pattern(&pattern.value);
                }
                for argument in arguments {
                    let Some(position) = argument.parameter else {
                        continue;
                    };
                    slots_read(&argument.value, &bindings, &slots, |slot| {
                        uses.entry((called, position))
                            .or_default()
                            .push(((caller, slot), Demand::Always));
                    });
                }
            });
        }
    }
    while let Some(((declaration, slot), demand)) = pending.pop_front() {
        let slots = required.entry(declaration).or_default();
        if slots.get(&slot).is_some_and(|earlier| *earlier >= demand) {
            continue;
        }
        slots.insert(slot, demand);
        if let Some(callers) = uses.get(&(declaration, slot)) {
            pending.extend(
                callers
                    .iter()
                    .map(|&(caller, edge)| (caller, demand.min(edge))),
            );
        }
    }
    required
}

fn local_bindings(module: &crate::Module) -> HashMap<(hgl_source::FileId, u32), Location> {
    module
        .uses
        .iter()
        .filter_map(|used| match &used.binding {
            Binding::Local { at, .. } => Some(((used.at.file, used.at.span.start), *at)),
            _ => None,
        })
        .collect()
}

/// Use lexical bindings so a captured parameter keeps its original slot.
fn slots_read(
    value: &Expr,
    bindings: &HashMap<(hgl_source::FileId, u32), Location>,
    slots: &HashMap<Location, usize>,
    mut found: impl FnMut(usize),
) {
    value.walk(&mut |value| {
        if matches!(
            value.kind,
            ExprKind::Local(_) | ExprKind::DeferredPresent(_)
        ) && let Some(location) = bindings.get(&(value.at.file, value.at.span.start))
            && let Some(slot) = slots.get(location)
        {
            found(*slot);
        }
    });
}

#[cfg(test)]
mod tests {
    use hgl_diagnostics::Diagnostics;
    use hgl_parser::{Entry, Limits, parse};
    use hgl_source::SourceMap;

    use super::*;
    use crate::{ParsedFile, analyse};

    /// Equal composite values have the same identity even when their named
    /// fields are given in a different order. Floating keys keep their bits.
    #[test]
    fn const_keys_compare_normalized_parts() {
        let mut sources = SourceMap::new();
        let mut diagnostics = Diagnostics::new();
        let file = sources
            .add(
                "m.hgl",
                r#"module m
struct Settings {
    part: tuple<str, f64>
    count: i64
}
const A: Settings = Settings(part: ("a", 0.0), count: 1)
const B: Settings = Settings(count: 1, part: ("a", 0.0))
const C: Settings = Settings(part: ("a", -0.0), count: 1)
"#,
            )
            .unwrap();
        let tokens = hgl_lexer::lex(&sources, file, &mut diagnostics);
        let tree = parse(
            &sources,
            file,
            &tokens,
            Entry::SourceFile,
            Limits::default(),
            &mut diagnostics,
        );
        let program = analyse(&[ParsedFile::new(file, tree.syntax())], &mut diagnostics);
        assert!(!diagnostics.has_errors(), "{diagnostics:?}");
        let key = |name: &str| {
            let module = &program.modules[0];
            let declaration = module.scope[name][0];
            let constant = module.constants[declaration].as_ref().unwrap();
            let mut reader = Reader {
                program: &program,
                steps_left: INPUT_BUDGET,
                text_left: INPUT_BUDGET,
                reading: Vec::new(),
            };
            let value = reader.read(&constant.value, INPUT_DEPTH, &[]).unwrap();
            let mut output = String::new();
            key_value(&value, &mut output).unwrap();
            output
        };
        assert_eq!(key("A"), key("B"));
        assert_ne!(key("A"), key("C"));
    }
}
