//! A temporal function as hgraph has it: a node, `@compute_node` or
//! `@sink_node`, with its `when` handlers, `start` and `stop`; a source,
//! a generator a node resumes; and a graph. Part of the transpiler, the
//! last stage: `emit` writes each temporal declaration of a module with
//! these.

use super::*;

impl Emitter<'_> {
    /// A temporal function or a temporal candidate, the declaration
    /// `index` of the module, as the Python function `python`: a node
    /// where its body is a runtime one, and a graph where it is
    /// composition (FN-3). `signature` and `body` are the declaration's
    /// own, or an instance's.
    pub(super) fn node_or_graph(
        &mut self,
        index: usize,
        python: &str,
        signature: &Signature,
        body: &Body,
    ) {
        let program = self.program;
        let module = &program.modules[self.module];
        match module.phases.get(index).copied().flatten() {
            // SOURCE-1: "A temporal `fn` whose body has a `yield` is a
            // source".
            Some(Phase::Node) if yields(&body.statements) => {
                // SOURCE-1 has a source take no temporal input, and FN-16
                // a node mark its result `ref`: a source's is not written.
                if signature.reference {
                    self.unsupported(module.declarations[index].at, "a `ref` result of a source");
                }
                self.source(index, python, signature, body)
            }
            Some(Phase::Node) => self.node(index, python, signature, body),
            Some(Phase::Graph) => self.graph(index, python, signature, body),
            None => self.unsupported(module.declarations[index].at, "this function"),
        }
    }

    /// A node: `@compute_node`, or `@sink_node` where it has no result.
    ///
    /// TIME-3: "In a `when` header, `modified()` subscribes to all
    /// temporal inputs; `modified(a, b)` subscribes to both named inputs
    /// ... `valid()` requires all temporal inputs to be valid ...
    /// A selector sets subscription or admission
    /// when it stands alone in the header or appears as a top-level
    /// conjunct." TIME-4: "Aggregate subscriptions cover every handler.
    /// Node-wide validity includes only requirements common to
    /// executable handlers; other guards stay per-handler."
    ///
    /// So the node's `active=` is every input a handler subscribes to,
    /// its `valid=` the inputs every handler requires, and each handler
    /// is guarded by what it asks beyond those: that one of its own
    /// inputs ticked, that the inputs only it requires are valid, and
    /// the rest of its condition. "Conditions are checked in source
    /// order ... If that body completes without returning, checking
    /// continues with the next condition."
    ///
    /// TIME-6: "State is persistent, recordable node history." A
    /// `state` is a field of the node's `STATE`, given its value in the
    /// node's `start`. TIME-7: `inject out` is the node's own output,
    /// hgraph's `_output`.
    fn node(&mut self, index: usize, python: &str, signature: &Signature, body: &Body) {
        let program = self.program;
        let declaration = &program.modules[self.module].declarations[index];
        let function = Function::of(self.program, body, Some(signature), Mode::Node);
        let (mut parameters, result) = self.temporal_signature(declaration, signature);
        self.window_published_once(signature, body);
        // The `const` parameters, as written for the node: a `start`
        // and a `stop` are functions of their own to hgraph, and are
        // given the configuration they may read by naming it.
        let mut lifecycle: Vec<String> = signature
            .parameters
            .iter()
            .zip(&parameters)
            .filter(|(parameter, _)| parameter.constant)
            .map(|(_, written)| written.clone())
            .collect();
        // The temporal inputs, as positions among the locals.
        let inputs: Vec<usize> = (0..body.locals.len())
            .filter(|&local| function.is_input(local))
            .collect();
        // What the body is made of: the declarations, and each hook.
        let NodeParts {
            states,
            start,
            stop,
            rest,
        } = NodeParts::of(body);
        let mut handlers: Vec<Handler> = Vec::new();
        let mut has_out = false;
        // TIME-8: the capabilities the node injects, each as hgraph
        // gives it.
        let mut capabilities: Vec<(&str, &str)> = Vec::new();
        // TIME-9: the scheduler or the alarm, where it injects one.
        let mut timer: Option<Timer> = None;
        for statement in rest {
            match statement {
                Stmt::Hook {
                    hook: Hook::When,
                    condition,
                    header: Some(header),
                    body,
                    ..
                } => handlers.push(Handler::of(header, condition.as_ref(), body)),
                Stmt::Declare { local, .. } => {
                    self.unsupported(body.locals[*local].at, "this declaration of a node");
                }
                Stmt::Inject { local, at } => {
                    let injected = &body.locals[*local].name;
                    if injected == "out" {
                        has_out = true;
                    } else if let Some(written) = capability(injected) {
                        let injects = match injected.as_str() {
                            "scheduler" => Some(Timer::Scheduler),
                            "alarm" => Some(Timer::Alarm),
                            _ => None,
                        };
                        if injects.is_some() && timer.is_some() {
                            // Core's CAP-5: "A node injects it in place
                            // of `scheduler`".
                            self.unsupported(*at, "a node that injects a scheduler and an alarm");
                        } else {
                            timer = timer.or(injects);
                            capabilities.push(written);
                        }
                    } else {
                        self.unsupported(*at, &format!("an `inject` of `{injected}`"));
                    }
                }
                other => self.unsupported(at_of(other, &body.locals), "this statement of a node"),
            }
        }
        // TIME-7: "`inject out` exposes that same result": hgraph's own
        // `_output`, which it gives the node and its `stop`.
        // An unsized list's delta is counted from the size the list
        // holds, and an empty one made to tick, on `_output` (see
        // `bundled`), in the result or in a part of it.
        let has_out = has_out
            || signature
                .result
                .as_ref()
                .is_some_and(|result| holds_growing_list(self.program, result));
        // A whole dictionary published replaces what the node published
        // before, which it reads from `_output` (see `bundled`).
        let has_out = has_out
            || signature.result.as_ref().is_some_and(|result| {
                whole_dictionary(result) && returns_whole(&body.statements, result)
            });
        // TIME-10: "A designation published again unchanged is no tick":
        // a `ref` result is published where it designates what it did not
        // before the evaluation (`_core.Designation`), which it reads
        // from `_output`.
        let has_out = has_out || signature.reference;
        let output = has_out.then(|| {
            let annotation = match &result {
                Some(result) => output_annotation(result),
                None => "_py.object".to_string(),
            };
            format!("_output: {annotation} = None")
        });
        if let Some(output) = &output {
            self.hgraph = true;
            parameters.push(output.clone());
        }
        // TIME-4: what the handlers ask together.
        let active: Vec<usize> = inputs
            .iter()
            .copied()
            .filter(|input| {
                handlers
                    .iter()
                    .any(|handler| handler.subscribed.contains(input))
            })
            .collect();
        // TIME-4: only validity requirements shared by all handlers may
        // prevent the whole node from evaluating.
        let valid: Vec<usize> = inputs
            .iter()
            .copied()
            .filter(|input| {
                handlers
                    .iter()
                    .all(|handler| handler.required.contains(input))
            })
            .collect();
        let has_state = !states.is_empty();
        if has_state {
            lifecycle.push("_state: _hg.STATE = None".to_string());
        }
        // FN-16: a `ref` result keeps in the node's `STATE` which
        // endpoint it designates (`_core.Designation`).
        if has_state || signature.reference {
            parameters.push("_state: _hg.STATE = None".to_string());
        }
        // TIME-5: "`stop` reads a temporal input under manual
        // admission". hgraph gives a `stop` each input it names, as the
        // node has it: the configuration, then the inputs. Its injected
        // output and state are available at stop too.
        let mut stopping: Vec<String> = signature
            .parameters
            .iter()
            .zip(&parameters)
            .filter(|(parameter, _)| parameter.constant)
            .map(|(_, written)| written.clone())
            .collect();
        stopping.extend(
            signature
                .parameters
                .iter()
                .zip(&parameters)
                .filter(|(parameter, _)| !parameter.constant)
                .map(|(_, written)| written.clone()),
        );
        if has_state {
            stopping.push("_state: _hg.STATE = None".to_string());
        }
        // TIME-7: "`out` is read in a `when` and in `stop`".
        if let Some(output) = output {
            stopping.push(output);
        }
        // A capability is given to the node, to its `start` and to its
        // `stop`, each for that one call.
        for (parameter, annotation) in &capabilities {
            let written = format!("{parameter}: {annotation} = None");
            parameters.push(written.clone());
            lifecycle.push(written.clone());
            stopping.push(written);
        }
        let stopping = stopping.join(", ");
        let lifecycle = lifecycle.join(", ");
        for at in handlers.iter().filter_map(|handler| handler.beside) {
            self.unsupported(at, "`modified([])` beside another `modified` selector");
        }
        // hgraph takes no node without an input. One with none is given
        // one it is never woken by: the node is written under another
        // name, and a graph of the function's own name gives it a
        // constant. Its scheduler or its alarm wakes it, or nothing
        // does (TIME-3: "the node is dormant"), and its `start` and
        // `stop` run as any node's.
        let hidden = inputs.is_empty();
        // FN-16: each input that is no `ref` parameter, or that may be
        // passive, and that the node designates, given to the node a
        // second time as a hidden `REF` input, after its own
        // (`designated_inputs`). The node is then written under another
        // name too, and the graph of the function's own name gives it
        // each such input twice. `changes` is whether the body changes
        // an input's activity, `activate` or `passivate`.
        let mut changes = false;
        hgl_analyser::hir::walk_statements(&body.statements, &mut |part| {
            changes |= matches!(
                part.kind,
                ExprKind::Endpoint {
                    operation: EndpointOperation::Passivate | EndpointOperation::Activate,
                    ..
                }
            );
        });
        let designations = self.designated_inputs(&function, body, &active, changes);
        let hiding = designations.iter().any(|port| !port.hidden.is_empty());
        // FN-16: "its readers follow what that endpoint is bound to, a
        // later re-binding upstream included, and the node is not woken
        // for it". A node that designates is evaluated where a hidden
        // `REF` input ticks, whatever else is valid, so no validity is
        // the whole node's: each handler is guarded by its own.
        let valid: Vec<usize> = if designations.is_empty() {
            valid
        } else {
            Vec::new()
        };
        let exposing = hidden || hiding;
        let exposed = python;
        let node_name = if exposing {
            format!("_node_{python}")
        } else {
            python.to_string()
        };
        let python = node_name.as_str();
        // The function's own parameters, as the graph that stands for
        // a node with a hidden input takes them.
        let mut declared: Vec<String> = parameters[..signature.parameters.len()].to_vec();
        // A list whose positions the node designates is taken by the
        // graph as a `REF` of it: hgraph gives the position of a
        // connection that is a reference, a `ref` result's, as a `REF`
        // that ticks where the list is re-bound (TIME-10), and the
        // position of the list it dereferences to as one that does not.
        for port in designations.iter().filter(|port| port.positions) {
            let input = &body.locals[port.local].name;
            let Some(index) = signature
                .parameters
                .iter()
                .position(|parameter| &parameter.name == input)
            else {
                continue;
            };
            if let Some((head, annotation)) = declared[index].split_once(": ") {
                declared[index] = format!("{head}: {}", designating(annotation));
            }
        }
        let designated_parameters: Vec<String> = designations
            .iter()
            .flat_map(|port| &port.hidden)
            .map(|hidden| format!("{}: {}", hidden.name, hidden.annotation))
            .collect();
        parameters.splice(
            signature.parameters.len()..signature.parameters.len(),
            designated_parameters,
        );
        // TIME-9: what says that the scheduler or the alarm woke the
        // node. A scheduler's wake is due "whether or not a temporal
        // input has ticked". An alarm's is told from the inputs: it
        // "holds where the node is evaluated and no temporal input has
        // ticked". hgraph's scheduler is asked too, since it evaluates
        // a node at a wake that was since withdrawn.
        //
        // An input has ticked where it published or was invalidated:
        // hgraph's `modified` is of a publication alone, and its
        // `invalidated` of the cycle the input lost its value (END-4:
        // "The transition to invalid notifies active subscribers").
        let ticked: Vec<String> = inputs
            .iter()
            .map(|&local| {
                let input = name(&body.locals[local].name);
                format!("{input}.modified or {input}.invalidated")
            })
            .collect();
        let timer_woke = timer.map(|timer| match timer {
            Timer::Alarm if !ticked.is_empty() => format!(
                "_scheduler.is_scheduled_now and not ({})",
                ticked.join(" or ")
            ),
            _ => "_scheduler.is_scheduled_now".to_string(),
        });
        // That an input woke the node: one that is active published or
        // became invalid. Where the node passivates none, each input a
        // handler subscribes to is active.
        let mut passivates = false;
        hgl_analyser::hir::walk_statements(&body.statements, &mut |part| {
            passivates |= matches!(
                part.kind,
                ExprKind::Endpoint {
                    operation: EndpointOperation::Passivate,
                    ..
                }
            );
        });
        let input_woke = {
            let woke: Vec<String> = inputs
                .iter()
                .map(|&local| {
                    let input = name(&body.locals[local].name);
                    if passivates {
                        format!("(({input}.modified or {input}.invalidated) and {input}.active)")
                    } else {
                        format!("{input}.modified or {input}.invalidated")
                    }
                })
                .collect();
            match woke.as_slice() {
                // One term that is in parentheses of its own.
                [one] if passivates => one.clone(),
                _ => format!("({})", woke.join(" or ")),
            }
        };
        // What woke the node is read where the node is entered, and
        // not again where a later handler asks: a handler before it
        // may passivate the input that woke the node, or ask its
        // scheduler for another wake under the tag that is due, and
        // neither changes what woke this evaluation. A node with one
        // handler has none before it, and reads it in place.
        let timed = |handler: &Handler| handler.scheduled || handler.subscribed.is_empty();
        let several = handlers.len() > 1;
        // TIME-9: `scheduled()` read as a bool, under a negation or a
        // disjunction in a header or in a handler's body, is what woke
        // the node: "a wake asked of the scheduler is due in this
        // evaluation". It is read once, where the node is entered, as
        // the handlers' guards are, so a handler that asks again under
        // the tag that is due does not change it.
        let mut asks = false;
        for handler in &handlers {
            let mut visit = |part: &Expr| {
                asks |= matches!(
                    part.kind,
                    ExprKind::Endpoint {
                        operation: EndpointOperation::Scheduled,
                        ..
                    }
                );
            };
            for condition in &handler.conditions {
                condition.walk(&mut visit);
            }
            hgl_analyser::hir::walk_statements(handler.body, &mut visit);
        }
        let holds_timer = timer.is_some() && (asks || (several && handlers.iter().any(timed)));
        let holds_input = several
            && timer.is_some()
            && handlers
                .iter()
                .any(|handler| !timed(handler) && !handler.selects);
        let timer_guard = match &timer_woke {
            Some(_) if holds_timer => Some("_timer_woke".to_string()),
            other => other.clone(),
        };
        let input_guard = if holds_input {
            "_input_woke".to_string()
        } else {
            input_woke.clone()
        };
        let names = |locals: &[usize]| -> String {
            let named: Vec<String> = locals
                .iter()
                .map(|&local| name(&body.locals[local].name))
                .collect();
            tuple(&named)
        };
        let mut options = Vec::new();
        if hidden {
            // The hidden input wakes the node at no time, and need not
            // be valid.
            options.push("active=()".to_string());
            options.push("valid=()".to_string());
        } else {
            // A hidden `REF` input wakes the node where what it is bound
            // to changes, and need not be valid: its inputs are named
            // where it has one.
            let named = !designations.is_empty();
            let mut subscribed: Vec<String> = active
                .iter()
                .map(|&local| name(&body.locals[local].name))
                .collect();
            subscribed.extend(
                designations
                    .iter()
                    .flat_map(|port| &port.hidden)
                    .map(|hidden| hidden.name.clone()),
            );
            if (named || active.len() != inputs.len()) && !handlers.is_empty() {
                options.push(format!("active={}", tuple(&subscribed)));
            }
            if (named || valid.len() != inputs.len()) && !handlers.is_empty() {
                options.push(format!("valid={}", names(&valid)));
            }
        }
        let decorator = if result.is_some() {
            "compute_node"
        } else {
            "sink_node"
        };
        self.hgraph = true;
        self.line(0, "");
        self.line(0, "");
        if options.is_empty() {
            self.line(0, &format!("@_hg.{decorator}"));
        } else {
            self.line(0, &format!("@_hg.{decorator}({})", options.join(", ")));
        }
        let publishes = result.is_some();
        let returns = result.map_or(String::new(), |result| format!(" -> {result}"));
        if hidden {
            parameters.insert(0, "_begin: _hg.TS[_py.bool]".to_string());
        }
        self.line(
            0,
            &format!("def {python}({}){returns}:", parameters.join(", ")),
        );
        let before = self.text.len();
        self.unheld(signature);
        // Core's CAP-4 has `is_scheduled` "True while a wake is
        // pending", and the wake being delivered is pending no longer.
        // hgraph holds that wake until the node has run, so the
        // provider takes it as the node is entered, and keeps that it
        // was due: the scheduler then says what is still to come, for
        // the scheduler as for the alarm.
        if timer.is_some() {
            self.natives = true;
            self.line(1, "_scheduler = _core.Wake(_scheduler)");
        }
        if let (true, Some(woke)) = (holds_timer, &timer_woke) {
            self.line(1, &format!("_timer_woke = {woke}"));
            self.scheduled = Some("_timer_woke".to_string());
        }
        if holds_input {
            self.line(1, &format!("_input_woke = {input_woke}"));
        }
        // FN-16: what a `ref` result designates, kept through the
        // evaluation by the provider, which follows a re-binding of the
        // endpoint it designates. TIME-10: the node is not woken for
        // that: where neither an active input nor its scheduler or its
        // alarm woke it, it returns before any handler.
        if signature.reference {
            self.natives = true;
            let ports: Vec<String> = designations
                .iter()
                .map(|port| format!("{}: {}", string(&port.name), port.read()))
                .collect();
            self.line(
                1,
                &format!(
                    "_designation = _core.Designation(_output, _state, {{{}}})",
                    ports.join(", ")
                ),
            );
            if hiding {
                let woken = self.woken(body, &inputs, &active, changes, timer_woke.as_ref());
                self.line(1, &format!("if not {woken}:"));
                self.line(2, "return _designation.published()");
            }
        }
        self.designated = designations
            .iter()
            .map(|port| (port.local, port.name.clone()))
            .collect();
        let endpoint = |local: usize| endpoint_name(&body.locals[local]);
        for handler in &handlers {
            let mut guards: Vec<String> = Vec::new();
            // TIME-3: "A wake by the scheduler or alarm wakes the
            // handlers that subscribe to no input and those whose
            // header has `scheduled()` ... Such a wake alone runs no
            // other handler: one with no `modified` selector
            // subscribes to every temporal input, and is run by an
            // input's wake."
            if timed(handler) {
                match &timer_guard {
                    Some(woke) => guards.push(woke.clone()),
                    // With neither, "nothing wakes it": the handler is
                    // never run, and is not written.
                    None => continue,
                }
            } else if timer.is_some() && !handler.selects {
                guards.push(input_guard.clone());
            }
            for &local in &handler.required {
                // TIME-9: `valid` of a value that holds a list is not
                // hgraph's (`valid_test`), whose `valid=` the node is
                // given too: it holds where the language's does. A
                // condition of the handler that is `all_valid` of the
                // input holds only where it is valid.
                let ty = &body.locals[local].ty;
                let all_valid = handler.conditions.iter().any(|condition| {
                    matches!(&condition.kind, ExprKind::Endpoint {
                        operation: EndpointOperation::AllValid,
                        inputs: Some(inputs),
                    } if inputs.as_slice() == [local])
                });
                // A `ref` parameter's `valid` is its `REF`'s own
                // (TIME-10, proposed): it designates something.
                let precise = if function.is_reference(local) {
                    None
                } else {
                    self.precise_valid(ty, &endpoint(local))
                };
                match precise {
                    Some(precise) if !all_valid => guards.push(precise),
                    _ if !valid.contains(&local) => {
                        guards.push(format!("{}.valid", endpoint(local)));
                    }
                    _ => {}
                }
            }
            for condition in &handler.conditions {
                if let ExprKind::Endpoint {
                    operation: EndpointOperation::Modified,
                    inputs: selected,
                } = &condition.kind
                {
                    // One selector matches a publication on any named input.
                    // Separate selectors remain separate conjuncts. END-4:
                    // invalidation can wake the node but is no publication.
                    let mut publications = Vec::new();
                    for &local in selected.as_deref().unwrap_or(&inputs) {
                        let input = endpoint(local);
                        let valid = if function.is_reference(local) {
                            format!("{input}.valid")
                        } else {
                            self.valid_test(&body.locals[local].ty, &input)
                        };
                        publications.push(if handler.required.contains(&local) {
                            format!("{input}.modified")
                        } else {
                            format!("({input}.modified and {valid})")
                        });
                    }
                    guards.push(match publications.as_slice() {
                        [] => "False".to_string(),
                        [one] => one.clone(),
                        several => format!("({})", several.join(" or ")),
                    });
                } else {
                    guards.push(self.expression(&function, condition));
                }
            }
            // TIME-3: "In that handler's body `modified([])`, as a bool,
            // is true, at a wake that lands with an input's tick
            // included: what the header selected is taken as nothing
            // modified."
            self.empty_selected = handler.empty;
            if guards.is_empty() {
                self.statements(&function, handler.body, 1);
            } else {
                self.line(1, &format!("if {}:", guards.join(" and ")));
                self.block(&function, handler.body, 2);
            }
            self.empty_selected = false;
        }
        // `start` and `stop` are no evaluation a wake is due in.
        self.scheduled = None;
        self.designated.clear();
        // FN-7: "Completing a runtime handler without a write produces
        // no tick", and a `ref` result's write is what the node returns.
        if signature.reference {
            self.line(1, "return _designation.published()");
        }
        if self.text.len() == before {
            self.line(1, "pass");
        }
        // `start` and `stop` are read as what they are to hgraph.
        let lifecycle_hook = function.in_mode(Mode::Lifecycle);
        let stop_hook = function.in_mode(Mode::Stop);
        // `start`: the state's first values, then the hook's own
        // statements.
        if has_state || start.is_some() {
            self.line(0, "");
            self.line(0, "");
            self.line(0, &format!("@{python}.start"));
            self.line(0, &format!("def _start_{python}({lifecycle}):"));
            let before = self.text.len();
            self.unheld(signature);
            self.state_values(&lifecycle_hook, &states);
            if let Some(start) = start {
                self.statements(&lifecycle_hook, start, 1);
            }
            // A `start` with nothing in it is still a function.
            if self.text.len() == before {
                self.line(1, "pass");
            }
        }
        if let Some(stop) = stop {
            self.line(0, "");
            self.line(0, "");
            self.line(0, &format!("@{python}.stop"));
            self.line(0, &format!("def _stop_{python}({stopping}):"));
            self.unheld(signature);
            self.block(&stop_hook, stop, 1);
        }
        if exposing {
            // The graph a call names: the node, given the constant its
            // hidden input is never woken by, or each input it designates
            // a second time.
            let mut given = Vec::new();
            if hidden {
                given.push("_begin=_hg.const(True)".to_string());
            }
            given.extend(signature.parameters.iter().map(|parameter| {
                let python = name(&parameter.name);
                format!("{python}={python}")
            }));
            given.extend(
                designations
                    .iter()
                    .flat_map(|port| &port.hidden)
                    .map(|hidden| format!("{}={}", hidden.name, hidden.given)),
            );
            self.line(0, "");
            self.line(0, "");
            self.line(0, "@_hg.graph");
            self.line(
                0,
                &format!("def {exposed}({}){returns}:", declared.join(", ")),
            );
            let gives = if publishes { "return " } else { "" };
            self.line(1, &format!("{gives}{python}({})", given.join(", ")));
        }
    }

    /// Whether a temporal input or the node's scheduler or alarm woke
    /// the node whose body is `body`: `inputs` are its temporal inputs,
    /// `active` those a handler subscribes to, `changes` whether the
    /// body changes an input's activity, and `timer` what says its
    /// scheduler or alarm woke it, where it injects one. An input wakes
    /// it where it is active and published or was invalidated (END-4:
    /// "The transition to invalid notifies active subscribers"), and the
    /// hidden `REF` inputs of its designations (`designated_inputs`) do
    /// not.
    fn woken(
        &self,
        body: &Body,
        inputs: &[usize],
        active: &[usize],
        changes: bool,
        timer: Option<&String>,
    ) -> String {
        // Where the body changes an input's activity (`changes`), each
        // input is asked whether it is active.
        let mut woke: Vec<String> = inputs
            .iter()
            .filter(|local| changes || active.contains(local))
            .map(|&local| {
                let input = name(&body.locals[local].name);
                if changes {
                    format!("(({input}.modified or {input}.invalidated) and {input}.active)")
                } else {
                    format!("{input}.modified or {input}.invalidated")
                }
            })
            .collect();
        woke.extend(timer.cloned());
        match woke.as_slice() {
            [] => "False".to_string(),
            several => format!("({})", several.join(" or ")),
        }
    }

    /// Each temporal input of the node `function` whose body is `body`
    /// that a `return` to its `ref` result, or an `out =` of it,
    /// designates (FN-16: "A parameter that is not `ref` may be
    /// designated too"), with the `REF` input the node reads it through
    /// (`Port`). `active` are the inputs its handlers subscribe to, and
    /// `changes` whether the body changes an input's activity.
    ///
    /// hgraph makes a reference only from a `REF` input, and the input
    /// itself stays the one the body reads and subscribes to: an input
    /// that is no `ref` parameter is given to the node a second time, as
    /// a hidden `REF` input (`Designated`). So is a `ref` parameter that
    /// may be passive: the hidden one is active, and wakes the node where
    /// what the parameter is bound to is re-bound. The input whole is
    /// `REF` of its type, and each position of a fixed list a `REF` of
    /// the element's, which the graph gives the position. One designated
    /// both ways is reported.
    fn designated_inputs(
        &mut self,
        function: &Function,
        body: &Body,
        active: &[usize],
        changes: bool,
    ) -> Vec<Port> {
        use hgl_analyser::hir::{Node, walk_statement_nodes};
        let mut found: Vec<(usize, Designated, Location)> = Vec::new();
        walk_statement_nodes(&body.statements, &mut |node| {
            let value = match node {
                Node::Statement(Stmt::Return {
                    value: Some(value), ..
                }) => value,
                Node::Statement(Stmt::Assign { local, value, .. }) if function.is_out(*local) => {
                    value
                }
                _ => return,
            };
            if let Some((local, how)) = designated(function, value) {
                found.push((local, how, value.at));
            }
        });
        let mut inputs: Vec<Port> = Vec::new();
        let mut kinds: Vec<(usize, Designated)> = Vec::new();
        for (local, how, at) in found {
            match kinds.iter().find(|(seen, _)| *seen == local) {
                Some((_, before)) if *before == how => continue,
                Some(_) => {
                    self.unsupported(
                        at,
                        "an input designated whole and by a position in one node",
                    );
                    continue;
                }
                None => kinds.push((local, how)),
            }
            // A `ref` parameter that is always active is its own port.
            if function.is_reference(local) && active.contains(&local) && !changes {
                inputs.push(Port {
                    local,
                    name: name(&body.locals[local].name),
                    hidden: Vec::new(),
                    positions: false,
                });
                continue;
            }
            let ty = function.locals[local].ty.clone();
            let input = name(&body.locals[local].name);
            let hidden = designated_name(&body.locals[local]);
            let port = match (how, &ty) {
                (Designated::Whole, _) => self.annotation(&ty, true, at).map(|annotation| Port {
                    local,
                    name: hidden.clone(),
                    hidden: vec![Hidden {
                        name: hidden.clone(),
                        annotation: designating(&annotation),
                        given: input.clone(),
                    }],
                    positions: false,
                }),
                // One hidden `REF` input for each position, which the
                // graph gives the list's position: hgraph binds that
                // wherever the list comes from, a reference among them
                // (`_ref0_items` for `items[0]`). A `TSL` of `REF`s
                // given the list whole is bound to the positions of a
                // list's output only, and fails as the graph runs for one
                // that is a reference ("TSOutput structural reference
                // alternatives are not implemented yet").
                (
                    Designated::Position,
                    Type::List {
                        element,
                        size: Some(size),
                    },
                ) => match &size.value {
                    ConstantValue::Int(count) if *count > 0 => {
                        self.child_annotation(&ty, element, at).map(|child| Port {
                            local,
                            name: hidden.clone(),
                            hidden: (0..*count)
                                .map(|position| Hidden {
                                    name: format!("_ref{position}_{input}"),
                                    annotation: designating(&child),
                                    given: format!("{input}[{position}]"),
                                })
                                .collect(),
                            positions: true,
                        })
                    }
                    _ => {
                        self.unsupported(at, &format!("a designation of a position of `{ty}`"));
                        None
                    }
                },
                (Designated::Position, _) => None,
            };
            inputs.extend(port);
        }
        inputs
    }

    /// Writes, in a node's `start`, the first value of each of its
    /// `state`s and `cache`s, `states`, as a field of its `STATE`:
    /// `hook` is the function `start` reads them in.
    fn state_values(&mut self, hook: &Function, states: &[(usize, &Option<Expr>)]) {
        for (local, value) in states {
            let field = name(&hook.locals[*local].name);
            let value = match value {
                Some(value) => {
                    let written = self.expression(hook, value);
                    self.kept(hook, *local, value, written)
                }
                None => "None".to_string(),
            };
            self.line(1, &format!("_state.{field} = {value}"));
        }
    }

    /// A source (SOURCE-1): "a node that produces its output and has no
    /// temporal parameters". It is written as three functions and a
    /// graph that wires them, `python`:
    ///
    /// - `_yields_python`, a Python generator that holds the source's
    ///   statements, each `yield time: value` a `yield (time, value)`;
    /// - `_source_python`, a node that resumes the generator and
    ///   publishes what it is due. hgraph takes no node without an
    ///   input, so it has one, `_begin`, which the graph gives a
    ///   constant: that wakes it once, when the graph starts
    ///   (SOURCE-2: "The first evaluation begins the body"), and its
    ///   own scheduler wakes it after;
    /// - its `start`, which gives each `cache` and `state` its value,
    ///   makes the generator and runs the source's `start`, and its
    ///   `stop`, the source's `stop`: "`stop`, where present, always
    ///   runs when the node ends, whatever state the generator is in".
    ///
    /// hgraph's own generator node is not used: it has no `start` and
    /// no `stop`, and publishes a target that has passed where
    /// SOURCE-3 skips it. The provider's `resume` holds a target to
    /// SOURCE-2 and SOURCE-3.
    fn source(&mut self, index: usize, python: &str, signature: &Signature, body: &Body) {
        let program = self.program;
        let declaration = &program.modules[self.module].declarations[index];
        let (parameters, result) = self.temporal_signature(declaration, signature);
        // What the body is made of: the declarations, the hooks, and
        // the statements that yield.
        let NodeParts {
            states,
            start,
            stop,
            rest,
        } = NodeParts::of(body);
        let mut capabilities: Vec<(&str, &str)> = Vec::new();
        let mut block: Vec<Stmt> = Vec::new();
        for statement in rest {
            match statement {
                Stmt::Inject { local, at } => {
                    let injected = &body.locals[*local].name;
                    match capability(injected) {
                        // SOURCE-1: a source injects "not `out`,
                        // `scheduler`, or `alarm`: `yield time: value`
                        // writes its output and sets its next
                        // evaluation". The analyser rejects one
                        // (`source.statement`); hgraph's scheduler,
                        // the one a node is given, resumes a source.
                        Some(_) if matches!(injected.as_str(), "scheduler" | "alarm") => {
                            self.unsupported(
                                *at,
                                &format!("an `inject` of `{injected}` in a source"),
                            );
                        }
                        Some(written) => capabilities.push(written),
                        None => self.unsupported(*at, &format!("an `inject` of `{injected}`")),
                    }
                }
                other => block.push(other.clone()),
            }
        }
        // The configuration by name, as the generator and the node are
        // given it.
        let names: Vec<String> = signature
            .parameters
            .iter()
            .map(|parameter| name(&parameter.name))
            .collect();
        let generator = format!("_yields_{python}");
        let node = format!("_source_{python}");
        self.hgraph = true;
        self.natives = true;

        // The statements that yield: a generator, given the
        // configuration and the node's state.
        let mut given = names.clone();
        given.push("_state".to_string());
        self.line(0, "");
        self.line(0, "");
        self.line(0, &format!("def {generator}({}):", given.join(", ")));
        let yielding = Function::of(self.program, body, Some(signature), Mode::Source);
        self.block(&yielding, &block, 1);

        // The node. The clock and the scheduler are the target's own
        // here: the clock says what the current evaluation time is,
        // which a duration is counted from, and the scheduler wakes
        // the node at the next target.
        let mut hooks: Vec<String> = parameters.clone();
        hooks.push("_state: _hg.STATE = None".to_string());
        for (parameter, annotation) in &capabilities {
            hooks.push(format!("{parameter}: {annotation} = None"));
        }
        let mut node_parameters = vec!["_begin: _hg.TS[_py.bool]".to_string()];
        node_parameters.extend(hooks.iter().cloned());
        if !capabilities
            .iter()
            .any(|(parameter, _)| *parameter == "_clock")
        {
            node_parameters.push("_clock: _hg.EvaluationClock = None".to_string());
        }
        node_parameters.push("_scheduler: _hg.SCHEDULER = None".to_string());
        // TIME-7, TYPE-16: what a source yields to an unsized list, alone
        // or in a struct or a tuple, is applied to each list as it is
        // when it is due, on the node's own output (`published_into`).
        let grows = signature
            .result
            .as_ref()
            .and_then(|result| list_parts_shape(self.program, result));
        // SOURCE-3: "On wake it publishes and resumes after yield." The
        // provider publishes through the node's output before it resumes
        // the generator, so that what is published stands where the rest
        // of the pass fails (ERR-8: "Earlier writes remain visible").
        if let Some(result) = &result {
            node_parameters.push(format!("_output: {} = None", output_annotation(result)));
        }
        let returns = result
            .as_ref()
            .map_or(String::new(), |result| format!(" -> {result}"));
        self.line(0, "");
        self.line(0, "");
        self.line(0, "@_hg.compute_node");
        self.line(
            0,
            &format!("def {node}({}){returns}:", node_parameters.join(", ")),
        );
        // A capability is good for one call: the generator reads the
        // ones of the evaluation it is resumed in.
        for (parameter, _) in &capabilities {
            self.line(1, &format!("_state.{parameter} = {parameter}"));
        }
        let output = if result.is_some() { "_output" } else { "None" };
        let shape = grows.map_or(String::new(), |shape| format!(", {shape}"));
        self.line(
            1,
            &format!(
                "_core.resume(_state._source, _clock.evaluation_time, _scheduler, {output}{shape})"
            ),
        );

        // `start`: the first values, the hook, and the generator.
        let hooks = hooks.join(", ");
        let lifecycle_hook = yielding.in_mode(Mode::Lifecycle);
        self.line(0, "");
        self.line(0, "");
        self.line(0, &format!("@{node}.start"));
        self.line(0, &format!("def _start_{node}({hooks}):"));
        self.unheld(signature);
        self.state_values(&lifecycle_hook, &states);
        // The generator is made before the source's `start` runs: a
        // `return` there ends the hook, and must not leave the node
        // with nothing to resume. Making it runs none of the source's
        // statements: Python begins a generator where it is first
        // resumed.
        self.line(
            1,
            &format!(
                "_state._source = _core.Source({generator}({}))",
                given.join(", ")
            ),
        );
        if let Some(start) = start {
            self.statements(&lifecycle_hook, start, 1);
        }
        if let Some(stop) = stop {
            let stop_hook = yielding.in_mode(Mode::Stop);
            self.line(0, "");
            self.line(0, "");
            self.line(0, &format!("@{node}.stop"));
            self.line(0, &format!("def _stop_{node}({hooks}):"));
            self.unheld(signature);
            self.block(&stop_hook, stop, 1);
        }

        // The graph a call names: the node, woken once as it starts.
        // Configuration the node keeps is given to it inside an object
        // hgraph keeps as it is.
        let mut wired = vec!["_hg.const(True)".to_string()];
        wired.extend(signature.parameters.iter().map(|parameter| {
            let python = name(&parameter.name);
            // The node keeps what the graph is given, which the caller
            // may go on changing where it is: a copy.
            let python = if parameter.constant && changed_in_place(self.program, &parameter.ty, 0) {
                self.natives = true;
                format!("_core.copied({python})")
            } else {
                python
            };
            self.held_value(held(&parameter.ty), python)
        }));
        self.line(0, "");
        self.line(0, "");
        self.line(0, "@_hg.graph");
        self.line(
            0,
            &format!("def {python}({}){returns}:", parameters.join(", ")),
        );
        self.line(1, &format!("return {node}({})", wired.join(", ")));
    }

    /// A graph: `@graph`, whose body wires the nodes it calls.
    pub(super) fn graph(&mut self, index: usize, python: &str, signature: &Signature, body: &Body) {
        let program = self.program;
        let declaration = &program.modules[self.module].declarations[index];
        let function = Function::of(self.program, body, Some(signature), Mode::Graph);
        let (parameters, result) = self.temporal_signature(declaration, signature);
        self.hgraph = true;
        self.line(0, "");
        self.line(0, "");
        self.line(0, "@_hg.graph");
        let returns = result.map_or(String::new(), |result| format!(" -> {result}"));
        self.line(
            0,
            &format!("def {python}({}){returns}:", parameters.join(", ")),
        );
        self.bound = (0..body.locals.len())
            .filter(|&local| body.locals[local].kind == LocalKind::Parameter)
            .collect();
        self.block(&function, &body.statements, 1);
    }

    /// The lines that take a node's kept configuration out of the
    /// objects it was given in, at the head of the node, its `start`
    /// or its `stop`: `items = items.value`.
    fn unheld(&mut self, signature: &Signature) {
        for parameter in &signature.parameters {
            if parameter.constant && held(&parameter.ty) {
                let python = name(&parameter.name);
                self.line(1, &format!("{python} = {python}.value"));
            }
        }
    }
}

