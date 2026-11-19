//! The body of a node and of a source: what each declares, its hooks,
//! what a `when` header subscribes to and admits, a hook's reads of its
//! temporal inputs, and what the node publishes (FN-2, SOURCE-1, TIME-1,
//! TIME-3, TIME-5, TIME-7). Part of the typing pass ([`super`]).

use hgl_diagnostics::{Category, Location};
use hgl_lexer::literal::decode_string;
use hgl_parser::{HasSpan, SyntaxKind, SyntaxNode, SyntaxToken};

use super::endpoints::endpoint_arguments;
use super::fit::{accepts_float, is_integer_literal, open};
use super::{Fact, Typer, children};
use crate::hir::{self, Body, EndpointOperation, Expr, ExprKind, Hook, LocalId, Stmt};
use crate::program::{Binding, LocalKind};
use crate::syntax::{child, name_token, tokens, ungrouped};
use crate::types::Type;
use crate::{codes, error, local_error, warning};

impl<'a> Typer<'a> {
    /// The body of a node: its declarations and then its hooks (FN-2).
    /// A statement that is neither has been reported by `phases`, and
    /// is not read.
    pub(super) fn node(mut self) -> Option<Body> {
        let block = child(&self.declaration.syntax, SyntaxKind::Block)?;
        let mut statements = Vec::new();
        for statement in block.children() {
            match statement.kind() {
                SyntaxKind::StateDecl | SyntaxKind::CacheDecl => {
                    // TIME-5: "State initializers cannot read current
                    // temporal inputs." TIME-1: nor can a cache's.
                    self.initializing = statement.kind() == SyntaxKind::StateDecl;
                    self.caching = statement.kind() == SyntaxKind::CacheDecl;
                    statements.push(self.local(&statement));
                    self.initializing = false;
                    self.caching = false;
                }
                SyntaxKind::InjectDecl => statements.extend(self.inject(&statement)),
                SyntaxKind::LifecycleBlock | SyntaxKind::WhenStmt => {
                    statements.push(self.hook(&statement));
                }
                _ => statements.push(self.unread(&statement)),
            }
        }
        // TIME-3: "`modified([])` subscribes to no input: the node's
        // scheduler or alarm is then what wakes the handler. Where the
        // node injects neither, or its `start` does not use the one it
        // injects, nothing wakes it: the node is dormant, and the
        // selector is warned of (`node.no_activation`, ERR-10) ... A
        // node that is dormant by design acknowledges the warning."
        if !self.driven && !self.acknowledges(codes::NODE_NO_ACTIVATION) {
            for selector in std::mem::take(&mut self.undriven) {
                self.diagnostics.emit(
                    warning(
                        Category::Phase,
                        codes::NODE_NO_ACTIVATION,
                        selector,
                        format!(
                            "`modified([])` subscribes to no input, and nothing else wakes \
                             `{}`: the node is dormant",
                            self.declaration.name
                        ),
                        "TIME-3",
                    )
                    .with_note(
                        "a scheduler or an alarm the node injects and uses in `start` wakes \
                         it: `start { schedule(scheduler, delay) }`",
                    )
                    .with_note(
                        "a node that is dormant by design says so: \
                         `@suppress_warning(\"node.no_activation\")` before the function",
                    ),
                );
            }
        }
        Some(Body {
            locals: self.locals,
            statements,
        })
    }

