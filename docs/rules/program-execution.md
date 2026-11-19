# Program execution

`program-execution.md`

## RUN-1

parser: done

- `run_expression`: `run`, a call, and then a run configuration

analyser: done

- A run statement's first argument is a call of an entry, a temporal function with no temporal parameters (`run.entry`), read as a graph's call of it is, so the entry's `const` parameters take constants
- Its second is a `RunConfig` of core's, and it takes no third
- The typed statement is `Module::run_body`

transpiler: done

- hgraph: a run statement gives the harness's `run` its entry, the entry's `const` arguments by name, and its configuration
- The run's output is the recording of the entry's result, a tuple for each publication in time order, and an entry with no result has none

## RUN-2

driver: partial

- `hgl run` runs the program's one application, or the one `--application` names, once the program is accepted and the target writes all the application needs
- `--arg NAME=VALUE` replaces an argument of its entry and `--config FIELD=VALUE` a field of its configuration: the value, one HGL expression, is put in the run statement's text and the program checked again, so it is held to the type it replaces and is a constant as a written argument is
- Left: a field of a configuration that is not written as `RunConfig(...)`; interfaces and providers

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
