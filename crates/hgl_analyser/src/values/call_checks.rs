//! What every call is held to once each of its arguments is given to its
//! parameter ([`Typer::finished_call`]): the place a `var` parameter
//! takes, a trace, what a `const` parameter takes, a capability, a
//! reference, a call on the global store, and the duration a wake is
//! asked for with (FN-1, FN-9, FN-15, TIME-8, TST-3, TYPE-12). Part of
//! the typing pass ([`super`]).

use hgl_diagnostics::{Category, Location};
use hgl_parser::HasSpan;

use super::arguments::Takes;
use super::evals::holds_trace;
use super::fit::whole;
use super::inference::placed;
use super::statements::child_loop_write;
use super::{Selected, Typer};
use crate::declared::substitute_within;
use crate::hir::{Expr, ExprKind, Form, Hook, LocalId, Value, ValueCategory};
use crate::program::{LocalKind, Program, Role};
use crate::types::{Argument, ConstantValue, Detail, Scalar, Type};
use crate::{codes, error};

impl<'e> Selected<'e> {
    /// The selected callee and the argument slots the call gives it.
    pub(super) fn of(program: &'e Program, kind: &'e ExprKind) -> Option<Self> {
        let ExprKind::Call {
            callee, arguments, ..
        } = kind
        else {
            return None;
        };
        let (module, declaration) = callee.declaration()?;
        Some(Self {
            module,
            declaration,
            generics: callee.generics(),
            signature: match program.modules[module].details.get(declaration) {
                Some(Detail::Callable(signature)) => Some(signature),
                _ => None,
            },
            arguments,
        })
    }
}

impl<'a> Typer<'a> {
    /// What every call is held to once it is built, with each argument
    /// given to its parameter. It is run where a call of a function and
    /// a call of an operator's candidate are built, and so for a call
    /// written by name, with generic arguments, by a symbol, as a link
    /// of a chain `a + b + c`, or by a compound assignment.
    pub(super) fn finished_call(&mut self, kind: &ExprKind, evaluating: bool) {
        // A call of a function or a candidate: an operation a
        // declaration requires has no candidate here, and a call that
        // is not read no parameters to judge.
        let Some(selected) = Selected::of(self.program, kind) else {
            self.store_call(kind);
            return;
        };
        let assigned = self.places_given(&selected);
        // A `var` call assigns ordinary values (FN-15). Its writes in
        // a graph are held to CTRL-14 and CTRL-15, like other assignments.
        if self.graph {
            for (local, at, inside) in assigned {
                if self.dynamic.is_some_and(|outside| local < outside) {
                    self.diagnostics
                        .emit(child_loop_write(&self.locals[local], at));
                } else {
                    self.assigned_category(local, (ValueCategory::Value, at), inside);
                }
            }
        }
        self.store_call(kind);
        self.capabilities_given(&selected);
        self.fixed_arguments(&selected);
        self.traces_given(&selected, evaluating);
        self.store_keys(&selected);
        self.wake_durations(&selected);
    }