    /// The body of a source (SOURCE-1): "a node that produces its
    /// output and has no temporal parameters ... It declares `cache`
    /// and `state` (TIME-6), which are what it keeps from one `yield` to
    /// the next ... A source with no declaration and no hook has a body
    /// of statements, `yield` among them. One with an `inject`, a
    /// `cache`, a `state`, a `start`, or a `stop` has its declarations,
    /// then `start`, then one block that holds every `yield`, then
    /// `stop`".
    ///
    /// Its declarations and its hooks are read as a node's are. Every
    /// other statement is one that yields, or stands with those that
    /// do: it is read as value code is (FN-3: "A runtime body calls only
    /// value functions"). Which statements may stand where is the phase
    /// pass's to say.
    pub(super) fn source(mut self) -> Option<Body> {
        let block = child(&self.declaration.syntax, SyntaxKind::Block)?;
        let mut statements = Vec::new();
        for statement in block.children() {
            match statement.kind() {
                SyntaxKind::StateDecl | SyntaxKind::CacheDecl => {
                    self.initializing = statement.kind() == SyntaxKind::StateDecl;
                    statements.push(self.local(&statement));
                    self.initializing = false;
                }
                SyntaxKind::InjectDecl => statements.extend(self.inject(&statement)),
                SyntaxKind::LifecycleBlock => statements.push(self.hook(&statement)),
                // "A source has no `when`": it is reported, and not
                // read.
                SyntaxKind::WhenStmt => statements.push(self.unread(&statement)),
                _ => {
                    // Each pass of the source begins here: nothing has
                    // returned.
                    self.returned = false;
                    self.generating = true;
                    statements.push(self.statement(&statement));
                    self.generating = false;
                }
            }
        }
        Some(Body {
            locals: self.locals,
            statements,
        })
    }

    /// `yield time: value` (SOURCE-1): it "writes its output and sets
    /// its next evaluation". The value is of the source's result type.
    /// What the time may be is proposed (SOURCE-2), and is not judged.
    pub(super) fn yield_statement(&mut self, node: &SyntaxNode) -> Stmt {
        let at = self.at(node);
        let mut parts = node.children();
        let (Some(time), Some(value)) = (parts.next(), parts.next()) else {
            return self.unread(node);
        };
        let time = self.expression(&time, None);
        let result = self.signature.result.clone();
        let value = match &result {
            Some(result) => self.published(&value, result, "the source's result"),
            // A source with no result has nothing for a value to be:
            // it is read for what it reports itself.
            None => self.expression(&value, Some(&Type::Unknown)),
        };
        Stmt::Yield { time, value, at }
    }

    /// `inject name, ...`: each name is a capability of the node
    /// (TIME-8). `out` is its result (TIME-7): "`inject out` exposes
    /// that same result for reads, assignment, and admitted piecewise
    /// mutation." What the others are used for is for a later part.
    pub(super) fn inject(&mut self, node: &SyntaxNode) -> Vec<Stmt> {
        let names: Vec<SyntaxToken> = tokens(node)
            .filter(|token| token.kind() == SyntaxKind::Ident)
            .collect();
        names
            .into_iter()
            .map(|name| {
                let at = Location::new(self.file, name.span());
                let is_out = name.text() == "out";
                // TIME-8: "`inject` binds each of the others to a local
                // of its capability's type, which core declares".
                let ty = match (&self.signature.result, is_out) {
                    (Some(result), true) => result.clone(),
                    (_, true) => Type::Unknown,
                    (_, false) => self
                        .program
                        .capability_type(name.text())
                        .unwrap_or(Type::Unknown),
                };
                // TIME-8: "A node injects `alarm` or `scheduler`, not
                // both."
                let other = match name.text() {
                    "alarm" => Some("scheduler"),
                    "scheduler" => Some("alarm"),
                    _ => None,
                };
                let earlier = other.and_then(|other| {
                    self.locals
                        .iter()
                        .find(|local| local.kind == LocalKind::Inject && local.name == other)
                        .map(|local| (other, local.at))
                });
                if let Some((other, first)) = earlier {
                    self.diagnostics.emit(
                        error(
                            Category::Injectable,
                            codes::INJECT_ALARM_AND_SCHEDULER,
                            at,
                            format!(
                                "a node injects `alarm` or `scheduler`, not both: `{other}` is \
                                 injected already"
                            ),
                            "TIME-8",
                        )
                        .with_related(first, "injected here"),
                    );
                }
                let local = self.declare(name.text(), at, LocalKind::Inject, ty, true);
                if is_out {
                    self.out = Some(local);
                }
                Stmt::Inject { local, at }
            })
            .collect()
    }