/// What the body of a node or a source is made of that both write
/// alike: each `state` and `cache` with its first value, and the
/// `start` and `stop` hooks. `rest` is every other statement, in the
/// order written, for the node or the source to write.
struct NodeParts<'b> {
    /// TIME-6: "Cache is reconstructible node-local value data,
    /// initialized on every start". hgraph's `STATE` is the node's own
    /// and is recorded nowhere, so a `state` and a `cache` are both
    /// fields of it, given their values in `start`, in the order
    /// written: each local with its value.
    states: Vec<(usize, &'b Option<Expr>)>,
    /// The `start` hook's statements.
    start: Option<&'b [Stmt]>,
    /// The `stop` hook's statements.
    stop: Option<&'b [Stmt]>,
    /// The other statements.
    rest: Vec<&'b Stmt>,
}

impl<'b> NodeParts<'b> {
    /// The parts of `body`.
    fn of(body: &'b Body) -> NodeParts<'b> {
        let mut parts = NodeParts {
            states: Vec::new(),
            start: None,
            stop: None,
            rest: Vec::new(),
        };
        for statement in &body.statements {
            match statement {
                Stmt::Declare { local, value }
                    if matches!(
                        body.locals[*local].kind,
                        LocalKind::State | LocalKind::Cache
                    ) =>
                {
                    parts.states.push((*local, value));
                }
                Stmt::Hook {
                    hook: Hook::Start,
                    body,
                    ..
                } => parts.start = Some(body),
                Stmt::Hook {
                    hook: Hook::Stop,
                    body,
                    ..
                } => parts.stop = Some(body),
                other => parts.rest.push(other),
            }
        }
        parts
    }
}

/// A capability a node injects (TIME-8), as hgraph gives one to a node
/// and to its `start` and `stop`: the parameter the target writes for
/// it, and that parameter's annotation. `None` for a name that is no
/// capability's.
///
/// Core's CAP-5: "An alarm is a cheaper scheduler". hgraph has one
/// scheduler for a node and no cheaper one, so a node that injects
/// `alarm` is given it too; the provider's functions keep the alarm to
/// what an alarm does.
///
/// hgraph gives a capability for one call and no longer, so each of
/// the three functions names the ones the node injects. That is the
/// language's rule too: a capability "is used within the node that
/// injects it and is not retained".
pub(super) fn capability(injected: &str) -> Option<(&'static str, &'static str)> {
    match injected {
        "clock" => Some(("_clock", "_hg.EvaluationClock")),
        "logger" => Some(("_logger", "_hg.LOGGER")),
        "global_state" => Some(("_store", "_hg.GlobalState")),
        "scheduler" | "alarm" => Some(("_scheduler", "_hg.SCHEDULER")),
        _ => None,
    }
}

/// What wakes a node beside its inputs (TIME-9): the scheduler or the
/// alarm it injects.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Timer {
    /// `inject scheduler`: `scheduled()` is its `is_scheduled_now`.
    Scheduler,
    /// `inject alarm`: `scheduled()` "holds where the node is evaluated
    /// and no temporal input has ticked".
    Alarm,
}

