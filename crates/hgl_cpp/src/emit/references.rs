//! The `ref` mode of a node's parameters and result (FN-16, TIME-10;
//! core's END-10): the family of the dispatch point (`forms`) that writes
//! a designation, and what a node's struct and wiring function take of
//! the mode (`nodes`). Part of the transpiler, the last stage.
//!
//! A reference is a temporal value whose payload designates an endpoint
//! (TIME-10), and hgraph's `REF` input and output are the engine's form
//! of it: a node's `REF` input given an ordinary connection holds a
//! reference to it, and an ordinary input given a `REF` output observes
//! what that designates, as hgraph binds them.
//!
//! | HGL | C++ |
//! | --- | --- |
//! | `ref low: i64`, a `ref` parameter | hgraph's `REF` input of the parameter's temporal form, `::hgraph::In<"low", ::hgraph::REF<::hgraph::TS<::hgraph::Int>>>`: active where a handler subscribes to it, which wakes the node where it is given a designation, and valid where a reference is present (TIME-10); required only by a header that asks `valid` of it (`admission`), so that one holding no reference, returned, gives the result the empty reference; its wiring function takes a connection of T |
//! | `-> ref i64`, a `ref` result | hgraph's `REF` output, `::hgraph::Out<::hgraph::REF<...>>`; the wiring function gives it as a connection of T, `.as<::hgraph::TS<...>>()`, to which an ordinary reader is bound as hgraph binds a reader of a `REF` |
//! | `return low` of a whole `ref` parameter | `out_.set(::hglrt::designation(low));`: what it designates as the `return` runs, which the result keeps until the node writes it again |
//! | `return price` of an input that is no `ref` parameter | the input is given to the node again, as a `REF` input it is active on (`designated_00_`); the node keeps which it designates in its state (`designates_`), publishes what that `REF` holds, and, where it ticks while the result designates it, publishes it again and runs no handler for that wake |
//! | `return items[idx]` of a position of a fixed list that is no `ref` parameter | each position given to the node again, `::hglrt::element_port(items, 0)`, as a `REF` input it follows as it follows an input (`designated_01_0_`); the index checked (`::hglrt::designated_position`), and the position it names published (`::hglrt::designation_of`) |
//! | `return book[key]` of an ordinary dictionary input | a structural view of the input and the selected key in node state (`::hglrt::designate_key`); membership and binding changes refresh the designation (FN-16) |
//! | `return pair.left` of a field of an ordinary struct input | the designated field port, `::hglrt::field_port<0>(pair)`, given as a `REF` input the node follows (`designated_01_f0_`) |
//! | `out = ...` of a `ref` result | as the `return`, and the evaluation goes on |
//! | `invalidate(out)` of a `ref` result that follows an input | the result follows none, then is invalidated |
//! | `prices[key]` of a dictionary connection in a graph (EXPR-7), the key a connection or a value | a node of its own, `selected_00_`, written in the graph's struct as a lifted call's is, whose `REF` result designates the entry the key names (`::hglrt::select_entry`, hgraph's own form of an entry by key: the dictionary observed for its membership), given as a connection of the entry |
//!
//! Not written yet, and reported: a designation of any other child, of a
//! `ref` parameter's (`xs[i]`, `pair.left`), of a tuple position, of a growing
//! list's position; an input designated whole and by a position in one node;
//! and a `ref` result of a source.

use hgl_analyser::hir::{Body, EndpointOperation, Expr, ExprKind, Hook, Local, Stmt};
use hgl_analyser::{ConstantValue, Program, Signature, Type};
use hgl_diagnostics::Location;
use hgl_lower::nodes::{
    Designated, Handler, designated, designates, designations, is_out, is_reference,
};

use super::Emitter;
use super::forms::Family;
use super::names::{self, identifier};
use super::nodes::{Kind, OUT, STATE};
use super::statements::Scope;
use super::values::INDENT;
use crate::runtime;

/// The name of the node's state member, and of the local bound to it in
/// `eval`, that says which input or position the result follows: its
/// place among those [`followed`] gives, or -1 for none. A name no HGL
/// name gives (`names`).
const DESIGNATES: &str = "designates_";