    /// `start { ... }`, `when condition { ... }`, `stop { ... }`: a hook
    /// of a node, read as value code (FN-3).
    fn hook(&mut self, node: &SyntaxNode) -> Stmt {
        let at = self.at(node);
        let hook = match tokens(node).next().map(|token| token.kind()) {
            Some(SyntaxKind::KwStart) => Hook::Start,
            Some(SyntaxKind::KwStop) => Hook::Stop,
            _ => Hook::When,
        };
        // Each hook is run from its beginning: nothing has returned.
        self.returned = false;
        let before = self.hook.replace(hook);
        let block = child(node, SyntaxKind::Block);
        // CTRL-9: what stands before the body's `{` is the condition of
        // a `when`, which may have none.
        let header = node
            .children()
            .find(|part| part.kind() != SyntaxKind::Block)
            .filter(|_| hook == Hook::When);
        let frame = self.present.len();
        // TIME-3: "`valid()` requires all temporal inputs to be valid.
        // Omitting either selector uses the same default as its
        // zero-argument form." So a `when` whose header sets no
        // admission of its own runs with every temporal input valid,
        // and one that sets it admits what its `valid` selectors name.
        // Either holds for the whole header, whatever order its
        // conjuncts are written in, and for the body.
        if hook == Hook::When {
            // TIME-3: "A header that is `scheduled()` alone is the
            // exception: it requires no input to be valid, as one that
            // writes `valid([])` does". `modified([])` alone has no such
            // exception, and the default stands.
            let alone = header
                .as_ref()
                .is_some_and(|header| self.scheduled_alone(header));
            let admitted = header
                .as_ref()
                .and_then(|header| self.admission(header))
                .unwrap_or_else(|| {
                    if alone {
                        Vec::new()
                    } else {
                        self.temporal_inputs()
                    }
                });
            self.present.extend(admitted.into_iter().map(Fact::Valid));
        }
        let reported = self.diagnostics.len();
        let condition = header.map(|condition| self.condition(&condition));
        // A header that is itself rejected says nothing sure of what it
        // admits: the reads in its body are then not judged (ERR-9).
        let rejected = self.diagnostics.len() != reported;
        if rejected {
            let inputs = self.temporal_inputs();
            self.present.extend(inputs.iter().copied().map(Fact::Valid));
            self.present
                .extend(inputs.into_iter().map(children::all_valid_fact));
        }
        // ERR-12, TIME-4: the body is reached only where the condition
        // holds.
        if let Some(condition) = &condition {
            let (when_true, _) = self.null_tests(condition);
            self.present.extend(when_true);
        }
        let body = match block {
            Some(block) => {
                let (mut statements, last) = self.block_head(&block);
                if let Some(last) = last {
                    statements.push(Stmt::Expr(self.flow(&last, None, false).0));
                }
                statements
            }
            None => Vec::new(),
        };
        self.present.truncate(frame);
        self.hook = before;
        let header = (hook == Hook::When).then(|| self.header_of(condition.as_ref()));
        // TIME-3: "The empty selection `modified([])` subscribes to no
        // input and selects a wake from the node's scheduler or alarm",
        // where it is a selector: "A selector sets subscription or
        // admission when it stands alone in the header or appears as a
        // top-level conjunct." Each such is judged once the whole node
        // is read (`node.no_activation`). One that "is a bool, in a body
        // or under a negation or a disjunction" leaves the default
        // subscription, which an input wakes. A header that is itself
        // rejected is not judged further (ERR-9): `modified(step)` of a
        // `const` parameter names no input either.
        let selectors = condition.iter().filter(|_| !rejected);
        let empty = selectors.flat_map(Expr::conjuncts).filter(|conjunct| {
            matches!(&conjunct.kind, ExprKind::Endpoint {
                operation: EndpointOperation::Modified,
                inputs: Some(inputs),
            } if inputs.is_empty())
        });
        for selector in empty {
            if !self.undriven.contains(&selector.at) {
                self.undriven.push(selector.at);
            }
        }
        Stmt::Hook {
            hook,
            condition,
            header,
            body,
            at,
        }
    }

