//! The optimiser's first graph slice. Connections and callback operations are
//! resolved here, before either transpiler chooses builder or value syntax.

mod lower;
use hgl_diagnostics::Location;
pub use lower::{application, test};

/// A form outside this stage's current coverage, with its source position.
#[derive(Debug)]
pub struct Unsupported(pub Location, pub &'static str);

/// Payload schemas in the scalar and keyed slices.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Schema {
    /// Signed 64-bit integer.
    Integer,
    /// Unicode text.
    Text,
    /// Boolean.
    Flag,
    /// Dictionary of strings to signed integers.
    Dictionary,
}

/// Owned scalar value in a replay or in node configuration.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Literal {
    /// Signed integer.
    Int(i64),
    /// String.
    Text(String),
    /// Boolean.
    Bool(bool),
}

/// A graph-local result or an explicit keyed child boundary (CTRL-15).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Port {
    /// Output of the indexed builder in this graph.
    Node(usize),
    /// Key of this child.
    Key,
    /// Current value for this child.
    Element,
}

/// Prepared graph in dependency order. Each eval constructs fresh instances.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Graph {
    /// Builders in this scope.
    pub nodes: Vec<Node>,
    /// Recorded root output, if present.
    pub result: Option<Port>,
}

/// One builder with the connections it needs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Node {
    /// Publications by offset in smallest time steps (CEVAL-2).
    Replay(Schema, Vec<(i64, Literal)>),
    /// Node implementation and ordered temporal inputs.
    Call(Kernel, Vec<Port>),
    /// Outputless dynamic dictionary iteration. Removal disposes of the child (CTRL-7).
    Each(Port, Graph),
}

/// A node specialised for its ordinary arguments.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Kernel {
    /// Qualified HGL declaration, for diagnostics and dumps.
    pub name: String,
    /// Input payloads in callback slot order.
    pub inputs: Vec<Schema>,
    /// Output schema of a compute node, absent for a sink.
    pub output: Option<Schema>,
    /// Whether this node owns an integer state cell.
    pub state: bool,
    /// Initialise state and run the start hook.
    pub start: Vec<Action>,
    /// Admitted handlers, in source order.
    pub eval: Vec<Action>,
    /// Stop hook for each retiring instance.
    pub stop: Vec<Action>,
}

/// An operation in a node callback. Scalar reads take the held input value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Value {
    /// A literal or specialised configuration value.
    Literal(Literal),
    /// Held scalar input by slot and schema.
    Input(usize, Schema),
    /// Current publication (END-3).
    Delta(usize, Schema),
    /// Node's persistent state cell.
    State,
    /// Callback-local binding.
    Local(usize),
    /// Presence of a run-wide store entry.
    Has(Box<Value>),
    /// Typed integer read from the run-wide store.
    Get(Box<Value>),
    /// Checked core integer addition.
    Add(Box<Value>, Box<Value>),
    /// Core scalar equality.
    Equal(Box<Value>, Box<Value>),
    /// Core scalar less-than comparison.
    Less(Box<Value>, Box<Value>),
    /// Lazy conditional value (CTRL-10).
    Choose(Box<Value>, Box<Value>, Box<Value>),
}

/// Normalised control, state and publication in a callback.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Action {
    /// Run when all required slots are valid and any subscribed slot ticks (TIME-3).
    Admit(Vec<usize>, Vec<usize>, Vec<Action>),
    /// Conditional statements.
    If(Value, Vec<Action>),
    /// Bind an ordinary value.
    Let(usize, Value),
    /// Initialise or assign the persistent cell.
    Set(Value),
    /// Write one integer to the run's store.
    Put(Value, Value),
    /// Publish a scalar and end the handler.
    Publish(Value),
    /// Publish a single dictionary update and end the handler.
    Update(Value, Value),
    /// Publish a single dictionary removal and end the handler.
    Remove(Value),
}

/// One independently executed trace assertion.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Case {
    /// Prepared graph.
    pub graph: Graph,
    /// Expected publications with exact offsets. Silence has no entry (TST-6).
    pub expected: Vec<(i64, i64)>,
    /// Location for assertion failures. Only its relative span enters the dump.
    pub at: Location,
}

/// Deterministic, location-independent text for reviewing semantic changes.
pub fn dump(cases: &[Case]) -> String {
    use std::fmt::Write;
    fn graph(out: &mut String, g: &Graph, indent: &str) {
        for (id, node) in g.nodes.iter().enumerate() {
            let _ = write!(out, "{indent}%{id} ");
            match node {
                Node::Replay(ty, events) => {
                    let _ = writeln!(out, "replay {ty:?} {events:?}");
                }
                Node::Call(k, ports) => {
                    let _ = writeln!(
                        out,
                        "{} {ports:?} : {:?} -> {:?}",
                        k.name, k.inputs, k.output
                    );
                    for (phase, actions) in
                        [("start", &k.start), ("eval", &k.eval), ("stop", &k.stop)]
                    {
                        for action in actions {
                            let _ = writeln!(out, "{indent}  {phase} {action:?}");
                        }
                    }
                }
                Node::Each(over, child) => {
                    let _ = writeln!(out, "each {over:?}");
                    graph(out, child, &format!("{indent}  "));
                }
            }
        }
        if let Some(result) = g.result {
            let _ = writeln!(out, "{indent}record {result:?}");
        }
    }
    let mut out = String::new();
    for (i, case) in cases.iter().enumerate() {
        let _ = writeln!(
            out,
            "assertion {} @{}..{}",
            i + 1,
            case.at.span.start,
            case.at.span.end
        );
        graph(&mut out, &case.graph, "  ");
        let _ = writeln!(out, "  expect {:?}", case.expected);
    }
    out
}