    /// TIME-8: "A wake asked for with a `duration` takes one more than
    /// zero, but in `start`, which takes zero too: `schedule(scheduler,
    /// 0s)` there asks for a wake at the start, the node's first
    /// evaluation. A duration that is not admitted, a negative one in
    /// `start` included, rejects where the compiler sees its value, a
    /// literal or a constant (UNIT-13), with `schedule.duration_bounds`,
    /// and elsewhere fails the call as the program runs, with
    /// `schedule.non_positive_duration`." Core's CAP-5 holds the alarm to
    /// the same.
    ///
    /// A wake is asked by a call of one of core's candidates of
    /// `schedule`, given the scheduler or the alarm. The duration the
    /// compiler sees is a compile-time constant, of literals and
    /// constants; any other, an input, a local or a `const` parameter,
    /// is checked as the program runs. A value function given the
    /// capability is no hook: a negative duration is admitted in none,
    /// and rejects there too, but whether a zero there is "in `start`"
    /// the rule does not say, and it is left to the target.
    fn wake_durations(&mut self, selected: &Selected) {
        let &Selected {
            module,
            declaration,
            arguments,
            ..
        } = selected;
        let Some(signature) = selected.signature else {
            return;
        };
        let program = self.program;
        if Some(module) != program.core()
            || program.modules[module].declarations[declaration].name != "schedule"
        {
            return;
        }
        // The capability it is given, the scheduler or the alarm.
        let timer = ["scheduler", "alarm"]
            .iter()
            .filter_map(|capability| program.capability_type(capability))
            .any(|timer| {
                signature
                    .parameters
                    .first()
                    .is_some_and(|parameter| parameter.ty == timer)
            });
        if !timer {
            return;
        }
        let duration = Type::Scalar(Scalar::Duration);
        let given = arguments.iter().find(|argument| {
            argument
                .parameter
                .and_then(|index| signature.parameters.get(index))
                .is_some_and(|parameter| parameter.ty == duration)
        });
        let Some(given) = given else {
            return;
        };
        // The expression as it is written: the innermost node of the
        // argument's span, and not the argument that holds it.
        let at = given.value.at;
        let written = (at.file == self.file)
            .then(|| {
                self.declaration
                    .syntax
                    .descendants()
                    .filter(|node| node.span() == at.span)
                    .last()
            })
            .flatten();
        let Some(ConstantValue::Duration(micros)) =
            written.and_then(|node| self.read_constant(&node, Some(&duration)).ok())
        else {
            return;
        };
        let starting = self.hook == Some(Hook::Start);
        let admitted = micros > 0 || (micros == 0 && (starting || self.hook.is_none()));
        if admitted {
            return;
        }
        let message = if micros < 0 {
            "a wake is asked for with a duration more than zero, or zero in `start`: this one is \
             less than zero"
        } else {
            "a wake is asked for with a duration more than zero outside `start`: this one is zero"
        };
        self.diagnostics.emit(
            error(
                Category::Type,
                codes::SCHEDULE_DURATION_BOUNDS,
                given.value.at,
                message.to_string(),
                "TIME-8",
            )
            .with_note("`schedule(scheduler, 0s)` in `start` asks for a wake at the start"),
        );
    }

    /// FN-9: "A graph injects `global_state` and no other, and uses it
    /// where the graph is built: each call on it is evaluated once
    /// then ... and is given no connection (`connection.as_constant`)."
    /// A call on the store is one given the capability the graph
    /// injects: it is run where the graph is built, when a connection
    /// has no value.
    fn store_call(&mut self, kind: &ExprKind) {
        if !self.graph || self.testing {
            return;
        }
        let ExprKind::Call { arguments, .. } = kind else {
            return;
        };
        // The capability itself, or a `let` it is bound to: `let store =
        // global_state` and then `put(store, ...)` is a call on it too.
        let on_store = arguments
            .iter()
            .any(|argument| self.capability_of(&argument.value).is_some());
        if !on_store {
            return;
        }
        for argument in arguments {
            if self.category(&argument.value) == ValueCategory::Connection {
                self.diagnostics.emit(
                    error(
                        Category::Phase,
                        codes::CONNECTION_AS_CONSTANT,
                        argument.value.at,
                        "a call on the store a graph injects is run where the graph is built: \
                         this is a connection, and it takes a value known then"
                            .to_string(),
                        "FN-9",
                    )
                    .with_note(
                        "a constant or a `const` parameter is known where the graph is built",
                    ),
                );
            }
        }
    }