    /// What a `when` header asks of its node, read from its typed
    /// condition for the stages after the analyser: see
    /// [`hir::Header`]. `condition` is `None` for a `when` with no
    /// header.
    ///
    /// TIME-3: "A selector sets subscription or admission when it
    /// stands alone in the header or appears as a top-level conjunct.
    /// Selectors under negation or disjunction leave the default in
    /// place." A selector is an endpoint operation the condition was
    /// typed as (TIME-9). The inputs it admits are read from the header
    /// before it is typed, by [`Typer::admission`], for the condition's
    /// own reads are judged by them.
    fn header_of(&self, condition: Option<&Expr>) -> hir::Header {
        let conjuncts = condition.map(Expr::conjuncts).unwrap_or_default();
        let inputs = self.temporal_inputs();
        // Every selector among the conjuncts adds what it names:
        // `valid(a) && valid(b)` requires both. One with no argument
        // names every input, and `valid([])` none. With no selector of
        // a kind, its default stands.
        let add = |set: &mut Option<Vec<LocalId>>, named: &Option<Vec<LocalId>>| {
            let set = set.get_or_insert_with(Vec::new);
            for &local in named.as_deref().unwrap_or(&inputs) {
                if !set.contains(&local) {
                    set.push(local);
                }
            }
        };
        let mut subscribed: Option<Vec<LocalId>> = None;
        let mut required: Option<Vec<LocalId>> = None;
        let mut conditions = Vec::new();
        let mut scheduled = false;
        // TIME-3: "The empty selection `modified([])` subscribes to no
        // input and selects a wake from the node's scheduler or alarm",
        // and `modified(a)` "requires a publication on" `a`. Where both
        // stand, which of the two runs the handler is not settled.
        let mut empty: Option<Location> = None;
        let mut names = false;
        // TIME-3: "A header that is `scheduled()` alone is the
        // exception: it requires no input to be valid".
        let alone = matches!(
            conjuncts.as_slice(),
            [Expr {
                kind: ExprKind::Endpoint {
                    operation: EndpointOperation::Scheduled,
                    ..
                },
                ..
            }]
        );
        for (position, conjunct) in conjuncts.iter().enumerate() {
            match &conjunct.kind {
                ExprKind::Endpoint {
                    operation: EndpointOperation::Modified,
                    inputs: named,
                } => {
                    add(&mut subscribed, named);
                    // TIME-3: scheduling and modification are distinct.
                    // An explicit guard still checks its publications;
                    // the empty selector listens for a scheduled wake
                    // instead.
                    if named.as_ref().is_some_and(Vec::is_empty) {
                        empty = empty.or(Some(conjunct.at));
                    } else {
                        names = true;
                        conditions.push(position);
                    }
                }
                ExprKind::Endpoint {
                    operation: EndpointOperation::Valid,
                    inputs: named,
                } => add(&mut required, named),
                // The node says what `scheduled()` is: it depends on
                // which of the scheduler and the alarm it injects.
                ExprKind::Endpoint {
                    operation: EndpointOperation::Scheduled,
                    ..
                } => scheduled = true,
                _ => conditions.push(position),
            }
        }
        hir::Header {
            selects: subscribed.is_some(),
            subscribed: subscribed.unwrap_or_else(|| inputs.clone()),
            required: required.unwrap_or_else(|| if alone { Vec::new() } else { inputs.clone() }),
            scheduled,
            beside: empty.filter(|_| names),
            conditions,
        }
    }

    /// True when the name at `name` may be one of the language's own
    /// forms: nothing binds it, or core's module declares it. TIME-9
    /// has core declare the endpoint operations, and in a `when` header
    /// `modified` and `valid` "are forms of the language": a name a
    /// module or a local binds is that module's or that local's.
    pub(super) fn is_the_languages(&self, name: &SyntaxToken) -> bool {
        match self.bindings.get(&(self.file, name.span().start)) {
            None => true,
            Some(Binding::Declarations { module, .. }) => Some(*module) == self.program.core(),
            Some(Binding::Local { .. }) => false,
        }
    }

