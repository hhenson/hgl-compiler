# Core

`spec/hgl_core/spec`. Required constants are evaluated by the analyser under
the same numeric rules the transpiler entries give.

## NUM-1

analyser: partial

- Result types of arithmetic on `i64` and `f64`, and of unary minus, in value functions, through core's candidates
- Left: temporal bodies

transpiler: planned

## NUM-2

transpiler: planned

## NUM-3

transpiler: planned

## NUM-4

transpiler: planned

## NUM-5

transpiler: planned

## NUM-6

analyser: partial

- In a value function a comparison outcome is a `CmpResult`, and is rejected where a bool or a number is expected
- Left: temporal bodies

transpiler: planned

## NUM-7

analyser: done

- A tolerance on `<=>` or an ordering is an option no candidate declares (`argument.unknown`); core declares no operation that orders a domain

optimiser: planned

- A tolerance grants no ordering law or rewrite

## NUM-8

analyser: partial

- In a value function, configuration selects among the candidates of the operator its symbol stands for, and an option left out takes the default its candidate declares
- Left: temporal bodies

optimiser: planned

- Folding alters no default or outcome

transpiler: planned

## NUM-9

analyser: partial

- A division, by name or by symbol, through core's candidates in a value function: its own type, or a `result` of it under `zero: .error`; `zero` is a generic value parameter, so one given no compile-time constant is rejected
- Left: temporal bodies

transpiler: planned

## NUM-10

transpiler: planned

## NUM-11

analyser: partial

- `eq_(a, b, abs: ..., rel: ...)` by name through core's candidates in a value function: the tolerances are options of the candidates for numbers, and of no other
- The same as the symbol's configuration, `a =={abs: ...} b`
- Left: temporal bodies

transpiler: planned

## NUM-12

analyser: partial

- Every division, by name or by symbol, through core's candidates in a value function: a division whose result is an `f64` admits each policy, one whose result is an `i64` or a `duration` only `.error` and `.fail`, and `.error` gives a `result`
- Left: temporal bodies

transpiler: planned

## OPS-1

analyser: partial

- In a value function a symbol is the call of the core operator it stands for, and admits the operand types that operator's candidates declare, core's and a module's own; any other pairing rejected (`operator.no_candidate`)
- Its operands are read as the call's arguments are, and the type its place expects is requested of the candidates
- Left: temporal bodies

transpiler: planned

## OPS-2

analyser: partial

- In a value function, ordering between two of one ordered type and equality between two values of one type, by symbol and by name through core's candidates
- An operand that is a generic parameter is compared as its declaration's requirements state (GEN-7)
- Two zoned datetimes, in a value function, a hook and a graph: `==` and `!=` select core's candidates for them, `utc` a `bool` they alone declare; no order of a zoned type; in a graph `utc` is no connection (EXPR-14)
- Left: a comparison with a nullable operand; other types in temporal bodies

transpiler: planned

## OPS-3

analyser: partial

- `a <=> b` and `compare_(a, b)` through core's candidates in a value function are a `CmpResult`
- Left: temporal bodies

## OPS-4

analyser: partial

- `convert<T>(value)` through core's candidates in a value function: the conversions core declares, and no other
- Left: a list of structs to a list of their parent; temporal bodies

transpiler: planned

## OPS-5

analyser: partial

- The Boolean connectives and `!` take bools, in a value function
- Left: temporal bodies

## OPS-6

analyser: done

- The type each conversion yields; `convert<list<B>>(values)` of a `list<A>`, `A` inheriting from the abstract `B`, is `list<B>` with no option

transpiler: planned

## OPS-7

analyser: done

- A conversion's options, and the `result` its checked error policy returns

transpiler: planned

## OPS-8

analyser: partial

- The options of `convert` through core's candidates in a value function: `invalid` selects `result<T, error>` or T and is given a compile-time constant, `rounding` and `nan` are parameters, and an option is admitted only for the conversions that declare it
- Left: temporal bodies

transpiler: planned

## OPS-9

analyser: done

- In a graph `invalid: .identity`, by symbol or by name, selects core's temporal candidate of `add_` or `sub_` for two operands of one type, where the value operations it requires exist for that type (GEN-16); without the option the value candidate is selected and lifted
- Core's two candidates are nodes, and are read as any node is

transpiler: planned

## OPS-10

transpiler: planned

## LIST-1

analyser: done

- `append` is core's native function, typed as any call; its list is a `var` parameter (FN-15)

transpiler: planned

## LIST-2

analyser: done

- `extend` is core's native function, typed as any call; its list is a `var` parameter (FN-15)

