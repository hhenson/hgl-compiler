//! Python rendering of the common graph and callback operations. Core's usual
//! replay/record instances still run each eval and compare its publications.
use super::*;
use hgl_lower::{Action, Case, Graph, Kernel, Literal, Node, Port, Schema, Value as V};
use std::fmt::Write as _;

fn payload(ty: Schema) -> &'static str {
    match ty {
        Schema::Integer => "_hg.TS[_py.int]",
        Schema::Text => "_hg.TS[_py.str]",
        Schema::Flag => "_hg.TS[_py.bool]",
        Schema::Dictionary => "_hg.TSD[_py.str, _hg.TS[_py.int]]",
    }
}
fn literal(v: &Literal) -> String {
    match v {
        Literal::Int(i) => i.to_string(),
        Literal::Text(s) => string(s),
        Literal::Bool(b) => if *b { "True" } else { "False" }.into(),
    }
}
fn val(v: &V) -> String {
    match v {
        V::Literal(v) => literal(v),
        V::State => "_state.cell".into(),
        V::Local(i) => format!("local{i}"),
        V::Input(i, _) => format!("p{i}.value"),
        V::Delta(i, _) => format!("p{i}.delta_value"),
        V::Has(k) => format!("_core.has_GlobalState_str(_store, {})", val(k)),
        V::Get(k) => format!("_core.get_GlobalState_str(_store, {})", val(k)),
        V::Add(a, b) => format!("_core.add__i64_i64({}, {})", val(a), val(b)),
        V::Equal(a, b) => format!("({} == {})", val(a), val(b)),
        V::Less(a, b) => format!("({} < {})", val(a), val(b)),
        V::Choose(c, a, b) => format!("({} if {} else {})", val(a), val(c), val(b)),
    }
}
fn actions(text: &mut String, list: &[Action], depth: usize) {
    let indent = "    ".repeat(depth);
    if list.is_empty() {
        writeln!(text, "{indent}pass").unwrap();
    }
    for a in list {
        let line = match a {
            Action::Admit(valid, modified, body) => {
                let mut terms = valid
                    .iter()
                    .map(|i| format!("p{i}.valid"))
                    .collect::<Vec<_>>();
                terms.push(if modified.is_empty() {
                    "False".into()
                } else {
                    format!(
                        "({})",
                        modified
                            .iter()
                            .map(|i| format!("p{i}.modified"))
                            .collect::<Vec<_>>()
                            .join(" or ")
                    )
                });
                writeln!(text, "{indent}if {}:", terms.join(" and ")).unwrap();
                actions(text, body, depth + 1);
                continue;
            }
            Action::If(c, body) => {
                writeln!(text, "{indent}if {}:", val(c)).unwrap();
                actions(text, body, depth + 1);
                continue;
            }
            Action::Let(i, v) => format!("local{i} = {}", val(v)),
            Action::Set(v) => format!("_state.cell = {}", val(v)),
            Action::Put(k, v) => format!(
                "_core.put_GlobalState_str_V(_store, {}, {})",
                val(k),
                val(v)
            ),
            Action::Publish(v) => format!("return {}", val(v)),
            Action::Update(k, v) => format!(
                "return _core.entries_published(_output, _core.dict_delta(updated={{{}: {}}}), \"entries\")",
                val(k),
                val(v)
            ),
            Action::Remove(k) => format!(
                "return _core.entries_published(_output, _core.dict_delta(removed=({},)), \"entries\")",
                val(k)
            ),
        };
        writeln!(text, "{indent}{line}").unwrap();
    }
}
fn node(text: &mut String, name: &str, k: &Kernel) {
    let mut params = k
        .inputs
        .iter()
        .enumerate()
        .map(|(i, t)| format!("p{i}: {}", payload(*t)))
        .collect::<Vec<_>>();
    if k.state {
        params.push("_state: _hg.STATE = None".into());
    }
    params.push("_store: _hg.GlobalState = None".into());
    if k.output == Some(Schema::Dictionary) {
        let annotation = output_annotation(payload(Schema::Dictionary));
        params.push(format!("_output: {annotation} = None"));
    }
    let signature = params.join(", ");
    let (kind, result) = k.output.map_or(("sink_node", String::new()), |t| {
        ("compute_node", format!(" -> {}", payload(t)))
    });
    writeln!(
        text,
        "\n\n@_hg.{kind}(valid=())\ndef {name}({signature}){result}:"
    )
    .unwrap();
    actions(text, &k.eval, 1);
    for (phase, body) in [("start", &k.start), ("stop", &k.stop)] {
        if !body.is_empty() {
            let signature = if phase == "start" {
                params[k.inputs.len()..].join(", ")
            } else {
                signature.clone()
            };
            writeln!(
                text,
                "\n\n@{name}.{phase}\ndef {name}_{phase}({signature}):"
            )
            .unwrap();
            actions(text, body, 1);
        }
    }
}
fn port(p: Port) -> String {
    match p {
        Port::Node(i) => format!("n{i}"),
        Port::Key => "key".into(),
        Port::Element => "element".into(),
    }
}
fn graph(text: &mut String, name: &str, g: &Graph, nested: bool, application: bool) {
    let mut params = Vec::new();
    let mut lines = Vec::new();
    if nested {
        params.extend([
            "key: _hg.TS[_py.str]".into(),
            "element: _hg.TS[_py.int]".into(),
        ]);
    }
    for (i, n) in g.nodes.iter().enumerate() {
        let local = format!("{name}_{i}");
        match n {
            Node::Replay(t, events) if application => {
                writeln!(
                    text,
                    "\n\n@_hg.generator\ndef {local}() -> {}:",
                    payload(*t)
                )
                .unwrap();
                for (offset, v) in events {
                    writeln!(
                        text,
                        "    yield _hg.MIN_ST + _dt.timedelta(microseconds={offset}), {}",
                        literal(v)
                    )
                    .unwrap();
                }
                if events.is_empty() {
                    writeln!(text, "    yield from ()").unwrap();
                }
                lines.push(format!("n{i} = {local}()"));
            }
            Node::Replay(t, _) => params.push(format!("n{i}: {}", payload(*t))),
            Node::Call(k, ports) => {
                node(text, &local, k);
                let inputs = ports
                    .iter()
                    .copied()
                    .map(port)
                    .collect::<Vec<_>>()
                    .join(", ");
                lines.push(format!("n{i} = {local}({inputs})"));
            }
            Node::Each(over, child) => {
                graph(text, &local, child, true, application);
                lines.push(format!(
                    "_hg.map_({local}, element={}, __key_arg__=\"key\")",
                    port(*over)
                ));
            }
        }
    }
    if let Some(p) = g.result {
        lines.push(format!("return {}", port(p)));
    }
    let result = if g.result.is_some() {
        " -> _hg.TS[_py.int]"
    } else {
        ""
    };
    writeln!(
        text,
        "\n\n@_hg.graph\ndef {name}({}){result}:",
        params.join(", ")
    )
    .unwrap();
    if lines.is_empty() {
        lines.push("pass".into());
    }
    for line in lines {
        writeln!(text, "    {line}").unwrap();
    }
}
impl Emitter<'_> {
    pub(super) fn lowered_application(&mut self, g: &Graph, at: Location) -> String {
        self.harness = true;
        self.hgraph = true;
        self.datetime = true;
        let previous = self.prelude.len();
        graph(&mut self.prelude, "_lowered_entry", g, false, true);
        self.natives |= self.prelude[previous..].contains("_core.");
        let recorder = if g.result.is_some() {
            match crate::instances::core_function(self.program, "record") {
                Some(id) => {
                    self.called_name(id, &[GenericArgument::Type(Type::Scalar(Scalar::I64))], at)
                }
                None => {
                    self.unsupported(at, "a run with no record in core");
                    "None".into()
                }
            }
        } else {
            "None".into()
        };
        format!("_harness.run(_lowered_entry, {recorder}, False)")
    }

    pub(super) fn can_render_lowered(&self, cases: &[Case]) -> bool {
        let available = |name: &str, ty: Scalar| {
            crate::instances::core_function(self.program, name).is_some_and(|id| {
                self.instances
                    .find(self.program, id, &[GenericArgument::Type(Type::Scalar(ty))])
                    .is_some()
            })
        };
        available("record", Scalar::I64)
            && cases.iter().all(|case| {
                case.graph.nodes.iter().all(|node| match node {
                    Node::Replay(t, _) => available(
                        "replay",
                        match t {
                            Schema::Integer => Scalar::I64,
                            Schema::Flag => Scalar::Bool,
                            Schema::Text => Scalar::Str,
                            Schema::Dictionary => return false,
                        },
                    ),
                    _ => true,
                })
            })
    }
    pub(super) fn lowered_test(&mut self, index: usize, cases: &[Case]) {
        self.harness = true;
        self.hgraph = true;
        self.datetime = true;
        let record = crate::instances::core_function(self.program, "record").unwrap();
        let replay = crate::instances::core_function(self.program, "replay").unwrap();
        for (i, c) in cases.iter().enumerate() {
            let name = format!("_lowered_{index}_{i}");
            let previous = self.prelude.len();
            graph(&mut self.prelude, &name, &c.graph, false, false);
            self.natives |= self.prelude[previous..].contains("_core.");
            let recorder = self.called_name(
                record,
                &[GenericArgument::Type(Type::Scalar(Scalar::I64))],
                c.at,
            );
            let mut sources = vec![];
            let mut args = vec![];
            for (slot, n) in c.graph.nodes.iter().enumerate() {
                if let Node::Replay(ty, events) = n {
                    let scalar = match ty {
                        Schema::Integer => Scalar::I64,
                        Schema::Text => Scalar::Str,
                        Schema::Flag => Scalar::Bool,
                        Schema::Dictionary => unreachable!(),
                    };
                    let source = self.called_name(
                        replay,
                        &[GenericArgument::Type(Type::Scalar(scalar))],
                        c.at,
                    );
                    sources.push(format!("\"n{slot}\": {source}"));
                    args.push(format!(
                        "n{slot}={}",
                        timed(events.iter().map(|(s, v)| (*s, literal(v))))
                    ));
                }
            }
            let mut given = vec![name, format!("{{{}}}", sources.join(", ")), recorder];
            given.extend(args);
            let expected = timed(c.expected.iter().map(|(s, i)| (*s, i.to_string())));
            let line = self.text.matches('\n').count();
            self.piece.statements.push((line, c.at));
            self.line(
                1,
                &format!(
                    "if not _harness.same(_harness.evaluate({}), {expected}):",
                    given.join(", ")
                ),
            );
            self.line(2, "raise _py.AssertionError()");
        }
    }
}
fn timed(events: impl Iterator<Item = (i64, String)>) -> String {
    let entries = events
        .map(|(s, v)| format!("(_dt.timedelta(microseconds={s}), {v})"))
        .collect::<Vec<_>>()
        .join(", ");
    format!("_harness.timed([{entries}])")
}
