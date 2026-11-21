//! Writes temporal functions whose body is a node's (FN-2: state and
//! hooks that run when the node is evaluated) as hgraph static nodes
//! (`<hgraph/types/generated.h>`), and the context every temporal piece
//! is written in. Part of the transpiler, the last stage.
//!
//! | HGL | C++ |
//! | --- | --- |
//! | `fn count(value: i64) -> i64 { ... }` | a struct `n_::count::impl_` of the module's namespace, with static `eval`, and `start` and `stop` where it needs them; and a wiring function `w_::count(wiring_, value)`, which its module's header declares, that adds the node to a graph |
//! | A temporal parameter; a `const` one | `::hgraph::In<"value", TS<...>>`, its activity and its validity the node's (TIME-4); `::hgraph::Scalar<"key", T>` |
//! | `state` and `cache` (TIME-6) | members of the node's one `state_type_`, held by hgraph as the node's `State`, each given its first value as the node starts, then bound by reference in each hook |
//! | `inject clock`, `logger`, `global_state` (TIME-8) | the hgraph view of the hook, and the library's capability made from it at the top of the hook (`endpoints`) |
//! | `inject scheduler`, `inject alarm` (TIME-8, TIME-9) | the same, made with the hook it is made in and in every `eval`, and what woke the node read as `eval` is entered, which guards the handlers (`scheduling`) |
//! | `when` handlers (TIME-3) | tested in source order in `eval`, each guarded by what it asks beyond the node's own validity |
//! | `return value`, `out = value` (TIME-7, FN-7) | the value published to the node's output, `out_`, as its temporal form publishes; a `return` then leaves the evaluation |
//! | A generic node (GEN-2) | a struct and a wiring function for each instance `hgl_lower` collects, `n_::i_::record::of_i64::impl_` |
//!
//! What a handler subscribes to and requires is `hgl_lower::nodes`'s, as
//! the analyser read the header. An input every executed handler
//! requires is the node's to require, `InputValidity::Valid`, and hgraph
//! then evaluates the node only once it is valid; one no handler
//! subscribes to is `InputActivity::Passive`.
//!
//! What the `ref` mode of a parameter and the result adds is
//! `references`'s (FN-16). Not written yet, and reported: a read of
//! `out`.

use hgl_analyser::hir::{Body, Expr, ExprKind, Hook, Local, Stmt};
use hgl_analyser::{Detail, LocalKind, Phase, Signature, Type};
use hgl_diagnostics::Location;
use hgl_lower::nodes::{Handler, Timer, is_input, is_out, required_inputs, subscribed_inputs};

use super::Emitter;
use super::forms::Family;
use super::names::{self, identifier};
use super::readers::{Reading, watches_failure};
use super::statements::{Scope, statement_at};
use super::values::INDENT;
use crate::runtime;

/// The name of every temporal function's struct, in a namespace of the
/// function's own name (`Placed`): no member it has, `name`, `eval` or
/// `compose`, has the name of the struct, which C++ refuses, whatever the
/// function is called.
pub(super) const IMPLEMENTATION: &str = "impl_";

/// The name of the view of a node's result, `out` (TIME-7).
pub(super) const OUT: &str = "out_";
/// The bundle keeping a projected node's declared endpoint and its
/// externally visible connection (ERR-15).
pub(super) const RESULT_OUT: &str = "result_out_";
/// The scalar that selects which connection the call takes (ERR-15).
pub(super) const RESULT_FORM: &str = "result_form_";
/// A scalar label that a source parameter name cannot match.
const RESULT_FORM_LABEL: &str = "hgl.result.form";

/// The name of the view of a node's state.
pub(super) const STATE: &str = "state_";

/// The name of the node's state as a hook changes it.
const KEPT: &str = "kept_";

/// What the code being written is part of, where it is temporal: the
/// emitter holds one while it writes a node's hook, a source's body, a
/// graph, a lifted call's node or an eval's entry, and the families of
/// the dispatch point read it.
pub(super) struct Context {
    /// What is being written.
    pub(super) kind: Kind,
    /// What each local of the body is to the C++ written, by its
    /// position among the body's locals.
    pub(super) roles: Vec<Role>,
    /// The result of the node, where it has one: what a `return` and an
    /// `out =` publish (TIME-7).
    pub(super) result: Option<Type>,
    /// A result published in one of the forms a call can take (ERR-15).
    pub(super) result_forms: Option<ResultForms>,
    /// Reads performed inside the handlers on physical input endpoints.
    pub(super) input_reads: Vec<Reading>,
    /// The delta type when a source can yield both it and a whole value.
    pub(super) mixed_source_delta: Option<Type>,
    /// The nodes of a graph's lifted calls written so far, each a struct
    /// written in the graph's own (`graphs`).
    pub(super) lifted: Vec<String>,
    /// The signature of the node whose hook is written, where it is one:
    /// the modes of its parameters and its result, which a family reads
    /// (FN-16, `references`).
    pub(super) signature: Option<Signature>,
    /// What wakes the node beside its inputs, the scheduler or the alarm
    /// it injects (TIME-9), which `scheduling` writes.
    pub(super) timer: Option<Timer>,
    /// True while the body of a handler whose header selects
    /// `modified([])` is written, where it is `true` read as a value
    /// (TIME-3, `scheduling`).
    pub(super) selected_empty: bool,
}

impl Context {
    /// A context of `kind` for a body of `locals` locals, each plain.
    pub(super) fn new(kind: Kind, locals: usize) -> Context {
        Context {
            kind,
            roles: vec![Role::Plain; locals],
            result: None,
            result_forms: None,
            input_reads: Vec::new(),
            mixed_source_delta: None,
            lifted: Vec::new(),
            signature: None,
            timer: None,
            selected_empty: false,
        }
    }
}

/// The recorded form in which a temporal call exposes its result (ERR-15).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ResultForm {
    /// Every outcome is published as data.
    Whole,
    /// Publish the success, or fail this node.
    Value,
    /// Publish the success, or publish a null for failure.
    Nullable,
}

impl ResultForm {
    /// The generated wiring function's distinguishing suffix.
    pub(super) fn suffix(self) -> &'static str {
        match self {
            Self::Whole => "",
            Self::Value => "_result_value_",
            Self::Nullable => "_result_nullable_",
        }
    }

    /// The bundle field exposed as the call's connection.
    fn field(self) -> &'static str {
        match self {
            Self::Whole => "whole",
            Self::Value => "success",
            Self::Nullable => "nullable",
        }
    }

    /// The mode held by the hgraph node's scalar configuration.
    fn mode(self) -> u8 {
        match self {
            Self::Whole => 0,
            Self::Value => 1,
            Self::Nullable => 2,
        }
    }
}

/// The physical output bundle of a fallible node. The node reads its
/// own `out` through the declared result endpoint, independent of the
/// form the call exposes to its consumers.
#[derive(Clone)]
pub(super) struct ResultForms {
    schema: String,
    success: Option<Type>,
    nullable: Option<Type>,
}

