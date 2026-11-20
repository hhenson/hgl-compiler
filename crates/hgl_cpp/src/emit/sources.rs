//! Writes a source (SOURCE-1: "A temporal `fn` whose body has a `yield`
//! is a source") as an hgraph static node whose body is a coroutine.
//! Part of the transpiler, the last stage; a family of the dispatch
//! point (`forms`) for the statements of a source's body.
//!
//! | HGL | C++ |
//! | --- | --- |
//! | The body, its statements and `yield` among them | a static member `body_` of the node's struct, an `::hglrt::generator` of `::hglrt::Yielded<T>`, given the source's `const` parameters by value: no hgraph view is held across a `yield` |
//! | `yield time: value` (SOURCE-2) | `co_yield yielded_type_{time, value};`, the time evaluated first, then the payload |
//! | A bare `return`, the body's end (SOURCE-4) | `co_return;`, the coroutine's end |
//! | Its node | `start` makes the body, held in the node's state with where it is; `eval` steps it with `::hglrt::resume`, which publishes what is due and asks the node's scheduler for the next target (SOURCE-2, SOURCE-3); `stop` destroys it, and the locals in its frame |
//!
//! What a source's declarations and hooks (`inject`, `cache`, `state`,
//! `start`, `stop`; SOURCE-1) add to its node is `scheduling`'s: the
//! parts it gives are written in their places here.

use hgl_analyser::hir::{Body, Stmt};
use hgl_analyser::{Signature, Type};

use super::Emitter;
use super::forms::Family;
use super::names::{self, identifier};
use super::nodes::{Context, IMPLEMENTATION, Kind, OUT, Placed, nested, registration};
use super::statements::Scope;
use super::values::INDENT;
use crate::runtime;

/// The sources' family: `yield` and `return` in a source's body.
pub(super) struct Sources;

impl Family for Sources {
    fn statement(
        &self,
        emitter: &mut Emitter<'_>,
        scope: &mut Scope<'_>,
        statement: &Stmt,
        depth: usize,
        text: &mut String,
    ) -> bool {
        // A `return` written inside a value, the block a lambda gives
        // (`expressions`), is the value's own: `conditional` marks it.
        if emitter.kind() != Some(Kind::Source) || scope.conditional > 0 {
            return false;
        }
        let indent = INDENT.repeat(depth);
        match statement {
            // SOURCE-2: "Yield evaluates its time once, then payload once,
            // before target validation": a braced list is evaluated in the
            // order written.
            Stmt::Yield { time, value, .. } => {
                let time = emitter.expression(scope, time);
                let value = emitter.expression(scope, value);
                text.push_str(&format!(
                    "{indent}co_yield yielded_type_{{{time}, {value}}};\n"
                ));
                true
            }
            // SOURCE-4: "A bare return or body completion finishes the
            // source."
            Stmt::Return { value: None, .. } => {
                text.push_str(&format!("{indent}co_return;\n"));
                true
            }
            Stmt::Return { at, .. } => {
                emitter.unsupported(*at, "a `return` with a value in a source");
                true
            }
            _ => false,
        }
    }
}

