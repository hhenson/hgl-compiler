# Temporal model

`temporal-model.md`

## TIME-1

analyser: partial

- A temporal input's payload is read where its `when` header or a guard proves it valid, and nowhere else: unguarded in a `when` or in `stop` (`payload.unguarded`, TIME-4, TIME-5), in `start` (`input.lifecycle`), in a `state`'s initialiser (`state.input_read`) and in a `cache`'s, which runs on every start (local `cache.input_read`)
- A read in an anonymous function is held to the admission where the function is written (FN-14), the callee of a call among them, which is read as any expression is
- An endpoint operation observes an invalid endpoint with no guard
- A collection's child, an element, a field or an entry at any depth, is an endpoint (`ChildEndpoint`); its value or `delta_value` is read where `all_valid` of its collection or `valid` of the child holds, and at a constant index, a module's constant or a field one that is not is `payload.unguarded`; at an index known when it runs the read is `CheckedChild`, checked then
- A `return` to a `ref` result names an input or a child and reads no payload, so it needs no admission (FN-16)
- Left: a function value kept in a `state` or a `cache`, which carries a read into a later evaluation, and which the rules give no type
- A struct, a tuple or a fixed list read whole, an input or a child named when the program is compiled, `return quote`, `rows[0]`, is admitted only by its own `all_valid`, in a header or as a guard: `valid` of it, and `all_valid` of what holds it, do not admit it (`payload.unguarded`); a dictionary and an unsized list are read whole where `valid` holds, an `atomic<V>` as any value that publishes whole

transpiler: done