impl ResultForms {
    /// Build the bundle for the forms supported by this target.
    pub(super) fn of(emitter: &mut Emitter<'_>, result: &Type) -> Option<Self> {
        let Type::Result { value, error } = result else {
            return None;
        };
        if **error != Type::Error {
            return None;
        }
        let whole = emitter.temporal_schema_of(result)?;
        let success_schema = emitter.temporal_schema_of(value);
        let nullable =
            (!matches!(**value, Type::Nullable(_))).then(|| Type::Nullable(value.clone()));
        let nullable_schema = nullable
            .as_ref()
            .and_then(|ty| emitter.temporal_schema_of(ty));
        if success_schema.is_none() && nullable_schema.is_none() {
            return None;
        }
        let fields = [
            (ResultForm::Whole, Some(whole)),
            (ResultForm::Value, success_schema.clone()),
            (ResultForm::Nullable, nullable_schema.clone()),
        ];
        let fields = fields
            .into_iter()
            .filter_map(|(form, schema)| {
                Some(format!(
                    "::hgraph::Field<\"{}\", {}>",
                    form.field(),
                    schema?
                ))
            })
            .collect::<Vec<_>>()
            .join(", ");
        Some(Self {
            schema: format!("::hgraph::UnNamedTSB<{fields}>"),
            success: success_schema.map(|_| (**value).clone()),
            nullable: nullable.filter(|_| nullable_schema.is_some()),
        })
    }

    /// The output view for one projected form.
    pub(super) fn view(form: ResultForm) -> String {
        format!("{RESULT_OUT}.field<\"{}\">()", form.field())
    }

    /// The eval parameter carrying every result form.
    pub(super) fn output_parameter(&self) -> String {
        format!("::hgraph::Out<{}> {RESULT_OUT}", self.schema)
    }

    /// The eval parameter selecting the form this call takes.
    pub(super) fn mode_parameter() -> String {
        let label = names::string(RESULT_FORM_LABEL);
        format!("::hgraph::Scalar<{label}, ::hgraph::Int> {RESULT_FORM}")
    }

    /// The wired field's position in the bundle schema.
    fn index(&self, form: ResultForm) -> usize {
        match form {
            ResultForm::Whole => 0,
            ResultForm::Value => 1,
            ResultForm::Nullable => 1 + usize::from(self.success.is_some()),
        }
    }
}

/// What is being written.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Kind {
    /// A hook of a node: `start`, a `when`, or `stop` (TIME-5).
    Hook(Hook),
    /// The body of a source, a coroutine (SOURCE-1).
    Source,
    /// The body of a graph (FN-2).
    Graph,
    /// The `eval` of the node a lifted call is (FN-6).
    Lifted,
    /// The entry of an eval's run (CEVAL-1).
    Entry,
}

/// What a local is to the C++ written.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Role {
    /// A C++ local of its own name: a `let` or a `var`, a `state` or a
    /// `cache` bound to its place in the node's state, a capability made
    /// at the top of its hook, a `const` parameter given by value.
    Plain,
    /// A temporal input of a node: its hgraph view, named as the
    /// parameter, whose payload is read checked (TIME-1).
    Input,
    /// A `const` parameter of a node: its `hgraph::Scalar`, named as the
    /// parameter, read with `.value()`.
    Config,
    /// `inject out`: the node's output view, [`OUT`].
    Out,
    /// A connection of a graph: an `hgraph::Port`, named as the local.
    Connection,
}