/// The references' family: the statements of a hook that designate.
pub(super) struct References;

impl Family for References {
    fn statement(
        &self,
        emitter: &mut Emitter<'_>,
        scope: &mut Scope<'_>,
        statement: &Stmt,
        depth: usize,
        text: &mut String,
    ) -> bool {
        if emitter.kind() != Some(Kind::Hook(Hook::When)) {
            return false;
        }
        let Some(signature) = emitter
            .temporal
            .as_ref()
            .and_then(|context| context.signature.clone())
            .filter(|signature| signature.reference)
        else {
            return false;
        };
        let body = scope.body;
        let indent = INDENT.repeat(depth);
        match statement {
            // FN-16: "A `return` to a `ref` result is given an expression
            // that names an endpoint"; it publishes, and leaves the
            // evaluation (FN-7).
            Stmt::Return {
                value: Some(value), ..
            } if designates(value) => {
                if let Some(written) = emitter.designation(scope, &signature, value, depth) {
                    text.push_str(&format!("{written}{indent}return;\n"));
                }
                true
            }
            // FN-16: "`out = items[idx]` designates as `return items[idx]`
            // does"; the evaluation goes on (TIME-7).
            Stmt::Assign {
                local,
                operator: None,
                value,
                ..
            } if is_out(&body.locals, *local) && designates(value) => {
                if let Some(written) = emitter.designation(scope, &signature, value, depth) {
                    text.push_str(&written);
                }
                true
            }
            // END-4: `invalidate(out)` leaves the result designating
            // nothing, so it follows no input after it.
            Stmt::Expr(
                value @ Expr {
                    kind:
                        ExprKind::Endpoint {
                            operation: EndpointOperation::Invalidate,
                            inputs: Some(inputs),
                        },
                    ..
                },
            ) if matches!(inputs.as_slice(), [only] if is_out(&body.locals, *only))
                && !followed(&signature, body).is_empty() =>
            {
                let written = emitter.expression(scope, value);
                text.push_str(&format!("{indent}{DESIGNATES} = -1;\n{indent}{written};\n"));
                true
            }
            _ => false,
        }
    }

    // EXPR-7: "Temporal dictionary key selection may give a reference, a
    // designation of the keyed child (TIME-10)": `prices[key]` of a
    // dictionary connection in a graph, the key a connection or a value.
    fn wiring(
        &self,
        emitter: &mut Emitter<'_>,
        scope: &mut Scope<'_>,
        value: &Expr,
    ) -> Option<String> {
        let ExprKind::Index { base, index } = &value.kind else {
            return None;
        };
        let Type::Dict { key, value: entry } = &base.ty else {
            return None;
        };
        if !base.is_connection() {
            return None;
        }
        // A dictionary whose form no family writes is not this family's:
        // its own form is reported where it is wired.
        let dictionary = emitter.temporal_schema_of(&base.ty)?;
        let entry = emitter.temporal_schema(entry, value.at)?;
        let keyed = emitter.temporal_schema(key, index.at)?;
        Some(emitter.selected_entry(scope, value, base, index, &dictionary, &keyed, &entry))
    }
}

/// Apply TIME-3 default admission to a node's reference parameters.
/// A handler with no validity selector can return an empty reference, which
/// withdraws the result's designation (FN-16, TIME-10). An explicit
/// `valid(target)` still requires the reference to hold a designation.
pub(super) fn admission(handler: &mut Handler, locals: &[Local], signature: &Signature) {
    if handler.asks_valid {
        return;
    }
    handler
        .required
        .retain(|&local| !is_reference(locals, Some(signature), local));
}

/// What a node follows of an input that is no `ref` parameter, which its
/// `ref` result designates (FN-16): the input whole, a fixed list position,
/// the entry of a dictionary selected by a key held in node state, or a
/// struct field path.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Place {
    /// The input, by its place among the body's locals.
    local: usize,
    /// How this follower projects the input.
    projection: Projection,
}

/// A follower's view of its input (FN-16).
#[derive(Debug, Clone, PartialEq, Eq)]
enum Projection {
    Whole,
    Position(i64),
    Key,
    Field(Vec<usize>),
}

