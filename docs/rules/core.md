# Core

`spec/hgl_core/spec`. Required constants are evaluated by the analyser under
the same numeric rules the transpiler entries give.

## NUM-1

analyser: done

- Selects core's numeric arithmetic and unary negation candidates in value functions, node hooks and graph composition.

transpiler: done

- Lowers numeric arithmetic to the selected core candidate, retaining result type, floor
  division, modulo and floating-point special cases.

## NUM-2

transpiler: done

- Uses checked signed arithmetic and the runtime's exact mixed comparison. Floating
  payloads use the SDK binary64 type.

## NUM-3

transpiler: done

- Names the selected zero policy as a native template argument. The provider supplies
  signed infinities, NaNs, error data or escaping failures as selected.

## NUM-4

transpiler: done

- Passes absolute and relative tolerances to the selected numeric equality native, which
  applies the exact and configured comparisons.
- The runtime raises `numeric.invalid_tolerance` for a negative or non-finite
  tolerance on `==` or `!=`.

## NUM-5

transpiler: done

- Numeric equality uses the native's special-value comparisons and overflow-safe finite
  tolerance test.

## NUM-6

analyser: done

- A full comparison yields `CmpResult` in value functions, node hooks and graph composition. A bool result place rejects.

transpiler: done

- Routes numeric full comparison to the selected candidate and preserves the distinct
  `CmpResult` enum, including unordered.

## NUM-7

analyser: done

- A tolerance on `<=>` or an ordering is an option no candidate declares (`argument.unknown`); core declares no operation that orders a domain

optimiser: planned

- A tolerance grants no ordering law or rewrite

## NUM-8

analyser: done

- Selects the same numeric candidate and its configuration defaults in value functions, node hooks and graph composition.

optimiser: planned

- Folding alters no default or outcome

transpiler: done

- Uses the same resolved native identity and policy for a symbol, named call, and lifted
  value helper. The runtime publishes nonfinite values as payloads.

## NUM-9

analyser: done

- Resolves configured division by name or by symbol in value functions, node hooks and graph composition. A compile-time `zero: .error` selects a tagged result.

transpiler: done

- Specializes division by its zero policy and retains the selected numeric or tagged
  result type and error code.

## NUM-10

transpiler: done

- Keeps an error from checked division as tagged data. A lifted division publishes that
  data whole, and an explicit result read propagates the error in its own phase.

## NUM-11

analyser: done

- Numeric `eq_` and `ne_` declare `abs` and `rel` in braces. `eq_{abs: ...}(a, b)` and `a =={abs: ...} b` bind the same option. A tolerance named in parentheses rejects (`argument.unknown`).
- Graph construction accepts scalar tolerances and rejects connected ones. Value functions and hooks evaluate them in the current invocation (EXPR-14).

transpiler: done

- Passes comparison tolerances as ordinary arguments in value code and as scalar
  configuration of a lifted comparison.

## NUM-12

analyser: done

- The result type determines the admitted policies in value functions, node hooks and graph composition. In each, `f64` admits every policy, `i64` and `duration` admit `.error` and `.fail`, and `.error` selects a tagged result.

transpiler: done

- Writes native division with the analysed zero policy for the applicable numeric or
  duration signature.

## OPS-1

analyser: partial