- hgraph: an input's `valid`, `modified`, `value` and `delta_value`
- Silence runs no node; an equal publication is a tick, and a delta that names no change, where the value is valid, is none (TIME-7); `valid` reads an invalid input in `when valid([])`
- A child is hgraph's own child of the input (`_core.position_of`, `quote["bid"]`, `_core.entry_port`, one not valid where the key is absent); a read checked when it runs fails with `value.unset_read` where the child is not valid (`_core.checked_child`); an unsized list's position is its own, valid or not (hgraph 0.9.5's unbounded `TSL`), and one it does not hold is a child that is not valid (`_core.list_position`): `valid` and `modified` false, `last_modified` null, a read failing with `value.unset_read` or giving that `err` where a `result` is expected

## TIME-2

analyser: partial

- In a graph a connection is given nothing the graph needs the value of while it is built: a `const` parameter, a `switch` label and a `while` condition (FN-3, CTRL-15), a call on the store (FN-9), a symbol's configuration (EXPR-14), each `connection.as_constant`; an endpoint operation takes no input there (TIME-9)
- Left: a connection written as a generic value argument or a list's size (GEN-8, TYPE-10), which is not rejected

transpiler: planned

- Evaluation in rank order, once per node per cycle; the order of two nodes of one rank undefined

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
- Left: subscription, which `modified` sets; a selector that takes no part in its header's condition

transpiler: partial

- hgraph: handlers run in source order
- Omitting `modified` subscribes to every input and adds no publication condition
- Explicit `modified()` and `modified(a, b)` require a current publication on any selected input, even when the selected inputs cover the entire node
- A handler with `scheduled()` or `modified([])` is under one guard, that the node's scheduler or alarm woke it (TIME-9), and a header that is `scheduled()` alone requires no input valid
- In a node that injects one, a handler with no `modified` selector is guarded by an input's wake: one published or was invalidated
- `scheduled()` under a negation or in a disjunction sets nothing: the handler keeps the defaults, is run by an input's wake and not by a wake of the scheduler or alarm alone, and requires every input valid
- In a node that injects neither, a handler that subscribes to no input is never run, and not written: a node no handler of which subscribes to an input is `active=()`, and one with no input is given one that never wakes it
- `modified([])` read as a bool is `True` in the body of a handler whose header selects it, at a wake that lands with an input's tick too; elsewhere, in another body or under a negation or a disjunction, it is that no temporal input is modified as `modified(input)` asks it, `(not ((a.modified and a.valid) or ...))`, so an invalidation is none and a `ref` parameter is modified where its designation changed, and `True` in a node with no input
- An input that counts as modified as its branch starts (CTRL-2) wakes its node, satisfies a `modified` selector and makes `modified([])` false, as hgraph samples an input it binds in that cycle
- Left: `modified([])` beside a `modified` selector that names inputs, which is reported: the rule does not say what the two ask together; `modified([])` in `start` or `stop`, which are no cycle of the node, and is reported; `valid` or `modified` with no argument or with several, as a bool under a negation or in a disjunction

## TIME-4

analyser: partial

- In a `when`, a payload read of an input its header does not admit needs a `valid(input)` that holds where it stands: in the branch it holds for, in the right operand of the `&&` it guards, after a branch that returns (`payload.unguarded`)
- A child's guard is `all_valid` of its collection or `valid` of the child as the read names it, in a body or as a header's test; a child's `valid` in a header names no input, and the default admission stands with it
- An endpoint argument is read through its grouping (EXPR-1) by every check: `valid((quote.bid))` is `valid(quote.bid)`
- The guard of a whole read of a struct, a tuple or a fixed list, a child among them, is `all_valid` of what it reads (TIME-1)
- A test of `modified(input)` guards what `valid(input)` does, in a header's top-level conjunct, `when modified(a) && valid([])`, and in a body, in `stop` too, and `modified` of a child as the read names it guards that child: "An input is not modified without being valid"; `modified(a, b)` and `modified()` guard none, nor does a saved bool
- Not settled: whether `modified` of a collection admits a read of its child, which `valid` of it does not; such a read is left rejected (`payload.unguarded`)
- Left: aggregate subscriptions; node-wide validity

transpiler: partial

- hgraph: the node subscribes to the union of handler subscriptions and requires the validity common to all handlers
- Each handler checks any additional validity requirements
- On an active invalidation, `when valid([])` admits the wake, and a modification guard reads each selected input's current publication flags
- A read under a test of `modified(input)` is the input's value: the test is hgraph's `(a.modified and a.valid)`, and of a child `_core.child_modified`, so an invalidation is no modification and admits no read
- Left: `all_valid` and structural inputs

## TIME-5

analyser: done

- A node's declarations precede its hooks and stand in its body itself; one `start`, one `stop`; a source is held to the same
- A state's initialiser reads no temporal input (`state.input_read`)
- `start` reads no temporal input and gives none to an endpoint operation (`input.lifecycle`); `stop` reads one under manual admission, each read under a guard (`payload.unguarded`), a struct's, a tuple's or a fixed list's whole read under its `all_valid` (TIME-4)
- A cache's initialiser reads none either (TIME-1)
- A read in an anonymous function written in any of them is held to the same

transpiler: done

- hgraph: `start` gives each `state` its first value and then runs the hook; a `stop` is given the node's configuration, its inputs, its state and, where the node injects `out`, its output (TIME-7), and reads an input under its guard as a `when` does
- It can also read the last publication time of an input or the output

## TIME-6

transpiler: partial

- hgraph: a `state` and a `cache` are fields of the node's state, each given its value in `start`, in the order written
- Left: recording and restoring a `state`

## TIME-7

analyser: partial

- `inject out` needs a result, and is of the result's type
- `out` is read in a `when` and in `stop`, not in a `state`'s or a `cache`'s initialiser, and written only in a `when`, and a `return` with a value stands in neither `start` nor `stop` (`out.lifecycle`)
- A field or an element of `out` is assigned in a `when` as `out` is (EXPR-3)
- A window that is a child of `out` is read as the `list<V>` it holds (TYPE-11)
- What a `return` in a hook, an assignment to `out` and a source's `yield` publish is a complete value of the result or its `delta<T>` (TYPE-16), as core's cases relay a set with `return delta_value(input)`
- A `ref` result takes no value: a `return` to it, and `out =` of it, names an endpoint, which it designates, and `out` of it is as a `ref` parameter is (FN-16)

transpiler: partial

- hgraph: `inject out` is the node's `_output`, read and assigned through its value; no write, no tick
- `out` read whole, in a `when` or in `stop`, before it holds a value, or after `invalidate(out)`, fails with `value.unset_read`, which a capture takes, where `valid(out)` is false
- A node whose result is written `T?` publishes its null: a `return`, a `yield` and an assignment to `out` give hgraph `Null` where the value is null, which ticks a value that is unset
- A `stop` is given the output too, and reads it under `valid(out)`
- A part of `out` is assigned through its own port, a struct's field, a tuple's position and a fixed list's position, each by a whole value of its type (EXPR-3 has a part take no `delta<T>`), and a dictionary's entry as that key's change; a place inside a part that publishes whole gives the part a new value, published whole; `out` read after holds it, and a required part of it that is not valid fails the read with `value.unset_read`
- A whole dictionary published removes each key it has not (`_core.dict_replaced`); a whole empty dictionary, and a whole set, empty or the same again, tick where they change nothing, as hgraph's do not (TIME-7: "A value published whole, the same again or empty, is no delta, and ticks as any publication does"): the set is cleared first through its own port (`_core.set_replaced`), from a `return`, `out =`, a part of `out`, a lifted function's result, and a struct or a tuple published whole, and a source's and a replay's are cleared alike
- A delta that names no change, `delta<set<i64>>()`, `delta<dict<str, i64>>()` or `delta<list<i64>>()`, ticks only where it makes the value valid and empty, never published or invalidated, and is otherwise no tick: it marks nothing `modified`, wakes no subscriber and is recorded by no trace; at the top, within a collection, and beside an unsized list in a struct or a tuple (END-3)
- Such a delta of a struct, a tuple or a fixed list, `delta<Quote>()`, `(null, null)`, `delta<list<i64, 2>>()`, sets no child and is never a tick, a dictionary's value among them, at any depth, where hgraph's `TSD` ticks for one; it adds no key the dictionary does not hold (`_core._pruned`, one place for every nesting)
- So, as TIME-7 has it, agreed, is one whose every entry is an ignored removal: core's three cases for it pass
- Not settled: a struct with no field, which has no child to be valid (TIME-9), is published as hgraph's `TS` of its class, and `delta<Empty>()` ticks and makes it valid, as `Empty()` does
- An unsized list's `out`, hgraph 0.9.5's unbounded `TSL`: a whole value replaces what it held, a delta's size is counted from what it holds, `out[i] = v` is `_core.list_child(_output, i)`, which fails past the list, and two writes in one evaluation publish their sum (END-5); a whole value that changes nothing hgraph sees, an empty list first among them, still ticks
- A part of `out` read is its own port, a fixed list's and an unsized list's position among them
- A struct's or a tuple's delta that holds an unsized list's delta is published through the output, each list's size counted from what it holds (`_core.published_into`); a source's node publishes what it yields so when it is due, an empty list in a struct among them
- Left: a place inside an entry of a dictionary `out`; an empty unsized list assigned to an entry or a list's position of `out`, which hgraph does not tick for and which has no port of its own where it is written, fails as the graph runs

## TIME-8

analyser: partial

- `inject` names one of the listed capabilities and binds each but `out` to a local of the type core declares for it; `alarm` and `scheduler` in one node are rejected (`inject.alarm_and_scheduler`)
- A capability type written other than as the whole type of a parameter of a `const fn` or a native value function, and a capability that is the value of a `state` or `cache`, an entry of a tuple, list, set, dictionary or struct, returned, or published, is rejected (`capability.as_value`); a `let` and an argument of a parameter of its type are uses, one given to a parameter written as a generic parameter or `any` is rejected (hhenson/hgl_spec#73), and a property is read with its type (NATIVE-6)
- A capture by an anonymous function is reported: of the injected name by the resolver (`name.captured`, FN-14), and of a `let` it is bound to or a parameter of its type here
- A wake asked of the scheduler or the alarm, a call of core's `schedule`, with a duration the compiler sees, a literal, a constant or a constant expression of them, that is not more than zero rejects (`schedule.duration_bounds`): in `start` a negative one, and in a `when` or `stop` zero too; in a value function given the capability a negative one, and a zero is left to the target, since the rule does not say whether a call there is in `start`

transpiler: partial

- hgraph: `inject clock`, `inject logger` and `inject global_state` are hgraph's own `EvaluationClock`, `LOGGER` and `GlobalState`, given to the node, to its `start` and to its `stop`, each for that call; one given to a function is passed as it is
- A graph's `inject global_state` is the store current where hgraph builds the graph, and a call on it there is run then (FN-9)
- `inject scheduler` and `inject alarm` are both hgraph's `SCHEDULER`: it has one for a node, and no cheaper one; given to a function or bound to a `let` it is passed as it is
- A source injects neither (SOURCE-1)
- A zero duration asked of the scheduler or the alarm in `start` wakes the node's first evaluation, and a wake asked for at a `datetime` before the evaluation time is dropped, with no failure
- A wake's duration that the target does not work out, an input, a local or a `const` parameter, is checked as the program runs, by the provider's `schedule` once every argument is evaluated (`_starting=`): one not more than zero fails the call with `schedule.non_positive_duration`, but a zero in `start`; hgraph 0.9.5 drops such a wake with no failure
- Left, reported: a wake asked for in a value function with a duration that may be zero, anything but one the target works out to be more than zero, a literal, a constant or core's `+`, `-` and `*` by an `i64` of those, since the rule does not say whether a call there is in `start`

## TIME-9

analyser: partial

- In a node's hooks the endpoint operations are read as the forms they are, by their name or through an alias of core, `c::valid(a)`, whether core declares them or nothing does yet: `valid`, `all_valid`, `modified` and `scheduled` are bools, `delta_value` the `delta<T>` of the input's type (TYPE-16), `last_modified` a `datetime?`; each takes a temporal input of the node by its name, or `out` (`endpoint.argument`), and `valid`, `all_valid`, `modified`, `delta_value` and `last_modified` a child of one, `parts[1]`, `quote.bid`, `parts[i]`, alone (END-8); an `atomic<V>`'s field is no child; a selector may stand with no argument or with several inputs, and `valid` and `modified` with `[]`
- An `endpoint<T>` parameter of a native value function is given an input by its name, a child of one, or `out`, in a hook, and its payload is not read; anything else, and any argument in a graph or a value function, is `endpoint.argument`
- Core declares the endpoint operations, including `delta_value` (UNIT-12)
- `scheduled()` in a node that injects neither `scheduler` nor `alarm` is `inject.no_scheduler`: in a hook, a `state`'s or a `cache`'s initialiser, and a source's body (SOURCE-1)
- `scheduled()` in a value function, a graph or a test, which is no node, is local `scheduled.outside_node` (FN-9): the catalogue has no row for it
- `scheduled()` in `start` or `stop` is `scheduled.lifecycle`, wherever it stands in the hook, beside `inject.no_scheduler` in a node that injects neither
- `scheduled()` in an anonymous function is judged where the function is written, each of the above (FN-14), wherever it is called; so is every endpoint operation in one, by what it is given (`endpoint.argument`: the function's own parameter or `let` is no input), how many arguments and generic arguments it takes, and the type its generic argument names
- `scheduled<T>()` is the selector and is judged as `scheduled()` is, and takes no generic argument (`generic.argument_count`): core declares no function of the name
- `is_active`, `activate` and `passivate` take only a direct temporal input, and `all_valid` only a list, tuple, struct, dictionary or window (`endpoint.argument`); a test of `all_valid` guards as one of `valid` does
- The core function `is_active` resolves by its declaration and returns a `bool` in a handler or in stop
- In a guard it sets neither subscription nor validity
- Left: which place a selector stands in; an operator's candidate with an `endpoint<T>` parameter; that an endpoint is not retained; `scheduled()` in the initialiser of a node that injects one, which the rule does not place; `scheduled()` in an anonymous function in a node that injects one, which FN-14 may make a use of the capability, and is accepted: the target writes no anonymous function

transpiler: partial

- hgraph: `scheduled()` in a header is `_scheduler.is_scheduled_now` in a node that injects a scheduler, one `if` over the handler, true whether or not an input ticked
- In one that injects an alarm it is that and that no input ticked, `not (a.modified or a.invalidated)`: hgraph's scheduler is asked too, since hgraph evaluates a node at a wake that was since withdrawn
- Under a negation, in a disjunction and in a handler's body `scheduled()` is that bool, read once where the node is entered (`_timer_woke`): a handler that asks again under the tag that is due leaves it true, and `scheduler.is_scheduled_now` with it (CAP-4)
- The endpoint operations are the input's own properties: `valid` of a struct is hgraph's `valid`, true once any child is, and `all_valid` hgraph's `all_valid`
- Where a list, fixed or unsized, is in the value, `valid` and `all_valid` are the provider's (`_core.valid_of`, `_core.all_valid_of`): hgraph's `TSL` stays valid once every position has lost its value or a size alone has grown it, where the language's list is not; a handler's and a lifted helper's admission of such an input is too
- `is_scheduled_now` stays true for the rest of a wake after `un_schedule` or `reset` in it (CAP-4), read as the property, by a function given the scheduler, and as `scheduled()` in a body and in a later handler: hgraph 0.9.5's own is false once the wake due is cancelled
- A window's `valid` and `all_valid` are the `TSW`'s, of ticks and of a duration (TYPE-11)
- `all_valid` asks no optional child, a `T?` or a field TYPE-4 makes optional, a generic one given a `T?` among them: of a struct or a tuple with one, the provider asks the others (`_core.all_valid_of`), and of one whose every child is optional, a list or a dictionary of a `T?` among them, it is the value's `valid`
- A list or a dictionary that a delta naming no change makes valid (TIME-7) has no element, and is valid and all valid

## TIME-10

analyser: done

- A `ref` parameter's `valid`, `modified` and `last_modified` are endpoint operations as any input's are (FN-16)

transpiler: partial

- hgraph: a `ref` parameter, a `REF` input, ticks when the reference it is given changes, and not when its target publishes; an ordinary input bound to a `ref` result, a lifted one's and a `signal` among them, follows the target and samples it when re-pointed
- A designation published again unchanged is no tick: a node publishes its result where it designates what it did not before the evaluation, so writes that end where they began are none (`_core.Designation`)
- Proposed, and as hgraph has it: a `ref` parameter's `valid` is true where it designates something, `modified` where its designation changed, and `last_modified` is when it last did, each the `REF`'s own
- Left: a route kept to a removed child
