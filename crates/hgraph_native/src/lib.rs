//! Rust transpiler for HGraph. The application builds graphs; a separate
//! generated library owns the concrete callbacks. Neither reads analysed HIR.
use hgl_lower::{Action, Case, Graph, Kernel, Literal, Node, Port, Schema, Value};
use std::fmt::Write;

/// A named execution root in one generated application workspace.
pub enum Entry {
    /// Selected assertions in declaration order.
    Test(String, Vec<Case>),
    /// A finite application simulation.
    Application(String, Graph),
}
const MANIFEST: &str = r#"[workspace]
members = ["nodes","runtime"]
resolver = "3"

[package]
name = "hgl_app"
version = "0.1.0"
edition = "2024"
publish = false

[dependencies]
hgl_nodes = { path = "nodes" }
hgl_native_runtime = { path = "runtime" }

[profile.dev.package.hgl_native_runtime]
opt-level = 3
"#;
const NODES: &str = r#"[package]
name = "hgl_nodes"
version = "0.1.0"
edition = "2024"
publish = false

[dependencies]
hgl_native_runtime = { path = "../runtime" }
"#;
const LINK: &str = r#"fn main() {
    println!("cargo::rerun-if-changed=build.rs");
    let dir=std::env::var("DEP_HGL_BRIDGE_LIBDIR").expect("runtime ABI location");
    if std::env::var("CARGO_CFG_TARGET_FAMILY").as_deref()==Ok("unix") {
        println!("cargo::rustc-link-arg=-Wl,-rpath,{dir}");
    }
}
"#;
/// Emit a self-contained Cargo workspace, including the stable facade source.
pub fn emit(entries: &[Entry]) -> Vec<(String, String)> {
    let mut kernels = Vec::new();
    let mut app = String::from("use hgl_native_runtime as rt;\n");
    let mut dispatch = String::new();
    let mut id = 0;
    for entry in entries {
        let name = match entry {
            Entry::Test(n, _) | Entry::Application(n, _) => n,
        };
        writeln!(dispatch, "        {name:?} => {{").unwrap();
        match entry {
            Entry::Test(_, cases) => {
                for (i, c) in cases.iter().enumerate() {
                    wire(&mut app, &c.graph, id, &mut kernels);
                    writeln!(dispatch, "            let actual = rt::run(wire_{id})?;").unwrap();
                    writeln!(
                        dispatch,
                        "            let expected: &[(i64,i64)] = &{:?};",
                        c.expected
                    )
                    .unwrap();
                    writeln!(dispatch,"            if actual != expected {{return Err(format!(\"assertion {} expected {{expected:?}}, recorded {{actual:?}}\"));}}",i+1).unwrap();
                    id += 1;
                }
            }
            Entry::Application(_, g) => {
                wire(&mut app, g, id, &mut kernels);
                writeln!(dispatch,"            let trace = rt::run(wire_{id})?;\n            for &(step, value) in &trace {{rt::print_publication(step,value);}}\n            println!(\"ran {{name}} on hgraph-native: {{}} publication{{}}\",trace.len(),if trace.len()==1 {{\"\"}} else {{\"s\"}});").unwrap();
                id += 1;
            }
        }
        dispatch.push_str("        }\n");
    }
    writeln!(app,"fn main() -> Result<(), String> {{\n    let name=std::env::args().nth(1).ok_or(\"expected a fully qualified test or application name\")?;\n    match name.as_str() {{\n{dispatch}        _=>return Err(format!(\"unknown execution root {{name}}\")),\n    }}\n    Ok(())\n}}").unwrap();
    let mut nodes = String::new();
    if !kernels.is_empty() {
        nodes.push_str("use hgl_native_runtime::Frame;\n");
    }
    for (i, k) in kernels.iter().enumerate() {
        for (phase, actions) in [("start", &k.start), ("eval", &k.eval), ("stop", &k.stop)] {
            if actions.is_empty() && phase != "eval" {
                continue;
            }
            writeln!(nodes,"// {}\npub extern \"C\" fn n{i}_{phase}(f: &Frame) -> i32 {{\n    fn body(f: &Frame) -> Result<(),i32> {{",k.name).unwrap();
            actions_rust(&mut nodes, actions, k.output, 2);
            if !matches!(
                actions.last(),
                Some(Action::Publish(_) | Action::Remove(_) | Action::Update(_, _))
            ) {
                nodes.push_str("        Ok(())\n");
            }
            nodes.push_str("    }\n    body(f).err().unwrap_or(0)\n}\n");
        }
    }
    let mut files: Vec<_> = [
        ("Cargo.toml", MANIFEST),
        (
            "rust-toolchain.toml",
            include_str!("../../../rust-toolchain.toml"),
        ),
        ("nodes/Cargo.toml", NODES),
        ("build.rs", LINK),
        (
            "runtime/Cargo.toml",
            include_str!("../../../runtime/hgraph_native/Cargo.toml"),
        ),
        (
            "runtime/build.rs",
            include_str!("../../../runtime/hgraph_native/build.rs"),
        ),
        (
            "runtime/lib.rs",
            include_str!("../../../runtime/hgraph_native/lib.rs"),
        ),
        (
            "runtime/bridge.cpp",
            include_str!("../../../runtime/hgraph_native/bridge.cpp"),
        ),
        (
            "runtime/CMakeLists.txt",
            include_str!("../../../runtime/hgraph_native/CMakeLists.txt"),
        ),
    ]
    .into_iter()
    .map(|(p, s)| (p.into(), s.into()))
    .collect();
    files.push(("src/main.rs".into(), app));
    files.push(("nodes/src/lib.rs".into(), nodes));
    files
}
fn wire(out: &mut String, g: &Graph, id: usize, kernels: &mut Vec<Kernel>) {
    writeln!(out, "extern \"C\" fn wire_{id}(p: &mut rt::Plan) {{").unwrap();
    graph(out, g, false, kernels);
    if let Some(Port::Node(i)) = g.result {
        writeln!(out, "    p.record(n{i});").unwrap();
    }
    out.push_str("}\n");
}
fn graph(out: &mut String, g: &Graph, child: bool, kernels: &mut Vec<Kernel>) {
    let used: std::collections::BTreeSet<_> = g
        .nodes
        .iter()
        .flat_map(|n| match n {
            Node::Call(_, ports) => ports.as_slice(),
            Node::Each(port, _) => std::slice::from_ref(port),
            Node::Replay(..) => &[],
        })
        .chain(g.result.iter())
        .filter_map(|p| match p {
            Port::Node(id) => Some(*id),
            _ => None,
        })
        .collect();
    for (i, n) in g.nodes.iter().enumerate() {
        match n {
            Node::Replay(t, events) => {
                let events = events
                    .iter()
                    .map(|(s, v)| {
                        format!(
                            "({s}, rt::Scalar::{})",
                            match v {
                                Literal::Int(i) => format!("Integer({i})"),
                                Literal::Text(s) => format!("Text({s:?})"),
                                Literal::Bool(b) => format!("Flag({b})"),
                            }
                        )
                    })
                    .collect::<Vec<_>>()
                    .join(", ");
                let binding = if used.contains(&i) {
                    format!("let n{i} = ")
                } else {
                    String::new()
                };
                writeln!(out, "    {binding}p.replay(rt::Type::{t:?}, &[{events}]);").unwrap();
            }
            Node::Call(k, ports) => {
                let id = kernels
                    .iter()
                    .position(|item| item == k)
                    .unwrap_or_else(|| {
                        kernels.push(k.clone());
                        kernels.len() - 1
                    });
                let name = if child { "child" } else { "n" };
                let inputs = k
                    .inputs
                    .iter()
                    .map(|t| format!("rt::Type::{t:?}"))
                    .collect::<Vec<_>>()
                    .join(", ");
                let output = k
                    .output
                    .map_or("None".into(), |t| format!("Some(rt::Type::{t:?})"));
                let hook = |phase: &str, empty: bool| {
                    if empty {
                        "None".into()
                    } else {
                        format!("Some(hgl_nodes::n{id}_{phase})")
                    }
                };
                let start = hook("start", k.start.is_empty());
                let stop = hook("stop", k.stop.is_empty());
                writeln!(out, "    let {name}{i} = p.node({child}, rt::Node {{").unwrap();
                writeln!(
                    out,
                    "        inputs: &[{inputs}],\n        output: {output},\n        state: {},",
                    k.state
                )
                .unwrap();
                writeln!(out, "        start: {start},\n        eval: hgl_nodes::n{id}_eval,\n        stop: {stop},\n    }});").unwrap();
                for (s, port) in ports.iter().enumerate() {
                    if let Port::Node(from) = port {
                        writeln!(out, "    p.edge({child}, {name}{from}, {name}{i}, {s});")
                            .unwrap();
                    }
                }
            }
            Node::Each(Port::Node(from), body) => {
                graph(out, body, true, kernels);
                let boundary = |wanted: Port| {
                    body.nodes
                        .iter()
                        .enumerate()
                        .find_map(|(i, n)| match n {
                            Node::Call(_, p) => p
                                .iter()
                                .position(|p| *p == wanted)
                                .map(|s| format!("rt::Port {{ node: child{i}, slot: {s} }}")),
                            _ => None,
                        })
                        .expect("lowered keyed boundary")
                };
                writeln!(
                    out,
                    "    p.each(n{from}, {}, {});",
                    boundary(Port::Key),
                    boundary(Port::Element)
                )
                .unwrap();
            }
            Node::Each(..) => unreachable!("map input belongs to its declaring graph"),
        }
    }
}
fn value(v: &Value) -> String {
    match v {
        Value::Literal(Literal::Int(i)) => format!("{i}i64"),
        Value::Literal(Literal::Text(s)) => format!("{s:?}"),
        Value::Literal(Literal::Bool(v)) => v.to_string(),
        Value::Input(i, t) | Value::Delta(i, t) => format!(
            "f.{}({i})",
            match t {
                Schema::Integer => "integer",
                Schema::Flag => "flag",
                Schema::Text => "text",
                Schema::Dictionary => unreachable!("scalar callback read"),
            }
        ),
        Value::State => "f.state()".into(),
        Value::Local(i) => format!("local{i}"),
        Value::Has(k) => format!("f.has({})", value(k)),
        Value::Get(k) => format!("f.get({})?", value(k)),
        Value::Add(a, b) => format!("hgl_native_runtime::add({}, {})?", value(a), value(b)),
        Value::Equal(a, b) => format!("{} == {}", operand(a), operand(b)),
        Value::Less(a, b) => format!("{} < {}", operand(a), operand(b)),
        Value::Choose(c, a, b) => {
            format!("if {} {{ {} }} else {{ {} }}", value(c), value(a), value(b))
        }
    }
}
// Rust comparisons cannot be chained. Preserve a nested comparison as an operand.
fn operand(v: &Value) -> String {
    match v {
        Value::Equal(..) | Value::Less(..) | Value::Choose(..) => format!("({})", value(v)),
        _ => value(v),
    }
}
fn actions_rust(out: &mut String, actions: &[Action], ty: Option<Schema>, depth: usize) {
    let indent = "    ".repeat(depth);
    for a in actions {
        let line = match a {
            Action::Admit(valid, modified, body) => {
                let mut terms = valid
                    .iter()
                    .map(|i| format!("f.valid({i})"))
                    .collect::<Vec<_>>();
                if modified.is_empty() {
                    terms.push("false".into())
                } else {
                    terms.push(format!(
                        "({})",
                        modified
                            .iter()
                            .map(|i| format!("f.modified({i})"))
                            .collect::<Vec<_>>()
                            .join(" || ")
                    ))
                }
                writeln!(out, "{indent}if {} {{", terms.join(" && ")).unwrap();
                actions_rust(out, body, ty, depth + 1);
                "}".into()
            }
            Action::If(c, body) => {
                writeln!(out, "{indent}if {} {{", value(c)).unwrap();
                actions_rust(out, body, ty, depth + 1);
                "}".into()
            }
            Action::Let(i, v) => format!("let local{i} = {};", value(v)),
            Action::Set(v) => format!("f.set({});", value(v)),
            Action::Put(k, v) => format!("f.put({}, {});", value(k), value(v)),
            Action::Publish(v) => format!(
                "f.{}({}); return Ok(());",
                match ty {
                    Some(Schema::Integer) => "publish",
                    Some(Schema::Flag) => "publish_flag",
                    Some(Schema::Text) => "publish_text",
                    _ => unreachable!("scalar publication"),
                },
                value(v)
            ),
            Action::Update(k, v) => format!("f.update({}, {}); return Ok(());", value(k), value(v)),
            Action::Remove(k) => format!("f.remove({}); return Ok(());", value(k)),
        };
        writeln!(out, "{indent}{line}").unwrap();
    }
}