impl Place {
    /// The C++ name of the `REF` input the node follows it through: of
    /// the target's own form (`names::temporary`), the projection after the
    /// input's place, `designated_01_0_` or `designated_01_f0_`, and no `__`
    /// in it, which C++ keeps for itself.
    fn follower(&self) -> String {
        let input = names::temporary("designated", self.local);
        match &self.projection {
            Projection::Whole => input,
            Projection::Position(position) => format!("{input}{position}_"),
            Projection::Key => format!("{input}key_"),
            Projection::Field(path) => {
                let fields: Vec<String> = path.iter().map(usize::to_string).collect();
                format!("{input}f{}_", fields.join("_"))
            }
        }
    }
}

/// The field's HGL type along a struct field path `path` starting from `root`.
fn field_path_type(program: &Program, root: &Type, path: &[usize]) -> Option<Type> {
    let mut ty = root.clone();
    for &member in path {
        let fields = hgl_lower::types::fields_of(program, &ty)?;
        ty = fields.get(member)?.1.clone();
    }
    Some(ty)
}

/// The size of `ty`, where it is a fixed list of a positive size.
fn fixed_size(ty: &Type) -> Option<i64> {
    match ty {
        Type::List {
            size: Some(size), ..
        } => match size.value {
            ConstantValue::Int(count) if count > 0 => Some(count),
            _ => None,
        },
        _ => None,
    }
}

/// What the `ref` result of the node of `signature`, whose body is `body`,
/// designates of its inputs that are no `ref` parameter, each once, in
/// the order first designated: an input designated whole, and each
/// position of a fixed list one of whose positions is designated
/// (`items[idx]`), and each dictionary with a selected key. The node follows each
/// (FN-16: "Readers follow a later rebinding of that endpoint without
/// waking the node for it"). None for a node whose result is not `ref`.
fn followed(signature: &Signature, body: &Body) -> Vec<Place> {
    if !signature.reference {
        return Vec::new();
    }
    let mut found: Vec<Place> = Vec::new();
    for (local, how, _) in designations(&body.locals, Some(signature), &body.statements) {
        if is_reference(&body.locals, Some(signature), local) {
            continue;
        }
        let places: Vec<Place> = match how {
            Designated::Whole => vec![Place {
                local,
                projection: Projection::Whole,
            }],
            Designated::Position => (0..fixed_size(&body.locals[local].ty).unwrap_or(0))
                .map(|position| Place {
                    local,
                    projection: Projection::Position(position),
                })
                .collect(),
            Designated::Key => vec![Place {
                local,
                projection: Projection::Key,
            }],
            Designated::Field(path) => vec![Place {
                local,
                projection: Projection::Field(path),
            }],
        };
        for place in places {
            if !found.contains(&place) {
                found.push(place);
            }
        }
    }
    found
}

/// What a node adds for the inputs that are no `ref` parameter which its
/// `ref` result designates (FN-16): each is given to it a second time, as
/// a `REF` input or a structural dictionary input it is active on. Its
/// state keeps which one the result follows and the selected dictionary key.
#[derive(Default)]
pub(super) struct Designations {
    /// Follower inputs in the order [`followed`] supplies them.
    followed: Vec<(Place, String)>,
}

impl Designations {
    /// The inputs the node requires valid itself, of `valid`, those its
    /// handlers ask together (TIME-4). A node that follows an input is
    /// evaluated where that input's `REF` ticks, whatever else is valid,
    /// so none is the node's own: each handler is guarded by its own.
    pub(super) fn validity(&self, valid: Vec<usize>) -> Vec<usize> {
        if self.followed.is_empty() {
            valid
        } else {
            Vec::new()
        }
    }

    /// The parameters of the node's `eval` that follow a designation,
    /// given after the node's own.
    pub(super) fn parameters(&self) -> Vec<String> {
        self.followed
            .iter()
            .map(|(_, parameter)| parameter.clone())
            .collect()
    }