- In a value function a symbol is the call of the core operator it stands for, and admits the operand types that operator's candidates declare, core's and a module's own; any other pairing rejected (`operator.no_candidate`)
- Its operands are read as the call's arguments are, and the type its place expects is requested of the candidates
- Numeric arithmetic and text concatenation use the same candidate selection in node hooks and graph composition.
- Left: a calendar given as a connection to a date symbol (#493).

transpiler: done

- Writes the selected arithmetic, text, instant, duration and tenor operations through
  core candidates for supported value and temporal forms.

## OPS-2

analyser: partial

- In a value function, ordering between two of one ordered type and equality between two values of one type, by symbol and by name through core's candidates
- An operand that is a generic parameter is compared as its declaration's requirements state (GEN-7)
- Two zoned datetimes, in a value function, a hook and a graph: `==` and `!=` select core's candidates for them. The ordinary `utc` parameter can be a lifted input (EXPR-14). A zoned type has no ordering.
- Left: a comparison with a nullable operand; other types in temporal bodies

transpiler: done

- Writes core's ordered comparison and value equality, including zoned equality and the
  `utc` option.

## OPS-3

analyser: done

- `a <=> b` and `compare_(a, b)` give the selected candidate's `CmpResult` in value functions, node hooks and graph composition.

## OPS-4

analyser: partial

- Resolves declared conversions in value functions, node hooks and graph composition. A list of concrete structs converts to a list of their abstract parent (OPS-6).
- Left: the enum conversions awaiting Core declarations and target support (#514).

transpiler: done

- Emits the selected core conversion with its result type and native template arguments.
  An admitted integer literal can be written directly as an f64.

## OPS-5

analyser: done

- Boolean forms take bools. Composition selects `and_`, `or_` or `not_`, and an unparenthesized chain of three or more operands selects `all_` or `any_` over the whole chain.

transpiler: done

- Value code and hooks preserve short-circuiting. Composed Boolean calls use ordinary lifting and give each connected operand its own input.

## OPS-6

analyser: done

- The type each conversion yields; `convert<list<B>>(values)` of a `list<A>`, `A` inheriting from the abstract `B`, is `list<B>` with no option

transpiler: done

- Uses the runtime conversion natives and the generated conversion of a list of concrete
  structs to a list of their abstract parent.

## OPS-7

analyser: done

- A conversion's options, and the `result` its checked error policy returns

transpiler: done

- Emits rounding, NaN and invalid-input policies through the selected conversion. The
  checked conversion preserves its ordinary tagged result.

## OPS-8

analyser: done

- In value functions, node hooks and graph composition, the compile-time `invalid` option selects T or `result<T, error>`. The candidate declares which options it admits.

transpiler: done

- Passes `rounding` and `nan` as arguments and instantiates the selected native using
  the analysed `invalid` policy.

## OPS-9

analyser: done

- In a graph `invalid: .identity`, by symbol or by name, selects core's temporal candidate of `add_` or `sub_` for two operands of one type, where the value operations it requires exist for that type (GEN-16); without the option the value candidate is selected and lifted
- Core's two candidates are nodes, and are read as any node is

transpiler: done

- Emits the HGL temporal identity candidate with its value operations specialized,
  including alternate admission when one input is invalid.

## OPS-10

transpiler: done

- Uses the duration and instant natives with microsecond rounding and checked ranges for
  the selected arithmetic.

## LIST-1

analyser: done

- `append` is core's native function, typed as any call; its list is a `var` parameter (FN-15)

transpiler: done

- Passes the list place to native `append` and preserves earlier retained values.

## LIST-2

analyser: done

- `extend` is core's native function, typed as any call; its list is a `var` parameter (FN-15)

transpiler: done

- Passes the place and ordered update values to native `extend`.

## LIST-3

transpiler: done

- Uses a mutable place for list natives and uses retained copies where needed to
  preserve their earlier values (FN-13).

## TEN-1

analyser: done

- The pairings are core's candidates, found as any operator's are: `date ± tenor`, `tenor ± tenor`, scaling in either order, negation, the order and `<=>`
- A tenor on the left of a date, an instant that is no date, and a division are `operator.no_candidate`

transpiler: done

- Writes tenor arithmetic, comparison and date movement through the selected native or
  HGL candidate.

## TEN-2

transpiler: done

- Uses the native count arithmetic, including mixed-unit and range errors.

## TEN-3

transpiler: done

- Uses count-wise tenor comparison, preserving the unordered result for incompatible
  counts or forms.

## TEN-4

transpiler: done

- Calls core's calendar movement candidate and its month-then-day native with checked
  final dates.

## TEN-5

analyser: done

- `calendar` is an option of `+` and `-`, an argument of the candidate that takes it (EXPR-10); a calendar is a value `business_day` has a candidate for, and one that has none finds no candidate
- A connected calendar is an input of the selected value candidate in either spelling (EXPR-14 and FN-6).

transpiler: done

- Binds the calendar argument or core's default and specializes the generic calls of the
  calendar predicate.

## TEN-6

transpiler: done

- Emits core's calendar candidate for movement by business days, including the starting
  business-day choice.

## TEN-7

analyser: done

- `rolling_rule` is an option of `+` and `-`, with `calendar` or alone; a rolling rule is a value `roll` has a candidate for

transpiler: done

- Emits core's movement and rolling calls with the selected calendar and rolling rule.

## TEN-8

transpiler: done

- Emits core's rolling candidates and the calendar primitives on which they depend.

## TEN-9

transpiler: done

- Writes the native tenor-to-text conversion including mixed month/day units, signed
  counts and zero.

## END-1

transpiler: done

- Retains validity and payload across silence and publishes present scalar values,
  including null and nonfinite values.

## END-2

transpiler: done

- Reports scalar ticks even when values repeat. A newly selected temporal child sees
  pre-existing valid data on startup.

## END-3

analyser: partial

- `delta_value(x)` is a `delta<T>` for an input of T, and for a child of one (END-8), the value itself for a scalar
- For a nullable scalar it is the optional TYPE-16 gives it, and for a window the V that arrived
- A result place takes `CheckedDelta`, including in a generic body such as core's `record`. A nullable place takes the same checked delta and discards its error (ERR-14, ERR-15).
- A child's `delta_value` needs the child's admission (TIME-1)
- A direct input's delta read needs its validity admission (TIME-1, TIME-4), as a child's does. This reads no whole collection payload, so it does not require every child to be valid.
- An admitted result or nullable form needs no static proof of a tick; it handles a cycle with no delta (ERR-14, ERR-15).

transpiler: partial

- Reads current scalar, nullable, collection and window deltas, with checked result
  and nullable forms where selected.
- Left: Reports delta reads outside supported endpoint forms. The temporal
  forms are under TYPE-9.

## END-4

analyser: done

- `invalidate` takes only the node's own `out`; a direct value or reference input is `endpoint.argument`.

transpiler: partial

- Emits result invalidation, including clearing collection results and reference
  designations, and permits observation of the transition without a modified guard.
- Left: unsupported output shapes follow TYPE-9.

## END-5

analyser: partial

- `activate` and `passivate` in `stop` reject as `input.lifecycle` (TIME-5); the input binding is passive there.

transpiler: partial

- Writes input activity control and suppresses silent no-change collection updates. A window result holds its last arrival until the handler's evaluation finishes (END-9).
- Left: unsupported temporal shapes follow TYPE-9.

## END-6

analyser: done

- `last_modified` on an input or the node's output has type `datetime?`

transpiler: done

- Reads the last valid publication timestamp of a supported endpoint. Null payload
  publications retain a timestamp, invalidated endpoints do not.

## END-7

analyser: done

- `all_valid` takes a list, tuple, struct, dictionary or window. A set rejects with `endpoint.argument` (TIME-9).

transpiler: partial

- Uses published-empty collection validity and one-level `all_valid`, excluding optional
  children in supported bundle forms.
- Left: deeper nested dictionary and list schemas follow TYPE-9.

## END-8

analyser: partial

- A collection's child is an endpoint, valid or not: `valid`, `modified`, `delta_value` and `last_modified` take it, and `all_valid` where it is a collection; its value and delta are read where it is admitted (TIME-1); a read at an index known only when it runs is `CheckedChild` (ERR-14)
- An inferred `let` holds its child and a value use reads it. A checked child,
  dictionary or unsized-list read takes the requested value, result or nullable form.
- A statically known struct, tuple or fixed list read whole still requires its own
  `all_valid` (TIME-1), as core's `quote_without_admission` rejection checks.

transpiler: partial

- Emits child reads and endpoint observations for supported list, tuple, struct and
  dictionary projections. Observes nested dictionary entries, including where a
  parent key or the final key is absent.
- Captures each held child's position or key at binding. A valid but incomplete
  dictionary entry fails the whole read, and a wholly unset entry is omitted.
- Left: The unsupported child forms under EXPR-7 and TYPE-9 are reported, including
  direct payload reads of nested dictionary entries and unsupported operations on a
  field or child.

## END-9

analyser: partial

- `all_valid` takes a window, and its value is read in a hook, and given to a value function in composition, as a `list<V>` (TYPE-11, FN-12)
- `delta_value(x)` of a window is the V that arrived, read before it is all valid as after. A duration window's delta takes a result or nullable place, and a plain read is `payload.unguarded` (TYPE-11).
- `removed_value(x)` is core's declaration, given a window, read where a `result<V, error>` is taken, and a plain read is `payload.unguarded` for every window

transpiler: partial

- Maps rolling windows to the SDK, writes their arrival publication and reads validity,
  deltas and removed values in supported contexts.
- A node's body observes its latest staged window arrival, including its
  retention and endpoint metadata (TIME-7).
- Left: Reports a handler selecting on its own window result, a native
  call borrowing that result in its `when`, and unsupported window reads
  and operations.

## END-10

analyser: done

- A `ref` parameter of T is given a connection of T, and a `ref` result designates what its `return` names, a child of an input valid or not, read as `EndpointOf` or `ChildEndpointOf` with no payload read (FN-16)
- An assignment to a part of a `ref` result's `out` is `binding.immutable`, and `ref` on a `signal` parameter `ref.position` (FN-16, TYPE-12)

transpiler: partial

- A reader observes the designated input, fixed-list child or dictionary entry through
  the reference port, including changes caused by rebinding or removing the target.
- A timer can wake a node that designates an ordinary input. A follower-only wake refreshes
  the result without running the node's handlers.
- Left: Unsupported reference destinations follow FN-16.

## CAP-1

analyser: done

- `inject` binds each capability but `out` to a local of the native type core declares for it; a property is read with its type and not assigned, and no call constructs the type (with TIME-8 and NATIVE-6)

transpiler: done

- Uses the declared native capability type and emits its properties as ordinary reads in
  their admitted scopes.

## CAP-2

analyser: done

- `log` and `log_debug`, `log_info`, `log_warning`, `log_error` are core's declarations, typed as any call; `logger.level` is a `LogLevel`

transpiler: done

- Supplies the run logger and emits its property read, the native log call and the
  standard HGL message helpers.

## CAP-3

analyser: done

- A clock's four properties, each with its type

transpiler: done

- Supplies the evaluation clock and its four distinct observations, including the cycle
  interval.

## CAP-4

analyser: done

- A scheduler's three properties, `is_scheduled_now` among them; `schedule` with a `duration` or a `datetime`, a `tag` and `on_wall_clock`; `un_schedule`, with a tag or without, and `reset`

transpiler: done

- Supplies scheduler state and writes schedule, cancellation and reset using the SDK
  adapter. Distinguishes due scheduler wakes from input ticks.
- Drops a nonfuture dated request in graph time, except for the node's first
  evaluation when asked at the current time in `start`. Logs the node's
  display name and the requested time on its first drop, then counts later
  drops at stop (TIME-8).

## CAP-5

analyser: done

- An alarm has no property, and a node injects it or `scheduler` (TIME-8)
- `schedule` on it takes a `duration` or a `datetime`, and neither a tag nor a clock

transpiler: done

- Supplies the alarm adapter and emits duration or datetime scheduling, retaining the
  earliest outstanding wake.
- Uses the same dated-wake rules and per-node warning as the scheduler (TIME-8).

## CAP-6

analyser: partial

- `get`, `has` and `put` on the global store, through core's candidates: the key a `str` that is a constant expression (`argument.constant`, FN-1), and `get` gives a `result<V, error>`, with V written by the caller or inferred from the place
- The run's own key, `"run.output"`, written out as a key is rejected (`store.reserved_key`)
- A key written out is used with one type in a program: a `put` or a `get` of another type than an earlier use is `store.key_type` (TIME-8)
- Left: a key resolved from a `const` parameter can name `run.output` or bypass the source check for inconsistent types.

transpiler: partial

- Reads a store entry as a whole result before applying the recorded form, and writes
  supported ordinary payloads before a run, at graph build and in hooks.
- A stored nullable value reads back through an explicitly typed `get<V?>`. A
  stored null is successful and the key remains present.
- Reading an absent store key gives `value.missing_key`, the same code as a
  strict dictionary lookup (ERR-6).
- Left: a temporal child graph using the surrounding store is declined (CRUN-7).

## CAP-7

analyser: done

- `run_config` binds a local of core's native `RunConfig`, whose `start`, `end` and `length` are read with their types; no call constructs it (RUN-6)

transpiler: done

- Exposes the supplied start, end and length as nullable properties, distinct from the
  bounds resolved for execution.

## CAP-8

transpiler: planned

- The meanings of the clock's, the scheduler's and the logger's members are the engine's, reached
  through the SDK as CAP-2 to CAP-5 write them. Nothing here cites or checks them yet: the
  examples of core's comments run them once examples run as tests (#458).

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

transpiler: partial

- Creates an eval entry wired to core's replay and record and binds ordinary const
  arguments for the run. Replay of a recorded trace uses its timestamps and deltas
  (TST-3, TST-5, CEVAL-2).
- Left: Replay and recording require a representable temporal schema (CEVAL-3, CEVAL-4).

## CEVAL-2

runner: planned

- Typed replay entries for present publications; a dense placeholder adds no entry and no run bound

transpiler: done

- Builds timestamped replay entries from literal traces, using the run start, dense
  offsets, and silent cells with no event.

## CEVAL-3

transpiler: partial

- Replays scalar publications, whole or delta collection entries, and window arrivals as
  their forms require.
- Replays a supported nested dictionary.
- Replays supported concrete struct applications with generic type arguments.
  Remaining schema limits are under TYPE-9 and TYPE-11.

## CEVAL-4

transpiler: partial

- Records independent timestamp and delta pairs, including an empty recording. A cycle
  that only ages a window adds no entry.
- Records supported concrete struct applications with generic type arguments.
  Remaining schema limits are under TYPE-9 and TYPE-16.

## CEVAL-5

runner: planned

- A fresh run, with its own node state and recording, for each eval; dense results built by TST-6

transpiler: done

- Each eval creates a fresh simulation with its own graph, store and recording.

## CRUN-1

runner: planned

- A run of one entry with its `const` arguments under the configuration it is given: built, started, evaluated and stopped once

transpiler: partial

- Each emitted entry wires a fresh run for the simulation or real-time host.
- Left: an empty time interval wires the entry but skips executor construction,
  start and stop.

## CRUN-2

transpiler: done

- Resolves inclusive start and exclusive end from the supplied configuration, including
  length and defaults, and refuses simultaneous end and length before graph
  construction.

runner: planned

- `start` inclusive and `end` exclusive; what a null `start` and a null `end` mean in each mode

## CRUN-3

transpiler: partial

- Uses the runtime's microsecond cycle resolution and sentinel bounds. Dense traces use
  the same smallest step.
- Left: The out-of-range bounds failure lacks the `run.bounds` code.

runner: planned

- Evaluation time constant in a cycle and later in each by a microsecond or more; the earliest start

## CRUN-4

runner: planned

- Simulation runs each cycle without waiting and ends when nothing is scheduled; the same run gives the same publications

transpiler: done

- The application and eval hosts use the SDK simulation clock and run until the next
  work is beyond the end or the schedule is empty.

## CRUN-5

runner: planned

- Real time runs a cycle when the host clock reaches it

transpiler: done

- The application and integration hosts use the real-time clock to wait for scheduled
  work and run due cycles.

## CRUN-6

runner: planned

- Every node that started is stopped, in reverse; a failure names its node and phase, and the run has no output

transpiler: partial

- The host stops the graph on success or failure and returns no result for a failed run.
  It preserves captured failure provenance for the supported hook phases.
- Left: Construction has no distinct HGL provenance tag where the underlying runtime
  reports a build failure. Cleanup errors use the runtime's failure report.

## CRUN-7

runner: planned

- One store for a run, which holds what its application put there when the graph is built; nothing outside the program puts a value there

transpiler: partial

- Creates one store for a run, populated by the application before graph construction,
  and executes its wiring-time store calls in order.
- Left: a temporal branch using its surrounding graph's store is declined by the target (FN-9, CAP-6).

## CRUN-8

analyser: partial

- The run's own key written out as a key of the global store is rejected (`store.reserved_key`)
- Left: a key supplied as a `const` parameter can name `run.output` (CAP-6).

transpiler: partial

- Adds recording to a result-bearing entry and retrieves the timestamped deltas after
  stop. Outputless entries have no recording.
- Left: Unsupported record types follow CEVAL-4. The reserved `run.output` check covers
  literal keys at source checking. Keys resolved at graph construction remain unchecked.

runner: planned

- The run's output, a `list<tuple<datetime, delta<T>>>` recorded under the run's own key

## CEVAL-6

runner: planned

- A failed run or a missing required recording fails the eval; neither is an empty result

## MAP-1

transpiler: partial

- Fixed-size list `map` runs its HGL candidate. The keyed and growing-list candidates are declared and C++ reports them as unsupported (#370).

## MAP-2

transpiler: partial

- A fixed-size map binds positional and named inputs, splits the fitted list inputs, and supplies whole inputs to each child. An explicitly specialized operator can supply the callback's member types (FN-17). An omitted callback input uses the callback's default (FN-8, UNIT-8). Native forms await #370.

## MAP-3

analyser: done

- A call with only whole arguments reports `operator.no_candidate`.

## MAP-4

transpiler: planned

- #370.

## MAP-5

transpiler: planned

- #370.

## MAP-6

transpiler: partial

- Fixed-size maps compose positions `0..<N`. The join of lengths for growing lists awaits #370.

## MAP-7

transpiler: partial

- A fixed-size map composes a child for each position at build time. Native child lifetimes await #370.

## MAP-8

transpiler: planned

- #370.

## MAP-9

transpiler: partial

- A fixed-size map supplies each child's position to the default `key` parameter or the renamed parameter. Native keys await #370.

## MAP-10

transpiler: partial

- Fixed-size results hold the children's publications and propagate an error leaving a child. A callback with no result needs the Core declaration tracked in [hgl_core#147](https://github.com/hhenson/hgl_core/issues/147). Native result shapes await #370.

## RED-1

transpiler: partial

- Fixed-size list reduction runs both HGL candidates. The native growing-list ordered fold also accepts distinct member and accumulator types. Growing-list tree, set and dictionary reduction await #370.

## RED-2

transpiler: partial

- A fixed-size fold composes reads of every position. Growing ordered reduction skips invalid members and publishes the seed for an empty list. Native tree candidates await #370.

## RED-3

transpiler: partial

- A fixed-size tree fold uses the first position as its initial value and visits the remaining positions in order. An associative call on a list of size zero rejects. Native grouping awaits #370.

## RED-4

analyser: done

- A set or dictionary candidate requires both laws. An ordered function does not admit a dictionary reduction.

transpiler: planned

- #370.

## RED-5

transpiler: partial

- Ordered folds start from their required zero and visit positions in order. The growing-list target wires a capture-free callback and guards each step on member validity. An omitted zero rejects during candidate selection.
- Left: callbacks that capture an enclosing connection (FN-14, [hgraph#1914](https://github.com/hhenson/hgraph/issues/1914)).

## RED-6

transpiler: partial

- The fixed-size tree fold combines the positions without adding the supplied identity. Native implementations await #370.

## RED-7

transpiler: planned

- #367.

## SET-1

transpiler: done

- The value candidate gives the dictionary's keys. The temporal candidate projects hgraph's key set, which publishes membership changes and is silent on a value-only update.

## SET-2

transpiler: planned

- #370.

## OUT-1

transpiler: done

- `write` selects standard output by default or standard error with `.err`. A value function or a node handler writes the given bytes to that stream. The run's publications remain in its independent record.

## OUT-2

transpiler: done

- `flush: true` flushes the selected stream before `write` returns.

## FMT-1

analyser: partial

- Direct native calls take a constant `str` pattern followed by positional and named packs. A generic value pattern and dependent argument types are checked together at their concrete call.
- An enclosing function can pass or capture an ordinary `const` parameter as the pattern. A generic value body carries the demand through deferred operator selection to the concrete call (FN-1, #695, #704).
- A `const` parameter given beside forwarded packs, `format(fmt_str, ...args, ...kwargs)`, through forwarding calls, generic relays given a pack entry, and candidates an instance selects, is an input of each instance whose body reads it at compile time, and is read there with the entries (FN-19, #706). So is one an enclosing value function, node, graph, option or operator candidate gives with its own `const`.
- Left: the graph and diagnostic candidates (#368, #434). The C++ target declines a graph that gives its own `const` parameter to a value call as the pattern (#745). A pattern in a `let`-bound anonymous function is not checked, forwarded or not (#750).

## FMT-2

analyser: done

- The pattern is split into literal text and positional or named places. Doubled braces become literal braces.

## FMT-3

analyser: partial

- A direct value call is checked for braces, missing and unused arguments, and specifiers. A dependent string is checked in its concrete instance. Diagnostics use `format.syntax`, `format.argument`, `format.unused` and `format.specifier`.
- The current compile-time pattern reader builds up to 64 KiB of text per result. The C++ target declines calls whose patterns exceed that support limit (#695).
- The analyser checks demanded const inputs and concrete argument conversions after selecting the value candidate. GEN-7 reports failures that depend on the instance at its outside call. Lowering uses the same concrete values to select the checked body.
- A pattern reached through forwarded calls is checked in the instance the entries make, and a failure is reported at the call outside every generic body that makes it, with the code of the same direct call (GEN-7, #706).
- Left: patterns made by general constant function evaluation (#695).

## FMT-4

analyser: partial

- A specifier is checked against the argument's type at compile time.
- The zero flag on `bool` or `str` and the integer `c` specifier reject with `format.specifier`. Boolean width and alignment, including an explicit fill, still apply.

transpiler: partial

- The C++ target gives primitive fields to pinned fmt with compiled specifiers. It emits checked chrono directives with fractional truncation on `%S`, `%T` and `%[S]`, duration totals and padding on the whole field.
- Boolean width and alignment without the zero flag use the pinned fmt formatter.
- A `str` at `{:?}` uses the same HGL literal writer as a string inside a collection (FMT-5).
- Plain string fields measure width and precision in Unicode code points. Default `f64` fields with a width apply the sign and padding to their FMT-5 text. Both NaN signs start from `nan` (OPS-6).
- Left: the alternate `#` form with an `f64` width, and width or precision combined with `?`.

## FMT-5

analyser: partial

- A user type uses its selected `convert<str>` and rejects if there is no candidate.
- A nullable field checks the text conversion of its payload type. The target writes that text when present and `null` when absent.
- Left: tuple and struct text candidates (#434, #512).

transpiler: partial

- Value defaults are written in Core's scalar forms. The growing list, fixed list, set and dictionary candidates use each member's conversion and traversal order, quote and escape strings and render absent members as `null`.
- Left: tuple and struct text candidates (#434, #512).

## FMT-6

transpiler: planned

- #434.

## FMT-7

transpiler: done

- A `Marked<T>` renders its payload with the payload's specifier or uses `not_valid` for an absent payload.

## FMT-8

transpiler: planned

- #434.

## FMT-9

analyser: partial

- Value calls admit `{wall_time}` and reject `{engine_time}` with `format.argument`. A marked place accepts `Marked<T>`.
- Left: a graph supplies evaluation time and the arguments' tick state (#434).

transpiler: partial

- `{wall_time}` reads the process clock. A marked place writes the configured tick prefix when its argument ticked.
- Left: a graph supplies evaluation time and the arguments' tick state (#434).

## DBG-1

transpiler: planned

- #434.

## DBG-2

transpiler: planned

- #434.

## DBG-3

transpiler: planned

- #434.

## DBG-4

transpiler: planned

- #434.

## DBG-5

transpiler: planned

- #434.

## DBG-6

transpiler: planned

- #434.

## DBG-7

transpiler: planned

- #434.

## DBG-8

transpiler: planned

- #434.

## DBG-9

transpiler: planned

- #434.