    /// FN-15: "A call gives a `var` parameter a place, as the left of an
    /// assignment is (EXPR-3): a `var`, a `state`, or a `cache`, or a
    /// place inside one, that holds an ordinary value ... What is
    /// read-only rejects, a `let`, a parameter that is not `var`, a
    /// constant, or an iteration binding (`binding.immutable`); so does
    /// what is no place, a literal or what a call gives
    /// (`argument.place`)." And "A function that declares one is not
    /// lifted (FN-6) ... No argument of such a call is a connection
    /// (`connection.as_constant`)".
    pub(super) fn places_given(&mut self, selected: &Selected) -> Vec<(LocalId, Location, bool)> {
        let &Selected {
            module,
            declaration,
            generics,
            arguments,
            ..
        } = selected;
        let program = self.program;
        let Some(signature) = selected.signature else {
            return Vec::new();
        };
        if !signature.parameters.iter().any(|parameter| parameter.var) {
            return Vec::new();
        }
        let callee = &program.modules[module].declarations[declaration].name;
        // What a parameter's type is at this call, the callee's generic
        // parameters in place.
        let own: Vec<Location> = signature
            .generics
            .iter()
            .map(|generic| generic.at)
            .collect();
        let map: Vec<(Location, &Argument)> = own.iter().copied().zip(generics).collect();
        let mut reports = Vec::new();
        // The places the call assigns: the local each is rooted in,
        // where it is written, and whether it is inside the local's
        // value.
        let mut assigned: Vec<(LocalId, Location, bool)> = Vec::new();
        for argument in arguments {
            let Some(parameter) = argument
                .parameter
                .and_then(|index| signature.parameters.get(index))
            else {
                continue;
            };
            let at = argument.value.at;
            if self.category(&argument.value) == ValueCategory::Connection {
                reports.push(error(
                    Category::Phase,
                    codes::CONNECTION_AS_CONSTANT,
                    at,
                    format!(
                        "`{callee}` has a `var` parameter and is not lifted: this is a connection, and it takes an ordinary value"
                    ),
                    "FN-15",
                ));
                continue;
            }
            if !parameter.var {
                continue;
            }
            // The root of the place: `book` of `book.orders[0]`.
            let root = argument.value.place_root();
            let takes = format!("the `var` parameter `{}` of `{callee}`", parameter.name);
            // "that holds an ordinary value of the parameter's type":
            // the call gives the place what the parameter holds, so
            // the two are one type. A place of `i64` takes no `i64?`
            // back, though an `i64` is a value of `i64?`.
            let taken = placed(substitute_within(&parameter.ty, &map), &own);
            let held = &argument.value.ty;
            let assignable = matches!(
                &root.kind,
                ExprKind::Local(local) if matches!(
                    self.locals[*local].kind,
                    LocalKind::Var | LocalKind::State | LocalKind::Cache
                )
            );
            if assignable && whole(&taken) && whole(held) && *held != taken {
                reports.push(error(
                    Category::Type,
                    codes::TYPE_MISMATCH,
                    at,
                    format!(
                        "this place is `{held}`, and {takes} is `{taken}`: the call gives \
                             the place what the parameter holds, so the two are one type"
                    ),
                    "FN-15",
                ));
                continue;
            }
            match &root.kind {
                ExprKind::Local(local) | ExprKind::DeferredPresent(local) => {
                    let declared = &self.locals[*local];
                    if !matches!(
                        declared.kind,
                        LocalKind::Var | LocalKind::State | LocalKind::Cache
                    ) {
                        reports.push(
                            error(
                                Category::Name,
                                codes::BINDING_IMMUTABLE,
                                at,
                                format!(
                                    "`{}` is {} and cannot be assigned, as {takes} is: only a `var`, a `state` or a `cache` can",
                                    declared.name,
                                    declared.kind.describe()
                                ),
                                "FN-15",
                            )
                            .with_related(declared.at, "declared here"),
                        );
                    } else if let Some((what, rule)) = Self::read_only_inside(&argument.value) {
                        // NATIVE-6, TYPE-16: a property of a native
                        // value or of a delta is read-only whatever
                        // holds the value, as an assignment has it.
                        reports.push(error(
                            Category::Name,
                            codes::BINDING_IMMUTABLE,
                            at,
                            format!(
                                "this is a property of {what}, and a property is read and not assigned, as {takes} is"
                            ),
                            rule,
                        ));
                    } else {
                        // The call assigns the place, and is held to
                        // what an assignment there is.
                        let inside = !matches!(&argument.value.kind, ExprKind::Local(_));
                        assigned.push((*local, at, inside));
                    }
                }
                ExprKind::Constant { .. } => reports.push(error(
                    Category::Name,
                    codes::BINDING_IMMUTABLE,
                    at,
                    format!("a constant cannot be assigned, as {takes} is"),
                    "FN-15",
                )),
                // A member awaiting its base type is checked with its
                // instance. What is unread says nothing sure (ERR-9).
                ExprKind::Unread | ExprKind::DeferredMember { .. } => {}
                _ => reports.push(
                    error(
                        Category::Type,
                        codes::ARGUMENT_PLACE,
                        at,
                        format!("{takes} takes a place, and this is none"),
                        "FN-15",
                    )
                    .with_note(
                        "a place is a `var`, a `state` or a `cache`, or a field or an element of one",
                    ),
                ),
            }
        }
        for report in reports {
            self.diagnostics.emit(report);
        }
        assigned
    }

