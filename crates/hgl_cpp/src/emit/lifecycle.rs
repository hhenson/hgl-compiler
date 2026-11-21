//! What a run's lifecycle asks of the code the target writes: the
//! bounds a run's configuration gives it. Part of the transpiler, the
//! last stage. (A failure that escapes a handler, `?` in a `when`, is the
//! lifting work's, with `error` and `result` as temporal values.)
//!
//! | HGL | C++ |
//! | --- | --- |
//! | `eval(call, RunConfig(start: s, length: l))` (RUN-1; core's CRUN-2) | the run given `::hglrt::testing::Bounds{.start = s, .length = l}`: each field the configuration writes, the others null, which the library resolves and checks before the run's graph is built (`run.end_and_length`) |
//! | `eval(call, config)`, where `config` is a value of `RunConfig` made otherwise: held in a `let`, given by a function | the value held once, `const RunConfig config_ = ...;`, and the run given `Bounds{.start = config_.start, .end = config_.end, .length = config_.length}` |

use hgl_analyser::hir::{Expr, ExprKind};
use hgl_analyser::{DeclarationKind, Detail, NamedKind, Type};

use super::Emitter;
use super::calls::{changes, may_fail};
use super::names;
use super::statements::Scope;

/// The fields of core's `RunConfig` (CRUN-2), in the order the library's
/// `Bounds` declares them, which its designated initializers follow.
const BOUNDS: [&str; 3] = ["start", "end", "length"];

impl Emitter<'_> {
    /// The bounds the configuration `config` of an eval gives its run
    /// (RUN-1; core's CRUN-2: "`start` is inclusive and `end` exclusive
    /// ... `length` gives the end counted from the start the run has"):
    /// `::hglrt::testing::Bounds`, each field the configuration writes in
    /// place and the others null. Its fields are evaluated in the order
    /// written (EXPR-2): where more than one may fail, or one runs
    /// statements, each is held first, in that order.
    pub(super) fn run_bounds(&mut self, scope: &mut Scope, config: &Expr) -> String {
        let program = self.program;
        let ExprKind::Construct {
            module,
            declaration,
            arguments,
        } = &config.kind
        else {
            return self.run_bounds_of_value(scope, config);
        };
        let declared = &program.modules[*module].declarations[*declaration];
        let core = Some(*module) == program.core();
        let Some(Detail::Struct(read)) = program.modules[*module].details.get(*declaration) else {
            return self.missing(config.at, "this configuration of a run");
        };
        if !core || declared.name != "RunConfig" || declared.kind != DeclarationKind::Struct {
            return self.missing(config.at, "this configuration of a run");
        }
        let mut fields: Vec<(&str, String)> = Vec::new();
        let ordered = arguments
            .iter()
            .filter(|argument| may_fail(&argument.value))
            .count()
            > 1
            || arguments.iter().any(|argument| changes(&argument.value));
        let mut steps = String::new();
        for argument in arguments {
            let Some(member) = argument.parameter.and_then(|index| read.members.get(index)) else {
                return self.missing(argument.value.at, "this field of a run's configuration");
            };
            let Some(field) = BOUNDS.iter().find(|field| **field == member.name) else {
                return self.missing(argument.value.at, "this field of a run's configuration");
            };
            let mut written = self.expression(scope, &argument.value);
            if ordered {
                let held = names::temporary("bound", self.temporary());
                steps.push_str(&format!("const auto {held} = {written}; "));
                written = held;
            }
            fields.push((field, written));
        }
        let library = names::testing("Bounds");
        let given: Vec<String> = BOUNDS
            .iter()
            .filter_map(|field| {
                fields
                    .iter()
                    .find(|(named, _)| named == field)
                    .map(|(_, written)| format!(".{field} = {written}"))
            })
            .collect();
        let bounds = format!("{library}{{{}}}", given.join(", "));
        if steps.is_empty() {
            bounds
        } else {
            format!("[&] {{ {steps}return {bounds}; }}()")
        }
    }

    /// The bounds of a run whose configuration `config` is a value of
    /// core's `RunConfig` made some other way than written in the eval:
    /// held in a `let`, given by a function, chosen by an `if` (RUN-1:
    /// "an optional `RunConfig`"; it is an ordinary value). The value is
    /// held once, where the eval's entry is built, and each of its fields
    /// is that bound of the run, null where it is null.
    fn run_bounds_of_value(&mut self, scope: &mut Scope, config: &Expr) -> String {
        let program = self.program;
        let Type::Named {
            module,
            declaration,
            kind: NamedKind::Struct,
            ..
        } = &config.ty
        else {
            return self.missing(config.at, "this configuration of a run");
        };
        let declared = &program.modules[*module].declarations[*declaration];
        let core = Some(*module) == program.core();
        let Some(Detail::Struct(read)) = program.modules[*module].details.get(*declaration) else {
            return self.missing(config.at, "this configuration of a run");
        };
        if !core || declared.name != "RunConfig" || declared.kind != DeclarationKind::Struct {
            return self.missing(config.at, "this configuration of a run");
        }
        let Some(ty) = self.written_type(&config.ty, config.at) else {
            return "{}".to_string();
        };
        let written = self.expression(scope, config);
        let held = names::temporary("config", self.temporary());
        let mut given = Vec::new();
        for field in BOUNDS {
            let Some(member) = read.members.iter().position(|member| member.name == field) else {
                return self.missing(config.at, "this configuration of a run");
            };
            let Some(place) = self.field_place(&config.ty, held.clone(), member) else {
                return self.missing(config.at, "this configuration of a run");
            };
            given.push(format!(".{field} = {place}"));
        }
        format!(
            "[&] {{ const {ty} {held} = {written}; return {}{{{}}}; }}()",
            names::testing("Bounds"),
            given.join(", ")
        )
    }
}
