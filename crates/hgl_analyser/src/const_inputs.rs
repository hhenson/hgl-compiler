//! Ordinary const parameters read by a compile-time check (FN-1, FMT-3).
//!
//! A const parameter can also be read at run time. Only a check that
//! needs its value during analysis adds its slot to the instance inputs.

use std::collections::{BTreeSet, HashMap, VecDeque};

use hgl_diagnostics::Location;

use crate::formatting;
use crate::hir::{
    Argument, BinaryOperator, Callee, Expr, ExprKind, Form, Stmt, Value, ValueCategory,
};
use crate::{
    Argument as GenericArgument, Binding, ConstantValue, Detail, Program, Role, Scalar, Type,
};

type Declaration = (usize, usize);
type Slot = (Declaration, usize);
pub(crate) type Demands = HashMap<Declaration, BTreeSet<usize>>;

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

/// Why a call's compile-time inputs could not be read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConstInputFault {
    /// The compiler cannot evaluate a required value here.
    Unread,
    /// A required value's evaluation exceeded the instance argument budget.
    Limit,
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
        use ConstInputFault::Unread;

        let Some((module, declaration)) = callee.declaration() else {
            return Ok(Vec::new());
        };
        let Some(demand) = self.const_demands.get(&(module, declaration)) else {
            return Ok(Vec::new());
        };
        let owner = &self.modules[module];
        let Some(Detail::Callable(signature)) = owner.details.get(declaration) else {
            return Err(Unread);
        };
        let mut reader = Reader {
            program: self,
            remaining: INPUT_BUDGET,
        };
        demand
            .iter()
            .map(|&slot| {
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
                if key.len() > INPUT_BUDGET {
                    return Err(ConstInputFault::Limit);
                }
                Ok(ConstInput { slot, value, key })
            })
            .collect()
    }
}

struct Reader<'a> {
    program: &'a Program,
    remaining: usize,
}

impl Reader<'_> {
    fn read(
        &mut self,
        expression: &Expr,
        depth: usize,
        generics: &[(Location, &GenericArgument)],
    ) -> Result<Expr, ConstInputFault> {
        use ConstInputFault::{Limit, Unread};

        let next = depth.checked_sub(1).ok_or(Limit)?;
        self.remaining = self.remaining.checked_sub(1).ok_or(Limit)?;
        let kind = match &expression.kind {
            ExprKind::Literal(value) => {
                if let Value::Str(text) = value {
                    self.remaining = self.remaining.checked_sub(text.len()).ok_or(Limit)?;
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
                    self.remaining = self.remaining.checked_sub(text.len()).ok_or(Limit)?;
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
                return self.read(&constant.value, next, &[]);
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
                return self.read(returned, next, &[]);
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
                        &given.value
                    } else {
                        let default = member.default.ok_or(Unread)?;
                        &self.program.modules[default.module]
                            .defaults
                            .get(default.declaration)
                            .and_then(|fields| fields.get(default.field))
                            .and_then(Option::as_ref)
                            .ok_or(Unread)?
                            .value
                    };
                    fields.push(Argument {
                        parameter: Some(position),
                        pack: None,
                        value: self.read(value, next, generics)?,
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
                self.remaining = self
                    .remaining
                    .checked_sub(1)
                    .ok_or(ConstInputFault::Limit)?;
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
    let mut uses: HashMap<Slot, Vec<Slot>> = HashMap::new();
    let mut pending = VecDeque::new();
    for (module_index, module) in program.modules.iter().enumerate() {
        let bindings: HashMap<_, _> = module
            .uses
            .iter()
            .filter_map(|used| match &used.binding {
                Binding::Local { at, .. } => Some(((used.at.file, used.at.span.start), *at)),
                _ => None,
            })
            .collect();
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
            crate::values::walk_body_calls(body, &mut |call| {
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
                    && crate::values::fixed_expression(&pattern.value, &|slot| {
                        slots.values().any(|position| *position == slot)
                    })
                {
                    slots_read(&pattern.value, &bindings, &slots, |slot| {
                        pending.push_back((caller, slot));
                    });
                }
                for argument in arguments {
                    let Some(position) = argument.parameter else {
                        continue;
                    };
                    slots_read(&argument.value, &bindings, &slots, |slot| {
                        uses.entry((called, position))
                            .or_default()
                            .push((caller, slot));
                    });
                }
            });
        }
    }
    while let Some((declaration, slot)) = pending.pop_front() {
        if required.entry(declaration).or_default().insert(slot)
            && let Some(callers) = uses.remove(&(declaration, slot))
        {
            pending.extend(callers);
        }
    }
    required
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
                remaining: INPUT_BUDGET,
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
