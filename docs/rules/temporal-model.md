# Temporal model

`temporal-model.md`

## TIME-1

analyser: partial

- A temporal input's payload is read where its `when` header or a guard proves it valid, and nowhere else: unguarded in a `when` or in `stop` (`payload.unguarded`, TIME-4, TIME-5), in `start` (`input.lifecycle`), in a `state`'s initialiser (`state.input_read`) and in a `cache`'s, which runs on every start (`cache.input_read`, TIME-5 and TIME-6)
- A read in an anonymous function is held to the admission where the function is written (FN-14), the callee of a call among them, which is read as any expression is
- An endpoint operation observes an invalid endpoint with no guard
- A collection's child, an element, a field or an entry at any depth, is an endpoint (`ChildEndpoint`); its value or `delta_value` is read where `all_valid` of its collection or `valid` of the child holds, and at a constant index, a module's constant or a field one that is not is `payload.unguarded`; at an index known when it runs the read is `CheckedChild`, checked then
- An inferred `let` holds a child endpoint. A hook's `var` reads its value at initialization (ERR-14).
- A `return` to a `ref` result names an input or a child and reads no payload, so it needs no admission (FN-16)
- Left: a function value kept in a `state` or a `cache`, which carries a read into a later evaluation, and which the rules give no type
- A struct, a tuple or a fixed list read whole, an input or a child named when the program is compiled, `return quote`, `rows[0]`, is admitted only by its own `all_valid`, in a header or as a guard: `valid` of it, and `all_valid` of what holds it, do not admit it (`payload.unguarded`); a dictionary and an unsized list are read whole where `valid` holds, an `atomic<V>` as any value that publishes whole
- A generic hook checks a whole read against the instance's input shape, including the `all_valid` guard for a fixed composite and the nullable form of a failing dictionary or unsized-list read (GEN-7, ERR-14).

transpiler: partial

- Checks validity of reads, preserving silence, null and equal publications. Whole and
  child reads use the value's temporal family.
- A guarded child below a fixed-list field follows its fields and positions to its own endpoint.
- Left: Whole reads, publications, temporal schemas and deltas outside TYPE-9's families
  are reported. Output reads, narrowed endpoint reads without a supported reader, and
  the projection limits of EXPR-7 remain.

## TIME-2

analyser: partial

- In a graph, `const` parameters, `switch` labels, `while` conditions, store calls, and a value function's declared configuration need their values during construction. A connection in any of those places rejects with `connection.as_constant` (FN-3, CTRL-15, FN-9, EXPR-14). An endpoint operation takes no input there (TIME-9).
- A connection given as a written generic value argument rejects with `generic.argument_constant` (GEN-8).
- A connection used as a fixed list size in a local annotation rejects with
  `list.size` (TYPE-10). An immediately invoked anonymous function preserves
  the connection category of its result, including through a local binding.
  The result depends on reached captures, supplied callbacks, selectors and
  writes that determine the returned value (CTRL-13 to CTRL-15).

transpiler: done

- Wires dependencies without reading live payloads. hgraph evaluates the graph in
  dependency order, coalescing same-cycle work.

## TIME-3

analyser: partial

- A `when` inside another block is rejected
- A `when` whose header has no `valid` selector alone or as a top-level conjunct runs with every temporal input valid; one that has admits those its selectors name: `valid(a, b)` each input it names, `valid([])` none
- A header that is `scheduled()` alone, in parentheses or not, admits none, and one that is `modified([])` alone every input
- What a header admits is its record's (`hir::Header`), which the target reads, and by which the header's and the body's reads are judged: core's `valid` named through an alias, `c::valid(a)`, is the selector; under a configured `&&{...}`, the call of a candidate its configuration selects, nothing is a top-level conjunct and the default stands, which TIME-3 does not say in words
- The record is learnt by reading the header once with nothing admitted, and that reading is taken back with what it spent of the hook's rereads: one more reading of each header, with no bound of its own, so a header's reads are judged by its record however many handlers come before it
- `modified(a, b)` names the inputs it subscribes to, each by its name, and `modified([])` none: where the node injects neither `scheduler` nor `alarm`, or its `start` does not use the one it injects, the selector is warned of (`node.no_activation`) and the source accepted; any use in `start` counts, and `@suppress_warning("node.no_activation")` before the function acknowledges it
- `modified([])` is that selector alone or as a top-level conjunct of its header; read as a bool, in a body or under a negation or a disjunction, it sets no subscription, and is not warned of: a handler whose header has no `modified` selector subscribes to every input
- A sequence that names inputs is no selector's form (`endpoint.argument`)

