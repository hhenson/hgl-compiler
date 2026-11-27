//! Capabilities, what a node injects beside `out` (TIME-8): which local
//! is one and where it would be retained, including as a capture by an
//! anonymous function. Part of the typing pass
//! ([`super`]).

use hgl_diagnostics::Category;
use hgl_parser::{SyntaxKind, SyntaxNode};

use super::Typer;
use crate::hir::{Expr, ExprKind, LocalId};
use crate::program::{DeclarationKind, LocalKind};
use crate::syntax::name_token;
use crate::{codes, error};

impl<'a> Typer<'a> {
    /// True when `local` is a capability (TIME-8): one the node
    /// injects, other than `out`, which is its result; a parameter of a
    /// capability's type; or a local one of those has been bound to.
    pub(super) fn is_capability(&self, local: LocalId) -> bool {
        self.capability(local).is_some()
    }

    /// The capability `local` is: itself, when the node injects it or it
    /// is a parameter of a capability's type; and for a local one of
    /// those has been bound to, that one.
    pub(super) fn capability(&self, local: LocalId) -> Option<LocalId> {
        if let Some(capability) = self.aliases.get(&local) {
            return Some(*capability);
        }
        let own = (self.locals[local].kind == LocalKind::Inject && Some(local) != self.out)
            || self.program.is_capability_type(&self.locals[local].ty);
        own.then_some(local)
    }

    /// The capability the value of `value` is: of a read of one, that
    /// one; of a block or an `if`, the one its value is.
    pub(super) fn capability_of(&self, value: &Expr) -> Option<LocalId> {
        match &value.kind {
            ExprKind::Local(local) => self.capability(*local),
            // The value of a block, or of a branch of an `if`.
            kind => kind
                .branch_values()?
                .into_iter()
                .find_map(|value| self.capability_of(value)),
        }
    }

    /// TIME-8 forbids capturing a capability through a `let` or a
    /// parameter as well as by its injected name. Look throughout the
    /// body, even where the typing pass cannot yet read an expression.
    pub(super) fn check_capability_captures(
        &mut self,
        function: &SyntaxNode,
    ) -> Vec<(SyntaxNode, LocalId)> {
        let mut keyed = Vec::new();
        for name in function
            .descendants()
            .filter(|name| name.kind() == SyntaxKind::NameExpr)
        {
            let Some(local) = self.local_of(&name) else {
                continue;
            };
            // A capture of the enclosing anonymous function has already
            // been checked. The resolver reports injected names (FN-14).
            if self.is_capture(local)
                || self.locals[local].kind == LocalKind::Inject
                || !self.is_capability(local)
            {
                continue;
            }
            if crate::syntax::is_keyed_field_name(&name) {
                keyed.push((name, local));
            } else {
                self.report_capability_capture(&name, local);
            }
        }
        keyed
    }

    /// A field label does not read the name which happens to be in scope.
    /// Prefer the committed lookup from the anonymous body. An open pack
    /// or guarded branch can leave its entries unread. In that case an
    /// call or typed block can still supply a fixed schema, independent of the
    /// pack. Keep only the key classification from that tentative lookup.
    pub(super) fn check_keyed_captures(&mut self, keyed: Vec<(SyntaxNode, LocalId)>, start: usize) {
        for (name, local) in keyed {
            let literal = name.parent().and_then(|pair| pair.parent()).unwrap();
            let at = self.at(&literal);
            let role = |typer: &Self| {
                let mut roles = typer.capture_key_reads.as_deref()?[start..]
                    .iter()
                    .filter_map(|(read, field)| (*read == at).then_some(*field));
                let first = roles.next()?;
                Some(first && roles.all(|field| field))
            };
            let field = role(self).or_else(|| {
                let contexts: Vec<_> = literal
                    .ancestors()
                    .take_while(|parent| {
                        !matches!(
                            parent.kind(),
                            SyntaxKind::FnExpr | SyntaxKind::PackComprehension
                        ) && !crate::syntax::is_pack_key_if(parent)
                    })
                    .filter(|parent| {
                        matches!(
                            parent.kind(),
                            SyntaxKind::Block
                                | SyntaxKind::CallExpr
                                | SyntaxKind::TypedConstructor
                                | SyntaxKind::UndecidedConstructor
                        )
                    })
                    .collect();
                contexts.iter().rev().find_map(|context| {
                    let mark = self.mark();
                    self.expression(context, None);
                    let field = role(self);
                    self.take_back(mark, false);
                    field
                })
            });
            if field != Some(true) {
                self.report_capability_capture(&name, local);
            }
        }
    }

    fn report_capability_capture(&mut self, name: &SyntaxNode, local: LocalId) {
        self.diagnostics.emit(
            error(
                Category::Injectable,
                codes::CAPABILITY_AS_VALUE,
                self.at(name),
                format!(
                    "`{}` is a capability, and is not retained: this is a capture of an \
                         anonymous function",
                    self.locals[local].name
                ),
                "TIME-8",
            )
            .with_note("give the function what it needs of the capability as an argument"),
        );
    }

