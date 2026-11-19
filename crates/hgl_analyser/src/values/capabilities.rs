//! Capabilities, what a node injects beside `out` (TIME-8): which local
//! is one, and the uses of one that reject: a capture by an anonymous
//! function, and a value that retains it. Part of the typing pass
//! ([`super`]).

use hgl_diagnostics::{Category, Location};
use hgl_parser::{HasSpan, SyntaxKind, SyntaxNode};

use super::Typer;
use crate::hir::{Expr, ExprKind, LocalId};
use crate::program::{DeclarationKind, LocalKind};
use crate::read::is_type;
use crate::syntax::{child, name_token, tokens};
use crate::types::Type;
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

    /// TIME-8: a capability is not "a capture of an anonymous function
    /// (FN-14)". FN-14 has the resolver reject the use of an injected
    /// capability by its own name (`name.captured`); a `let` it is
    /// bound to, and a parameter of a capability's type, may be used
    /// there as any `let` or parameter is, and are reported here.
    pub(super) fn captured_capabilities(&mut self, function: &SyntaxNode) {
        let captured: Vec<(LocalId, Location)> = function
            .descendants()
            .filter(|name| name.kind() == SyntaxKind::NameExpr)
            .filter_map(|name| {
                // A name of the function's own is no local of this
                // body, and is found by none.
                let local = self.local_of(&name)?;
                (self.locals[local].kind != LocalKind::Inject && self.is_capability(local))
                    .then(|| (local, self.at(&name)))
            })
            .collect();
        for (local, at) in captured {
            self.diagnostics.emit(
                error(
                    Category::Injectable,
                    codes::CAPABILITY_AS_VALUE,
                    at,
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
    }

    /// The body of the anonymous function `function`, read where it is
    /// written in a node, for what it reads there.
    ///
    /// FN-14: "An anonymous function's scope is the scope around it
    /// plus its parameters ... its body uses only parameters, generic
    /// parameters, `let` locals, and iteration bindings." A temporal
    /// input is a parameter: in a hook it is the payload (FN-3), and
    /// TIME-1: "A payload read requires admission/readability". So each
    /// read is held to the admission where the function is written: its
    /// `when` header, the guards around it and those in the body itself
    /// (TIME-4), `start` and `stop` (TIME-5), a `state`'s or a
    /// `cache`'s initializer (TIME-5, TIME-1). An input's validity does
    /// not change within an evaluation, so a read proved where the
    /// function is written is proved wherever it is called in that
    /// evaluation.
    ///
    /// The function's parameters and result take their types from its
    /// accepting form (FN-7), which is not read, and so do its `return`
    /// and its `?` (CTRL-5, ERR-5). A parameter is of the type written
    /// for it, or of none known. What the body declares, assigns or
    /// proves is taken back, and of what the walk reports only the
    /// reads of the node's inputs stand: the rest would depend on those
    /// types, and is not judged here.
    pub(super) fn anonymous_reads(&mut self, function: &SyntaxNode) {
        /// The reports of a read of a node's input where nothing admits
        /// it (TIME-1, TIME-4, TIME-5).
        const READS: [&str; 4] = [
            codes::PAYLOAD_UNGUARDED,
            codes::INPUT_LIFECYCLE,
            codes::STATE_INPUT_READ,
            codes::local::CACHE_INPUT_READ,
        ];
        if !(self.hook.is_some() || self.initializing || self.caching) {
            return;
        }
        let mark = self.mark();
        let undriven = self.undriven.len();
        let parameters = child(function, SyntaxKind::ParamList)
            .into_iter()
            .flat_map(|list| list.children())
            .filter(|parameter| parameter.kind() == SyntaxKind::LambdaParam);
        for parameter in parameters {
            let Some(name) = tokens(&parameter).find(|token| token.kind() == SyntaxKind::Ident)
            else {
                continue;
            };
            let ty = parameter
                .children()
                .find(|part| is_type(part.kind()))
                .and_then(|part| self.written.get(&(self.file, part.span().start)))
                .map_or(Type::Unknown, |ty| (*ty).clone());
            let at = Location::new(self.file, name.span());
            self.declare(name.text(), at, LocalKind::Parameter, ty, true);
        }
        let body = function.children().find_map(|part| match part.kind() {
            SyntaxKind::ArrowBody => part.children().next(),
            SyntaxKind::Block => Some(part),
            _ => None,
        });
        if let Some(body) = body {
            self.flow(&body, Some(&Type::Unknown), false);
        }
        let reads: Vec<_> = self.diagnostics.as_slice()[mark.diagnostics..]
            .iter()
            .filter(|diagnostic| READS.contains(&diagnostic.code.as_str()))
            .cloned()
            .collect();
        self.take_back(mark, false);
        // A `modified([])` of the body is no selector of the node's.
        self.undriven.truncate(undriven);
        for diagnostic in reads {
            self.diagnostics.emit(diagnostic);
        }
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
            SyntaxKind::FnDecl if used.kind() == SyntaxKind::Block => {
                self.signature.result.as_ref().map(|_| "what is returned")
            }
            SyntaxKind::ArrowBody => self.signature.result.as_ref().map(|_| "what is returned"),
            SyntaxKind::StateDecl => Some("the value of a `state`"),
            SyntaxKind::CacheDecl => Some("the value of a `cache`"),
            SyntaxKind::TupleExpr => Some("an entry of a tuple"),
            SyntaxKind::SequenceExpr => Some("an entry of a list or a set"),
            // `[key: value]`: either side of a dictionary's entry.
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