transpiler: partial

- Runs eligible handlers in source order and stops on return. Filters scheduled and
  modified-input wakes and applies each handler's validity requirements. Ordinary
  block expressions can run in handler conditions.
- Left: Reports a handler guard that needs preceding statements, validity checks of
  unsupported endpoint forms, and `modified([])` combined with another modified
  selector.

## TIME-4

analyser: partial

- In a `when`, a payload read of an input its header does not admit needs a `valid(input)` that holds where it stands: in the branch it holds for, in the right operand of the `&&` it guards, after a branch that returns (`payload.unguarded`)
- A child's guard is `all_valid` of its collection or `valid` of the child as the read names it, in a body or as a header's test; a child's `valid` in a header names no input, and the default admission stands with it
- A direct pack entry has its own guards, such as `valid(items[0])` and `all_valid(items["prior"])` (FN-19). Its validity does not admit another entry.
- An `all_valid` guard with a deferred pack key is checked in the instance for a read of an immediate child. The resolved entry must be the one whose child is read (GEN-7, TIME-1).
- A guard on a child of a heterogeneous pack entry, such as `valid(items[0][0])`, checks the child against the fitted shape and admits that child only (FN-19, GEN-7, TIME-1).
- An endpoint argument is read through its grouping (EXPR-1) by every check: `valid((quote.bid))` is `valid(quote.bid)`
- The guard of a whole read of a struct, a tuple or a fixed list, a child among them, is `all_valid` of what it reads (TIME-1)
- A test of `modified(input)` guards what `valid(input)` does, in a header's top-level conjunct, `when modified(a) && valid([])`, and in a body, in `stop` too, and `modified` of a child as the read names it guards that child: "An input is not modified without being valid"; `modified(a, b)` and `modified()` guard none, nor does a saved bool
- Not settled: whether `modified` of a collection admits a read of its child, which `valid` of it does not; such a read is left rejected (`payload.unguarded`)

transpiler: partial

- Lowering takes the union of the handlers' subscriptions and shares only the
  validity requirements common to executable handlers. Other checks stay in
  their handlers.
- Resolves guards and subscriptions on direct pack entries to the corresponding concrete input slots. The SDK case observes the selected entry's active subscription and the other entry's passive subscription with `is_active`.
- Direct guards on `out` and fixed-list input children can run per handler.
- Left: Other endpoint views follow the limits in TIME-3 and END-9.

## TIME-5

analyser: done

- A node's declarations precede its hooks and stand in its body itself; one `start`, one `stop`; a source is held to the same
- A state's initialiser reads no temporal input (`state.input_read`)
- `start` reads no temporal input and gives none to an endpoint operation (`input.lifecycle`); `stop` reads one under manual admission, each read under a guard (`payload.unguarded`), a struct's, a tuple's or a fixed list's whole read under its `all_valid` (TIME-4)
- Inputs are passive in `stop`; `activate` and `passivate` there are `input.lifecycle`.
- A cache's initialiser reads no temporal input (`cache.input_read`, TIME-6).
- A read in an anonymous function written in any of them is held to the same

transpiler: done

- Builds the node's state, cache and capability members, then emits distinct start,
  evaluation and stop hooks. Initializers and start do not read temporal inputs.

## TIME-6

transpiler: partial

- Emits state members and reconstructs cache at start. Sources preserve coroutine locals
  across yields.
- Left: The target has no checkpoint and restore interface. TIME-6 describes
  checkpointing as an additional runtime facility.

## TIME-7

analyser: partial