impl Emitter<'_> {
    /// Writes the source `index`, or its instance `instance`, of
    /// `signature` and `body`, which `shown` describes.
    pub(super) fn source(
        &mut self,
        index: usize,
        instance: Option<&str>,
        signature: &Signature,
        body: &Body,
        shown: &str,
    ) {
        let program = self.program;
        let at = program.modules[self.module].declarations[index].at;
        // SOURCE-1: what the source's declarations and hooks add to its
        // node (`scheduling`): nothing for one that has none.
        let Some(hooked) = self.source_hooks(signature, body) else {
            return;
        };
        let Some(result) = &signature.result else {
            self.unsupported(at, "a source with no result");
            return;
        };
        // TIME-7: a `yield` gives "a complete value of the result's type
        // T, published whole, or a `delta<T>`": the body's yields give what
        // their values are, which is T's delta where every one is a delta
        // and T's is no T, as core's `replay` yields.
        let mut yielded: Vec<Type> = Vec::new();
        hgl_analyser::hir::walk_statement_nodes(&body.statements, &mut |node| {
            if let hgl_analyser::hir::Node::Statement(Stmt::Yield { value, .. }) = node
                && !yielded.contains(&value.ty)
            {
                yielded.push(value.ty.clone());
            }
        });
        let delta = Type::delta(result.clone());
        let given = match yielded.as_slice() {
            [only] if *only == delta => &delta,
            _ => result,
        };
        if delta != *result && yielded.contains(&delta) && yielded.len() > 1 {
            self.unsupported(at, "a source that yields both a whole value and a delta");
            return;
        }
        let payload = if given == result {
            self.payload_type(result, at)
        } else {
            self.written_type(given, at)
        };
        let Some(payload) = payload else {
            return;
        };
        let Some(schema) = self.temporal_schema(result, at) else {
            return;
        };
        // SOURCE-1: a source "has no temporal parameters"; the body takes
        // each `const` one by value, which its frame then holds.
        let mut taken = hooked.taken.clone();
        let mut views = Vec::new();
        let mut given = hooked.given.clone();
        for (local, parameter) in signature.parameters.iter().enumerate() {
            let name = identifier(&body.locals[local].name);
            let Some(ty) = self.written_type(&parameter.ty, parameter.at) else {
                return;
            };
            taken.push(format!("[[maybe_unused]] {ty} {name}"));
            views.push(format!(
                "::hgraph::Scalar<{}, {ty}> {name}",
                names::string(&parameter.name)
            ));
            given.push(format!("{name}.value()"));
        }
        let mut context = Context::new(Kind::Source, body.locals.len());
        context.result = Some(result.clone());
        let mut scope = Scope::cold(program, body);
        let mut statements = hooked.prologue.clone();
        self.in_context(&mut context, |emitter| {
            emitter.statements(&mut scope, &hooked.yielding, 2, &mut statements);
        });
        let placed = Placed::of(program, self.module, index, instance);
        let qualified = placed.qualified(program, self.module);
        let published = self.temporal_publish(result, OUT, "value_", at);
        let library = runtime::NAMESPACE;
        let state = "::hgraph::State<state_type_> state_".to_string();
        let mut start = views.clone();
        start.push(state.clone());
        start.extend(hooked.start_parameters.iter().cloned());
        let mut eval: Vec<String> = views
            .iter()
            .map(|view| format!("[[maybe_unused]] {view}"))
            .collect();
        eval.push(state.clone());
        eval.push("::hgraph::DateTime now_".to_string());
        eval.push("::hgraph::NodeScheduler scheduler_".to_string());
        eval.push(format!("::hgraph::Out<{schema}> {OUT}"));
        eval.extend(hooked.eval_parameters.iter().cloned());
        let mut stop = vec![state.clone()];
        stop.extend(hooked.stop_parameters.iter().cloned());
        let text = format!(
            "// {shown}\n\
             struct {IMPLEMENTATION}\n{{\n\
             {INDENT}static constexpr auto name = {display};\n\
             {INDENT}// SOURCE-2: \"The first evaluation begins the body\".\n\
             {INDENT}static constexpr bool schedule_on_start = true;\n\
             {INDENT}using yielded_type_ = ::{library}::Yielded<{payload}>;\n\
             {INDENT}using body_type_ = ::{library}::generator<yielded_type_>;\n\n\
             {declared}\
             {INDENT}// The body (SOURCE-1), given its configuration by value.\n\
             {INDENT}static body_type_ body_({taken})\n{INDENT}{{\n{statements}{INDENT}}}\n\n\
             {INDENT}// The node's state (TIME-6): the body, where it is, and the\n\
             {INDENT}// timing of its yields.\n\
             {INDENT}struct state_type_\n{INDENT}{{\n\
             {INDENT}{INDENT}::{library}::Source<{payload}> source_{{}};\n\
             {kept}\
             {INDENT}{INDENT}std::optional<body_type_> frame_{{}};\n\
             {INDENT}{INDENT}std::optional<body_type_::iterator> at_{{}};\n\
             {INDENT}}};\n\n\
             {INDENT}static void start({start})\n{INDENT}{{\n\
             {INDENT}{INDENT}state_.set(state_type_{{}});\n\
             {INDENT}{INDENT}state_type_& kept_ = state_.modify();\n\
             {start_before}\
             {INDENT}{INDENT}kept_.frame_.emplace(body_({given}));\n\
             {start_after}\
             {INDENT}}}\n\n\
             {INDENT}// SOURCE-3: what is due now is published, and the node asks for\n\
             {INDENT}// a wake at the next target.\n\
             {INDENT}static void eval({eval})\n{INDENT}{{\n\
             {INDENT}{INDENT}state_type_& kept_ = state_.modify();\n\
             {eval_before}\
             {INDENT}{INDENT}::{library}::resume(\n\
             {INDENT}{INDENT}{INDENT}kept_.source_, now_, scheduler_,\n\
             {INDENT}{INDENT}{INDENT}[&kept_]() -> std::optional<yielded_type_> {{\n\
             {INDENT}{INDENT}{INDENT}{INDENT}if (!kept_.at_.has_value())\n\
             {INDENT}{INDENT}{INDENT}{INDENT}{{\n\
             {INDENT}{INDENT}{INDENT}{INDENT}{INDENT}kept_.at_.emplace(kept_.frame_->begin());\n\
             {INDENT}{INDENT}{INDENT}{INDENT}}}\n\
             {INDENT}{INDENT}{INDENT}{INDENT}else\n\
             {INDENT}{INDENT}{INDENT}{INDENT}{{\n\
             {INDENT}{INDENT}{INDENT}{INDENT}{INDENT}++*kept_.at_;\n\
             {INDENT}{INDENT}{INDENT}{INDENT}}}\n\
             {INDENT}{INDENT}{INDENT}{INDENT}if (*kept_.at_ == std::default_sentinel)\n\
             {INDENT}{INDENT}{INDENT}{INDENT}{{\n\
             {INDENT}{INDENT}{INDENT}{INDENT}{INDENT}return std::nullopt;\n\
             {INDENT}{INDENT}{INDENT}{INDENT}}}\n\
             {INDENT}{INDENT}{INDENT}{INDENT}return std::optional<yielded_type_>{{**kept_.at_}};\n\
             {INDENT}{INDENT}{INDENT}}},\n\
             {INDENT}{INDENT}{INDENT}[&{OUT}]({payload} value_) {{ {published} }});\n\
             {INDENT}}}\n\n\
             {INDENT}// SOURCE-4: the body's frame, and the locals in it, end with the run.\n\
             {INDENT}static void stop({stop})\n{INDENT}{{\n\
             {INDENT}{INDENT}state_type_& kept_ = state_.modify();\n\
             {INDENT}{INDENT}kept_.at_.reset();\n\
             {INDENT}{INDENT}kept_.frame_.reset();\n\
             {stop_after}\
             {INDENT}}}\n\
             }};\n",
            display = names::string(&placed.display(program, self.module)),
            taken = taken.join(", "),
            start = start.join(", "),
            eval = eval.join(", "),
            stop = stop.join(", "),
            given = given.join(", "),
            declared = hooked.declared,
            kept = hooked.kept,
            start_before = hooked.start_before,
            start_after = hooked.start_after,
            eval_before = hooked.eval_before,
            stop_after = hooked.stop_after,
        );
        let mut whole = nested(&placed.structs, &text);
        whole.push_str(&registration(
            program,
            self.module,
            &format!("{qualified}::state_type_"),
            &format!("{}.state", placed.display(program, self.module)),
        ));
        let Some((prototype, wiring)) = self.wiring_function(
            signature,
            &placed,
            shown,
            &format!("::hgraph::wire<{qualified}>"),
            body,
        ) else {
            return;
        };
        whole.push_str(&wiring);
        self.piece.declaration = prototype;
        self.piece.text = whole;
    }
}