/// One `when` of a node: what its header asks, and its body.
struct Handler<'a> {
    /// The inputs whose tick wakes it (TIME-3), as local positions.
    subscribed: Vec<usize>,
    /// The inputs it requires to be valid.
    required: Vec<usize>,
    /// Predicates checked when the node runs, including explicit modification guards.
    conditions: Vec<&'a Expr>,
    /// True where its header has `scheduled()`, alone or as a top-level
    /// conjunct: a wake by the node's scheduler or alarm runs it
    /// (TIME-3).
    scheduled: bool,
    /// True where its header writes a `modified` selector. One that
    /// writes none "subscribes to every temporal input".
    selects: bool,
    /// Where its header has `modified([])` beside a `modified` selector
    /// that names inputs, each a top-level conjunct: the specification
    /// does not say what the two ask together, and the target reports it.
    beside: Option<Location>,
    /// True where its header has `modified([])` alone or as a top-level
    /// conjunct: the selector, which a wake of the scheduler or the
    /// alarm runs, and in its body `modified([])` is true (TIME-3).
    empty: bool,
    /// Its statements.
    body: &'a [Stmt],
}

impl<'a> Handler<'a> {
    /// A handler as the analyser read its header (`hir::Header`):
    /// what it subscribes to and requires, and the conjuncts of its
    /// condition that are checked when it runs.
    fn of(header: &'a Header, condition: Option<&'a Expr>, body: &'a [Stmt]) -> Handler<'a> {
        let conjuncts = condition.map(Expr::conjuncts).unwrap_or_default();
        let empty = conjuncts.iter().any(|conjunct| {
            matches!(&conjunct.kind, ExprKind::Endpoint {
                operation: EndpointOperation::Modified,
                inputs: Some(inputs),
            } if inputs.is_empty())
        });
        Handler {
            subscribed: header.subscribed.clone(),
            required: header.required.clone(),
            conditions: header
                .conditions
                .iter()
                .filter_map(|&position| conjuncts.get(position).copied())
                .collect(),
            scheduled: header.scheduled,
            selects: header.selects,
            beside: header.beside,
            empty,
            body,
        }
    }
}

/// True when `statements` hold a `yield`, at any depth: in a loop, a
/// `switch`, an `if` or a block. SOURCE-1: "A temporal `fn` whose body
/// has a `yield` is a source".
pub(super) fn yields(statements: &[Stmt]) -> bool {
    fn within(value: &Expr) -> bool {
        match &value.kind {
            // A block ends with an expression where its last statement
            // is one: an `if` that yields, say.
            ExprKind::Block { statements, value } => {
                yields(statements) || value.as_deref().is_some_and(within)
            }
            ExprKind::If {
                then, otherwise, ..
            } => within(then) || otherwise.as_deref().is_some_and(within),
            _ => false,
        }
    }
    statements.iter().any(|statement| match statement {
        Stmt::Yield { .. } => true,
        Stmt::For { body, .. } | Stmt::While { body, .. } => yields(body),
        Stmt::Switch { cases, default, .. } => {
            cases.iter().any(|case| yields(&case.body)) || default.as_deref().is_some_and(yields)
        }
        Stmt::Expr(value) => within(value),
        _ => false,
    })
}

/// An input a node designates (FN-16), as the provider's `Designation`
/// reads it (`Emitter::designated_inputs`).
pub(super) struct Port {
    /// The input, among the node's locals.
    pub(super) local: usize,
    /// The name the provider is told it by: the input's own, a `ref`
    /// parameter's whose `REF` is read, or the hidden input's.
    pub(super) name: String,
    /// The hidden `REF` inputs the node is given beside the input, none
    /// where its own is read: one for the input whole, or one for each
    /// position of a fixed list.
    pub(super) hidden: Vec<Hidden>,
    /// True where `hidden` has one for each position.
    pub(super) positions: bool,
}

impl Port {
    /// What the provider reads the input through: its `REF` input, or
    /// the tuple of those of its positions.
    fn read(&self) -> String {
        if self.positions {
            let inputs: Vec<&str> = self
                .hidden
                .iter()
                .map(|hidden| hidden.name.as_str())
                .collect();
            format!("({},)", inputs.join(", "))
        } else {
            self.name.clone()
        }
    }
}

/// A hidden `REF` input a node is given for what it designates (`Port`).
pub(super) struct Hidden {
    /// Its Python name.
    pub(super) name: String,
    /// Its annotation, a `REF` of what it is given.
    pub(super) annotation: String,
    /// What the graph of the function's name gives it.
    pub(super) given: String,
}