    /// The global store: "The key `"run.output"` is the run's own,
    /// where it keeps its output: a program that names it rejects
    /// (`store.reserved_key`)."
    ///
    /// A key is the `key` of one of the store's operations: core's own
    /// `get`, `has` and `put` (CAP-6), which are the candidates of core
    /// that take the store. A function of a program that is given the
    /// store reads and writes it through those, so its own parameters
    /// are no keys. A key written out is judged here. One that is a
    /// `const` parameter of the function it is in is known where the
    /// graph is built, which no pass reads yet.
    fn store_keys(&mut self, selected: &Selected) {
        let &Selected {
            module, arguments, ..
        } = selected;
        let Some(signature) = selected.signature else {
            return;
        };
        let Some(store) = self.program.capability_type("global_state") else {
            return;
        };
        // An operation of the store is core's, and takes the store.
        if Some(module) != self.program.core()
            || !signature
                .parameters
                .iter()
                .any(|parameter| parameter.ty == store)
        {
            return;
        }
        let reserved: Vec<Location> = arguments
            .iter()
            .filter(|argument| {
                argument
                    .parameter
                    .and_then(|index| signature.parameters.get(index))
                    .is_some_and(|parameter| parameter.constant && parameter.name == "key")
            })
            .filter(|argument| {
                matches!(
                    &argument.value.kind,
                    ExprKind::Literal(Value::Str(key)) if key == RUN_OUTPUT_KEY
                )
            })
            .map(|argument| argument.value.at)
            .collect();
        for at in reserved {
            self.diagnostics.emit(
                error(
                    Category::Injectable,
                    codes::STORE_RESERVED_KEY,
                    at,
                    format!(
                        "the key `\"{RUN_OUTPUT_KEY}\"` is the run's own: its output is kept there"
                    ),
                    "TIME-8",
                )
                .with_note("a program's own values go under another key"),
            );
        }
    }

    /// TST-3: a trace "is bound with `let`, compared ..., and given to
    /// a `const fn` as an argument. It is not a temporal argument
    /// outside an eval". `evaluating` is true for the call an `eval`
    /// stands for, whose temporal parameters take traces (TST-5).
    ///
    /// A value that holds a trace at any depth, `[seen]`, is held to
    /// the same: wrapping a trace does not take it where it may not go.
    /// A trace a `const fn` is given is the list of its publications
    /// there (TST-16), and is held to the parameter's type where the
    /// argument is read: see `publications`. A value that holds one,
    /// `[seen]`, is given as it is and not held to the parameter's type.
    fn traces_given(&mut self, selected: &Selected, evaluating: bool) {
        let &Selected {
            module,
            declaration,
            arguments,
            signature,
            ..
        } = selected;
        // FN-1: a value function, or a value candidate.
        let value = matches!(
            self.program.modules[module].declarations[declaration]
                .kind
                .role(),
            Some(Role::Value)
        );
        if value {
            return;
        }
        let constant = |index: usize| {
            signature.is_some_and(|signature| {
                signature
                    .parameters
                    .get(index)
                    .is_some_and(|parameter| parameter.constant)
            })
        };
        let escaped: Vec<Location> = arguments
            .iter()
            .filter(|argument| holds_trace(&argument.value.ty))
            .filter(|argument| {
                // An eval's own trace, given to a parameter that takes
                // one.
                let taken = evaluating
                    && matches!(argument.value.ty, Type::Trace { .. })
                    && argument.parameter.is_some_and(|index| !constant(index));
                !taken
            })
            .map(|argument| argument.value.at)
            .collect();
        for at in escaped {
            self.trace_escape(at, Takes::Parameters);
        }
    }

    /// FN-1: "a `const` parameter takes what is fixed when the graph is
    /// built. In composition that is any ordinary value. In a hook and
    /// in a value function it is a constant expression: of literals,
    /// enum members, generic value parameters, and the enclosing
    /// function's own `const` parameters."
    ///
    /// A graph's own rule, that a `const` parameter is given no
    /// connection, is `constants_of`'s. This is the other half: where
    /// values are those of one evaluation, in a hook, a source or a
    /// value function. Value code, a test's outside an `eval` or a `run`
    /// and an application's statements, is evaluated once, before the
    /// run's graph is built (TST-3, UNIT-14): what it gives is fixed.
    fn fixed_arguments(&mut self, selected: &Selected) {
        if self.graph || self.value_code {
            return;
        }
        let &Selected {
            arguments,
            signature,
            ..
        } = selected;
        let Some(signature) = signature else {
            return;
        };
        let given: Vec<(Location, String)> = arguments
            .iter()
            .filter_map(|argument| {
                let parameter = signature.parameters.get(argument.parameter?)?;
                // What was not read has been reported, or is for a
                // later part (ERR-9).
                let judged = parameter.constant
                    && !matches!(argument.value.kind, ExprKind::Unread)
                    && !self.is_fixed(&argument.value);
                judged.then(|| (argument.value.at, parameter.name.clone()))
            })
            .collect();
        for (at, parameter) in given {
            self.diagnostics.emit(
                error(
                    Category::Type,
                    codes::ARGUMENT_CONSTANT,
                    at,
                    format!(
                        "`{parameter}` is a `const` parameter, and takes what is fixed when the \
                         graph is built: this is a value of one evaluation"
                    ),
                    "FN-1",
                )
                .with_note(
                    "a literal, an enum member, a generic value parameter, or a `const` \
                     parameter of the enclosing function is fixed",
                ),
            );
        }
    }