- `inject out` needs a result, and is of the result's type
- `out` is read in a `when` and in `stop`, not in a `state`'s or a `cache`'s initialiser, and written only in a `when`, and a `return` with a value stands in neither `start` nor `stop` (`out.lifecycle`)
- A field or an element of `out` is assigned in a `when` as `out` is (EXPR-3)
- A window that is a child of `out` is read as the `list<V>` it holds (TYPE-11)
- What a `return` in a hook, an assignment to `out` and a source's `yield` publish is a complete value of the result or its `delta<T>` (TYPE-16), as core's cases relay a set with `return delta_value(input)`; a window's result takes a V, which arrives, and nothing else (`type.mismatch`), but of a nullable V its delta as well
- A `ref` result takes no value: a `return` to it, and `out =` of it, names an endpoint, which it designates, and `out` of it is as a `ref` parameter is (FN-16)

transpiler: partial

- A scalar `out` read in a `when` sees the latest write in the evaluation or the retained value. A read in `stop` sees the last published value.
- Publishes returns, whole `out` assignments and supported struct or tuple child writes.
  A removal-only delta on a never-published or wholly invalidated dictionary
  establishes a valid empty value; later ignored removals are silent (TYPE-16).
  Whole-output invalidation notifies readers of supported collections. A source
  yields whole values or deltas. A window result exposes its latest
  staged arrival to reads in the handler body, and publishes the last arrival once.
  Validity, modification time, and delta reads use the staged arrival directly.
  The first read of retained data after each write copies the samples in time
  and space proportional to their count. A later read reuses that copy until
  another write or withdrawal.
- A multiply written set or dictionary result stages delta assignments and a
  final return. Cancellation leaves an already valid result silent, including
  when the change cancels inside a nested keyed entry. The staging records only
  keys named by the writes. A scalar dictionary entry that is withdrawn and
  restored during the evaluation publishes its final value. Output validity,
  `all_valid`, modification, delta, and modification time are visible to the
  handlers as they write. Single-write nodes keep direct publication.
- Left: A whole assignment to `out`, invalidation, other output observations or
  tuple-valued dictionary entries keep their existing keyed publication path.
  Unsupported reads and writes of `out` and its children are reported.
  Other publication forms are under TYPE-9 and window observation limits are under END-9.

## TIME-8

analyser: partial

- `inject` names one of the listed capabilities and binds each but `out` to a local of the type core declares for it; `alarm` and `scheduler` in one node are rejected (`inject.alarm_and_scheduler`)
- `run_config` is an application's and an integration test's, and no function's (`inject.position`, RUN-6); braces after any other capability are `inject.configuration`
- A known global store key, including one resolved from a `const` parameter, is
  used with one type in a program. A conflicting `put` or `get` is `store.key_type`
  at the later use (CAP-6).