transpiler: planned

## LIST-3

transpiler: planned

## TEN-1

analyser: done

- The pairings are core's candidates, found as any operator's are: `date ± tenor`, `tenor ± tenor`, scaling in either order, negation, the order and `<=>`
- A tenor on the left of a date, an instant that is no date, and a division are `operator.no_candidate`

transpiler: planned

## TEN-2

transpiler: planned

## TEN-3

transpiler: planned

## TEN-4

transpiler: planned

## TEN-5

analyser: done

- `calendar` is an option of `+` and `-`, an argument of the candidate that takes it (EXPR-10); a calendar is a value `business_day` has a candidate for, and one that has none finds no candidate

transpiler: planned

## TEN-6

transpiler: planned

## TEN-7

analyser: done

- `rolling_rule` is an option of `+` and `-`, with `calendar` or alone; a rolling rule is a value `roll` has a candidate for

transpiler: planned

## TEN-8

transpiler: planned

## TEN-9

transpiler: planned

## END-1

transpiler: planned

## END-2

transpiler: planned

## END-3

analyser: partial

- `delta_value(x)` is a `delta<T>` for an input of T, and for a child of one (END-8), the value itself for a scalar
- For a nullable scalar it is the optional TYPE-16 gives it, and for a window the V that arrived
- Where its place takes `result<delta<T>, error>` it is of that result (`CheckedDelta`, the language's ERR-14), of any endpoint, a generic body's `T` among them, as core's `record` reads it
- A child's `delta_value` needs the child's admission (TIME-1)
- Left: proof that `x` is valid and modified

transpiler: planned

## END-4

transpiler: planned

## END-5

transpiler: planned

## END-6

analyser: done

- `last_modified` on an input or the node's output has type `datetime?`

transpiler: planned

## END-7

analyser: done

- `all_valid` is an endpoint operation on a list, tuple, struct, dictionary or window (TIME-9)

transpiler: planned

## END-8

analyser: partial

- A collection's child is an endpoint, valid or not: `valid`, `modified`, `delta_value` and `last_modified` take it, and `all_valid` where it is a collection; its value and delta are read where it is admitted (TIME-1); a read at an index known only when it runs is `CheckedChild` (ERR-14)
- A struct, a tuple or a fixed list read whole, a child among them, is admitted only where it is all valid by its own `all_valid`, which core's rejection case `quote_without_admission` checks; one at an index known only when it runs fails with `value.unset_read` where it is not all valid, or gives that `err` (ERR-14)

transpiler: planned

## END-9

analyser: partial

- `all_valid` takes a window, and its value is read in a hook, and given to a value function in composition, as a `list<V>` (TYPE-11, FN-12)
- `delta_value(x)` of a window is the V that arrived, read before it is all valid as after; of a window of a duration it is read where a `result` is taken, and a plain read is `payload.unguarded` (TYPE-11)
- `removed_value(x)` is core's declaration, given a window, read where a `result<V, error>` is taken, and a plain read is `payload.unguarded` for every window

transpiler: planned

## END-10

analyser: done

- A `ref` parameter of T is given a connection of T, and a `ref` result designates what its `return` names, a child of an input valid or not, read as `EndpointOf` or `ChildEndpointOf` with no payload read (FN-16)
- An assignment to a part of a `ref` result's `out` is `binding.immutable`, and `ref` on a `signal` parameter `ref.position` (FN-16, TYPE-12)

transpiler: planned

## CAP-1

analyser: done

- `inject` binds each capability but `out` to a local of the native type core declares for it; a property is read with its type and not assigned, and no call constructs the type (with TIME-8 and NATIVE-6)

transpiler: planned

## CAP-2

analyser: done

- `log` and `log_debug`, `log_info`, `log_warning`, `log_error` are core's declarations, typed as any call; `logger.level` is a `LogLevel`

transpiler: planned

## CAP-3

analyser: done

- A clock's four properties, each with its type

transpiler: planned

## CAP-4

analyser: done

- A scheduler's three properties, `is_scheduled_now` among them; `schedule` with a `duration` or a `datetime`, a `tag` and `on_wall_clock`; `un_schedule`, with a tag or without, and `reset`

transpiler: planned

- C++: hgraph 0.9.23's own `NodeScheduler` has it so, with no adapter: in the evaluation a wake brings about, `is_scheduled`, `next_scheduled_time` and `un_schedule` with no tag see only the wakes to come, and `is_scheduled_now` holds through the evaluation, after `un_schedule` or `reset` too

## CAP-5

analyser: done

- An alarm has no property, and a node injects it or `scheduler` (TIME-8)
- `schedule` on it takes a `duration` or a `datetime`, and neither a tag nor a clock

transpiler: planned

## CAP-6

analyser: partial

- `get`, `has` and `put` on the global store, through core's candidates: the key a `str` that is a constant expression (`argument.constant`, FN-1), and `get` gives a `result<V, error>`, with V written by the caller or inferred from the place
- The run's own key, `"run.output"`, written out as a key is rejected (`store.reserved_key`)
- A key written out is used with one type in a program: a `put` or a `get` of another type than an earlier use is `store.key_type` (TIME-8)
- Left: the run's key, and a key, where it is a `const` parameter, which is known when the graph is built

transpiler: partial

- C++ `get` returns the whole result to the recorded ERR-15 form.
- Left: nullable store puts.

## CAP-7

analyser: done

- `run_config` binds a local of core's native `RunConfig`, whose `start`, `end` and `length` are read with their types; no call constructs it (RUN-6)

transpiler: planned

## CONF-1

runner: planned

- Runs core's cases as written and adds no expectation

## CONF-2

runner: planned

- Conformance runs with core and its native providers only

## CONF-3

runner: planned

- `conformance` and `conformance.rejections` assembled from parts; never shipped

## CONF-4

runner: planned

- Conformance results reported apart from this repository's own tests

## CEVAL-1

runner: planned

- Eval uses core's designated replay and record

transpiler: planned

## CEVAL-2

runner: planned

- Typed replay entries for present publications; a dense placeholder adds no entry and no run bound

transpiler: planned

## CEVAL-3

transpiler: planned

## CEVAL-4

transpiler: planned

## CEVAL-5

runner: planned

- A fresh run, with its own node state and recording, for each eval; dense results built by TST-6

transpiler: planned

## CRUN-1

runner: planned

- A run of one entry with its `const` arguments under the configuration it is given: built, started, evaluated and stopped once

transpiler: planned

## CRUN-2

transpiler: planned

runner: planned

- `start` inclusive and `end` exclusive; what a null `start` and a null `end` mean in each mode

## CRUN-3

transpiler: planned

runner: planned

- Evaluation time constant in a cycle and later in each by a microsecond or more; the earliest start

## CRUN-4

runner: planned

- Simulation runs each cycle without waiting and ends when nothing is scheduled; the same run gives the same publications

transpiler: planned

## CRUN-5

runner: planned

- Real time runs a cycle when the host clock reaches it

transpiler: planned

## CRUN-6

runner: planned

- Every node that started is stopped, in reverse; a failure names its node and phase, and the run has no output

transpiler: planned

## CRUN-7

runner: planned

- One store for a run, which holds what its application put there when the graph is built; nothing outside the program puts a value there

transpiler: planned

## CRUN-8

analyser: partial

- The run's own key written out as a key of the global store is rejected (`store.reserved_key`)
- Left: a key that is a `const` parameter

transpiler: planned

runner: planned

- The run's output, a `list<tuple<datetime, delta<T>>>` recorded under the run's own key

## CEVAL-6

runner: planned

- A failed run or a missing required recording fails the eval; neither is an empty result

## MAP-1

transpiler: planned

- #369 tracks fixed lists and #370 tracks native forms.

## MAP-2

transpiler: planned

- #369 tracks fixed lists and #370 tracks native forms.

## MAP-3

transpiler: planned

- #369 and #370.

## MAP-4

transpiler: planned

- #370.

## MAP-5

transpiler: planned

- #370.

## MAP-6

transpiler: planned

- #370.

## MAP-7

transpiler: planned

- #369 tracks fixed lists and #370 tracks native forms.

## MAP-8

transpiler: planned

- #370.

## MAP-9

transpiler: planned

- #369 tracks fixed lists and #370 tracks native forms.

## MAP-10

transpiler: planned

- #369 tracks fixed lists and #370 tracks native forms.

## RED-1

transpiler: planned

- #369 tracks fixed lists and #370 tracks native forms.

## RED-2

transpiler: planned

- #369 tracks fixed lists and #370 tracks native forms.

## RED-3

transpiler: planned

- #369 tracks fixed lists and #370 tracks native forms.

## RED-4

transpiler: planned

- #370.

## RED-5

transpiler: planned

- #369 tracks fixed lists and #370 tracks native forms.

## RED-6

transpiler: planned

- #369 and #370.

## RED-7

transpiler: planned

- #367.

## SET-1

transpiler: planned

- #370.

## SET-2

transpiler: planned

- #370.
