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

- A test declares no `state` or `cache`, injects nothing but an integration test's `run_config` (RUN-6), and has no hook, `yield` or `return`, in its block or a block inside it (`test.statement_phase`)
- Outside the call an `eval` or a `run` takes its statements are value code, with `assert` among them: a call of a node, a graph or a source is `call.role` (the code proposed), an operator selects among all its candidates and one that selects a temporal candidate is `call.role` too (GEN-5), a `const` parameter takes any value, and nothing is lifted; the call an `eval` or a `run` takes is read as composition is
- An eval of a target with a result is a trace of the result's type, bound with `let` and compared; a trace, or a value that holds one, stored in a field or given to a temporal function is `trace.escape`; one given to a `const fn` is the list of its publications there (TST-16)
- Left: an `eval` or a `run` outside a named test, which is not read; a trace returned

## TST-4

analyser: partial

- An `assert` takes a `bool` (`type.mismatch`) that is no connection (`connection.as_constant`)
- `==` and `!=` with a trace are the harness's own comparison, a `bool`, selected through no operator; the other side is a trace of the same cells

transpiler: planned

runner: planned

- Harness comparison and the mismatch report

## TST-5

parser: done

- `eval_expression`: `eval`, a configuration in braces where one is written, and a call of a name (`run_target`); no call after the word is `syntax.expected_token`

analyser: partial

- An eval's call is of a function of a module, the temporal definition where there are both (`eval.target`: an operator, a struct)
- The call's arguments are bound as any call's are, its generic arguments among them: a `const` parameter takes configuration and every other a trace
- Each positional or named pack capture takes its own trace (FN-8). A heterogeneous pack infers the types it replays from those traces or takes its written types.
- A cell is written as a `return` is: a T whole or a `delta<T>`, each by its type, for a list, a set, a dictionary, a struct and a tuple; a tuple that is a T is the whole tuple, a null position a published null, and one that is no T, `(null, 3)` of a `tuple<i64, i64>`, its delta, a tuple of its positions; a window's cell is the V that arrived, its delta (TYPE-16), a generic target's window once its arguments are known
- A generic target's parameters are found from the cells, or from a trace an eval gave; a tuple that names one, `tuple<T, T>`, has its cells read as any tuple's once they are found
- A trace is an argument of the eval's call and of no call inside it, which takes a value there (`type.mismatch`)
- Its configuration, `eval{length: 4s} target(...)`, is read before the call, each entry a field of core's `RunConfig` (RUN-6)
- Left: a generic target given structural cells other than a tuple's

transpiler: planned

## TST-6

transpiler: planned

runner: planned

- Dense traces: positions, `_` placeholders, a result ending at its last publication

## TST-7

analyser: partial

- A timed trace's keys are all durations or all datetimes; constant duration keys strictly increase (`trace.key_order`); it has no `_` (`trace.placeholder`); an eval's traces and its expectation share dense or timed notation (`trace.style`), and separate timed traces can use absolute or relative keys
- Left: the order of datetime keys, which are not evaluated

runner: planned

- Sparse expectations match the actual ticks and times

transpiler: planned

## TST-8

analyser: done

- An eval of a target with no result stands as a statement; bound or compared it is `eval.no_result`

runner: planned

- Eval builds core replay, the target and core record; fresh state for each invocation

## TST-9

transpiler: planned

runner: planned

- Arguments and profile admission validated before the graph starts

## TST-10

analyser: done

- `assert raises` takes one string literal whose value is an execution code of the catalogue (`test.raises_code`), `value.index_out_of_range` among them

transpiler: planned

runner: planned

- The block runs once; a matching escaping error passes after cleanup

## TST-11

transpiler: planned

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

transpiler: planned

## TST-15

runner: done

- When a run succeeds; identity, kind and outcome of each case
- A test is run by the executor the run is given, and is not run with none

transpiler: planned

## TST-16

analyser: partial

- A trace given to a `const fn`, an eval or a `let` bound to one, is a `list<tuple<datetime, delta<T>>>` there: it is held to the parameter's type and says what a generic parameter is; a window's holds its arrivals, V
- A sequence written as a `const fn`'s argument is the list it is written as
- Left: a value that holds a trace, `[seen]`, which is given as it is and not held to the parameter's type

transpiler: planned

runner: planned

- The recorded trace as that value

## TST-17

parser: done

- `integration_test name { ... }` is a `TestDecl`; `run`, a reserved word, begins a `RunExpr` wherever an operand is due
- `@timeout(5s)` on the line before an integration test is a `TimeoutAnnotation`: its argument is a duration, and it stands before nothing else

analyser: done

- An integration test is a test, marked as one, with the limit `@timeout(...)` gives it
- Its `run call` is read as an eval is, in real time; in a test that is none it is `run.position`
- It may inject `run_config` before its other statements, and read it in its value code (RUN-6)

transpiler: planned

runner: done

- An integration test is left out of a run unless `--integration` asks for them or `--test` names it