- A capability type may be written as the whole type of a parameter of a `const fn` or native value function, or as a type listed in an operator declaration's membership (GEN-3). Other positions reject with `capability.as_value`.
- The analyser reports `capability.as_value` for a capability held in `state`, `cache`, a tuple, list, set, dictionary or struct, returned or published, or given to a parameter written as a generic parameter or `any` (hhenson/hgl_spec#73). A `let` and an argument to a parameter of its type are uses. A property is read with its type (NATIVE-6).
- A capture by an anonymous function is reported: of the injected name by the resolver (`name.captured`, FN-14), and of a `let` it is bound to or a parameter of its type here
- A wake asked of the scheduler or the alarm, a call of core's `schedule`, with a duration the compiler sees, a literal, a constant or a constant expression of them, that is not more than zero rejects (`schedule.duration_bounds`): in `start` a negative one, and in a `when` or `stop` zero too; in a value function given the capability a negative one, and a zero is left to the target, since the rule does not say whether a call there is in `start`

transpiler: partial

- Supplies the declared node, source and application capabilities, including timers and
  run configuration. Wires them before initializers that read them.
- Drops a graph-time `datetime` wake at or before the evaluation time, except
  for an equal-time request in `start`. The first drop warns once per node
  with its display name and the requested time. At stop, the node logs the
  number of further drops.
- Startup-only selected branches use the run's store when they are activated
  (FN-9, CRUN-7).
- Left: The C++ target reports store access in connection-driven loop children
  (CTRL-7, CAP-6, CRUN-7).

## TIME-9

analyser: partial

- In a node's hooks the endpoint operations are read as the forms they are, by their name or through an alias of core, `c::valid(a)`, whether core declares them or nothing does yet: `valid`, `all_valid`, `modified` and `scheduled` are bools, `delta_value` the `delta<T>` of the input's type (TYPE-16), or where its place takes a `result` that result (ERR-14), `last_modified` a `datetime?`; each takes a temporal input of the node by its name, or `out` (`endpoint.argument`), and `valid`, `all_valid`, `modified`, `delta_value` and `last_modified` a child of one, `parts[1]`, `quote.bid`, `parts[i]`, alone (END-8); an `atomic<V>`'s field is no child; a selector may stand with no argument or with several inputs, and `valid` and `modified` with `[]`
- An `endpoint<T>` parameter of a native value function is given an input by its name, a child of one, or `out`, in a hook, and its payload is not read; anything else, and any argument in a graph or a value function, is `endpoint.argument`
- Its default retains the endpoint expectation in the cold defining scope (UNIT-7, UNIT-8), so an ordinary value cannot supply the borrowed binding, even where the payload type remains generic.
- Core declares the endpoint operations, including `delta_value` (UNIT-12)
- `scheduled()` in a node that injects neither `scheduler` nor `alarm` is `inject.no_scheduler`: in a hook, a `state`'s or a `cache`'s initialiser, and a source's body (SOURCE-1)
- The unqualified call still selects the wake when the same name binds a declaration or a local (UNIT-10).
- `scheduled()` in a value function, a graph or a test, which is no node, is `scheduled.outside_node` (FN-9)
- `scheduled()` in `start` or `stop` is `scheduled.lifecycle`, wherever it stands in the hook, beside `inject.no_scheduler` in a node that injects neither
- `scheduled()` in an anonymous function is judged where the function is written, each of the above (FN-14), wherever it is called; so is every endpoint operation in one, by what it is given (`endpoint.argument`: the function's own parameter or `let` is no input), how many arguments and generic arguments it takes, and the type its generic argument names
- `scheduled<T>()` is the selector and is judged as `scheduled()` is, and takes no generic argument (`generic.argument_count`): core declares no function of the name
- `is_active`, `activate` and `passivate` take only a direct temporal input, and `all_valid` only a list, tuple, struct, dictionary or window (`endpoint.argument`); a test of `all_valid` guards as one of `valid` does
- `valid`, `modified` and `all_valid` name direct temporal pack entries by constant key (FN-19). `is_active`, `activate` and `passivate` on an entry obey the direct input's lifecycle checks (TIME-5).
- `delta_value` and `last_modified` observe a child of a fitted heterogeneous pack entry. `is_active` reports the fitted child as an indirect input at the instantiating call (GEN-7).
- The core function `is_active` resolves by its declaration and returns a `bool` in a handler or in stop
- Core's `removed_value` resolves by its declaration, which takes a window's endpoint; where its place takes `result<V, error>` the place says what V is, and the read is of that result (ERR-14); given no window it binds nothing of the window's sizes (`generic.unresolved`)
- In a guard it sets neither subscription nor validity
- An `endpoint<T>` result or state member rejects with `endpoint.position` (TYPE-12).
- Left: An operator call through an ordinary public parameter does not match
  the native candidate's borrowed endpoint parameter. The analyser accepts
  `scheduled()` in state initializers and inline callbacks in a node with a
  scheduler.

transpiler: partial

- Writes observation and activity calls on supported endpoints, including borrowed
  arguments to known native helpers and scheduler wake detection.
- Left: Reports unsupported endpoint views, `modified([])` used as a value in
  `stop`, and `scheduled()` in a state initializer or an inline callback.
  END-5 records changes of input activity during stop.

## TIME-10

analyser: done

- A `ref` parameter's `valid`, `modified` and `last_modified` are endpoint operations as any input's are (FN-16)

transpiler: partial

- Uses hgraph reference ports to follow endpoint rebinding without re-evaluating the
  designating node. Supports sampled, whole-input, fixed-list child, dictionary entry,
  and ordinary struct field path designations.
- Left: Other unsupported child targets (such as children of a `ref` parameter or tuple
  positions) and output combinations are listed under FN-16.