    /// True when `value` is a constant expression (FN-1): "of literals,
    /// enum members, constants (UNIT-13), generic value parameters, and
    /// the enclosing function's own `const` parameters."
    fn is_fixed(&self, value: &Expr) -> bool {
        match &value.kind {
            ExprKind::Literal(_)
            | ExprKind::EnumMember { .. }
            | ExprKind::GenericValue { .. }
            | ExprKind::Constant { .. } => true,
            // A parameter's local has its position among the parameters.
            ExprKind::Local(local) => self
                .signature
                .parameters
                .get(*local)
                .is_some_and(|parameter| parameter.constant),
            ExprKind::Widen(inner) => self.is_fixed(inner),
            // ERR-12 and ERR-15 read the argument's existing value.
            ExprKind::Raised { result, .. } | ExprKind::OrNull { result, .. } => {
                self.is_fixed(result)
            }
            ExprKind::NullRead { value, .. } => self.is_fixed(value),
            // `Side(N)`, of what is fixed: the member is fixed too.
            ExprKind::EnumFrom { value, .. } => self.is_fixed(value),
            ExprKind::Unary { operand, .. } | ExprKind::DeferredNegation { operand, .. } => {
                self.is_fixed(operand)
            }
            ExprKind::Binary { left, right, .. } | ExprKind::DeferredBinary { left, right, .. } => {
                self.is_fixed(left) && self.is_fixed(right)
            }
            // A symbol over constants: `-1`, `1 + 2`.
            ExprKind::Call {
                arguments,
                form: Form::Binary(_) | Form::Unary(_),
                ..
            } => arguments
                .iter()
                .all(|argument| self.is_fixed(&argument.value)),
            ExprKind::Tuple(entries) | ExprKind::Sequence(entries) => {
                entries.iter().all(|entry| self.is_fixed(entry))
            }
            ExprKind::Pairs(entries) => entries
                .iter()
                .all(|(key, value)| self.is_fixed(key) && self.is_fixed(value)),
            _ => false,
        }
    }

    /// TIME-8: "giving one to a parameter of its type [is a use]". A
    /// parameter written as a generic parameter or `any` is of no one
    /// type: it takes any, and what the function does with its
    /// argument, keep it in a list say, it would do with the
    /// capability. Such a parameter takes none (owner's decision of
    /// 2026-11-16 on hhenson/hgl_spec#73, question 5).
    ///
    /// The call is read, so each argument is known for the parameter it
    /// is given to: of a function, or of the candidate an operator's
    /// call or a symbol selected. A capability given to a parameter of
    /// another type written out does not fit it, and is reported as any
    /// value is (EXPR-5).
    fn capabilities_given(&mut self, selected: &Selected) {
        let &Selected {
            arguments,
            signature,
            ..
        } = selected;
        let Some(signature) = signature else {
            return;
        };
        let mut given: Vec<(Location, String, String)> = Vec::new();
        for argument in arguments {
            let (Some(parameter), Some(capability)) = (
                argument
                    .parameter
                    .and_then(|index| signature.parameters.get(index)),
                self.capability_of(&argument.value),
            ) else {
                continue;
            };
            // `T?` and `any?` take what `T` and `any` take, or null.
            let mut written = &parameter.ty;
            while let Type::Nullable(inner) = written {
                written = inner;
            }
            let open = match written {
                Type::Parameter { .. } => "a generic parameter",
                Type::Scalar(Scalar::Any) => "`any`",
                _ => continue,
            };
            given.push((
                argument.value.at,
                self.locals[capability].name.clone(),
                format!("`{}` is written as {open}", parameter.name),
            ));
        }
        for (at, capability, parameter) in given {
            self.diagnostics.emit(
                error(
                    Category::Injectable,
                    codes::CAPABILITY_AS_VALUE,
                    at,
                    format!(
                        "`{capability}` is a capability, and is given only to a parameter of \
                         its type: {parameter}"
                    ),
                    "TIME-8",
                )
                .with_note("a parameter that takes any type takes no capability"),
            );
        }
    }
}

/// The key of the global store under which a run keeps its output
/// (core's CRUN-8).
const RUN_OUTPUT_KEY: &str = "run.output";
