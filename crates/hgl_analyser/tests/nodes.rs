//! The analyser against the rules on a node's hooks. Each test cites
//! its rule.
//!
//! A finding is written `file:line:column category/code rule`.

mod common;

use common::{Checked, check};
use hgl_analyser::hir::{Body, EndpointOperation, ExprKind, Hook, Stmt};

/// No lines.
const NONE: [&str; 0] = [];

/// Checks one file, `m.hgl`, of module `m`. The `module` line is added,
/// so the text given starts on line 2.
fn module(text: &str) -> Checked {
    check(&[("m.hgl", &format!("module m\n{text}"))])
}

/// The typed body of the function `name` of module `m`.
fn body<'a>(checked: &'a Checked, name: &str) -> &'a Body {
    let module = checked.program.module("m").expect("module m exists");
    let index = module
        .declarations
        .iter()
        .position(|declaration| declaration.name == name)
        .expect("the declaration exists");
    module.bodies[index].as_ref().expect("it has a body")
}

/// FN-3: "In a hook a temporal parameter is a value of its declared
/// type, read as in a value function". FN-2: a node's body is its
/// declarations and then its hooks. TIME-9: "The operations on an
/// endpoint are intrinsics of the language ... each takes a temporal
/// input of the enclosing function by its name, or `out`". TIME-7: "`out`
/// is read in a `when` and in `stop`, and written only in a `when`".
/// FN-7: a `return` with no value "ends the evaluation and publishes
/// nothing itself".
#[test]
fn a_hook_is_read_as_value_code() {
    let checked = module(concat!(
        "fn counted(value: i64, const step: i64) -> i64 {\n",
        "    state seen: i64 = 0\n",
        "    cache scale: i64 = step * 2\n",
        "    inject out, scheduler\n",
        "    start { }\n",
        "    when modified() && valid(value) {\n",
        "        seen += value * scale\n",
        "        if seen > 100 {\n",
        "            invalidate(out)\n",
        "            return\n",
        "        }\n",
        "        let published = delta_value(value)\n",
        "        let when_last = last_modified(value)\n",
        "        out = seen\n",
        "    }\n",
        "    when scheduled() && valid([]) { return seen }\n",
        "    stop { let last = out }\n",
        "}\n",
    ));
    assert_eq!(checked.summary(), NONE);
    let body = body(&checked, "counted");
    let locals: Vec<String> = body
        .locals
        .iter()
        .map(|local| format!("{}: {}", local.name, local.ty))
        .collect();
    assert_eq!(
        locals,
        [
            "value: i64",
            "step: i64",
            "seen: i64",
            "scale: i64",
            "out: i64",
            "scheduler: _",
            "published: i64",
            "when_last: datetime?",
            "last: i64",
        ]
    );
    // Its declarations and then its hooks.
    let shape: Vec<String> = body
        .statements
        .iter()
        .map(|statement| match statement {
            Stmt::Declare { local, .. } => format!("declare {}", body.locals[*local].name),
            Stmt::Inject { local, .. } => format!("inject {}", body.locals[*local].name),
            Stmt::Hook { hook, .. } => hook.as_str().to_string(),
            other => panic!("a declaration or a hook: {other:?}"),
        })
        .collect();
    assert_eq!(
        shape,
        [
            "declare seen",
            "declare scale",
            "inject out",
            "inject scheduler",
            "start",
            "when",
            "when",
            "stop",
        ]
    );
    // `valid([])`: the endpoints named, here none.
    let Stmt::Hook {
        hook: Hook::When,
        condition: Some(condition),
        ..
    } = &body.statements[6]
    else {
        panic!("a `when` with a condition");
    };
    let (_, _, right) = condition.kind.binary().expect("`&&`");
    assert_eq!(
        right.kind,
        ExprKind::Endpoint {
            operation: EndpointOperation::Valid,
            inputs: Some(Vec::new()),
        }
    );
}

/// TIME-9: an endpoint operation "takes a temporal input of the
/// enclosing function by its name, or `out`". TIME-7: "`start` neither
/// reads nor writes it, and a `return` with a value stands in neither
/// `start` nor `stop`." FN-3: "A runtime hook can call value helpers but
/// cannot silently construct a temporal child call."
#[test]
fn a_hook_rejects_what_its_phase_has_not() {
    let checked = module(concat!(
        "fn doubled(value: i64) -> i64 => value\n",
        "fn bad(value: i64, other: f64, const step: i64) -> i64 {\n",
        "    state seen: i64 = 0\n",
        "    inject out\n",
        "    start {\n",
        "        let first = out\n",
        "        return 1\n",
        "    }\n",
        // A state, a `const` parameter and an expression are no
        // endpoints.
        "    when valid(seen) && modified(step) {\n",
        "        let late = delta_value(value + 1)\n",
        "        return other\n",
        "    }\n",
        "    when { return doubled(value) }\n",
        "    stop {\n",
        "        out = 2\n",
        "        invalidate(out)\n",
        "        let last = valid(out)\n",
        "    }\n",
        "}\n",
    ));
    assert_eq!(
        checked.summary(),
        [
            "m.hgl:7:21 phase/out.lifecycle TIME-7",
            "m.hgl:8:16 phase/out.lifecycle TIME-7",
            "m.hgl:10:16 phase/endpoint.argument TIME-9",
            "m.hgl:10:34 phase/endpoint.argument TIME-9",
            "m.hgl:11:32 phase/endpoint.argument TIME-9",
            "m.hgl:12:16 type/type.mismatch EXPR-5",
            "m.hgl:14:19 function-kind/call.role FN-3",
            "m.hgl:16:9 phase/out.lifecycle TIME-7",
            "m.hgl:17:9 phase/out.lifecycle TIME-7",
        ]
    );
}
