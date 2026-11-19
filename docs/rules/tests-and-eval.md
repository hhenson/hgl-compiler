# Tests and eval

`tests-and-eval.md`

## TST-1

parser: done

- An unnamed test context holds functions, structs, enums and named tests only

analyser: done

- One test scope across parts; exports and operator implementations rejected there

## TST-2

analyser: done

- Production names no test-only declaration; a test-only declaration takes no name of its module; duplicate test names rejected

transpiler: planned

- Artifacts omit test-only declarations

## TST-3

analyser: partial

- A test declares no `state` or `cache`, injects nothing, and has no hook, `yield` or `return`, in its block or a block inside it (`test.statement_phase`)
- Its block is read as a graph's body is, with `assert` among its statements
- An eval of a target with a result is a trace of the result's type, bound with `let` and compared; a trace, or a value that holds one, stored in a field or given to a temporal function is `trace.escape`; one given to a `const fn` is the list of its publications there (TST-16)
- Left: an `eval` outside a named test, which is not read; a trace returned

## TST-4

analyser: partial

- An `assert` takes a `bool` (`type.mismatch`) that is no connection (`connection.as_constant`)
- `==` and `!=` with a trace are the harness's own comparison, a `bool`, selected through no operator; the other side is a trace of the same cells

transpiler: partial

- hgraph: the harness compares the time and value of each publication in dense and timed traces
- A set delta compares additions and removals separately
- A cell written whole is compared with the value the result holds once its cycle has published (TST-5)
- Trailing placeholders add no publications (TST-6)
- Left: the mismatch report says only that the assertion does not hold

runner: planned

- Harness comparison and the mismatch report

## TST-5

parser: done

- `eval_expression`: a call and then a run configuration, neither with a label

analyser: partial

- An eval's first argument is a call of a function of a module, the temporal definition where there are both (`eval.target`: an operator, a struct, a value)
- The call's arguments are bound as any call's are, its generic arguments among them: a `const` parameter takes configuration and every other a trace
- A cell is written as a `return` is: a T whole or a `delta<T>`, each by its type, for a list, a set, a dictionary, a struct and a tuple; a tuple that is a T is the whole tuple, a null position a published null, and one that is no T, `(null, 3)` of a `tuple<i64, i64>`, its delta, a tuple of its positions
- A generic target's parameters are found from the cells, or from a trace an eval gave; a tuple that names one, `tuple<T, T>`, has its cells read as any tuple's once they are found
- A trace is an argument of the eval's call and of no call inside it, which takes a value there (`type.mismatch`)
- A second argument is a `RunConfig` of core's, and an eval takes no third
- Left: a generic target given structural cells other than a tuple's

transpiler: partial

- hgraph: a cell written whole is `_harness.whole(value)`, and a tuple's `_harness.whole(value, shape=...)`, how hgraph's bundle takes each position (`bundled`): an input publishes a null position as a published null
- An expectation's is compared with the value the result holds after its cycle, read from the result's port (`_hold`), and one written as a delta with the delta recorded
- An unsized list's whole cell replays as the `TSLDelta` that replaces what the input held, the tail it has not removed (`_core.whole_delta`); core's `record` records hgraph's `TSLDelta`
- The value held follows the owner's decision on hhenson/hgl_spec#126, ahead of the pinned text: a struct, a tuple or a list is whole only where each part is valid, an optional part that is not being null; where a required part is not valid there is no whole value, and no cell written whole equals it: it is read as a part is (`part_value`)
- A tuple's cell written as its delta is that tuple of its positions, published and compared as recorded
- Left: a whole cell of a tuple with a position that is composite, a list or a struct, which is reported, as a struct's with a composite field is

## TST-6

transpiler: done

- hgraph: each present dense cell publishes at its position in smallest steps from the run's start
- `null` publishes hgraph's `Null` and `_` is silence
- The recording ends at its last publication
- Comparison matches each publication at its offset from its own run start

runner: planned

- Dense traces: positions, `_` placeholders, a result ending at its last publication

## TST-7

analyser: partial

- A timed trace's keys are all durations or all datetimes; constant duration keys strictly increase (`trace.key_order`); it has no `_` (`trace.placeholder`); an eval's traces and its expectation share dense or timed notation (`trace.style`), and separate timed traces can use absolute or relative keys
- Left: the order of datetime keys, which are not evaluated