    /// How the capability named at `node` is retained where it stands,
    /// in a few words for the message; `None` where it is used and not
    /// retained.
    ///
    /// TIME-8: "it is not the value of a `state` or a `cache`, an entry
    /// of a tuple, list, set, dictionary, or struct, what a function
    /// returns, what a node publishes, or a capture of an anonymous
    /// function (FN-14). Binding one to a `let` and giving one to a
    /// parameter of its type are uses." A capture is the resolver's to
    /// report (`name.captured`).
    pub(super) fn retained(&self, node: &SyntaxNode) -> Option<&'static str> {
        // Where the capability goes: out through what only passes its
        // value on, to the place that takes it.
        let mut used = node.clone();
        loop {
            let parent = used.parent()?;
            match parent.kind() {
                // "Parentheses select grouping" (EXPR-1).
                SyntaxKind::ParenExpr => used = parent,
                // CTRL-10: "A block whose last statement is an
                // expression has that expression's value". Any other
                // statement's value is dropped, and is no use at all.
                SyntaxKind::ExprStmt => {
                    let block = parent
                        .parent()
                        .filter(|block| block.kind() == SyntaxKind::Block)?;
                    if block.children().last().as_ref() != Some(&parent) {
                        return None;
                    }
                    used = block;
                }
                // A branch of an `if` gives the `if` its value; its
                // condition does not.
                SyntaxKind::IfExpr
                    if matches!(used.kind(), SyntaxKind::Block | SyntaxKind::IfExpr) =>
                {
                    used = parent;
                }
                _ => break,
            }
        }
        let parent = used.parent()?;
        match parent.kind() {
            // FN-7: "A body that ends in an expression returns that
            // expression, as an arrow body does ... In a function that
            // declares no result the expression is a statement", whose
            // value is dropped.
            SyntaxKind::FnDecl | SyntaxKind::FnExpr if used.kind() == SyntaxKind::Block => {
                self.result_type().map(|_| "what is returned")
            }
            SyntaxKind::ArrowBody => self.result_type().map(|_| "what is returned"),
            SyntaxKind::StateDecl => Some("the value of a `state`"),
            SyntaxKind::CacheDecl => Some("the value of a `cache`"),
            SyntaxKind::TupleExpr => Some("an entry of a tuple"),
            SyntaxKind::SequenceExpr => Some("an entry of a list or a set"),
            SyntaxKind::SetExpr => Some("an entry of a set"),
            // Either side of a colon entry, in brackets or braces.
            SyntaxKind::SequencePair => Some("an entry of a dictionary"),
            SyntaxKind::ReturnStmt => Some("what is returned"),
            // `yield time: value`: the value is what the source
            // publishes.
            SyntaxKind::YieldStmt => (parent.children().nth(1).as_ref() == Some(&used))
                .then_some("what the node publishes"),
            // `out = clock` publishes it; `kept = clock`, of a `state`
            // or a `cache`, keeps it.
            SyntaxKind::AssignStmt => {
                let place = parent.children().next()?;
                if place == used {
                    return None;
                }
                // `holder.clock = clock`, `kept[0] = clock`: the place
                // is a field or an element, which keeps it.
                if matches!(place.kind(), SyntaxKind::MemberExpr | SyntaxKind::IndexExpr) {
                    return Some("a field or an element of a value");
                }
                let target = (place.kind() == SyntaxKind::NameExpr)
                    .then(|| self.local_of(&place))
                    .flatten()?;
                if Some(target) == self.out {
                    return Some("what the node publishes");
                }
                match self.locals[target].kind {
                    LocalKind::State => Some("the value of a `state`"),
                    LocalKind::Cache => Some("the value of a `cache`"),
                    _ => None,
                }
            }
            // An argument: of a struct's construction, an entry of the
            // struct; of any other call, a use.
            SyntaxKind::Arg => {
                let call = parent.parent().and_then(|list| list.parent())?;
                // What is called: `Name(...)`, or with generic
                // arguments written `Name<T>(...)`, which is a struct's
                // construction only where the name is a struct's.
                let callee = match call.kind() {
                    SyntaxKind::CallExpr => call
                        .children()
                        .next()
                        .filter(|callee| callee.kind() == SyntaxKind::NameExpr)
                        .and_then(|callee| name_token(&callee)),
                    SyntaxKind::TypedConstructor | SyntaxKind::UndecidedConstructor => {
                        name_token(&call)
                    }
                    _ => None,
                };
                let constructs = callee
                    .and_then(|name| self.declarations(&name))
                    .is_some_and(|(module, declarations)| {
                        declarations.iter().any(|&declaration| {
                            self.program.modules[module].declarations[declaration].kind
                                == DeclarationKind::Struct
                        })
                    });
                constructs.then_some("an entry of a struct")
            }
            _ => None,
        }
    }
}