    /// True when a `when` header is `scheduled()` alone, in parentheses
    /// or not: the language's selector (TIME-9), and no call of a
    /// declaration a module or a local binds under that name.
    fn scheduled_alone(&self, header: &SyntaxNode) -> bool {
        let header = &ungrouped(header.clone());
        match header.kind() {
            SyntaxKind::CallExpr => header
                .children()
                .next()
                .filter(|callee| callee.kind() == SyntaxKind::NameExpr)
                .and_then(|callee| name_token(&callee))
                .is_some_and(|name| name.text() == "scheduled" && self.is_the_languages(&name)),
            _ => false,
        }
    }

    /// The inputs a `when` header admits by its own `valid` selectors;
    /// `None` when it has none, and the default stands.
    ///
    /// TIME-3: "A selector sets subscription or admission when it
    /// stands alone in the header or appears as a top-level conjunct.
    /// Selectors under negation or disjunction leave the default in
    /// place." A selector is the language's `valid`: a name a
    /// declaration binds is a call of that declaration, and no selector
    /// (TIME-9).
    fn admission(&self, header: &SyntaxNode) -> Option<Vec<LocalId>> {
        let header = &ungrouped(header.clone());
        match header.kind() {
            SyntaxKind::BinaryExpr
                if tokens(header).any(|token| token.kind() == SyntaxKind::AmpAmp) =>
            {
                // Every selector among the conjuncts adds what it names.
                let sets: Vec<Vec<LocalId>> = header
                    .children()
                    .filter_map(|operand| self.admission(&operand))
                    .collect();
                (!sets.is_empty()).then(|| sets.concat())
            }
            // `valid(a)`, and the same written with its generic
            // argument, `valid<i64>(a)`, which is the selector it is
            // without one (TIME-9).
            SyntaxKind::CallExpr
            | SyntaxKind::TypedConstructor
            | SyntaxKind::UndecidedConstructor => {
                let name = match header.kind() {
                    SyntaxKind::CallExpr => {
                        let callee = header.children().next()?;
                        match callee.kind() {
                            SyntaxKind::NameExpr => name_token(&callee)?,
                            _ => return None,
                        }
                    }
                    _ if child(header, SyntaxKind::QualifiedName).is_none() => name_token(header)?,
                    _ => return None,
                };
                // GEN-14: a form that reads two ways is the call only
                // where the resolver decided so.
                if header.kind() == SyntaxKind::UndecidedConstructor {
                    let at = self.at(header);
                    let call =
                        self.module.decisions.iter().any(|decision| {
                            decision.at == at && decision.constructor == Some(true)
                        });
                    if !call {
                        return None;
                    }
                }
                if name.text() != "valid" || !self.is_the_languages(&name) {
                    return None;
                }
                let list = child(header, SyntaxKind::ArgList)?;
                let arguments = endpoint_arguments(&list);
                if arguments.is_empty() {
                    // `valid()`: every temporal input.
                    return Some(self.temporal_inputs());
                }
                // TIME-4: `valid(parts[1])`, of a child, is a guard: "a
                // `when` header that has the test alone or as a top-level
                // conjunct guards its body". It names no input, and the
                // default admission stands with it.
                let of_child = arguments.iter().any(|entry| {
                    matches!(entry.kind(), SyntaxKind::IndexExpr | SyntaxKind::MemberExpr)
                });
                if of_child {
                    return None;
                }
                // TIME-3: "`valid(a, b)` requires both named inputs,
                // and similarly for more inputs ... `valid([])` requires
                // no inputs to be valid." What names no input, the `[]`
                // among them, admits nothing here, and any other is
                // reported where the header is read.
                Some(
                    arguments
                        .iter()
                        .filter(|entry| entry.kind() == SyntaxKind::NameExpr)
                        .filter_map(|entry| self.local_of(entry))
                        .filter(|local| self.is_temporal_input(*local))
                        .collect(),
                )
            }
            _ => None,
        }
    }

