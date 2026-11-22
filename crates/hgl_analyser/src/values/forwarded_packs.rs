//! Turns a call's pack spreads into typed argument entries in an instance
//! (FN-19). Argument placement uses each entry's key and reads the input
//! slot that holds it.

use hgl_diagnostics::Category;
use hgl_parser::{SyntaxKind, SyntaxNode};

use super::Typer;
use crate::declared::names_parameter;
use crate::hir::{Callee, DeferredPackCall, Expr, ExprKind, Form, PackKey};
use crate::syntax::ungrouped;
use crate::types::Type;
use crate::{codes, error};

/// A concrete entry supplied by one spread in the original call.
pub(super) struct ForwardedArgument {
    /// The source pack key before binding to the destination.
    pub(super) key: PackKey,
    /// Where the spread's pack expression is written.
    pub(super) source: SyntaxNode,
    /// The typed read of the entry in the enclosing instance.
    pub(super) value: Expr,
}

impl Typer<'_> {
    /// Checks that a spread names a pack and preserves its call until the
    /// pack's concrete argument types and keys are available (FN-19).
    pub(super) fn defer_pack_call(
        &mut self,
        list: &SyntaxNode,
        result: Option<&Type>,
        expected: Option<&Type>,
    ) -> Option<(ExprKind, Type)> {
        if self.pack_inputs.is_some() {
            return None;
        }
        let mut spreads = spread_operands(list).peekable();
        spreads.peek()?;
        let source = list.parent()?;
        let before = self.diagnostics.len();
        for spread in spreads {
            let operand = ungrouped(spread.clone());
            let pack = self
                .local_of(&operand)
                .and_then(|local| self.signature.parameters.get(local))
                .is_some_and(|parameter| parameter.pack.is_some());
            if !pack {
                self.diagnostics.emit(error(
                    Category::Type,
                    codes::TYPE_MISMATCH,
                    self.at(&spread),
                    "a spread argument needs a pack".to_string(),
                    "FN-19",
                ));
            }
        }
        if self.diagnostics.len() != before {
            return Some((ExprKind::Unread, Type::Unknown));
        }
        let ty = result
            .filter(|ty| !names_parameter(ty))
            .or(expected)
            .or(result)
            .cloned()
            .unwrap_or(Type::Unknown);
        Some((
            ExprKind::Call {
                callee: Callee::DeferredPackCall(Box::new(DeferredPackCall {
                    source: self.at(&source),
                    expected: expected.cloned(),
                    hook: self.hook,
                })),
                arguments: Vec::new(),
                form: Form::Named,
                braced: Vec::new(),
                lifted: false,
            },
            ty,
        ))
    }

    /// The entries the instance gives to this call, in spread and capture order.
    pub(super) fn forwarded_arguments(&self, list: &SyntaxNode) -> Vec<ForwardedArgument> {
        let Some(inputs) = &self.pack_inputs else {
            return Vec::new();
        };
        let mut forwarded = Vec::new();
        for source in spread_operands(list) {
            let Some(local) = self.local_of(&ungrouped(source.clone())) else {
                continue;
            };
            let category = self.locals[local].category;
            for input in inputs.iter().filter(|input| input.parameter == local) {
                let key = input.key.clone();
                let value = Expr {
                    kind: ExprKind::PackEntry {
                        pack: local,
                        key: key.clone(),
                    },
                    ty: input.ty.clone(),
                    at: self.at(&source),
                    category,
                };
                forwarded.push(ForwardedArgument {
                    key,
                    source: source.clone(),
                    value,
                });
            }
        }
        forwarded
    }
}

/// The operand of each spread, in the order it is written.
pub(super) fn spread_operands(list: &SyntaxNode) -> impl Iterator<Item = SyntaxNode> + '_ {
    list.children()
        .filter(|argument| argument.kind() == SyntaxKind::PackSpreadArg)
        .filter_map(|argument| argument.children().next())
}

/// Reread only calls that depend on the concrete pack inputs (FN-19).
pub(crate) fn resolve_forwarded_calls(
    program: &crate::Program,
    (module, declaration): (usize, usize),
    (signature, map): (
        &crate::Signature,
        &[(hgl_diagnostics::Location, &crate::Argument)],
    ),
    (functions, inputs): (&[(usize, Callee)], &[crate::hir::PackInput]),
    body: &mut crate::hir::Body,
    diagnostics: &mut hgl_diagnostics::Diagnostics,
    project: &dyn Fn(&Type) -> Type,
) {
    let mut pending = false;
    crate::hir::walk_statements(&body.statements, &mut |value| {
        pending |= matches!(
            value.kind,
            ExprKind::Call {
                callee: Callee::DeferredPackCall(_),
                ..
            }
        );
    });
    if !pending {
        return;
    }
    let owner = &program.modules[module];
    let declared = &owner.declarations[declaration];
    let (bindings, written, calls) = super::tables(owner);
    let selecting = std::cell::RefCell::new(Vec::new());
    let reading = crate::Signature {
        generics: match &owner.details[declaration] {
            crate::Detail::Callable(declared) => {
                super::candidates::in_instance(declared, map).generics
            }
            _ => Vec::new(),
        },
        ..signature.clone()
    };
    let mut typer = Typer::bare(
        program,
        owner,
        declared,
        &reading,
        super::Tables {
            bindings: &bindings,
            written: &written,
            calls: &calls,
            selecting: &selecting,
        },
        diagnostics,
    );
    typer.use_instance_locals(std::mem::take(&mut body.locals));
    typer.instance = map
        .iter()
        .map(|(at, argument)| (*at, (*argument).clone()))
        .collect();
    typer.pack_inputs = Some(inputs.to_vec());
    typer.pack_functions = functions.to_vec();
    let graph = owner.phases[declaration] == Some(crate::program::Phase::Graph);
    crate::hir::walk_statements_mut(&mut body.statements, &mut |value| {
        let ExprKind::Call {
            callee: Callee::DeferredPackCall(call),
            ..
        } = &value.kind
        else {
            return;
        };
        let Some(source) = typer.node_at(call.source) else {
            return;
        };
        typer.hook = call.hook;
        typer.graph = graph && call.hook.is_none();
        let expected = call.expected.as_ref().map(project);
        *value = typer.expression(&source, expected.as_ref());
    });
    body.locals = typer.locals;
    super::record(program, body, graph);
}
