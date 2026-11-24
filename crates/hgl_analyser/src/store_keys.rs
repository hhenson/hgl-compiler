//! The keys of the global store across a whole program: one type for
//! each (TIME-8, core's CAP-6). A pass of the analyser, run once every
//! body of the program is typed: it reads the typed bodies and nothing
//! else.
//!
//! TIME-8: "All uses of a key bind the same exact ordinary type: a use
//! of another type in one program rejects (`store.key_type`)." The
//! catalogue says what a use is, and where the report goes: "a `put` or
//! a `get` of another type than an earlier use of the key has (TIME-8,
//! core's CAP-6); the later use".

use std::collections::HashMap;

use hgl_diagnostics::{Category, Diagnostics, Location};

use crate::hir::{Callee, Expr, ExprKind, Value, walk_statements};
use crate::program::Program;
use crate::types::{Argument, Type};
use crate::{codes, error};

/// One use of a key: the key, the type the use binds it to, and where
/// the use is written.
struct KeyUse {
    key: String,
    ty: Type,
    at: Location,
}

/// Reports each use of a key of the global store whose type is not the
/// type of the key's first use (`store.key_type`).
///
/// A use is a call of core's `put` or `get` on the store whose key is
/// written out, a string literal; its type is the candidate's `V`, the
/// value put or the value read (CAP-6: "`get<V>(global_state, key)` is
/// the value under `key`, of the type the call writes or its place
/// expects"). `has` binds no type. A key that is a `const` parameter is
/// known where a graph is built, which no pass reads; a `V` that is not
/// known, or that names a generic parameter, is not compared. The uses
/// are taken in the order of the program: its modules, and in each its
/// functions, its tests and its application's statements, each in the
/// order written.
pub(crate) fn key_types(program: &Program, diagnostics: &mut Diagnostics) {
    let mut uses: Vec<KeyUse> = Vec::new();
    for module in &program.modules {
        let bodies = module
            .bodies
            .iter()
            .flatten()
            .chain(module.test_bodies.iter().flatten())
            .chain(&module.store_body);
        for body in bodies {
            walk_statements(&body.statements, &mut |expression| {
                if let Some(found) = key_use(program, expression) {
                    uses.push(found);
                }
            });
        }
    }
    // The type each key has so far, and the use that gave it.
    let mut first: HashMap<&str, &KeyUse> = HashMap::new();
    for found in &uses {
        let Some(earlier) = first.get(found.key.as_str()).copied() else {
            first.insert(found.key.as_str(), found);
            continue;
        };
        if earlier.ty == found.ty {
            continue;
        }
        diagnostics.emit(
            error(
                Category::Injectable,
                codes::STORE_KEY_TYPE,
                found.at,
                format!(
                    "the key `\"{}\"` of the global store is used here as `{}`, and before as \
                     `{}`: a key has one type in a program",
                    found.key, found.ty, earlier.ty
                ),
                "TIME-8",
            )
            .with_related(earlier.at, "used here before"),
        );
    }
}

/// The use of a key `expression` is, where it is a call of core's `put`
/// or `get` on the store with a key written out and a type known.
fn key_use(program: &Program, expression: &Expr) -> Option<KeyUse> {
    let ExprKind::Call {
        callee, arguments, ..
    } = &expression.kind
    else {
        return None;
    };
    let (module, declaration) = callee.declaration()?;
    if Some(module) != program.core() {
        return None;
    }
    let name = program.modules[module].declarations[declaration]
        .name
        .as_str();
    if name != "put" && name != "get" {
        return None;
    }
    let crate::types::Detail::Callable(signature) =
        program.modules[module].details.get(declaration)?
    else {
        return None;
    };
    // A candidate of the store's: it takes the store, which the
    // dictionary's `get` does not.
    let store = program.capability_type("global_state")?;
    if !signature
        .parameters
        .iter()
        .any(|parameter| parameter.ty == store)
    {
        return None;
    }
    let key = arguments.iter().find_map(|argument| {
        let parameter = signature.parameters.get(argument.parameter?)?;
        match &argument.value.kind {
            ExprKind::Literal(Value::Str(key)) if parameter.name == "key" => Some(key.clone()),
            _ => None,
        }
    })?;
    // `V`: the candidate's one generic parameter, as the call gives it.
    let ty = match callee {
        Callee::Function { generics, .. } | Callee::Candidate { generics, .. } => {
            match generics.first()? {
                Argument::Type(ty) => ty.clone(),
                Argument::Constant(_) | Argument::NoResult => return None,
            }
        }
        _ => return None,
    };
    let unknown = ty.any_part(
        &|part| matches!(part, Type::Unknown | Type::Parameter { .. }),
        &|_| false,
    );
    if unknown {
        return None;
    }
    Some(KeyUse {
        key,
        ty,
        at: expression.at,
    })
}