    /// A read of a temporal input's payload in a node.
    ///
    /// TIME-1: "A payload read requires admission/readability; metadata
    /// may observe an invalid endpoint." The endpoint operations are
    /// metadata, and are not read here (TIME-9): each takes the
    /// endpoint and not its payload. A read is admitted where the
    /// input is known to be valid, and nowhere else.
    ///
    /// TIME-4: "A payload read under manual admission must be dominated
    /// by a successful guard": in a `when`, the input is one its header
    /// admits, or a `valid(input)` holds where the read stands. TIME-5:
    /// "State initializers cannot read current temporal inputs. `start`
    /// reads no temporal input and gives none to an endpoint operation:
    /// it runs before any has a value. `stop` reads a temporal input
    /// under manual admission, as a `when` under `valid([])` does: no
    /// header stands over it, so each payload read is dominated by a
    /// guard".
    pub(super) fn payload_read(&mut self, local: LocalId, at: Location) {
        let name = self.locals[local].name.clone();
        if self.initializing {
            self.diagnostics.emit(error(
                Category::Phase,
                codes::STATE_INPUT_READ,
                at,
                format!(
                    "`{name}` is a temporal input, and a `state` is given its first value \
                     before any input has one"
                ),
                "TIME-5",
            ));
            return;
        }
        // TIME-1: a cache's initializer admits no read either. TIME-6:
        // a cache is "initialized on every start", and TIME-5: that is
        // "before any has a value". The catalogue's `state.input_read`
        // names a state's initializer alone, so the code is local.
        if self.caching {
            self.diagnostics.emit(
                local_error(
                    Category::Phase,
                    codes::local::CACHE_INPUT_READ,
                    at,
                    format!(
                        "`{name}` is a temporal input, and a `cache` is given its first value \
                         on every start, before any input has one"
                    ),
                    "TIME-1",
                )
                .with_note("a `when` reads it, and may assign the cache"),
            );
            return;
        }
        let (unproved, rule) = match self.hook {
            Some(Hook::Start) => {
                self.input_lifecycle(at, &name);
                return;
            }
            Some(Hook::When) => ("this `when` does not require it to be", "TIME-4"),
            Some(Hook::Stop) => ("`stop` requires no input to be", "TIME-5"),
            None => return,
        };
        if self.present.contains(&Fact::Valid(local)) {
            return;
        }
        self.diagnostics.emit(
            error(
                Category::Phase,
                codes::PAYLOAD_UNGUARDED,
                at,
                format!(
                    "`{name}` may not be valid here: {unproved}, and no `valid({name})` \
                     guards the read"
                ),
                rule,
            )
            .with_note(format!(
                "a guard proves it where the read stands: `if valid({name}) {{ ... }}`"
            )),
        );
        // One report for one input on a path.
        self.present.push(Fact::Valid(local));
    }

    /// Reports a temporal input used in `start` (TIME-5): "`start`
    /// reads no temporal input and gives none to an endpoint operation:
    /// it runs before any has a value."
    pub(super) fn input_lifecycle(&mut self, at: Location, name: &str) {
        self.diagnostics.emit(
            error(
                Category::Phase,
                codes::INPUT_LIFECYCLE,
                at,
                format!("`{name}` is a temporal input, and `start` runs before any has a value"),
                "TIME-5",
            )
            .with_note("a `when` reads it, and `stop` under a guard: `if valid(input) { ... }`"),
        );
    }

    /// Reports a use of the node's output where TIME-7 has none: "`out`
    /// is read in a `when` and in `stop`, and written only in a `when`:
    /// `start` neither reads nor writes it".
    pub(super) fn out_lifecycle(&mut self, at: Location, message: String) {
        self.diagnostics.emit(
            error(Category::Phase, codes::OUT_LIFECYCLE, at, message, "TIME-7").with_note(
                "a node that publishes when the graph starts schedules itself in `start` and \
                 writes in its first scheduled evaluation",
            ),
        );
    }

