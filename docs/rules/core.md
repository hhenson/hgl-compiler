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
- Selects Core's unsigned-octet ordering candidates when both operands are `bytes`.
- An operand that is a generic parameter is compared as its declaration's requirements state (GEN-7)
- Two zoned datetimes, in a value function, a hook and a graph: `==` and `!=` select core's candidates for them. The ordinary `utc` parameter can be a lifted input (EXPR-14). A zoned type has no ordering.
- Left: a comparison with a nullable operand; other types in temporal bodies

transpiler: done

- Writes core's ordered comparison and value equality, including zoned equality and the
  `utc` option.
- Compares `bytes` lexicographically as unsigned octets (TYPE-1).

## OPS-3

analyser: done

- `a <=> b` and `compare_(a, b)` give the selected candidate's `CmpResult` in value functions, node hooks and graph composition.

## OPS-4

analyser: done

- Resolves declared conversions in value functions, node hooks and graph composition. A list of concrete structs converts to a list of their abstract parent (OPS-6).
- Selects the enum candidates for a written enum destination or an enum operand. Other pairings are rejected at the operator.

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
- The enum natives use the member's HGL name and number, including explicit and sparse numbers.

## OPS-7

analyser: done

- A conversion's options, and the `result` its checked error policy returns

transpiler: done

- Emits rounding, NaN and invalid-input policies through the selected conversion. The
  checked conversion preserves its ordinary tagged result.
- Proposed: an integer without a member gives `convert.invalid` in the
  checked result or raises it under `.fail`.

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

## OPS-11

analyser: done

- Selects `join` for one list of strings or for a positional pack of strings. Its `separator` configuration defaults to one space. A connection as the separator rejects (FN-18, EXPR-14).

transpiler: done

- Writes Core's HGL loops using the list or fitted pack entries in order.

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
- A `"run.output"` key the text fixes is rejected (`store.reserved_key`):
  a constant expression, or a `const` argument or generic value given
  through the calls that reach it. An instance rejection is reported at
  the call that fixes the key (GEN-7). Source is the earlier place; the
  target's operation checks every key.
- A key that can differ between the calls of a cycle is not checked in
  source: one computed on the way round, exchanged with another `const`
  parameter in any function of the cycle, or given anew by a literal or a
  default. A cycle closed through an operator's candidates is one. No
  instance is made for each key such a cycle reaches (TIME-8).
- A known key is used with one type in a program. A `put` or a `get` of a
  different type than an earlier use is `store.key_type` (TIME-8).