impl Emitter<'_> {
    /// What `local` is to the C++ written: [`Role::Plain`] outside a
    /// temporal context.
    pub(super) fn role(&self, local: usize) -> Role {
        self.temporal
            .as_ref()
            .and_then(|context| context.roles.get(local).copied())
            .unwrap_or(Role::Plain)
    }

    /// What is being written, where it is temporal.
    pub(super) fn kind(&self) -> Option<Kind> {
        self.temporal.as_ref().map(|context| context.kind)
    }

    /// The hgraph view of `local`, of `locals`, where it is an endpoint:
    /// a temporal input, named as it is, or `out`.
    pub(super) fn view(&self, locals: &[Local], local: usize) -> Option<String> {
        match self.role(local) {
            Role::Input => Some(identifier(&locals[local].name)),
            Role::Out => Some(OUT.to_string()),
            _ => None,
        }
    }

    /// Writes temporal function `index` of the module: a node, a source
    /// or a graph, as its body describes (FN-2, SOURCE-1), or, for one
    /// with generic parameters, a piece for each of its instances
    /// (GEN-2).
    pub(super) fn temporal_function(&mut self, index: usize) {
        let program = self.program;
        let declared = &program.modules[self.module];
        let at = declared.declarations[index].at;
        let Some(phase) = declared.phases.get(index).copied().flatten() else {
            self.unsupported(at, "a temporal function with no HGL body");
            return;
        };
        let (Some(Detail::Callable(signature)), Some(Some(body))) =
            (declared.details.get(index), declared.bodies.get(index))
        else {
            self.unsupported(at, "a temporal function with no body");
            return;
        };
        if self.instances.instanced(program, self.module, index) {
            let instances = self.instances;
            for instance in instances.declared(self.module, index) {
                if let Some(at) = instance.pack_use {
                    self.unsupported(at, "a read of a pack as a whole");
                } else {
                    self.temporal_piece(
                        index,
                        phase,
                        Some(&instance.suffix),
                        &instance.signature,
                        &instance.body,
                    );
                }
                let piece = self.close();
                self.instanced.push((index, instance.suffix.clone(), piece));
            }
            return;
        }
        self.temporal_piece(index, phase, None, signature, body);
    }

    /// Writes into the piece the temporal function `index`, or its
    /// instance `instance`, of `signature` and `body`.
    fn temporal_piece(
        &mut self,
        index: usize,
        phase: Phase,
        instance: Option<&str>,
        signature: &Signature,
        body: &Body,
    ) {
        let module = &self.program.modules[self.module];
        let at = module.declarations[index].at;
        // SOURCE-1 and FN-16: writing a source with a `ref` result
        // would need a designation without a temporal input.
        if module.is_source(index) && signature.reference {
            self.unsupported(at, "a `ref` result of a source");
            return;
        }
        // The body's locals start with its parameters, in order.
        let ordered = signature
            .parameters
            .iter()
            .zip(&body.locals)
            .all(|(parameter, local)| {
                parameter.name == local.name && local.kind == LocalKind::Parameter
            });
        if !ordered || body.locals.len() < signature.parameters.len() {
            self.unsupported(
                at,
                "a temporal function whose locals are not its parameters first",
            );
            return;
        }
        let shown = self.described(index, instance, signature, phase, module.is_source(index));
        match phase {
            // SOURCE-1: "A temporal `fn` whose body has a `yield` is a
            // source".
            Phase::Node if module.is_source(index) => {
                self.source(index, instance, signature, body, &shown);
            }
            Phase::Node => self.node(index, instance, signature, body, &shown),
            Phase::Graph => self.graph(index, instance, signature, body, &shown),
        }
    }

    /// What the comment above temporal function `index`, or its instance
    /// `instance`, calls it: its HGL name and signature, what it is, and
    /// where it is declared.
    fn described(
        &self,
        index: usize,
        instance: Option<&str>,
        signature: &Signature,
        phase: Phase,
        source: bool,
    ) -> String {
        let declaration = &self.program.modules[self.module].declarations[index];
        let what = match phase {
            Phase::Node if source => "source",
            Phase::Node => "node",
            Phase::Graph => "graph",
        };
        let instance = instance
            .map(|suffix| format!(", the instance `{suffix}`"))
            .unwrap_or_default();
        format!(
            "{}{signature} ({what}){instance}, {}",
            declaration.name,
            self.place_of(index)
        )
    }

    /// Writes the node `index`, or its instance `instance`, of
    /// `signature` and `body`, which `shown` describes.
    fn node(
        &mut self,
        index: usize,
        instance: Option<&str>,
        signature: &Signature,
        body: &Body,
        shown: &str,
    ) {
        self.node_with_inputs(index, instance, signature, body, shown, None);
        let readers = self.readers;
        for (number, reads) in readers
            .declared(self.module, index, instance)
            .iter()
            .enumerate()
        {
            self.node_with_inputs(
                index,
                instance,
                signature,
                body,
                shown,
                Some((number, reads)),
            );
        }
    }

    /// Write the same node with its physical input schemas for one
    /// call's recorded reader forms (ERR-12 and ERR-15).
    fn node_with_inputs(
        &mut self,
        index: usize,
        instance: Option<&str>,
        signature: &Signature,
        body: &Body,
        shown: &str,
        readers: Option<(usize, &[Reading])>,
    ) {
        let program = self.program;
        let placed_at = program.modules[self.module].declarations[index].at;
        let mut context = Context::new(Kind::Hook(Hook::When), body.locals.len());
        context.result = signature.result.clone();
        // FN-16: a reference result publishes a designation. Its
        // contextual result form is read from the designated endpoint.
        let forms = if signature.reference {
            None
        } else {
            signature
                .result
                .as_ref()
                .and_then(|ty| ResultForms::of(self, ty))
        };
        context.result_forms = forms.clone();
        context.signature = Some(signature.clone());
        let mut physical = signature.clone();
        let mut watched = Vec::new();
        if let Some((_, reads)) = readers {
            context.input_reads = reads.to_vec();
            for read in reads {
                physical.parameters[read.parameter].ty =
                    super::lifting::reader_operand(&read.form).ty.clone();
                if let Some(form) = watches_failure(&read.form) {
                    watched.push((read.parameter, form));
                }
            }
        }
        let mut inputs = Vec::new();
        for (local, role) in context.roles.iter_mut().enumerate() {
            if is_input(&body.locals, Some(signature), local) {
                *role = Role::Input;
                inputs.push(local);
            } else if local < signature.parameters.len() {
                *role = Role::Config;
            } else if is_out(&body.locals, local) {
                *role = Role::Out;
            }
        }
        // What the body is made of: its declarations, then its hooks
        // (FN-2).
        let mut states: Vec<(usize, Option<&Expr>)> = Vec::new();
        let mut injected: Vec<usize> = Vec::new();
        let mut timed: Vec<usize> = Vec::new();
        let mut start: Option<&[Stmt]> = None;
        let mut stop: Option<&[Stmt]> = None;
        let mut running: Vec<Handler> = Vec::new();
        let mut places: Vec<Location> = Vec::new();
        for statement in &body.statements {
            match statement {
                Stmt::Declare { local, value }
                    if matches!(
                        body.locals[*local].kind,
                        LocalKind::State | LocalKind::Cache
                    ) =>
                {
                    states.push((*local, value.as_ref()));
                }
                Stmt::Inject { local, at, .. } => {
                    let name = &body.locals[*local].name;
                    if name == "out" {
                        continue;
                    }
                    if capability(name).is_none() {
                        self.unsupported(*at, &format!("an injected `{name}`"));
                        return;
                    }
                    // TIME-9: the scheduler or the alarm, which wakes the
                    // node beside its inputs (`scheduling`).
                    if let Some(timer) = Timer::of(name) {
                        context.timer = Some(timer);
                        timed.push(*local);
                    }
                    injected.push(*local);
                }
                Stmt::Hook {
                    hook: Hook::Start,
                    body: statements,
                    ..
                } => start = Some(statements),
                Stmt::Hook {
                    hook: Hook::Stop,
                    body: statements,
                    ..
                } => stop = Some(statements),
                Stmt::Hook {
                    hook: Hook::When,
                    condition,
                    header: Some(header),
                    body: statements,
                    at,
                } => {
                    let mut handler = Handler::of(header, condition.as_ref(), statements);
                    // FN-16: what a handler requires of a `ref` parameter
                    // (`references`).
                    super::references::admission(&mut handler, &body.locals, signature);
                    if let Some(at) = handler.beside {
                        // The specification does not say what the two ask
                        // together (TIME-3).
                        self.unsupported(at, "`modified([])` beside another `modified` selector");
                        return;
                    }
                    // TIME-3: "A wake by the scheduler or alarm wakes the
                    // handlers that subscribe to no input and those whose
                    // header has `scheduled()` ... Where the node injects
                    // neither ... nothing wakes it": in a node that injects
                    // neither such a handler is never run, and is not
                    // written.
                    if (handler.scheduled || handler.subscribed.is_empty())
                        && context.timer.is_none()
                    {
                        continue;
                    }
                    running.push(handler);
                    places.push(*at);
                }
                other => {
                    self.unsupported(
                        statement_at(other, &body.locals),
                        "this statement of a node",
                    );
                    return;
                }
            }
        }
        // TIME-4: "Aggregate subscriptions cover every handler. Node-wide
        // validity includes only requirements common to executable
        // handlers; other guards stay per-handler."
        let active = subscribed_inputs(&inputs, &running);
        // ERR-15: a new Err can fail this node before any handler's
        // admission is met. In these variants validity stays per handler.
        let valid = if running.is_empty() || !watched.is_empty() {
            Vec::new()
        } else {
            required_inputs(&inputs, &running)
        };
        // FN-16: the ordinary inputs a `ref` result designates, which the
        // node follows (`references`), and the validity that leaves the
        // node's own.
        let Some(designated) = self.designated_inputs(signature, body, &injected) else {
            return;
        };
        let valid = designated.validity(valid);
        // The node's parameters as hgraph's views, in the order of the
        // signature: the order `wire` gives them in; then the inputs it
        // follows the designations of.
        let Some(mut parameters) = self.node_parameters(&physical, body, &active, &valid) else {
            return;
        };
        parameters.extend(designated.parameters());
        let indent = INDENT.repeat(2);
        let mut checks = String::new();
        for &(local, form) in &watched {
            let parameter = &physical.parameters[local];
            let Some(schema) = self.temporal_schema(&parameter.ty, form.at) else {
                return;
            };
            let view = names::result_watch(local);
            // The extra subscription sees Err even if the HGL input is
            // passive. A successful observation does not wake a handler.
            parameters.push(format!(
                "::hgraph::In<\"hgl.result.input.{local}\", {schema}, ::hgraph::InputValidity::Unchecked> {view}"
            ));
            checks.push_str(&self.check_result_tick(form, &view, &indent));
        }
        let output = if let Some(forms) = &forms {
            Some(forms.output_parameter())
        } else {
            match &signature.result {
                Some(result) => {
                    match self.endpoint_schema(result, signature.reference, placed_at) {
                        Some(schema) => Some(format!("::hgraph::Out<{schema}> {OUT}")),
                        None => return,
                    }
                }
                None => None,
            }
        };
        // The one struct of the node's `state` and `cache` (TIME-6).
        let mut members = String::new();
        for (local, _) in &states {
            let declared = &body.locals[*local];
            let Some(ty) = self.written_type(&declared.ty, declared.at) else {
                return;
            };
            members.push_str(&format!(
                "{INDENT}{INDENT}{ty} {}{{}};\n",
                identifier(&declared.name)
            ));
        }
        members.push_str(&designated.members());
        let held = !states.is_empty() || designated.holds();
        let mut placed = Placed::of(program, self.module, index, instance);
        if let Some((number, _)) = readers {
            placed.add_input_variant(number);
        }
        let qualified = placed.qualified(program, self.module);
        let mut text = format!("// {shown}\nstruct {IMPLEMENTATION}\n{{\n");
        text.push_str(&format!(
            "{INDENT}static constexpr auto name = {};\n",
            names::string(&placed.display(program, self.module))
        ));
        if held {
            text.push_str(&format!(
                "\n{INDENT}// The node's `state` and `cache` (TIME-6).\n{INDENT}struct state_type_\n{INDENT}{{\n{members}{INDENT}}};\n"
            ));
        }
        let configs: Vec<String> = parameters
            .iter()
            .zip(&signature.parameters)
            .filter(|(_, parameter)| parameter.constant)
            .map(|(written, _)| written.clone())
            .collect();
        let state_parameter = format!("::hgraph::State<state_type_> {STATE}");
        // `start`: the state's first values, then the hook's statements
        // (TIME-5: "Start follows initialization").
        if held || start.is_some() {
            let statements = start.unwrap_or(&[]);
            let mut hook = configs;
            let mut written = String::new();
            if held {
                hook.push(state_parameter.clone());
                written.push_str(&format!("{INDENT}{INDENT}{STATE}.set(state_type_{{}});\n"));
            }
            written.push_str(&prologue(body, &states));
            // TIME-6: "Cache is reconstructible node-local value data,
            // initialized on every start"; a `state` is given its first
            // value as the node starts too, since no state is restored.
            // An initializer may read a capability, which is made first.
            let initialized: Vec<Stmt> = states
                .iter()
                .filter_map(|(local, value)| {
                    value.map(|value| Stmt::Assign {
                        local: *local,
                        operator: None,
                        value: value.clone(),
                        at: body.locals[*local].at,
                    })
                })
                .collect();
            let used: Vec<Stmt> = initialized.iter().chain(statements).cloned().collect();
            let Some(made) = self.capabilities(body, &injected, &[], &used, Hook::Start, &mut hook)
            else {
                return;
            };
            written.push_str(&made);
            context.kind = Kind::Hook(Hook::Start);
            let mut scope = Scope::cold(program, body);
            self.in_context(&mut context, |emitter| {
                emitter.statements(&mut scope, &initialized, 2, &mut written);
                emitter.statements(&mut scope, statements, 2, &mut written);
            });
            text.push_str(&format!(
                "\n{INDENT}static void start({})\n{INDENT}{{\n{written}{INDENT}}}\n",
                maybe_unused(&hook)
            ));
        }
        // `eval`: each handler in source order (TIME-3).
        let mut hook = parameters.clone();
        if held {
            hook.push(state_parameter.clone());
        }
        if let Some(output) = &output {
            hook.push(output.clone());
        }
        let mut written = checks;
        written.push_str(&prologue(body, &states));
        if forms.is_some() {
            hook.push(ResultForms::mode_parameter());
            let declared = ResultForms::view(ResultForm::Whole);
            written.push_str(&format!(
                "{INDENT}{INDENT}[[maybe_unused]] const auto {OUT} = {declared};\n"
            ));
        }
        let all: Vec<Stmt> = running
            .iter()
            .flat_map(|handler| {
                let mut used: Vec<Stmt> = handler.body.to_vec();
                used.extend(
                    handler
                        .conditions
                        .iter()
                        .map(|condition| Stmt::Expr((*condition).clone())),
                );
                used
            })
            .collect();
        // The scheduler or the alarm is made in every evaluation: what woke
        // the node is read from the scheduler, or for an alarm from the
        // inputs (`scheduling`).
        let Some(made) = self.capabilities(body, &injected, &timed, &all, Hook::When, &mut hook)
        else {
            return;
        };
        written.push_str(&made);
        let woken = super::scheduling::woken(&context, body, &inputs);
        let designation = self.designation_prologue(&designated, body, &inputs, &active);
        if watched.is_empty() {
            written.push_str(&designation);
            written.push_str(&woken);
        } else {
            written.push_str(&woken);
            let library = runtime::NAMESPACE;
            let mut wakes = Vec::new();
            if context.timer.is_some() {
                wakes.push(super::scheduling::TIMER_WOKE.to_string());
            }
            for &local in &inputs {
                let name = identifier(&body.locals[local].name);
                wakes.push(format!(
                    "(::{library}::fn_is_active({name}) && ::{library}::ticked({name}))"
                ));
            }
            written.push_str(&format!("{indent}if (!({})) return;\n", wakes.join(" || ")));
            written.push_str(&designation);
        }
        context.kind = Kind::Hook(Hook::When);
        let mut handlers = String::new();
        for (handler, at) in running.iter().zip(&places) {
            if !self.handler(
                &mut context,
                body,
                &inputs,
                &valid,
                handler,
                *at,
                &mut handlers,
            ) {
                return;
            }
        }
        // END-5: a result's form may hold what the evaluation publishes,
        // and publish it once the handlers have run.
        match &signature.result {
            Some(result) => {
                let selected: Vec<(&Handler, Location)> =
                    running.iter().zip(places.iter().copied()).collect();
                let mut evaluated = String::new();
                self.in_context(&mut context, |emitter| {
                    evaluated = emitter.evaluation(result, &selected, &handlers);
                });
                written.push_str(&evaluated);
            }
            None => written.push_str(&handlers),
        }
        text.push_str(&format!(
            "\n{INDENT}static void eval({})\n{INDENT}{{\n{written}{INDENT}}}\n",
            maybe_unused(&hook)
        ));
        // `stop` (TIME-5), given the node's parameters, its state, and
        // its output where it reads it (TIME-7: "`out` is read in a `when`
        // and in `stop`").
        if let Some(statements) = stop {
            let mut hook = parameters.clone();
            if held {
                hook.push(state_parameter.clone());
            }
            let reads_output = reads_out(&context, statements);
            if let Some(output) = &output
                && reads_output
            {
                hook.push(output.clone());
            }
            let mut written = prologue(body, &states);
            if forms.is_some() && reads_output {
                let declared = ResultForms::view(ResultForm::Whole);
                written.push_str(&format!(
                    "{INDENT}{INDENT}[[maybe_unused]] const auto {OUT} = {declared};\n"
                ));
            }
            let Some(made) =
                self.capabilities(body, &injected, &[], statements, Hook::Stop, &mut hook)
            else {
                return;
            };
            written.push_str(&made);
            context.kind = Kind::Hook(Hook::Stop);
            let mut scope = Scope::cold(program, body);
            self.in_context(&mut context, |emitter| {
                emitter.statements(&mut scope, statements, 2, &mut written);
            });
            text.push_str(&format!(
                "\n{INDENT}static void stop({})\n{INDENT}{{\n{written}{INDENT}}}\n",
                maybe_unused(&hook)
            ));
        }
        text.push_str("};\n");
        let mut whole = nested(&placed.structs, &text);
        if held {
            let registered = match readers {
                Some((number, _)) => format!(
                    "{}.state.reader{number}",
                    placed.display(program, self.module)
                ),
                None => format!("{}.state", placed.display(program, self.module)),
            };
            whole.push_str(&registration(
                program,
                self.module,
                &format!("{qualified}::state_type_"),
                &registered,
            ));
        }
        let watched: Vec<usize> = watched.iter().map(|&(local, _)| local).collect();
        let Some((prototype, wiring)) =
            self.node_wiring(&physical, &placed, shown, body, forms.as_ref(), &watched)
        else {
            return;
        };
        whole.push_str(&wiring);
        self.piece.declaration.push_str(&prototype);
        self.piece.text.push_str(&whole);
    }

    /// The parameters of a node's `eval`, in the order of `signature`:
    /// each temporal one an `hgraph::In` of its temporal form, `Passive`
    /// where no handler subscribes to it and `Unchecked` where the node
    /// does not require it (`active`, `valid`), and each `const` one an
    /// `hgraph::Scalar`. `None` where a type is not written yet, which is
    /// reported.
    pub(super) fn node_parameters(
        &mut self,
        signature: &Signature,
        body: &Body,
        active: &[usize],
        valid: &[usize],
    ) -> Option<Vec<String>> {
        let mut written = Vec::new();
        for (local, parameter) in signature.parameters.iter().enumerate() {
            let name = identifier(&body.locals[local].name);
            let label = names::string(&parameter.name);
            if parameter.constant {
                let ty = self.written_type(&parameter.ty, parameter.at)?;
                // FN-1: as hgraph holds a configuration (`configuration`).
                let held = super::configuration::held(self.program, &parameter.ty, &ty);
                written.push(format!("::hgraph::Scalar<{label}, {held}> {name}"));
                continue;
            }
            let schema = self.endpoint_schema(&parameter.ty, parameter.reference, parameter.at)?;
            let mut policies = String::new();
            if !active.contains(&local) {
                policies.push_str(", ::hgraph::InputActivity::Passive");
            }
            if !valid.contains(&local) {
                policies.push_str(", ::hgraph::InputValidity::Unchecked");
            }
            written.push(format!("::hgraph::In<{label}, {schema}{policies}> {name}"));
        }
        Some(written)
    }

    /// Runs `write` with `context` as the emitter's, and the emitter's
    /// own again after it.
    pub(super) fn in_context(&mut self, context: &mut Context, write: impl FnOnce(&mut Self)) {
        let placeholder = Context::new(context.kind, 0);
        let outer = self
            .temporal
            .replace(std::mem::replace(context, placeholder));
        write(self);
        if let Some(inner) = self.temporal.take() {
            *context = inner;
        }
        self.temporal = outer;
    }

    /// The capabilities of `injected` that `statements` use, and those of
    /// `always` whether they do or not, each made at the top of its hook,
    /// `hook`, from the view hgraph gives it, which is added to
    /// `parameters` (TIME-8). `None` where one is not written yet, which
    /// is reported.
    pub(super) fn capabilities(
        &mut self,
        body: &Body,
        injected: &[usize],
        always: &[usize],
        statements: &[Stmt],
        hook: Hook,
        parameters: &mut Vec<String>,
    ) -> Option<String> {
        let mut written = String::new();
        for &local in injected {
            if !uses(statements, local) && !always.contains(&local) {
                continue;
            }
            let name = &body.locals[local].name;
            let Some((view, made)) = capability(name) else {
                self.unsupported(body.locals[local].at, &format!("an injected `{name}`"));
                return None;
            };
            let parameter = names::temporary("capability", local);
            parameters.push(format!("{view} {parameter}"));
            // A scheduler and an alarm are made with the hook they are
            // used in (`scheduling`).
            written.push_str(&format!(
                "{INDENT}{INDENT}[[maybe_unused]] const {made} {}{{{parameter}{}}};\n",
                identifier(name),
                super::scheduling::made_in(name, hook)
            ));
        }
        Some(written)
    }

    /// Writes the `when` handler `handler`, written at `at`, of the node
    /// whose temporal inputs are `inputs` and which requires `valid`
    /// itself, into `text`: its body, under a guard of what it asks
    /// beyond that (TIME-3, TIME-4). False where something is not
    /// written yet, which is reported.
    #[allow(clippy::too_many_arguments)]
    fn handler(
        &mut self,
        context: &mut Context,
        body: &Body,
        inputs: &[usize],
        valid: &[usize],
        handler: &Handler,
        at: Location,
        text: &mut String,
    ) -> bool {
        let indent = INDENT.repeat(2);
        let mut guards: Vec<String> = Vec::new();
        // TIME-3: in a node with a scheduler or an alarm, what woke the
        // node decides which handlers it runs (`scheduling`).
        guards.extend(super::scheduling::wake_guard(context.timer, handler));
        let mut scope = Scope::cold(self.program, body);
        let mut written = String::new();
        let mut broken = false;
        self.in_context(context, |emitter| {
            // What the handler requires that the node does not: each is
            // tested as the handler is reached.
            for &local in &handler.required {
                if valid.contains(&local) {
                    continue;
                }
                match emitter.view(&body.locals, local) {
                    Some(view) => {
                        guards.push(format!("::{}::fn_valid({view})", runtime::NAMESPACE))
                    }
                    None => {
                        emitter.unsupported(at, "a handler that requires this valid");
                        broken = true;
                    }
                }
            }
            for condition in &handler.conditions {
                // TIME-3: "`modified(a, b)` subscribes to the named inputs
                // and requires a publication on at least one of them", and
                // "Invalidation ... never satisfies a modification test".
                if let ExprKind::Endpoint {
                    operation: hgl_analyser::hir::EndpointOperation::Modified,
                    inputs: selected,
                } = &condition.kind
                {
                    let named: Vec<usize> = selected.clone().unwrap_or_else(|| inputs.to_vec());
                    let publications: Vec<String> = named
                        .iter()
                        .filter_map(|&local| emitter.view(&body.locals, local))
                        .map(|view| format!("::{}::fn_modified({view})", runtime::NAMESPACE))
                        .collect();
                    guards.push(match publications.as_slice() {
                        [] => "false".to_string(),
                        [one] => one.clone(),
                        several => format!("({})", several.join(" || ")),
                    });
                } else {
                    let before = scope.before.len();
                    let guard = emitter.expression(&mut scope, condition);
                    if scope.before.len() != before {
                        emitter.unsupported(
                            condition.at,
                            "a handler's condition that runs statements before it",
                        );
                        broken = true;
                    }
                    guards.push(guard);
                }
            }
            // TIME-3: "In that handler's body `modified([])`, as a bool, is
            // true" where its header selects it (`scheduling`).
            if let Some(current) = emitter.temporal.as_mut() {
                current.selected_empty = handler.empty;
            }
            emitter.statements(&mut scope, handler.body, 3, &mut written);
            if let Some(current) = emitter.temporal.as_mut() {
                current.selected_empty = false;
            }
        });
        if broken {
            return false;
        }
        let line = self.line_of(at);
        text.push_str(&format!("{indent}// when, line {line} (TIME-3)\n"));
        if guards.is_empty() {
            text.push_str(&format!("{indent}{{\n{written}{indent}}}\n"));
        } else {
            let guarded = format!(
                "{indent}if ({})\n{indent}{{\n{written}{indent}}}\n",
                guards.join(" && ")
            );
            // The HGL file and line of the handler, which a failure of its
            // condition names, on each line of it that no statement of its
            // body numbers (`stamped`).
            match self.line_directive(at) {
                Some(directive) => text.push_str(&super::stamped(&directive, &guarded)),
                None => text.push_str(&guarded),
            }
        }
        true
    }

    /// The line, counted from 1, that `at` is on in the module's file
    /// it is in.
    pub(super) fn line_of(&self, at: Location) -> usize {
        let declared = &self.program.modules[self.module];
        let Some(declaration) = declared
            .declarations
            .iter()
            .find(|declaration| declaration.at.file == at.file)
        else {
            return 0;
        };
        let root = declaration
            .syntax
            .ancestors()
            .last()
            .unwrap_or_else(|| declaration.syntax.clone());
        let text = root.text().to_string();
        let before = (at.span.start as usize).min(text.len());
        text.as_bytes()[..before]
            .iter()
            .filter(|byte| **byte == b'\n')
            .count()
            + 1
    }

    /// The wiring function of the temporal function `placed`, of
    /// `signature`, whose body is `body`: its prototype, for the module's
    /// header, in the namespace of the wiring functions, and its
    /// definition, which calls `wired` with the wiring and each of its
    /// parameters in order. `None` where a type is not written yet, which
    /// is reported.
    pub(super) fn wiring_function(
        &mut self,
        signature: &Signature,
        placed: &Placed,
        shown: &str,
        wired: &str,
        body: &Body,
    ) -> Option<(String, String)> {
        self.wiring_function_in_form(signature, placed, shown, body, (wired, None), &[])
    }

    /// Write the ordinary wiring function and the available result
    /// projections for a node or a source (ERR-15).
    pub(super) fn node_wiring(
        &mut self,
        signature: &Signature,
        placed: &Placed,
        shown: &str,
        body: &Body,
        forms: Option<&ResultForms>,
        watched: &[usize],
    ) -> Option<(String, String)> {
        let qualified = placed.qualified(self.program, self.module);
        let wired = format!("::hgraph::wire<{qualified}>");
        let (mut declarations, mut definitions) = self.wiring_function_in_form(
            signature,
            placed,
            shown,
            body,
            (&wired, forms.map(|forms| (forms, ResultForm::Whole))),
            watched,
        )?;
        if let Some(forms) = forms {
            for (form, ty) in [
                (ResultForm::Value, &forms.success),
                (ResultForm::Nullable, &forms.nullable),
            ] {
                let Some(ty) = ty else {
                    continue;
                };
                let mut selected = placed.clone();
                selected.name.push_str(form.suffix());
                let signature = Signature {
                    result: Some(ty.clone()),
                    ..signature.clone()
                };
                let (declaration, definition) = self.wiring_function_in_form(
                    &signature,
                    &selected,
                    shown,
                    body,
                    (&wired, Some((forms, form))),
                    watched,
                )?;
                declarations.push_str(&declaration);
                definitions.push_str(&definition);
            }
        }
        Some((declarations, definitions))
    }

    /// The wiring function that selects an output field of a fallible
    /// node, or wires a function with an ordinary output.
    fn wiring_function_in_form(
        &mut self,
        signature: &Signature,
        placed: &Placed,
        shown: &str,
        body: &Body,
        (wired, form): (&str, Option<(&ResultForms, ResultForm)>),
        watched: &[usize],
    ) -> Option<(String, String)> {
        let program = self.program;
        let mut parameters = vec![format!("::hgraph::Wiring& {}", super::graphs::WIRING)];
        let mut given = vec![super::graphs::WIRING.to_string()];
        let mut wired_arguments: Vec<(Option<String>, String)> = Vec::new();
        // A node's `const` parameters are its `hgraph::Scalar`s; a graph's
        // `compose` takes each as a value function takes it.
        let node = wired.starts_with("::hgraph::wire<");
        for (local, parameter) in signature.parameters.iter().enumerate() {
            let name = identifier(&body.locals[local].name);
            if parameter.constant {
                let ty = self.written_type(&parameter.ty, parameter.at)?;
                let taken = super::values::parameter_type(program, &parameter.ty, false, &ty);
                parameters.push(format!("{taken} {name}"));
                // FN-1: the value as the node's `hgraph::Scalar` holds it
                // (`configuration`).
                let configured = if node {
                    super::configuration::given(program, &parameter.ty, &ty, &name)
                } else {
                    name.clone()
                };
                wired_arguments.push((Some(parameter.name.clone()), configured.clone()));
                given.push(configured);
            } else {
                let schema = self.temporal_schema(&parameter.ty, parameter.at)?;
                parameters.push(format!("::hgraph::Port<{schema}> {name}"));
                wired_arguments.push((None, name.clone()));
                given.push(name);
            }
        }
        // FN-16: each input a `ref` result designates is given again, to
        // the input the node follows its re-binding through.
        for port in self.designated_ports(signature, body) {
            wired_arguments.push((None, port.clone()));
            given.push(port);
        }
        for &local in watched {
            let port = identifier(&body.locals[local].name);
            wired_arguments.push((None, port.clone()));
            given.push(port);
        }
        if let Some((_, form)) = form {
            let mode = format!("::hgraph::Int{{{}}}", form.mode());
            wired_arguments.push((Some(RESULT_FORM_LABEL.to_string()), mode.clone()));
            given.push(mode);
        }
        let schema = match &signature.result {
            Some(result) => Some(self.temporal_schema(result, placed.at)?),
            None => None,
        };
        let result = match &schema {
            Some(schema) => format!("::hgraph::Port<{schema}>"),
            None => "void".to_string(),
        };
        let gives = if signature.result.is_some() {
            "return "
        } else {
            ""
        };
        let prototype = format!("{result} {}({})", placed.name, parameters.join(", "));
        // hgraph issue 1866: a node whose configuration holds an `f64` is
        // wired as a node of its own (`unique`).
        let configured: Vec<&Type> = signature
            .parameters
            .iter()
            .filter(|parameter| parameter.constant)
            .map(|parameter| &parameter.ty)
            .collect();
        let called = match wired
            .strip_prefix("::hgraph::wire<")
            .and_then(|node| node.strip_suffix('>'))
        {
            Some(node) if configured.iter().any(|ty| holds_f64(program, ty, 0)) => {
                let node_schema =
                    form.map_or(schema.as_deref(), |(forms, _)| Some(forms.schema.as_str()));
                unique(node, node_schema, &wired_arguments)
            }
            _ => format!("{wired}({})", given.join(", ")),
        };
        let called = match form {
            Some((forms, form)) => {
                let index = forms.index(form);
                format!("::{}::field_port<{index}>({called})", runtime::NAMESPACE)
            }
            None => called,
        };
        // FN-16: "a `ref` result is a connection of T to whoever reads it".
        let called = self.result_port(signature, schema.as_deref(), called);
        let declaration = nested(&placed.wiring, &format!("// {shown}\n{prototype};\n"));
        let definition = nested(
            &placed.wiring,
            &format!("// {shown}\n{prototype}\n{{\n{INDENT}{gives}{called};\n}}\n"),
        );
        Some((declaration, definition))
    }

    /// Publish one value in the form the call takes. A failure first
    /// raises in this node, before any success output (ERR-5, ERR-15).
    pub(super) fn publish_result(
        &mut self,
        ty: &Type,
        value: &str,
        at: Location,
        indent: &str,
    ) -> String {
        let Some(forms) = self
            .temporal
            .as_ref()
            .and_then(|context| context.result_forms.clone())
        else {
            return format!("{indent}{}\n", self.temporal_publish(ty, OUT, value, at));
        };
        let held = names::temporary("outcome", self.temporary());
        let raw = self.temporal_publish(ty, OUT, &held, at);
        let mut text = format!("{indent}const auto {held} = {value};\n");
        for (form, ty) in [
            (ResultForm::Value, forms.success),
            (ResultForm::Nullable, forms.nullable),
        ] {
            let Some(ty) = ty else {
                continue;
            };
            let value = match form {
                ResultForm::Value => {
                    self.result_value(hgl_analyser::hir::Raise::CallNode, &held, at)
                }
                ResultForm::Nullable => format!("::{}::result_or_null({held})", runtime::NAMESPACE),
                ResultForm::Whole => unreachable!(),
            };
            let saved = names::temporary("published", self.temporary());
            let view = ResultForms::view(form);
            let publish = self.temporal_publish(&ty, &view, &saved, at);
            let mode = form.mode();
            text.push_str(&format!(
                "{indent}if ({RESULT_FORM}.value() == {mode})\n{indent}{{\n\
                 {indent}{INDENT}const auto {saved} = {value};\n\
                 {indent}{INDENT}{raw}\n{indent}{INDENT}{publish}\n{indent}}}\n{indent}else\n"
            ));
        }
        text.push_str(&format!("{indent}{{\n{indent}{INDENT}{raw}\n{indent}}}\n"));
        text
    }

    /// Invalidate the declared output and the field exposed to the
    /// consumer, when the output has contextual result forms (END-4).
    pub(super) fn invalidate_result(&self) -> Option<String> {
        let forms = self.temporal.as_ref()?.result_forms.as_ref()?;
        let invalidate = format!("::{}::fn_invalidate", runtime::NAMESPACE);
        let mut text = format!("[&] {{ {invalidate}({OUT});");
        for (form, projected) in [
            (ResultForm::Value, &forms.success),
            (ResultForm::Nullable, &forms.nullable),
        ] {
            if projected.is_some() {
                let view = ResultForms::view(form);
                let mode = form.mode();
                text.push_str(&format!(
                    " if ({RESULT_FORM}.value() == {mode}) {{ {invalidate}({view}); }}"
                ));
            }
        }
        text.push_str(" }()");
        Some(text)
    }
}

