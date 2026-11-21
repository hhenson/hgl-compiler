# Program execution

`program-execution.md`

## RUN-1

parser: done

- `run_expression`: `run`, a configuration in braces where one is written, and `run_target`, a name or a qualified name with generic arguments where written and the call's arguments; `eval_expression` the same

analyser: done

- A run statement's call is of an entry, a temporal function with no temporal parameters (`run.entry`), read as a graph's call of it is, so the entry's `const` parameters take constants
- It gives no configuration (RUN-6)
- The typed statement is `Module::run_body`

transpiler: done

- hgraph: a run statement gives the harness's `run` its entry, the entry's `const` arguments by name, and the defaults of the application's `inject run_config` as its configuration
- The run's output is the recording of the entry's result, a tuple for each publication in time order, and an entry with no result has none

## RUN-2

driver: partial

- `hgl run` runs the program's one application, or the one `--application` names, once the program is accepted and the target writes all the application needs
- `--arg NAME=VALUE` replaces an argument of its entry: the value, one HGL expression, is put in the run statement's text and the program checked again, so it is held to the type it replaces and is a constant as a written argument is
- `--config FIELD=VALUE` supplies a field of the run's configuration (RUN-6): written in the braces of the application's `inject run_config`, over the default of the field, or in an `inject run_config{...}` written before the run statement where the application has none
- Left: interfaces and providers

## RUN-3

analyser: done

- `run` is written in an application and in an integration test: in a test that is none it is `run.position`
- A run statement in a module is UNIT-14's

transpiler: partial

- hgraph: an application's `eval` is a simulation of hgraph's and its `run` a run in real time, each from the start and before the end its configuration gives
- An interrupt of `hgl run` is the host's stop of a run in real time, which ends it as its end does
- `run` in an integration test is that run in real time too
- Left: outside events

## RUN-4

transpiler: partial

- hgraph: an application's run and a test's eval are one run of the harness's, each with a graph, a store and a recording of its own; hgraph stops what started when a run fails
- Left: the phase a failure is of is not reported

## RUN-5

optimiser: planned

- No pass changes an observable trace, effect, failure or cleanup

transpiler: planned

- The generated program gives the same observations

## RUN-6

parser: done

- A configuration in braces after `eval` and `run`, and after a name in an `inject`, read as an operator's (`OperatorConfig`)

analyser: done

- Each entry names a property of core's `RunConfig` (`argument.unknown`), once (`argument.duplicate`), and is value code of its field's type: `ConfigEntry`
- `RunConfig` is a native type, which no call constructs (`call.not_callable`)
- A configuration on an application's run statement is `run.configuration`
- An application injects `run_config` and `global_state`; its `put` values may read `run_config` after its `inject`
- An integration test injects it before its other statements; an eval there names the injected local for the fields it leaves out (`ExprKind::Eval::inherited`)
- `inject run_config` in a function or a test that is no integration test, or after an integration test's other statements, is `inject.position`; braces on any other capability are `inject.configuration`

driver: done

- `hgl run --config FIELD=VALUE` is what starts an application: a field it gives takes the place of the application's default, an `end` takes out a default `length` and a `length` a default `end` (RUN-2)
- `hgl test` supplies nothing to an integration test's `run_config`: its defaults stand

transpiler: partial

- hgraph: an eval's entries are the harness's `RunConfig`, as an application's defaults are
- Left, reported: a read of `run_config`, and an eval in an integration test that injects it
