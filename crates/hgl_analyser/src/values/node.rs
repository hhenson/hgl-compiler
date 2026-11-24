//! The body of a node and of a source: what each declares, its hooks,
//! what a `when` header subscribes to and admits, a hook's reads of its
//! temporal inputs, and what the node publishes (FN-2, SOURCE-1, TIME-1,
//! TIME-3, TIME-5, TIME-7). Part of the typing pass ([`super`]).

use hgl_diagnostics::{Category, Location};
use hgl_parser::{HasSpan, SyntaxElement, SyntaxKind, SyntaxNode, SyntaxToken};

use super::calls::call_callee;
use super::control::{ValueUse, value_of_branch};
use super::endpoints::endpoint_argument;
use super::fit::{accepts_float, is_integer_literal, open, whole};
use super::symbols::symbol_span;
use super::{Fact, Typer, children};
use crate::hir::{
    self, Body, ConfigEntry, EndpointOperation, Expr, ExprKind, Hook, LocalId, Stmt, ValueCategory,
};
use crate::program::{Binding, LocalKind, RUN_CONFIG};
use crate::syntax::{child, leads_its_test, name_token, tokens, ungrouped};
use crate::types::{Detail, Scalar, Type};
use crate::{codes, error, warning};

impl<'a> Typer<'a> {
    /// The body of a node: its declarations and then its hooks (FN-2).
    /// A statement that is neither has been reported by `phases`, and
    /// is not read.
    pub(super) fn node(mut self) -> Option<Body> {
        let block = child(&self.declaration.syntax, SyntaxKind::Block)?;
        let mut statements = Vec::new();
        for statement in block.children() {
            // Each declaration and hook has its own allowance of rereads
            // (`Typer::begin_construct`).
            self.begin_construct(&statement);
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
        self.plain_cycle_reads(&statements);
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
            self.begin_construct(&statement);
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
                    self.append_statement(&statement, &mut statements);
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
    pub(super) fn yield_statement(&mut self, node: &SyntaxNode) -> Stmt {
        let at = self.at(node);
        let mut parts = node.children();
        let (Some(time), Some(value)) = (parts.next(), parts.next()) else {
            return self.unread(node);
        };
        let time = self.yield_time(&time);
        let result = self.signature.result.clone();
        let value = match &result {
            // TIME-7: "A `ref` result takes no value", and a source has
            // no temporal parameters (SOURCE-1), so a `yield` to one
            // names no endpoint: "A value that names no endpoint rejects
            // there (`type.mismatch`)" (FN-16).
            Some(result) if self.signature.reference => self.designation(&value, result),
            Some(result) => self.published(&value, result, "the source's result"),
            // A source with no result has nothing for a value to be:
            // it is read for what it reports itself.
            None => self.expression(&value, Some(&Type::Unknown)),
        };
        Stmt::Yield { time, value, at }
    }

    /// SOURCE-2: "A duration is relative to current evaluation time; a
    /// datetime is absolute." Either type is taken, reading a result
    /// or nullable's payload as that type (ERR-12, ERR-15). A generic
    /// time keeps its type for the instance where its requirement
    /// admits only those types (GEN-7).
    fn yield_time(&mut self, node: &SyntaxNode) -> Expr {
        let time = self.expression(node, None);
        if let Some(taken) = [Scalar::Duration, Scalar::Datetime]
            .into_iter()
            .find_map(|scalar| self.taken(&Type::Scalar(scalar), &time.ty))
        {
            return self.take(time, taken);
        }
        if let Type::Parameter { at, .. } = &time.ty
            && let Some(types) = self.yield_time_types(*at)
            && !types.is_empty()
            && types
                .iter()
                .all(|ty| matches!(ty, Type::Scalar(Scalar::Duration | Scalar::Datetime)))
        {
            return time;
        }
        if whole(&time.ty)
            && !open(&time.ty)
            && !matches!(time.ty, Type::Scalar(Scalar::Duration | Scalar::Datetime))
        {
            self.diagnostics.emit(error(
                Category::Type,
                codes::YIELD_TIME_TYPE,
                time.at,
                format!(
                    "this is `{}`, and a yield time is a `duration` or a `datetime`",
                    time.ty
                ),
                "SOURCE-2",
            ));
        }
        time
    }

    /// `inject name, ...`: each name is a capability of the node
    /// (TIME-8). `out` is its result (TIME-7): "`inject out` exposes
    /// that same result for reads, assignment, and admitted piecewise
    /// mutation." What the others are used for is for a later part.
    ///
    /// RUN-6: an application and an integration test inject
    /// `run_config`, "giving the fields that leaves out their defaults,
    /// `inject run_config{length: 3s}`", whose entries are read here
    /// ([`Typer::run_configuration`]) before the name is bound. Where
    /// `inject run_config` has no place is the phase pass's to report
    /// (`inject.position`). Braces "on an `inject` of any other
    /// capability" reject (`inject.configuration`), and what they hold
    /// is not read (ERR-9).
    pub(super) fn inject(&mut self, node: &SyntaxNode) -> Vec<Stmt> {
        // Each name, and the configuration written after it, which the
        // parser keeps beside the name in the `InjectDecl`.
        let mut injected: Vec<(SyntaxToken, Option<SyntaxNode>)> = Vec::new();
        for element in node.children_with_tokens() {
            match element {
                SyntaxElement::Token(token) if token.kind() == SyntaxKind::Ident => {
                    injected.push((token, None));
                }
                SyntaxElement::Node(braces) if braces.kind() == SyntaxKind::OperatorConfig => {
                    if let Some((_, config)) = injected.last_mut() {
                        *config = Some(braces);
                    }
                }
                _ => {}
            }
        }
        injected
            .into_iter()
            .map(|(name, braces)| {
                let at = Location::new(self.file, name.span());
                let config = match braces {
                    // Where `run_config` has a place: what the phase pass
                    // reports elsewhere is not judged again (ERR-9).
                    Some(braces) if name.text() == RUN_CONFIG => {
                        if self.applying || (self.integration && leads_its_test(node)) {
                            self.run_configuration(&braces)
                        } else {
                            Vec::new()
                        }
                    }
                    Some(braces) => {
                        // An application's `inject` of a capability it
                        // may not inject has been reported itself
                        // (`application.statement`).
                        // And so has any capability but `run_config` in a test
                        // (`test.statement_phase`).
                        if !self.testing && (!self.applying || name.text() == "global_state") {
                            self.inject_configuration(&braces, name.text());
                        }
                        Vec::new()
                    }
                    None => Vec::new(),
                };
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
                Stmt::Inject { local, config, at }
            })
            .collect()
    }

    /// Reports the braces `braces` written after `name`, a capability
    /// that is not `run_config` (RUN-6: "and so do braces on an `inject`
    /// of any other capability (`inject.configuration`)"), at the brace.
    fn inject_configuration(&mut self, braces: &SyntaxNode, name: &str) {
        let at = match braces.first_token() {
            Some(brace) => Location::new(self.file, brace.span()),
            None => self.at(braces),
        };
        self.diagnostics.emit(
            error(
                Category::Injectable,
                codes::INJECT_CONFIGURATION,
                at,
                format!("`{name}` takes no configuration: only `run_config` is injected with one"),
                "RUN-6",
            )
            .with_note("`inject run_config{length: 3s}` gives the run's configuration defaults"),
        );
    }

    /// The entries of a run's configuration, `braces` being the braces
    /// they are written in: after `eval` or `run` in a test, or after
    /// `run_config` in an `inject` (RUN-6).
    ///
    /// RUN-6: "Each entry names a field of the configuration, and its
    /// value is value code of that field's type; a field no entry names
    /// is left out. An entry that names no field is an argument that
    /// names no parameter (`argument.unknown`)." The fields are the
    /// properties of core's `RunConfig` (core's CAP-7). EXPR-10, whose
    /// form the braces have: "Reject empty configuration, duplicate
    /// options": the parser reports empty braces, and a field named
    /// twice is a parameter given twice (`argument.duplicate`). The
    /// entries are read in the order written (EXPR-13).
    pub(super) fn run_configuration(&mut self, braces: &SyntaxNode) -> Vec<ConfigEntry> {
        let fields: Vec<(String, Type)> = self
            .program
            .run_config_type()
            .and_then(|ty| match ty {
                Type::Named {
                    module,
                    declaration,
                    ..
                } => match self.program.modules[module].details.get(declaration) {
                    Some(Detail::Native(native)) => Some(
                        native
                            .properties
                            .iter()
                            .map(|property| (property.name.clone(), property.ty.clone()))
                            .collect(),
                    ),
                    _ => None,
                },
                _ => None,
            })
            .unwrap_or_default();
        let mut entries: Vec<ConfigEntry> = Vec::new();
        let mut named: Vec<String> = Vec::new();
        for entry in braces
            .children()
            .filter(|entry| entry.kind() == SyntaxKind::ConfigArg)
        {
            // `configuration_argument = member_name, ":", expression ;`
            let Some(name) = tokens(&entry).next() else {
                continue;
            };
            let Some(value) = entry.children().next() else {
                continue;
            };
            let at = Location::new(self.file, name.span());
            let field = name.text().to_string();
            let Some((_, ty)) = fields.iter().find(|(known, _)| *known == field) else {
                // Core not supplied, or its `RunConfig` not read: what
                // the field is is not known, and nothing is judged.
                if !fields.is_empty() {
                    let known: Vec<&str> = fields.iter().map(|(name, _)| name.as_str()).collect();
                    self.diagnostics.emit(
                        error(
                            Category::Name,
                            codes::ARGUMENT_UNKNOWN,
                            at,
                            format!("a run's configuration has no field `{field}`"),
                            "RUN-6",
                        )
                        .with_note(format!(
                            "its fields are `{}`, core's `RunConfig`'s",
                            known.join("`, `")
                        )),
                    );
                }
                // The value is read all the same, with nothing expected
                // of it.
                self.expression(&value, Some(&Type::Unknown));
                continue;
            };
            if named.contains(&field) {
                self.diagnostics.emit(error(
                    Category::Name,
                    codes::ARGUMENT_DUPLICATE,
                    at,
                    format!("the field `{field}` of the run's configuration is given twice"),
                    "EXPR-10",
                ));
                self.expression(&value, Some(&Type::Unknown));
                continue;
            }
            named.push(field.clone());
            let read = self.expression(&value, Some(ty));
            let value = self.fit(
                read,
                ty,
                &format!("the field `{field}` of the run's configuration"),
            );
            entries.push(ConfigEntry { field, value });
        }
        entries
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
        // conjuncts are written in, and for the body. What the header
        // admits is what the hook's record of it says (`header_of`),
        // which the target follows too: one decision. The condition's
        // own reads are judged by it (TIME-4: "A payload read under
        // manual admission must be dominated by a successful guard"),
        // so the record is learnt first (`header_requires`), and the
        // condition is then read where it stands.
        if hook == Hook::When {
            let record = match &header {
                Some(header) => self.header_requires(header),
                None => self.header_of(None),
            };
            self.admit(&record);
        }
        let reported = self.diagnostics.len();
        let condition = header.map(|condition| self.condition(&condition));
        // A header that is itself rejected says nothing sure of what it
        // admits: the reads in its body are then not judged (ERR-9).
        let rejected = self.diagnostics.len() != reported;
        let record = (hook == Hook::When).then(|| self.header_of(condition.as_ref()));
        // The body is judged by the record of the header as it stands.
        if let Some(record) = &record {
            self.present.truncate(frame);
            self.admit(record);
        }
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
                    statements.push(Stmt::Expr(self.flow(&last, None, ValueUse::Discarded).0));
                }
                statements
            }
            None => Vec::new(),
        };
        self.present.truncate(frame);
        self.hook = before;
        // TIME-3: "The empty selection `modified([])` makes every
        // temporal input passive for its handler: it subscribes to none,
        // and selects a wake from the node's scheduler or alarm", where
        // it is a selector: "A selector sets subscription or admission
        // when it stands alone in the header or appears as a top-level
        // conjunct." Each such is judged once the whole node is read
        // (`node.no_activation`). One read "as a bool", in a body or
        // "under a negation or a disjunction", sets no subscription: a
        // handler with none of its header's subscribes to every input,
        // which wakes it. A header that is itself rejected is not judged
        // further (ERR-9): `modified(step)` of a `const` parameter names
        // no input either.
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
            header: record,
            body,
            at,
        }
    }

    /// What the `when` header `header` requires valid, by its record
    /// (`header_of`), learnt before the header is read where it stands.
    ///
    /// Which conjuncts are selectors is known only once the condition is
    /// typed: a name a declaration other than core's binds is no
    /// selector, `c::valid(a)` through an alias of core is one, and
    /// under a configured `&&{...}` nothing is a top-level conjunct. So
    /// the condition is read once with nothing of it admitted, its
    /// record taken, and that reading taken back: what it reported,
    /// declared and assigned, and what it spent of the hook's
    /// rereads ([`super::REREADS`]), so the reading that stands is the
    /// one there would be without it. A `when` stands only at the top of
    /// a node's body, and is read once there, so this is one more
    /// reading of each header and no more: it needs no bound, and the
    /// judgment of a header's reads never falls back to the default
    /// admission, however many handlers come before it.
    fn header_requires(&mut self, header: &SyntaxNode) -> hir::Header {
        let rereads = self.rereads;
        let mark = self.mark();
        let probe = self.condition(header);
        let record = self.header_of(Some(&probe));
        self.take_back(mark, false);
        self.rereads = rereads;
        record
    }

    /// Holds the temporal inputs among `required`, what a `when`
    /// header's record requires valid, to be valid where the header and
    /// its body are read (TIME-3, TIME-4). `out` among them is not
    /// admitted: the record's own words.
    fn admit(&mut self, header: &hir::Header) {
        let admitted: Vec<LocalId> = header
            .required
            .iter()
            .copied()
            .filter(|local| self.is_temporal_input(*local))
            .collect();
        self.present.extend(admitted.into_iter().map(Fact::Valid));
        self.present.extend(
            header
                .required_entries
                .iter()
                .cloned()
                .flat_map(|entry| children::pack_guard_facts(EndpointOperation::Valid, entry)),
        );
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
    /// typed as (TIME-9): a name a declaration other than core's binds is
    /// a call of that declaration, and no selector, and a configured
    /// `&&{...}` is the call of the candidate its configuration selects,
    /// under which nothing is a top-level conjunct. The handler's reads
    /// are judged by the record too (`Typer::hook`).
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
        let mut subscribed_entries = Vec::new();
        let mut required_entries = Vec::new();
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
                ExprKind::PackedEndpoint { operation, inputs }
                    if matches!(
                        operation,
                        EndpointOperation::Valid | EndpointOperation::Modified
                    ) =>
                {
                    let (direct, packed) = if *operation == EndpointOperation::Valid {
                        (&mut required, &mut required_entries)
                    } else {
                        names = true;
                        conditions.push(position);
                        (&mut subscribed, &mut subscribed_entries)
                    };
                    let direct = direct.get_or_insert_with(Vec::new);
                    for input in inputs {
                        if let ExprKind::EndpointOf(local) = input.kind {
                            if !direct.contains(&local) {
                                direct.push(local);
                            }
                        } else if let Some(entry) = self.pack_endpoint_selection(input)
                            && !packed.contains(&entry)
                        {
                            packed.push(entry);
                        }
                    }
                }
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
            subscribed_entries,
            required: required.unwrap_or_else(|| if alone { Vec::new() } else { inputs.clone() }),
            required_entries,
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
        self.input_read(
            local,
            at,
            children::read_whole_all_valid(&self.locals[local].ty),
        );
    }

    /// END-3: a delta read needs the input's validity, but reads no
    /// whole collection payload and needs no proof of a tick. The
    /// selected place handles a cycle with no delta (ERR-14, ERR-15).
    pub(super) fn delta_read(&mut self, local: LocalId, at: Location) {
        self.input_read(local, at, false);
    }

    /// Shared input lifecycle and admission checks. Only a whole
    /// payload read needs all_valid of a struct, tuple or fixed list.
    pub(super) fn input_read(&mut self, local: LocalId, at: Location, needs_all_valid: bool) {
        self.input_payload_read(local, at, needs_all_valid, None);
    }

    /// FN-19: the selected entry has the same lifecycle and admission
    /// checks as a direct input, with the entry's own guards.
    pub(super) fn pack_input_read(
        &mut self,
        pack: LocalId,
        key: Option<hir::PackKey>,
        written: &SyntaxNode,
        needs_all_valid: bool,
    ) {
        self.input_payload_read(
            pack,
            self.at(written),
            needs_all_valid,
            Some((key, written.text().to_string())),
        );
    }

    fn input_payload_read(
        &mut self,
        local: LocalId,
        at: Location,
        needs_all_valid: bool,
        packed: Option<(Option<hir::PackKey>, String)>,
    ) {
        let is_packed = packed.is_some();
        let (key, name) = packed.unwrap_or_else(|| (None, self.locals[local].name.clone()));
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
        // A cache is "initialized on every start" (TIME-6), which is
        // "before any [input] has a value" (TIME-5).
        if self.caching {
            self.diagnostics.emit(
                error(
                    Category::Phase,
                    codes::CACHE_INPUT_READ,
                    at,
                    format!(
                        "`{name}` is a temporal input, and a `cache` is given its first value \
                         on every start, before any input has one"
                    ),
                    "TIME-5",
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
        // TIME-1: "A read of a struct, a tuple or a fixed list whole,
        // `return quote`, requires `all_valid` of it ... Where such a
        // read is seen when the program is compiled and `all_valid` does
        // not admit it, it rejects (`payload.unguarded`)." TIME-4: "The
        // guard of a whole read of a struct, a tuple or a fixed list ...
        // is a test of `all_valid` of what it reads". TIME-9: `valid` of
        // a collection "admits no child's read", and a whole read is one
        // of every child.
        if is_packed
            && self.present.iter().any(|fact| {
                matches!(fact, Fact::DeferredPackGuard { pack, all_valid }
                if *pack == local && (!needs_all_valid || *all_valid))
            })
        {
            // The instance resolves its selector before it checks this read.
            return;
        }
        let pack_path = key.map(|key| children::ChildPath::pack_entry(local, key));
        let valid = pack_path
            .as_ref()
            .map_or(Fact::Valid(local), |path| Fact::ChildValid(path.clone()));
        let is_valid = self.present.contains(&valid) || self.present.contains(&Fact::Valid(local));
        if needs_all_valid {
            let direct = children::all_valid_fact(local);
            let all_valid = pack_path.map_or_else(|| direct.clone(), Fact::AllValid);
            if self.present.contains(&all_valid) || self.present.contains(&direct) {
                return;
            }
            let given = if is_valid {
                format!("`valid({name})` is true once any child is, and admits no whole read")
            } else {
                unproved.to_string()
            };
            self.diagnostics.emit(
                error(
                    Category::Phase,
                    codes::PAYLOAD_UNGUARDED,
                    at,
                    format!(
                        "`{name}` is read whole here, and may not be all valid: {given}, and \
                         no `all_valid({name})` guards the read"
                    ),
                    "TIME-1",
                )
                .with_note(format!(
                    "a struct, a tuple or a fixed list is read whole where its `all_valid` \
                     holds: `when all_valid({name}) {{ ... }}`"
                )),
            );
            // One report for one input on a path.
            self.present.push(valid);
            self.present.push(all_valid);
            return;
        }
        if is_valid {
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
        self.present.push(valid);
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
    /// stops expansion at a complete ordinary V"). TIME-7: "A result of
    /// `rolling_window<V, Max, Min>` takes a V, which arrives, and nothing
    /// else (TYPE-11)", and a nullable V's delta.
    ///
    /// A value written out takes its type from its place (EXPR-5), and
    /// the place here is one or the other: `(1.0, null)` is no
    /// `tuple<f64, f64>` and is its delta. So what does not read as a
    /// complete value is read again as the delta, and stands as that
    /// where it reads clean; otherwise the complete value's reading
    /// stands, with what it reports.
    pub(super) fn published(&mut self, node: &SyntaxNode, result: &Type, what: &str) -> Expr {
        if let Type::RollingWindow {
            value: arriving, ..
        } = result
        {
            let value = self.expression(node, Some(arriving));
            // TIME-7: "One of a nullable V takes its delta as well,
            // `delta<f64?>(value: 1.0)` or `delta<f64?>()`, as the arrival
            // it holds".
            let delta = Type::delta(result.clone());
            if matches!(**arriving, Type::Nullable(_))
                && !self.admits(arriving, &value.ty)
                && self.admits(&delta, &value.ty)
            {
                return value;
            }
            return self.fit(value, arriving, &format!("what arrives at {what}"));
        }
        let whole = match result {
            Type::Atomic(value) => (**value).clone(),
            other => other.clone(),
        };
        let delta = Type::delta(result.clone());
        let mark = self.mark();
        let value = self.expression(node, Some(&whole));
        let clean = self.diagnostics.len() == mark.diagnostics;
        if !clean
            && matches!(whole, Type::Nullable(_))
            && self.unresolved_publication_result(node, &value, mark.diagnostics)
        {
            return value;
        }
        // A delta, read as one whatever was expected of it:
        // `delta_value(input)`, `delta<set<i64>>(added: [1])`.
        let given_whole = self.admits(result, &value.ty);
        let given_delta = delta != whole && !given_whole && self.admits(&delta, &value.ty);
        if clean {
            if open(&value.ty) || given_whole {
                // ERR-12 and ERR-15: the publication still supplies a
                // nullable place when the provisional whole value fits.
                if matches!(result, Type::Nullable(_)) {
                    self.record_result_read(&value, Some((result, what)));
                }
                return value;
            }
            if given_delta {
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
            // A delta that is malformed has said so itself (TYPE-16:
            // `delta.overlap`), and is a delta all the same: no complete
            // value is asked of it.
            if self.diagnostics.len() == mark.diagnostics || given_delta {
                return as_delta;
            }
            self.take_back(mark, false);
            let value = self.expression(node, Some(&whole));
            return self.fit(value, &whole, what);
        }
        self.fit(value, &whole, what)
    }

    /// GEN-2 and TIME-7: keep a nullable publication's unresolved bare
    /// call result. A delta retry still supplies context to a call whose
    /// own result has a delta shape.
    fn unresolved_publication_result(
        &self,
        node: &SyntaxNode,
        value: &Expr,
        reported: usize,
    ) -> bool {
        let node = ungrouped(node.clone());
        match &value.kind {
            ExprKind::Block {
                value: Some(tail), ..
            } => {
                return value_of_branch(&node).is_some_and(|source| {
                    self.unresolved_publication_result(&source, tail, reported)
                });
            }
            ExprKind::If {
                then, otherwise, ..
            } => {
                return node
                    .children()
                    .skip(1)
                    .zip(std::iter::once(then.as_ref()).chain(otherwise.as_deref()))
                    .any(|(source, value)| {
                        self.unresolved_publication_result(&source, value, reported)
                    });
            }
            _ => {}
        }
        let bare = match &value.ty {
            Type::Result { value, error } if **error == Type::Error => value.as_ref(),
            other => other,
        };
        if *bare != Type::Unknown {
            return false;
        }
        let Some(span) = call_callee(&node)
            .map(|callee| callee.span())
            .or_else(|| symbol_span(&node))
        else {
            return false;
        };
        self.diagnostics.as_slice()[reported..]
            .iter()
            .any(|report| {
                report.code.as_str() == codes::GENERIC_UNRESOLVED
                    && report.primary.file == self.file
                    && span.start <= report.primary.span.start
                    && report.primary.span.end <= span.end
            })
    }

    /// True when `value` is one the place of type `whole` takes: of that
    /// type, an integer literal where an `f64` is expected (EXPR-5), or a
    /// result or a `T?` whose form the place takes (ERR-12, ERR-15).
    fn fits_whole(&self, value: &Expr, whole: &Type) -> bool {
        self.admits(whole, &value.ty)
            || (is_integer_literal(value) && accepts_float(whole))
            || self.taken(whole, &value.ty).is_some()
    }

    /// True when the function acknowledges the warning `code` (ERR-10):
    /// "At a named function, `@suppress_warning(\"node.no_activation\")`
    /// acknowledges the dormant-node warning for that declaration."
    fn acknowledges(&self, code: &str) -> bool {
        crate::syntax::suppresses_warning(&self.declaration.syntax, code)
    }

    /// What a `return` to a `ref` result of type `result`, or `out =` of
    /// one, is given at `node`. FN-16: "A `return` to a `ref` result is
    /// given an expression that names an endpoint: a temporal input of
    /// the node, `ref` or not, or a child of one, `items[idx]`. The
    /// result designates the logical endpoint that path names ... No
    /// payload is read, so nothing needs admitting (TIME-1), and a child
    /// is an endpoint whether or not it is valid. A value that names no
    /// endpoint rejects there (`type.mismatch`)." TIME-7: "A `ref`
    /// result takes no value". Core's END-10: "A `ref` result designates
    /// the endpoint its `return` names, a child of an input among them,
    /// `items[idx]`, valid or not".
    ///
    /// The endpoint is of the result's type, as what a `return` gives is
    /// (FN-7, EXPR-5). A value is read for what it reports of itself, a
    /// read of a `ref` parameter among them, and is reported as no
    /// endpoint where it reports nothing.
    pub(super) fn designation(&mut self, node: &SyntaxNode, result: &Type) -> Expr {
        let at = self.at(node);
        let named = endpoint_argument(node);
        let reported = self.diagnostics.len();
        let endpoint = match self.child_endpoint(&named) {
            Some(found) => found.map(|child| {
                let ty = child.ty.clone();
                Expr {
                    kind: ExprKind::ChildEndpointOf(Box::new(child)),
                    ty,
                    at,
                    category: ValueCategory::Unsettled,
                }
            }),
            None => (named.kind() == SyntaxKind::NameExpr)
                .then(|| self.local_of(&named))
                .flatten()
                .filter(|&local| self.is_temporal_input(local))
                .map(|local| Expr {
                    kind: ExprKind::EndpointOf(local),
                    ty: self.locals[local].ty.omission_removed().clone(),
                    at,
                    category: ValueCategory::Unsettled,
                }),
        };
        if let Some(endpoint) = endpoint {
            self.conforms(&endpoint.ty, result, at, "the function's result");
            return endpoint;
        }
        if self.diagnostics.len() == reported {
            let previous = self.designating;
            let source = ungrouped(node.clone());
            self.designating = call_callee(&source)
                .and_then(|callee| name_token(&callee))
                .map(|name| name.span())
                .or_else(|| symbol_span(&source))
                .map(|span| Location::new(self.file, span));
            let value = self.expression(node, Some(result));
            self.designating = previous;
            if self.diagnostics.len() == reported {
                self.diagnostics.emit(
                    error(
                        Category::Type,
                        codes::TYPE_MISMATCH,
                        value.at,
                        format!(
                            "a `ref` result takes an endpoint: a temporal input of `{}`, or a \
                             child of one",
                            self.declaration.name
                        ),
                        "FN-16",
                    )
                    .with_note(
                        "the result designates what the `return` names, `return items[idx]`, \
                         and publishes no value",
                    ),
                );
            }
        }
        Expr {
            kind: ExprKind::Unread,
            ty: Type::Unknown,
            at,
            category: ValueCategory::Unsettled,
        }
    }

    /// True when `local` is a temporal parameter of the node marked `ref`
    /// (FN-16): an endpoint, "not a value".
    pub(super) fn is_reference(&self, local: LocalId) -> bool {
        self.signature
            .parameters
            .get(local)
            .is_some_and(|parameter| !parameter.constant && parameter.reference)
    }

    /// True when `local` is an endpoint whose value is not read: a `ref`
    /// parameter, or `out` of a `ref` result. FN-16, proposed:
    /// "otherwise `out` of a `ref` result is as a `ref` parameter is".
    pub(super) fn designates(&self, local: LocalId) -> bool {
        self.is_reference(local) || (Some(local) == self.out && self.signature.reference)
    }

    /// Reports, at `at`, a read in a hook of `local`, an endpoint whose
    /// value is not read (`designates`): of its value, its
    /// `delta_value` or its `removed_value`, as `read` says; `child` is
    /// the child of it read, as written, where one is. FN-16: "A read of
    /// the value or of the `delta_value` of one rejects when the program
    /// is compiled ... That read rejects with the code `ref.read`", and
    /// "`removed_value` (TYPE-11) of a window that is a child of a `ref`
    /// parameter ... is set aside as `delta_value` is: a read of it there
    /// rejects with `ref.read`".
    pub(super) fn ref_read(
        &mut self,
        local: LocalId,
        child: Option<&str>,
        read: RefRead,
        at: Location,
    ) {
        let name = &self.locals[local].name;
        let what = if self.is_reference(local) {
            format!("the `ref` parameter `{name}`")
        } else {
            "`out` of a `ref` result".to_string()
        };
        let read = match read {
            RefRead::Value => "value",
            RefRead::Delta => "`delta_value`",
            RefRead::Removed => "`removed_value`",
        };
        let message = match child {
            Some(child) => {
                format!("`{child}` is a child of {what}, an endpoint whose {read} is not read")
            }
            None if self.is_reference(local) => {
                format!("`{name}` is a `ref` parameter, an endpoint whose {read} is not read")
            }
            None => format!("`out` is a `ref` result, an endpoint whose {read} is not read"),
        };
        self.diagnostics.emit(
            error(Category::Phase, codes::REF_READ, at, message, "FN-16").with_note(format!(
                "the endpoint operations take it, `valid({name})`, and a `return` to a `ref` \
                 result designates it"
            )),
        );
    }

    /// Reports a read of a `signal` input's payload in a hook. TYPE-12:
    /// "`signal` is a complete temporal input constraint with no
    /// readable payload": it is given to an endpoint operation, as
    /// `modified(tick)`, and read nowhere. A read in `start` is
    /// `input.lifecycle`, reported once.
    /// For a pack, `entry` supplies the selected type and expression.
    pub(super) fn signal_read(
        &mut self,
        local: LocalId,
        at: Location,
        entry: Option<(&Type, &SyntaxNode)>,
    ) {
        let ty = entry.map_or(&self.locals[local].ty, |(ty, _)| ty);
        let hook = matches!(self.hook, Some(Hook::When | Hook::Stop));
        // A `ref` one is reported as a `ref` parameter (`ref_read`).
        if !matches!(ty.omission_removed(), Type::Signal)
            || !self.is_temporal_input(local)
            || !hook
            || self.is_reference(local)
        {
            return;
        }
        let name = match entry {
            Some((_, written)) => written.text().to_string(),
            None => self.locals[local].name.clone(),
        };
        let diagnostic = error(
            Category::Type,
            codes::TYPE_NOT_ORDINARY,
            at,
            format!("`{name}` is a `signal`, which has no payload to read"),
            "TYPE-12",
        );
        // The ordinary note names an input that can be given directly
        // to an endpoint operation.
        self.diagnostics.emit(match entry {
            Some(_) => diagnostic,
            None => diagnostic.with_note(format!(
                "a signal is given to an endpoint operation: `modified({name})`"
            )),
        });
    }
}

/// What a read of an endpoint whose value is not read reads (FN-16):
/// its value, its `delta_value`, or a window's `removed_value`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum RefRead {
    /// Its value.
    Value,
    /// Its `delta_value`.
    Delta,
    /// A window's `removed_value`.
    Removed,
}