/// True where a value of `ty` holds an `f64`: is one, or holds one in a
/// tuple, a list, a set, a dictionary, an optional, a result, a delta, or
/// a field of a struct, at any depth up to the analyser's bound of a
/// type's depth.
pub(super) fn holds_f64(program: &hgl_analyser::Program, ty: &Type, depth: usize) -> bool {
    if depth > 32 {
        return true;
    }
    let inner = |ty: &Type| holds_f64(program, ty, depth + 1);
    match ty {
        Type::Scalar(hgl_analyser::Scalar::F64) => true,
        Type::Tuple(positions) => positions.iter().any(inner),
        Type::List { element, .. } => inner(element),
        Type::Set(element) => inner(element),
        Type::Dict { key, value } => inner(key) || inner(value),
        Type::Nullable(payload) | Type::Delta(payload) | Type::Atomic(payload) => inner(payload),
        Type::Result { value, error } => inner(value) || inner(error),
        Type::Named { .. } => hgl_lower::types::fields_of(program, ty)
            .is_some_and(|fields| fields.iter().any(|(_, field)| inner(field))),
        _ => false,
    }
}

/// The wiring of `node` as a node of its own, which hgraph does not
/// intern, given `arguments` in the order of its `eval`'s parameters:
/// each a port, or a value and the name of its scalar. Its output is of
/// `schema`, where it has one. An expression, a lambda called where it
/// is written.
///
/// hgraph issue 1866: hgraph takes two nodes for one where their scalar
/// configurations are equal under `==`, which holds of `0.0` and `-0.0`
/// though NUM-2 keeps them apart ("preserves IEEE special values,
/// including signed zero"). A node whose configuration holds an `f64` is
/// wired so until hgraph tells the two apart, as the runtime library
/// wires its constants (`hglrt::constant`).
pub(super) fn unique(
    node: &str,
    schema: Option<&str>,
    arguments: &[(Option<String>, String)],
) -> String {
    let wiring = super::graphs::WIRING;
    let mut text = format!(
        "[&]() {{\n{INDENT}{INDENT}// hgraph issue 1866: wired as a node of its own, since hgraph would take\n{INDENT}{INDENT}// two whose configurations hold `0.0` and `-0.0` for one.\n"
    );
    text.push_str(&format!(
        "{INDENT}{INDENT}::hgraph::Value scalars_{{::hgraph::value_type_for_wiring(::hgraph::StaticNodeSignature<{node}>::scalar_schema(), {wiring}.realization_options())}};\n"
    ));
    text.push_str(&format!("{INDENT}{INDENT}{{\n{INDENT}{INDENT}{INDENT}auto configured_ = scalars_.as_bundle().begin_mutation();\n"));
    for (label, value) in arguments {
        if let Some(label) = label {
            text.push_str(&format!(
                "{INDENT}{INDENT}{INDENT}configured_[{}].set({value});\n",
                names::string(label)
            ));
        }
    }
    text.push_str(&format!("{INDENT}{INDENT}}}\n"));
    text.push_str(&format!(
        "{INDENT}{INDENT}::hgraph::NodeBuilder builder_;\n{INDENT}{INDENT}builder_.implementation<{node}>();\n"
    ));
    let ports: Vec<String> = arguments
        .iter()
        .filter(|(label, _)| label.is_none())
        .map(|(_, port)| format!("{port}.erased()"))
        .collect();
    text.push_str(&format!(
        "{INDENT}{INDENT}const ::std::array<::hgraph::WiringPortRef, {}> inputs_{{{}}};\n",
        ports.len(),
        ports.join(", ")
    ));
    let added = format!(
        "{wiring}.add_unique_node(typeid({node}), ::std::move(builder_), ::std::span<const ::hgraph::WiringPortRef>{{inputs_}}, ::std::move(scalars_))"
    );
    match schema {
        Some(schema) => text.push_str(&format!(
            "{INDENT}{INDENT}return ::hgraph::Port<{schema}>{{{wiring}, {added}}};\n"
        )),
        None => text.push_str(&format!("{INDENT}{INDENT}static_cast<void>({added});\n")),
    }
    text.push_str(&format!("{INDENT}}}()"));
    text
}