runner: planned

- Sparse expectations match the actual ticks and times

transpiler: done

- hgraph: replay and expectation keys are duration offsets from the run's start or absolute datetimes
- Computed keys must strictly increase
- Input replay rejects a key before the run's start (CEVAL-2)
- The harness compares every publication's time and value without allocating intervening cells

## TST-8

analyser: done

- An eval of a target with no result stands as a statement; bound or compared it is `eval.no_result`

runner: planned

- Eval builds core replay, the target and core record; fresh state for each invocation

## TST-9

transpiler: partial

- hgraph: a structural input cell written whole replaces what the input held: a dictionary's removes each key it held that the cell has not, a set's each member, and a tuple's sets each position, a null one to a published null; one written as a delta is that change
- Left: what a whole value nested in another replaces

runner: planned

- Arguments and profile admission validated before the graph starts

## TST-10

analyser: done

- `assert raises` takes one string literal whose value is an execution code of the catalogue (`test.raises_code`)

transpiler: partial

- hgraph: the block is run once, in a `try`; an error that escapes with the code passes, an HGL error or the one behind a node's failure in an `eval`, and the test goes on; a block that completes fails the test at the assertion, and one that lets out another code fails where that was raised
- Left: cleanup that fails

runner: planned

- The block runs once; a matching escaping error passes after cleanup

## TST-11

transpiler: partial

- hgraph: a failed assertion in the block, and an error that is no HGL error with a code, do not satisfy it; an integration test past its time limit has failed whatever its block then does
- Left: a crash, and a test that does not end once it is stopped, which end the interpreter

runner: planned

- What cannot satisfy `raises`

## TST-12

runner: done

- `# expect-error` annotations read from line comments; the owner of each case

diagnostics: done

- The catalogue's source categories and codes, which an annotation may name

## TST-13

parser: done

- An error at the end of the file points at the end of the file

runner: done

- Rejection owners excluded; diagnostics and annotations matched one to one

## TST-14

runner: done

- Invalid metadata and surviving-source errors stop the run, and nothing is then executed; selectors; counts of rejection cases and of tests passed, failed and not run

transpiler: partial

- hgraph: a test whose interpreter does not start or stops early, or whose module does not load, is not run and has not failed
- An integration test is held to a time limit
- Left: a limit on how long a test that is no integration test may run

## TST-15

runner: done

- When a run succeeds; identity, kind and outcome of each case
- A test is run by the executor the run is given, and is not run with none

transpiler: partial

- hgraph: a test is named `module.path::name`, and one given to the host has an outcome: passed, failed with the statement that failed, or not run with the reason
- Left: the tests that need a form the target does not write yet, which are not run

## TST-16

analyser: partial

- A trace given to a `const fn`, an eval or a `let` bound to one, is a `list<tuple<datetime, delta<T>>>` there: it is held to the parameter's type and says what a generic parameter is
- A sequence written as a `const fn`'s argument is the list it is written as
- Left: a value that holds a trace, `[seen]`, which is given as it is and not held to the parameter's type

transpiler: done

- hgraph: a `const fn` receives the recording's publications at their absolute instants, including published nulls
- Silent positions add no entries
- A written trace uses the earliest start for dense positions and duration keys and preserves absolute keys; a cell written whole is held as the delta its publication records (`_core.whole_delta`), the delta an input replays

runner: planned

- The recorded trace as that value

## TST-17

parser: done

- `integration_test name { ... }` is a `TestDecl`; in a test's block `run(` begins a `RunExpr`, and outside one `run` is a name
- `@timeout(5s)` on the line before an integration test is a `TimeoutAnnotation`: its argument is a duration, and it stands before nothing else

analyser: done

- An integration test is a test, marked as one, with the limit `@timeout(...)` gives it
- Its `run(call, config)` is read as an eval is, in real time; in a test that is none it is `run.position`

transpiler: done

- hgraph: `run(call, config)` in an integration test is the eval's run in real time, from the host clock's present time where its configuration gives no start
- An integration test has a time limit, the one `@timeout(...)` gives it or a minute: one still running then is stopped, and has failed at the statement it was at

runner: done

- An integration test is left out of a run unless `--integration` asks for them or `--test` names it
