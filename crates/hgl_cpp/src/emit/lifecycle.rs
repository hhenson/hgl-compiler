//! What a run's lifecycle asks of the code the target writes: the
//! bounds a run's configuration gives it, and the capability
//! `run_config`. Part of the transpiler, the last stage. (A failure that
//! escapes a handler, `?` in a `when`, is the lifting work's, with
//! `error` and `result` as temporal values.)
//!
//! | HGL | C++ |
//! | --- | --- |
//! | `eval{start: s, length: l} call` (RUN-6; core's CRUN-2) | the run given `::hglrt::testing::Bounds{.start = s, .length = l}`: each field the configuration names, the others null, which the library resolves and checks before the run's graph is built (`run.end_and_length`) |
//! | The same in an integration test that injects `run_config` (TST-17) | each field the eval does not name the injected configuration's: `Bounds{.start = s, .end = run_config.end(), .length = l}` |
//! | `inject run_config{length: 3s}` in an application or an integration test (RUN-6) | `const ::hglrt::RunConfig run_config{::hglrt::RunConfig::Fields{.length = 3s}};`: the configuration as supplied. An application's braces are what `hgl run` has written what starts it gives over (`hgl_cli`); `hgl test` supplies nothing, and the defaults stand |
//! | `run_config.start`, a property (core's CAP-7) | `(run_config).start()`, as any capability's (`endpoints`) |

use hgl_analyser::hir::{ConfigEntry, Expr};

use super::Emitter;
use super::calls::{changes, may_fail};
use super::names;
use super::statements::Scope;
use crate::runtime;

/// The fields of core's `RunConfig` (CAP-7), in the order the library's
/// `Bounds` and `RunConfig::Fields` declare them, which their designated
/// initializers follow.
const FIELDS: [&str; 3] = ["start", "end", "length"];

impl Emitter<'_> {
    /// The bounds the configuration `config` of an eval gives its run
    /// (RUN-6; core's CRUN-2: "`start` is inclusive and `end` exclusive
    /// ... `length` gives the end counted from the start the run has"):
    /// `::hglrt::testing::Bounds`, each field the configuration names in
    /// place. RUN-6: "a field the statement does not name is the
    /// injected configuration's", read from `inherited`, the C++ name of
    /// the `run_config` an integration test injects, where it has one;
    /// null otherwise. The entries are evaluated in the order written
    /// (EXPR-2): where more than one may fail, or one runs statements,
    /// each is held first, in that order.
    pub(super) fn run_bounds(
        &mut self,
        scope: &mut Scope,
        config: &[ConfigEntry],
        inherited: Option<&str>,
    ) -> String {
        let Some((steps, fields)) = self.config_fields(scope, config) else {
            return "{}".to_string();
        };
        let given: Vec<String> = FIELDS
            .iter()
            .filter_map(|field| {
                match fields.iter().find(|(named, _)| named == field) {
                    Some((_, written)) => Some(written.clone()),
                    None => inherited.map(|local| format!("{local}.{field}()")),
                }
                .map(|written| format!(".{field} = {written}"))
            })
            .collect();
        let bounds = format!("{}{{{}}}", names::testing("Bounds"), given.join(", "));
        held(steps, bounds)
    }

    /// The capability `run_config` an application or an integration test
    /// injects with the entries `config` (RUN-6): "`run_config` reads the
    /// configuration as supplied", here the entries written, which `hgl
    /// run` has written what starts an application gives over, and a
    /// field none names null.
    pub(super) fn run_config_value(&mut self, scope: &mut Scope, config: &[ConfigEntry]) -> String {
        let Some((steps, fields)) = self.config_fields(scope, config) else {
            return "{}".to_string();
        };
        let given: Vec<String> = FIELDS
            .iter()
            .filter_map(|field| {
                fields
                    .iter()
                    .find(|(named, _)| named == field)
                    .map(|(_, written)| format!(".{field} = {written}"))
            })
            .collect();
        let library = runtime::NAMESPACE;
        let value = format!(
            "::{library}::RunConfig{{::{library}::RunConfig::Fields{{{}}}}}",
            given.join(", ")
        );
        held(steps, value)
    }

    /// The C++ of each entry of `config`, by its field, and the
    /// statements that hold them first where they are held. `None`
    /// where an entry names no field of core's `RunConfig`, which the
    /// analyser has reported, or where one could not be written.
    fn config_fields(
        &mut self,
        scope: &mut Scope,
        config: &[ConfigEntry],
    ) -> Option<(String, Vec<(&'static str, String)>)> {
        let values = || config.iter().map(|entry| &entry.value);
        let ordered = values().filter(|value| may_fail(value)).count() > 1 || values().any(changes);
        let mut steps = String::new();
        let mut fields = Vec::new();
        for entry in config {
            let Some(field) = FIELDS.iter().find(|field| **field == entry.field) else {
                self.missing(entry.value.at, "this field of a run's configuration");
                return None;
            };
            let mut written = self.expression(scope, &entry.value);
            if ordered {
                let held = names::temporary("bound", self.temporary());
                steps.push_str(&format!("const auto {held} = {written}; "));
                written = held;
            }
            fields.push((*field, written));
        }
        Some((steps, fields))
    }

    /// The bounds a run of an application is given from its
    /// `run_config`, the C++ name `local` (RUN-6, core's CRUN-2): each
    /// field of it.
    pub(super) fn bounds_of(&self, local: &str) -> String {
        let given: Vec<String> = FIELDS
            .iter()
            .map(|field| format!(".{field} = {local}.{field}()"))
            .collect();
        format!("{}{{{}}}", names::testing("Bounds"), given.join(", "))
    }
}

/// True where an entry of `config` may fail or runs statements, and so
/// is held where it is evaluated among other parts (EXPR-2).
pub(super) fn config_held(config: &[ConfigEntry]) -> (usize, bool) {
    let values = || config.iter().map(|entry| &entry.value);
    (
        values().filter(|value: &&Expr| may_fail(value)).count(),
        values().any(changes),
    )
}

/// `value`, after the statements `steps` that hold its parts, as one
/// expression.
fn held(steps: String, value: String) -> String {
    if steps.is_empty() {
        value
    } else {
        format!("[&] {{ {steps}return {value}; }}()")
    }
}