/// What every hook of a node with `states` starts with: the node's
/// state, changed in place, and each `state` and `cache` bound to its
/// place in it, by its name.
fn prologue(body: &Body, states: &[(usize, Option<&Expr>)]) -> String {
    let indent = INDENT.repeat(2);
    let mut text = String::new();
    if !states.is_empty() {
        text.push_str(&format!(
            "{indent}state_type_& {KEPT} = {STATE}.modify();\n"
        ));
        for (local, _) in states {
            let name = identifier(&body.locals[*local].name);
            text.push_str(&format!(
                "{indent}[[maybe_unused]] auto& {name} = {KEPT}.{name};\n"
            ));
        }
    }
    text
}

/// The parameters of a hook, each `[[maybe_unused]]`: a hook need not
/// read every one hgraph gives it.
fn maybe_unused(parameters: &[String]) -> String {
    parameters
        .iter()
        .map(|parameter| format!("[[maybe_unused]] {parameter}"))
        .collect::<Vec<_>>()
        .join(", ")
}

/// The capability `inject name` binds (TIME-8), as the hook's hgraph
/// view of it and the library's type made from that: the endpoints'
/// (`clock`, `logger`, `global_state`), and the scheduler and the alarm
/// (`scheduling`). `None` for any other name.
fn capability(name: &str) -> Option<(&'static str, &'static str)> {
    super::endpoints::capability(name).or_else(|| super::scheduling::capability(name))
}