    /// A value a node publishes to its result: by a `return` in a
    /// hook, an assignment to `out`, or a source's `yield`.
    ///
    /// TIME-7: "A runtime return publishes to its result." What is
    /// published is a complete value of the result, or what the result
    /// publishes in one cycle, its `delta<T>` (TYPE-16): core's cases
    /// relay a set with `return delta_value(input)`. A result of
    /// `atomic<V>` is published whole, as a V (TYPE-9: "`atomic<V>`
    /// stops expansion at a complete ordinary V").
    ///
    /// A value written out takes its type from its place (EXPR-5), and
    /// the place here is one or the other: `(1.0, null)` is no
    /// `tuple<f64, f64>` and is its delta. So what does not read as a
    /// complete value is read again as the delta, and stands as that
    /// where it reads clean; otherwise the complete value's reading
    /// stands, with what it reports.
    pub(super) fn published(&mut self, node: &SyntaxNode, result: &Type, what: &str) -> Expr {
        let whole = match result {
            Type::Atomic(value) => (**value).clone(),
            other => other.clone(),
        };
        let delta = Type::delta(result.clone());
        let mark = self.mark();
        let value = self.expression(node, Some(&whole));
        let clean = self.diagnostics.len() == mark.diagnostics;
        if clean {
            if open(&value.ty) {
                return value;
            }
            // The result's own type: a complete value, and what an
            // atomic input is read as. Or a delta, read as one whatever
            // was expected of it: `delta_value(input)`.
            if self.admits(result, &value.ty) || (delta != whole && self.admits(&delta, &value.ty))
            {
                return value;
            }
        }
        // Not a complete value as it reads: as the delta, where the
        // result has one that is another type.
        let fitted = clean && self.fits_whole(&value, &whole);
        if !fitted && delta != whole && self.may_reread() {
            self.take_back(mark, false);
            let mark = self.mark();
            let as_delta = self.expression(node, Some(&delta));
            let as_delta = self.fit(as_delta, &delta, what);
            if self.diagnostics.len() == mark.diagnostics {
                return as_delta;
            }
            self.take_back(mark, false);
            let value = self.expression(node, Some(&whole));
            return self.fit(value, &whole, what);
        }
        self.fit(value, &whole, what)
    }

    /// True when `value` is one the place of type `whole` takes: of that
    /// type, or an integer literal where an `f64` is expected (EXPR-5).
    fn fits_whole(&self, value: &Expr, whole: &Type) -> bool {
        self.admits(whole, &value.ty) || (is_integer_literal(value) && accepts_float(whole))
    }

    /// True when the function acknowledges the warning `code` (ERR-10):
    /// "At a named function, `@suppress_warning(\"node.no_activation\")`
    /// acknowledges the dormant-node warning for that declaration."
    fn acknowledges(&self, code: &str) -> bool {
        self.declaration
            .syntax
            .children()
            .filter(|annotation| annotation.kind() == SyntaxKind::WarningAnnotation)
            .filter_map(|annotation| {
                tokens(&annotation).find(|token| token.kind() == SyntaxKind::Str)
            })
            // The string's value, not its spelling: an escape, or a
            // text block, that says the code names the same warning.
            .any(|string| decode_string(string.text()).is_ok_and(|value| value == code))
    }

    /// Reports a read of a `signal` input's payload in a hook. TYPE-12:
    /// "`signal` is a complete temporal input constraint with no
    /// readable payload": it is given to an endpoint operation, as
    /// `modified(tick)`, and read nowhere. A read in `start` is
    /// `input.lifecycle`, reported once.
    pub(super) fn signal_read(&mut self, local: LocalId, at: Location) {
        let signal = matches!(self.locals[local].ty.omission_removed(), Type::Signal);
        let hook = matches!(self.hook, Some(Hook::When | Hook::Stop));
        if !signal || !self.is_temporal_input(local) || !hook {
            return;
        }
        self.diagnostics.emit(
            error(
                Category::Type,
                codes::TYPE_NOT_ORDINARY,
                at,
                format!(
                    "`{}` is a `signal`, which has no payload to read",
                    self.locals[local].name
                ),
                "TYPE-12",
            )
            .with_note(format!(
                "a signal is given to an endpoint operation: `modified({})`",
                self.locals[local].name
            )),
        );
    }
}