    /// The members of the node's state that keep a designation.
    pub(super) fn members(&self) -> String {
        if self.followed.is_empty() {
            return String::new();
        }
        let mut state = format!(
            "{INDENT}{INDENT}// FN-16: the input or position the `ref` result follows, by its\n\
             {INDENT}{INDENT}// place among those it designates, or -1 for none.\n\
             {INDENT}{INDENT}::hgraph::Int {DESIGNATES}{{-1}};\n"
        );
        for (place, _) in &self.followed {
            if place.projection == Projection::Key {
                let name = place.follower();
                let lib = runtime::NAMESPACE;
                state.push_str(&format!(
                    "{INDENT}{INDENT}::{lib}::KeyDesignation {name}{{}};\n"
                ));
            }
        }
        state
    }

    /// True where the node keeps a designation in its state.
    pub(super) fn holds(&self) -> bool {
        !self.followed.is_empty()
    }
}

impl Emitter<'_> {
    /// hgraph's schema of a node's temporal parameter or result of `ty`:
    /// its temporal form, and where `reference` (FN-16) hgraph's `REF` of
    /// it, `::hgraph::REF<::hgraph::TS<::hgraph::Int>>`. One whose type
    /// is not written is reported at `at`.
    pub(super) fn endpoint_schema(
        &mut self,
        ty: &Type,
        reference: bool,
        at: Location,
    ) -> Option<String> {
        let schema = self.temporal_schema(ty, at)?;
        Some(if reference {
            format!("::hgraph::REF<{schema}>")
        } else {
            schema
        })
    }

    /// The inputs that are no `ref` parameter which the `ref` result of
    /// the node of `signature`, whose body is `body`, designates (FN-16),
    /// as [`Designations`]. `None` where
    /// one is not written yet, which is reported.
    pub(super) fn designated_inputs(
        &mut self,
        signature: &Signature,
        body: &Body,
    ) -> Option<Designations> {
        if !signature.reference {
            return Some(Designations::default());
        }
        let found = designations(&body.locals, Some(signature), &body.statements);
        for (local, how, at) in &found {
            // The container's own form first, so that one not written yet is
            // reported as that.
            if *how != Designated::Whole {
                self.temporal_schema(&body.locals[*local].ty, *at)?;
            }
            if found.iter().any(|(other, again, _)| {
                other == local
                    && matches!(
                        (how, again),
                        (Designated::Whole, Designated::Position)
                            | (Designated::Position, Designated::Whole)
                    )
            }) {
                self.unsupported(
                    *at,
                    "an input designated whole and by a position in one node",
                );
                return None;
            }
        }
        let followed = followed(signature, body);
        let mut following = Vec::new();
        for place in followed {
            let declared = &body.locals[place.local];
            let (schema, label) = match (&place.projection, &declared.ty) {
                (Projection::Whole, ty) => (
                    self.endpoint_schema(ty, true, declared.at)?,
                    format!("designated:{}", declared.name),
                ),
                (Projection::Position(position), Type::List { element, .. }) => (
                    self.endpoint_schema(element, true, declared.at)?,
                    format!("designated:{}[{position}]", declared.name),
                ),
                (Projection::Key, Type::Dict { .. }) => (
                    self.temporal_schema(&declared.ty, declared.at)?,
                    format!("designated:{}[key]", declared.name),
                ),
                (Projection::Field(path), ty) => {
                    let Some(field) = field_path_type(self.program, ty, path) else {
                        self.unsupported(declared.at, "a designation of this child");
                        return None;
                    };
                    let suffix: Vec<String> = path.iter().map(usize::to_string).collect();
                    (
                        self.endpoint_schema(&field, true, declared.at)?,
                        format!("designated:{}.{}", declared.name, suffix.join(".")),
                    )
                }
                _ => unreachable!("a designation projects its input's form"),
            };
            // A label no HGL name is: hgraph names the node's inputs by
            // their labels. Only dictionary followers watch membership.
            let label = names::string(&label);
            let activity = if place.projection == Projection::Key {
                ", ::hgraph::InputActivity::Structural"
            } else {
                ""
            };
            let name = place.follower();
            following.push((
                place,
                format!(
                    "::hgraph::In<{label}, {schema}{activity}, ::hgraph::InputValidity::Unchecked> {name}"
                ),
            ));
        }
        Some(Designations {
            followed: following,
        })
    }

    /// What a node's `eval` starts with where it follows the inputs of
    /// `designated`: the designation its state keeps, bound by reference;
    /// for the input it follows, where that input's `REF` ticks, what it
    /// designates now published (FN-16: "Readers follow a later rebinding
    /// of that endpoint"); and, where none of the node's own inputs woke
    /// it, the evaluation left before any handler ("without waking the
    /// node for it"). `body` is the node's, `inputs` its temporal inputs,
    /// `active` those a handler subscribes to.
    pub(super) fn designation_prologue(
        &self,
        designated: &Designations,
        body: &Body,
        inputs: &[usize],
        active: &[usize],
        has_timer: bool,
    ) -> String {
        if designated.followed.is_empty() {
            return String::new();
        }
        let indent = INDENT.repeat(2);
        let inner = INDENT.repeat(3);
        let mut text = format!(
            "{indent}// FN-16: the input the `ref` result follows, kept in the node's state.\n\
             {indent}::hgraph::Int& {DESIGNATES} = {STATE}.modify().{DESIGNATES};\n\
             {indent}// FN-16: \"Readers follow a later rebinding of that endpoint without\n\
             {indent}// waking the node for it\".\n"
        );
        let lib = runtime::NAMESPACE;
        for (place, (followed, _)) in designated.followed.iter().enumerate() {
            let view = followed.follower();
            if followed.projection == Projection::Key {
                text.push_str(&format!(
                    "{indent}if ({DESIGNATES} == {place})\n{indent}{{\n\
                     {inner}::{lib}::refresh_key_designation({view}, {STATE}.modify().{view}, {OUT});\n{indent}}}\n"
                ));
            } else {
                text.push_str(&format!(
                    "{indent}if ({DESIGNATES} == {place} && {view}.modified())\n{indent}{{\n\
                     {inner}{OUT}.set(::{lib}::designation({view}));\n{indent}}}\n"
                ));
            }
        }
        // An input wakes the node where it is active and publishes, or is
        // invalidated (END-4: "The transition to invalid notifies active
        // subscribers"). Where the body changes an input's activity
        // (END-5), each input is asked whether it is active.
        let mut changes = false;
        hgl_analyser::hir::walk_statements(&body.statements, &mut |value| {
            changes |= matches!(
                value.kind,
                ExprKind::Endpoint {
                    operation: EndpointOperation::Activate | EndpointOperation::Passivate,
                    ..
                }
            );
        });
        let mut woken: Vec<String> = inputs
            .iter()
            .filter(|local| changes || active.contains(local))
            .map(|&local| {
                let input = identifier(&body.locals[local].name);
                if changes {
                    format!("(({input}.modified() || {input}.invalidated()) && {input}.active())")
                } else {
                    format!("{input}.modified() || {input}.invalidated()")
                }
            })
            .collect();
        if has_timer {
            woken.push(super::scheduling::TIMER_WOKE.to_string());
        }
        let woken = if woken.is_empty() {
            "false".to_string()
        } else {
            woken.join(" || ")
        };
        text.push_str(&format!(
            "{indent}if (!({woken}))\n{indent}{{\n{inner}return;\n{indent}}}\n"
        ));
        text
    }

    /// The ports the wiring function of the node of `signature`, whose
    /// body is `body`, gives a second time, after its parameters: each
    /// input it follows, named as the wiring function's parameter is, and
    /// each position it follows, that position's connection (TIME-9: "a
    /// child of one, `parts[1]`").
    pub(super) fn designated_ports(&self, signature: &Signature, body: &Body) -> Vec<String> {
        followed(signature, body)
            .into_iter()
            .map(|place| {
                let port = identifier(&body.locals[place.local].name);
                match &place.projection {
                    Projection::Whole | Projection::Key => port,
                    Projection::Position(position) => {
                        format!("::{}::element_port({port}, {position})", runtime::NAMESPACE)
                    }
                    Projection::Field(path) => path.iter().fold(port, |acc, &member| {
                        format!("::{}::field_port<{member}>({acc})", runtime::NAMESPACE)
                    }),
                }
            })
            .collect()
    }

    /// `value`, `base[index]` of a dictionary connection `base` of hgraph's
    /// `dictionary` schema in a graph, whose key is of `keyed` and entry of
    /// `entry`, as the `Port` it gives: a node of its own, a struct
    /// `selected_00_` written in the graph's (as a lifted call's is), whose
    /// result designates the entry the key names
    /// (`hglrt::select_entry`), wired with the dictionary and the key, a
    /// value key as a constant connection (FN-3), and given as a
    /// connection of the entry (FN-16: "a `ref` result is a connection of
    /// T to whoever reads it").
    #[allow(clippy::too_many_arguments)]
    fn selected_entry(
        &mut self,
        scope: &mut Scope,
        value: &Expr,
        base: &Expr,
        index: &Expr,
        dictionary: &str,
        keyed: &str,
        entry: &str,
    ) -> String {
        let namespace = runtime::NAMESPACE;
        // A node of an eval's entry is written ahead of the test, at the
        // scope of the test's namespace, and named by where it is, as a
        // lifted call's is (`lifting`).
        let name = if self.kind() == Some(Kind::Entry) {
            format!(
                "selected_0{}_{}_",
                value.at.file.index(),
                value.at.span.start
            )
        } else {
            names::temporary("selected", self.temporary())
        };
        let line = self.line_of(value.at);
        let graph = &self.program.modules[self.module].path;
        let text = format!(
            "// The entry of a dictionary its key selects, on line {line}: a\n\
             // designation of it (EXPR-7, TIME-10).\n\
             struct {name}\n{{\n\
             {INDENT}static constexpr auto name = {};\n\
             {INDENT}static void start(::hgraph::State<::hgraph::Int> {STATE}) {{ {STATE}.set(::hgraph::Int{{0}}); }}\n\
             {INDENT}static void eval(::hgraph::In<\"dictionary\", {dictionary}, ::hgraph::InputActivity::Structural, ::hgraph::InputValidity::Unchecked> dictionary, ::hgraph::In<\"key\", {keyed}> key, ::hgraph::State<::hgraph::Int> {STATE}, ::hgraph::Out<::hgraph::REF<{entry}>> {OUT})\n\
             {INDENT}{{\n\
             {INDENT}{INDENT}::{namespace}::select_entry(dictionary, key, {STATE}.modify(), {OUT});\n\
             {INDENT}}}\n}};\n",
            names::string(&format!("{graph}:entry"))
        );
        if let Some(context) = self.temporal.as_mut() {
            context.lifted.push(text);
        }
        // EXPR-2: the dictionary is wired before its key is evaluated:
        // where either runs statements, or both may fail
        // (`expressions::held_read`), the port is held first, as C++
        // evaluates a call's arguments in no order it promises.
        let held = self.held_read(base, index);
        let port = self.wiring_form(scope, base);
        let (steps, port) = if held {
            let kept = names::temporary("dictionary", self.temporary());
            (format!("const auto {kept} = {port}; "), kept)
        } else {
            (String::new(), port)
        };
        let key = if index.is_connection() {
            self.wiring_form(scope, index)
        } else {
            let written = self.expression(scope, index);
            self.temporal_constant(&index.ty, &written, index.at)
        };
        let wired = format!(
            "::hgraph::wire<{name}>({}, {port}, {key}).as<{entry}>()",
            super::graphs::WIRING
        );
        if steps.is_empty() {
            wired
        } else {
            format!("[&]() {{ {steps}return {wired}; }}()")
        }
    }

    /// `called`, what a wiring function gives of the node of `signature`,
    /// whose result is of `schema`, as the connection a caller is given:
    /// for a `ref` result, a connection of T (FN-16: "a `ref` result is a
    /// connection of T to whoever reads it"), which hgraph binds an
    /// ordinary reader of to what the result designates.
    pub(super) fn result_port(
        &self,
        signature: &Signature,
        schema: Option<&str>,
        called: String,
    ) -> String {
        match schema {
            Some(schema) if signature.reference => format!("{called}.as<{schema}>()"),
            _ => called,
        }
    }

    /// The statements, `depth` levels in, that make the result of the node
    /// of `signature` designate `value`, what a `return` or an `out =`
    /// names (FN-16), in `scope`, the node's hook. `None` where it is not
    /// written yet, which is reported.
    fn designation(
        &mut self,
        scope: &mut Scope,
        signature: &Signature,
        value: &Expr,
        depth: usize,
    ) -> Option<String> {
        let body = scope.body;
        let indent = INDENT.repeat(depth);
        let following = followed(signature, body);
        let namespace = runtime::NAMESPACE;
        let published =
            |view: &str| format!("{indent}{OUT}.set(::{namespace}::designation({view}));\n");
        match designated(&body.locals, Some(signature), value) {
            Some((local, Designated::Whole)) => {
                if is_reference(&body.locals, Some(signature), local) {
                    // FN-16: "Returning a whole `ref` parameter samples its
                    // current designation when the `return` executes ...
                    // the result keeps the sampled designation until the
                    // node writes its result again": nothing is followed.
                    let mut text = String::new();
                    if !following.is_empty() {
                        text.push_str(&format!("{indent}{DESIGNATES} = -1;\n"));
                    }
                    text.push_str(&published(&identifier(&body.locals[local].name)));
                    return Some(text);
                }
                // FN-16: "Returning an ordinary input ... designates the
                // logical endpoint named by the path", which the node
                // follows.
                let whole = Place {
                    local,
                    projection: Projection::Whole,
                };
                let Some(place) = following.iter().position(|followed| followed == &whole) else {
                    self.unsupported(value.at, "this designation");
                    return None;
                };
                Some(format!(
                    "{indent}{DESIGNATES} = {place};\n{}",
                    published(&whole.follower())
                ))
            }
            // FN-16: "Returning ... a child path such as `items[idx]`
            // designates that logical endpoint": the position is known
            // when the `return` runs, and the node follows each.
            Some((local, Designated::Position)) => {
                let (ExprKind::ChildEndpointOf(child), Some(size)) =
                    (&value.kind, fixed_size(&body.locals[local].ty))
                else {
                    self.unsupported(value.at, "a designation of a position of a list");
                    return None;
                };
                let ExprKind::Index { index, .. } = &child.kind else {
                    self.unsupported(value.at, "a designation of a position of a list");
                    return None;
                };
                let first = Place {
                    local,
                    projection: Projection::Position(0),
                };
                let Some(base) = following.iter().position(|followed| followed == &first) else {
                    self.unsupported(value.at, "this designation");
                    return None;
                };
                let views: Vec<String> = (0..size)
                    .map(|position| {
                        Place {
                            local,
                            projection: Projection::Position(position),
                        }
                        .follower()
                    })
                    .collect();
                let held = names::temporary("position", self.temporary());
                let index = self.expression(scope, index);
                // EXPR-3: "An index outside a list ... fails where it is
                // evaluated", as the child at it does (`hglrt::child`).
                Some(format!(
                    "{indent}const ::hgraph::Int {held} = ::{namespace}::designated_position({index}, {size});\n\
                     {indent}{DESIGNATES} = {base} + {held};\n\
                     {indent}{OUT}.set(::{namespace}::designation_of({held}, {}));\n",
                    views.join(", ")
                ))
            }
            Some((local, Designated::Key)) => {
                let ExprKind::ChildEndpointOf(child) = &value.kind else {
                    unreachable!("a key designates a dictionary child");
                };
                let ExprKind::Index { index, .. } = &child.kind else {
                    unreachable!("a dictionary child has an index");
                };
                let endpoint = Place {
                    local,
                    projection: Projection::Key,
                };
                let place = following.iter().position(|item| item == &endpoint)?;
                let view = endpoint.follower();
                let key = self.expression(scope, index);
                Some(format!(
                    "{indent}::{namespace}::designate_key({view}, {key}, {STATE}.modify().{view}, {OUT});\n\
                     {indent}{DESIGNATES} = {place};\n"
                ))
            }
            Some((local, Designated::Field(path))) => {
                let field = Place {
                    local,
                    projection: Projection::Field(path),
                };
                let Some(place) = following.iter().position(|followed| followed == &field) else {
                    self.unsupported(value.at, "this designation");
                    return None;
                };
                Some(format!(
                    "{indent}{DESIGNATES} = {place};\n{}",
                    published(&field.follower())
                ))
            }
            None => {
                self.unsupported(value.at, "a designation of this child");
                None
            }
        }
    }
}