/// True where `statements` name `local`.
pub(super) fn uses(statements: &[Stmt], local: usize) -> bool {
    let mut found = false;
    hgl_analyser::hir::walk_statements(statements, &mut |value| {
        found |= matches!(value.kind, ExprKind::Local(named) if named == local);
    });
    found
}

/// True where `statements` name the node's `out`.
fn reads_out(context: &Context, statements: &[Stmt]) -> bool {
    let mut found = false;
    hgl_analyser::hir::walk_statements(statements, &mut |value| {
        found |= match &value.kind {
            ExprKind::Local(local) | ExprKind::EndpointOf(local) => {
                context.roles.get(*local) == Some(&Role::Out)
            }
            ExprKind::Endpoint {
                inputs: Some(inputs),
                ..
            } => inputs
                .iter()
                .any(|local| context.roles.get(*local) == Some(&Role::Out)),
            _ => false,
        };
    });
    found
}

/// `text` inside the namespace `namespace`, under the module's own.
pub(super) fn nested(namespace: &str, text: &str) -> String {
    format!("namespace {namespace}\n{{\n{text}}}  // namespace {namespace}\n")
}

/// The registration of `state`, a struct the generated code holds as a
/// node's state, with hgraph under the name `name` (TIME-6). hgraph holds
/// a state's value as a scalar of its type registry, which a
/// specialization of `hgraph::scalar_descriptor` at the global namespace
/// registers; it is written after the struct and before any wiring of
/// the node, so the module's namespace, which the unit wraps every piece
/// in (`assemble`), is closed around it and opened again.
pub(super) fn registration(
    program: &hgl_analyser::Program,
    module: usize,
    state: &str,
    name: &str,
) -> String {
    let namespace = names::module_namespace(&program.modules[module].path);
    let namespace = namespace.trim_start_matches("::");
    format!(
        "}}  // namespace {namespace}\n\n\
         // The state of the node, as hgraph holds it (TIME-6).\n\
         template <>\n\
         struct hgraph::scalar_descriptor<{state}>\n{{\n\
         {INDENT}[[nodiscard]] static constexpr bool is_concrete() noexcept {{ return true; }}\n\
         {INDENT}[[nodiscard]] static const ValueTypeMetaData* value_meta()\n{INDENT}{{\n\
         {INDENT}{INDENT}return TypeRegistry::instance().register_scalar<{state}>({});\n\
         {INDENT}}}\n}};\n\n\
         namespace {namespace}\n{{\n",
        names::string(&format!("hgl:{name}"))
    )
}