- Left: a helper outside the cycle that also formats its key is checked
  in source for each key an instance of the cycle gives it (#877).
- Left: two forms the text fixes are judged as cycles, so their key fails
  at the operation: one key handed round through two `const` parameters
  of a function, and a cycle that only an operator candidate no instance
  selects would close (#885).

transpiler: partial

- Reads a store entry as a whole result before applying the recorded form, and writes
  supported ordinary payloads before a run, at graph build and in hooks.
- A stored nullable value reads back through an explicitly typed `get<V?>`. A
  stored null is successful and the key remains present.
- Reading an absent store key gives `value.missing_key`, the same code as a
  strict dictionary lookup (ERR-6).
- Reading or writing a key whose stored type differs gives `store.key_type`.
- Every store operation checks its key as it is evaluated, and fails with
  `store.reserved_key` on `"run.output"`. The check follows the call's
  arguments, each evaluated once in the order written (EXPR-2), and comes
  before the operation runs. Only core's recorder, which keeps the run's
  output under that key, writes it (TIME-8).
- Earlier places fail a key the graph fixes: a graph's call as the graph
  is built, and a node's keys its configuration fixes, its own and those it
  hands helpers, when the SDK attaches it, before any start hook. A key
  that can differ between the calls of a cycle is left to the operation.
- A selected branch may use the surrounding store and its captured aliases
  during its startup prefix (FN-9, CRUN-7).
- Left: store-dependent child wiring and startup outside the supported
  branch form (#541).

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
- Replays supported concrete struct applications with generic type and value arguments
  (`i64`, `bool`, finite `f64`, enum, and `Type`).
  Remaining schema limits are under TYPE-9 and TYPE-11.

## CEVAL-4

transpiler: partial

- Records independent timestamp and delta pairs, including an empty recording. A cycle
  that only ages a window adds no entry.
- Records supported concrete struct applications with generic type and value arguments
  (`i64`, `bool`, finite `f64`, enum, and `Type`).
  Remaining schema limits are under TYPE-9 and TYPE-16.

## CEVAL-5

runner: planned

- A fresh run, with its own node state and recording, for each eval; dense results built by TST-6

transpiler: done

- Each eval creates a fresh simulation with its own graph, store and recording.

## CRUN-1

runner: planned

- A run of one entry with its `const` arguments under the configuration it is given: built, started, evaluated and stopped once

transpiler: done

- Each run builds a fresh graph and starts and stops it in simulation or real time,
  including when the time interval contains no cycles.

## CRUN-2

transpiler: done

- Resolves inclusive start and exclusive end from the supplied configuration, including
  length and defaults, and refuses simultaneous end and length before graph
  construction.

runner: planned

- `start` inclusive and `end` exclusive; what a null `start` and a null `end` mean in each mode

## CRUN-3

transpiler: done

- Uses the runtime's microsecond cycle resolution and sentinel bounds. Dense traces use
  the same smallest step.
- An out-of-range start or end fails with `run.bounds` before graph construction.

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
- Executes the ordinary store prefix of a selected branch before its
  directly wired scalar node starts. Every activation uses the same run
  store (FN-9, CAP-6).
- Left: store-dependent child wiring and startup outside that branch
  form (#541).

## CRUN-8

analyser: partial

- A `"run.output"` key the text fixes is rejected (`store.reserved_key`),
  including where a call supplies a fixed `const` argument or generic
  value (CAP-6).
- Left: a helper outside a cycle that also formats its key (CAP-6, #877);
  two forms judged as cycles fail at the operation (CAP-6, #885).

transpiler: partial

- Adds recording to a result-bearing entry and retrieves the timestamped deltas after
  stop. Outputless entries have no recording.
- A program's key that names the recording key fails with
  `store.reserved_key` at its store operation, and earlier as the graph is
  built or the node attaches where those fix it (CAP-6).
- Left: Unsupported record types follow CEVAL-4.

runner: planned

- The run's output, a `list<tuple<datetime, delta<T>>>` recorded under the run's own key

## CEVAL-6

runner: planned

- A failed run or a missing required recording fails the eval; neither is an empty result

## MAP-1

transpiler: partial

- Fixed-size list `map` runs its HGL candidates. C++ wires a result-bearing keyed map over one or more split dictionaries.
- Core currently declares result-bearing native map candidates. Native calls with resultless callbacks reject during checking.
- Left: native growing lists (#370).

## MAP-2

analyser: partial

- A bare operator or generic function prefers the collection member when its type fits the call. A known type can require a less specific whole-collection reading. The candidate's declared membership supplies both possibilities, and the call still splits an input (GEN-3, FN-17, MAP-3).

transpiler: partial

- A fixed-size map binds positional and named inputs, splits the fitted list inputs, and supplies whole inputs to each child. An explicitly specialized operator can supply the callback's member types (FN-17). An omitted callback input uses the callback's default (FN-8, UNIT-8).
- The native keyed map binds the callback's checked arguments, passes the selected entry from each split dictionary, and gives each whole input to every child.
- Left: a function supplied to a keyed child and growing lists (#370). Bare operator selection is tracked in #776 and inline dictionary argument fitting in #826.

## MAP-3

analyser: done

- A call with only whole arguments reports `operator.no_candidate`.

## MAP-4

transpiler: done

- An explicit key set supplies K. Otherwise the first split dictionary in callback parameter order supplies K, shared by the other split dictionaries. The keyed candidate requires an injected parameter to have type K.

## MAP-5

transpiler: partial

- When `keys` is omitted or written as `null`, an inner join uses the intersection, an outer join uses the union, and a left join uses the first split dictionary in callback parameter order. An explicit nonnullable set takes precedence and has no children before it becomes valid.
- Left: nullable `keys` connections, which need nullable set publications to switch the key set (#370).

## MAP-6

transpiler: partial

- Fixed-size maps compose positions `0..<N`. The join of lengths for growing lists awaits #370.

## MAP-7

transpiler: partial

- A fixed-size map composes a child for each position at build time. In the supported keyed map, hgraph keeps a child as its inputs change and starts a new child when a removed key returns.
- Left: child lifetimes for the remaining native forms (#370).

## MAP-8

transpiler: partial

- A keyed map with an outer join, left join, or explicit keys leaves a split input invalid while its selected key has no entry.
- Left: missing entries under joins of growing lists (#370).

## MAP-9

analyser: partial

- A keyed map rejects a callback whose selected key parameter is `const`, because the child's key is a connection (`connection.as_constant`, FN-1).

transpiler: partial

- A fixed-size map supplies each child's position to the default `key` parameter or the renamed parameter. The supported keyed map supplies a key connection when the child starts, regardless of the parameter's position.
- Left: Key injection in the other native forms remains in #370.

## MAP-10

transpiler: partial

- Fixed-size results hold the children's publications. A callback with no result runs at each fixed position, and the map gives no result. A failure that leaves a child leaves either form.
- The C++ target's existing [size-zero temporal list limit](https://github.com/hhenson/hgl-compiler/issues/829) applies to both fixed-size forms (TYPE-10).
- The native keyed map holds published entries, including a child's first empty dictionary, and publishes a valid empty dictionary once if no child publishes. Lifecycle changes of an unpublished child leave an already valid result silent (TIME-7). A child failure propagates, and an entry is removed when its child stops.
- Proposed. Invalidating a child result removes its entry until that child publishes again. Under explicit keys, a pass-through result withdraws when its split entry leaves, while the child remains alive (MAP-8).
- Left: other native result shapes (#370).

## RED-1

transpiler: partial

- Fixed-size list reduction runs both HGL candidates. The native growing-list ordered fold also accepts distinct member and accumulator types. Growing-list tree, set and dictionary reduction await #370.

## RED-2

transpiler: partial

- The fixed-size HGL folds skip invalid members and recompute after withdrawal. A step runs while both its accumulator and member are valid. A step that stops starts again with fresh callback state. A tree without any valid member gives its identity if supplied and otherwise has no valid output.
- A nullable combiner over a fixed list of nonnullable members gets null for each invalid position. A held null in a list whose members are nullable is passed as a valid member. The nullable tree starts on the first list publication and takes no part of its zero.
- Left: native tree reductions and nullable combiners over nonnullable members of growing lists and keyed collections (#370).

## RED-3

transpiler: partial

- A fixed-size tree fold starts from the first valid member and visits the remaining positions in order. It forwards the only valid side of a pair. An associative call on a list of size zero rejects. Native grouping awaits #370.

## RED-4

analyser: done

- A set or dictionary candidate requires both laws. An ordered function does not admit a dictionary reduction.

transpiler: planned

- #370.

## RED-5

transpiler: done

- Ordered folds wait for their required zero and visit positions in order.
  An omitted zero rejects during candidate selection.
- A growing-list callback retains captured values and reads captured connections
  (FN-14). A captured connection's tick re-evaluates the fold, and a returning
  member starts its callback with fresh state. A change of list length
  publishes the result again even if its value stays the same.

## RED-6

transpiler: partial

- The fixed-size tree fold uses the supplied identity when no valid members remain. It combines valid members without applying the identity. Native implementations await #370.

## RED-7

transpiler: planned

- #367.

## SET-1

transpiler: done

- The value candidate gives the dictionary's keys. The temporal candidate projects hgraph's key set, which publishes membership changes and is silent on a value-only update.
- Boolean-keyed dictionary cases cover both membership changes and silent value-only updates on the SDK.

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
- Plain string fields measure width and precision in Unicode code points. A default `f64` field applies its sign and layout to its FMT-5 text, including with zero or absent width. Both NaN signs start from `nan` (OPS-6).
- Left: the alternate `#` form with an `f64` width, and width or precision combined with `?`.

## FMT-5

analyser: partial

- Core declares a native value candidate for `convert<str>(Type)`.

- A user type uses its selected `convert<str>` and rejects if there is no candidate.
- Core HGL candidates admit growing/fixed list, set, dictionary and tuple text, and list, set and dictionary delta text, when each written member has its FMT-5 text. A member without an infallible text conversion rejects at the call, including inside a nested tuple. Formatting and named requirements retain a selected tuple conversion's type pack.
- A nullable field checks the text conversion of its payload type. The target writes that text when present and `null` when absent.
- Left: struct text candidates (#751).

transpiler: partial

- A Type value converts to its canonical diagnostic spelling, including fixed collection sizes, generic arguments and nullable suffixes (TYPE-18).
- Enum arguments display their declared enum name, independently of their qualified identity key. Left: generic-type diagnostics retain source-form duration/float arguments while Type values normalize them (#909).
- Value defaults are written in Core's scalar forms. Ordinary Core HGL candidates assemble growing lists, fixed lists, sets, dictionaries and tuples in traversal order, select each member's conversion, quote and escape strings and render absent members as `null`. Tuple text preserves the singleton comma (EXPR-8).
- Collection delta text is assembled by Core HGL bodies using the concrete Type-query constructor prefix (TYPE-16/18), omitting empty/null properties and preserving non-null sizes, iteration order, quoted strings and selected nested/member conversions. Ordinary HGL calls and loops lower these bodies.
- Left: struct text candidates (#751).

## FMT-6

transpiler: partial

- A graph call with connections lifts the native value formatter (FN-11), publishing text at every admitted evaluation, including an equal result.
- Left: #434 step 2 supplies the HGL graph candidate's validity policy and triggers. The lifted value candidate currently binds `null` for a nullable argument that is not valid and writes that value, rather than withholding text until the argument is valid as FMT-6 requires.

## FMT-7

transpiler: done

- A `Marked<T>` renders its payload with the payload's specifier or uses `not_valid` for an absent payload.

## FMT-8

transpiler: partial

- The lifted native value candidate formats a collection whole when it ticks. Explicit `Marked<T>` values retain the value formatter's existing tick-prefix support (FMT-7, FMT-9).
- Left: #434 step 2 supplies the HGL graph body, default collection-delta formatting, marks, `require_valid`, sampling and triggers; the current lifted value call uses FN-11's ordinary input binding instead.

## FMT-9

analyser: partial

- Value calls admit `{wall_time}` and reject `{engine_time}` with `format.argument`. A marked place accepts `Marked<T>`.
- A native call from a graph admits `{engine_time}` as a reserved datetime place; it consumes no supplied argument and uses datetime's checked specifiers.
- Trace cells and const arguments supplied to an eval remain value code and reject `{engine_time}` (TST-3).
- Left: the HGL graph candidate supplies the arguments' tick state (#434).

transpiler: partial

- `{wall_time}` reads the process clock. A marked place writes the configured tick prefix when its argument ticked.
- A lifted native format call reads `{engine_time}` from its node's output endpoint, once per call, with the checked pattern retained through lifting.
- Left: the HGL graph candidate supplies the arguments' tick state and its trigger (#434 step 2, FMT-6); an `{engine_time}` place without a lifted node is explicitly unsupported until that candidate provides the graph-start tick.

## DBG-1

transpiler: partial

- Core's value candidate writes and flushes one line to the program's standard output
  where it is executed. Its output stays separate from publications and test outcomes.
- Left: the HGL graph candidate (#434).

## DBG-2

transpiler: planned

- #434.

## DBG-3

analyser: done

- The value candidate forwards the pattern and both packs through the FMT-9 checks.
  An explicit `Marked<T>` retains the mark exception.

transpiler: done

- Value functions and handlers execute Core's line write at the call.

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

transpiler: partial

- The bundled HGL value candidate calls `format` and `write` with flushing.
- Left: the HGL graph candidate and its options (#434).