/// Where a temporal function, or an instance of one, is written: its
/// struct is [`IMPLEMENTATION`] in the namespace `structs` under the
/// module's, and its wiring function `name` in `wiring`.
#[derive(Clone)]
pub(super) struct Placed {
    /// The namespace of its struct, under the module's, named as the
    /// function: `n_::count`, or `n_::i_::record::of_i64` for an
    /// instance.
    pub(super) structs: String,
    /// The namespace of its wiring function, under the module's: `w_`,
    /// or `w_::i_::record`.
    pub(super) wiring: String,
    /// The name of both: `count`, or `of_i64` for an instance.
    pub(super) name: String,
    /// Its declaration's HGL name.
    hgl: String,
    /// The suffix of its instance, where it is one.
    instance: Option<String>,
    /// Where its declaration is.
    pub(super) at: Location,
}

impl Placed {
    /// Distinguish a physical reader variant without changing the
    /// node identity that a failure reports (ERR-4, ERR-15).
    pub(super) fn add_input_variant(&mut self, number: usize) {
        let suffix = names::reader_suffix(number);
        self.name.push_str(&suffix);
        self.structs.push_str(&suffix);
    }

    /// Where the temporal function `declaration` of `module`, or its
    /// instance `instance`, is written. `n_` and `w_` have the form the
    /// target's own names have, which no HGL name gives (`names`).
    pub(super) fn of(
        program: &hgl_analyser::Program,
        module: usize,
        declaration: usize,
        instance: Option<&str>,
    ) -> Placed {
        let (namespace, name) = names::placed(program, module, declaration, instance);
        let under = |root: &str| match &namespace {
            Some(namespace) => format!("{root}::{namespace}"),
            None => root.to_string(),
        };
        Placed {
            structs: format!("{}::{name}", under("n_")),
            wiring: under("w_"),
            name,
            hgl: program.modules[module].declarations[declaration]
                .name
                .clone(),
            instance: instance.map(str::to_string),
            at: program.modules[module].declarations[declaration].at,
        }
    }

    /// Its struct, qualified from the global namespace.
    pub(super) fn qualified(&self, program: &hgl_analyser::Program, module: usize) -> String {
        format!(
            "{}::{}::{IMPLEMENTATION}",
            names::module_namespace(&program.modules[module].path),
            self.structs
        )
    }

    /// Its wiring function, qualified from the global namespace.
    pub(super) fn wiring_qualified(
        &self,
        program: &hgl_analyser::Program,
        module: usize,
    ) -> String {
        format!(
            "{}::{}::{}",
            names::module_namespace(&program.modules[module].path),
            self.wiring,
            self.name
        )
    }

    /// The name hgraph gives its node: the module, the HGL name, and the
    /// instance's suffix, `core::record[i64]`. A node's display name is
    /// not a stable code (ERR-4).
    pub(super) fn display(&self, program: &hgl_analyser::Program, module: usize) -> String {
        let path = &program.modules[module].path;
        match &self.instance {
            Some(suffix) => format!("{path}::{}[{suffix}]", self.hgl),
            None => format!("{path}::{}", self.hgl),
        }
    }
}

/// The nodes' family: the statements of a hook and of a lifted call's
/// node that publish (TIME-7).
pub(super) struct Nodes;

impl Family for Nodes {
    fn statement(
        &self,
        emitter: &mut Emitter<'_>,
        scope: &mut Scope<'_>,
        statement: &Stmt,
        depth: usize,
        text: &mut String,
    ) -> bool {
        // A `return` written inside a value, the block a lambda gives
        // (`expressions`), is the value's own, and publishes nothing:
        // `conditional` marks it.
        let publishing = matches!(emitter.kind(), Some(Kind::Hook(Hook::When) | Kind::Lifted))
            && scope.conditional == 0;
        let indent = INDENT.repeat(depth);
        match statement {
            // FN-7: "a runtime return publishes and exits the function's
            // current evaluation, including any later `when` handlers".
            Stmt::Return {
                value: Some(value),
                at,
            } if publishing => {
                let Some(result) = emitter.temporal.as_ref().and_then(|c| c.result.clone()) else {
                    emitter.unsupported(*at, "a `return` with a value here");
                    return true;
                };
                let written = emitter.expression(scope, value);
                text.push_str(&emitter.publish_result(&result, &written, *at, &indent));
                text.push_str(&format!("{indent}return;\n"));
                true
            }
            // TIME-7: `out = value` publishes, and the evaluation goes on.
            Stmt::Assign {
                local,
                value,
                at,
                operator,
            } if emitter.role(*local) == Role::Out => {
                if operator.is_some() || !publishing {
                    emitter.unsupported(*at, "this assignment to `out`");
                    return true;
                }
                let Some(result) = emitter.temporal.as_ref().and_then(|c| c.result.clone()) else {
                    emitter.unsupported(*at, "this assignment to `out`");
                    return true;
                };
                let written = emitter.expression(scope, value);
                text.push_str(&emitter.publish_result(&result, &written, *at, &indent));
                true
            }
            // TIME-7: "admitted piecewise mutation" of `out` is the
            // collections' work.
            Stmt::AssignPlace { place, at, .. } if matches!(place.place_root().kind, ExprKind::Local(local) if emitter.role(local) == Role::Out) =>
            {
                emitter.unsupported(*at, "an assignment to a part of `out`");
                true
            }
            _ => false,
        }
    }
}
