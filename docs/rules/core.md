# Core

`spec/hgl_core/spec`. Required constants are evaluated by the analyser under
the same numeric rules the transpiler entries give.

## NUM-1

analyser: partial

- Result types of arithmetic on `i64` and `f64`, and of unary minus, in value functions, through core's candidates
- Left: temporal bodies

transpiler: done

- hgraph: `//` rounds toward negative infinity and `%` has the divisor's sign, for `i64` and with a finite `f64` operand; where an `f64` operand is an infinity or NaN, `//` is the floor of the IEEE quotient and `%` what IEEE `fmod` gives; `/` of numbers gives an `f64`
- A duration by a duration: `/` is an `f64`, the quotient of their counts of microseconds, and `//` an `i64`, its floor

## NUM-2

transpiler: done

- hgraph: `+`, `-`, `*`, `//` and negation of `i64` raise `numeric.out_of_range` where the result leaves the signed 64-bit range; float arithmetic is the host's binary64; an `i64` is compared with an `f64` by their exact values

## NUM-3

transpiler: done

- hgraph: a zero divisor gives a signed infinity or NaN under `ieee`, NaN under `nan`, and an error with the code `numeric.division_by_zero` under `fail`

## NUM-4

transpiler: done

- hgraph: `==` with no tolerance is exact, and with one the symmetric condition, made in exact arithmetic; `!=` is its complement; a tolerance that is not finite and nonnegative is an error

## NUM-5

transpiler: done

- hgraph: NaN is equal and close to nothing, an infinity to itself alone, and the two zeros to each other; the finite test is exact, so a difference or a scaled tolerance that is no `f64` admits nothing wrongly

## NUM-6

analyser: partial

- In a value function a comparison outcome is a `CmpResult`, and is rejected where a bool or a number is expected
- Left: temporal bodies

transpiler: done

- hgraph: `<=>` and `compare_` give the member of core's `CmpResult`: `unordered` for a NaN operand, and `equivalent` for equal numbers, the two zeros among them; an `i64` and an `f64` are compared exactly
- The Boolean comparisons are the provider's, false for a NaN operand but `!=`

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

transpiler: done

- hgraph: a lifted call runs the same function as a value call, a candidate the provider's, with the same options and defaults; a NaN or an infinity is published as data, and an error out of range fails both

## NUM-9

analyser: partial

- A division, by name or by symbol, through core's candidates in a value function: its own type, or a `result` of it under `zero: .error`; `zero` is a generic value parameter, so one given no compile-time constant is rejected
- Left: temporal bodies

transpiler: done

- hgraph: under `zero: .error` a zero divisor gives `err` of an error with the code `numeric.division_by_zero`, and any other divisor `ok` of the ordinary division

## NUM-10

transpiler: partial

- hgraph: a checked division's `Err` is a value like any other, and `?` returns it from a value function or fails the node in a `when`
- Left: an `Err` published by a node

## NUM-11

analyser: partial

- `eq_(a, b, abs: ..., rel: ...)` by name through core's candidates in a value function: the tolerances are options of the candidates for numbers, and of no other
- The same as the symbol's configuration, `a =={abs: ...} b`
- Left: temporal bodies

transpiler: done

- hgraph: a tolerance of a lifted comparison is configuration of its node, given where the graph is built, and no input

## NUM-12

analyser: partial

- Every division, by name or by symbol, through core's candidates in a value function: a division whose result is an `f64` admits each policy, one whose result is an `i64` or a `duration` only `.error` and `.fail`, and `.error` gives a `result`
- Left: temporal bodies

transpiler: done

- hgraph: `/`, `//` and `%` of numbers, and `/` and `//` of durations, take `zero`, given to the provider's function by name: the policy written, or the candidate's default

## OPS-1

analyser: partial

- In a value function a symbol is the call of the core operator it stands for, and admits the operand types that operator's candidates declare, core's and a module's own; any other pairing rejected (`operator.no_candidate`)
- Its operands are read as the call's arguments are, and the type its place expects is requested of the candidates
- Left: temporal bodies

transpiler: partial

- hgraph: `+` and `-` of two durations, `-x` of one, and a duration times an `i64` or an `f64`; a `datetime` or a `civil_datetime` with a duration added or taken away, and one less another; a `date` less a `date`; a `date` and a `time`; a `date` with a duration added or taken away, a `civil_datetime`; `+` of two strings
- A duration divided by an `i64` or an `f64`, and by a duration with `/` and `//`
- A `zoned_datetime` with a duration added or taken away: its instant moved, its zone kept
- A string times an `i64`, in either order: the text that many times, none for a count of zero
- Left: a string times a count below zero, `numeric.out_of_range`, which the provider reports as the graph runs as not written; an assertion that it raises is reported (TST-10)

## OPS-2

analyser: partial

- In a value function, ordering between two of one ordered type and equality between two values of one type, by symbol and by name through core's candidates
- An operand that is a generic parameter is compared as its declaration's requirements state (GEN-7)
- Two zoned datetimes, in a value function, a hook and a graph: `==` and `!=` select core's candidates for them, `utc` a `bool` they alone declare; no order of a zoned type; in a graph `utc` is no connection (EXPR-14)
- Left: a comparison with a nullable operand; other types in temporal bodies

transpiler: partial

- hgraph: `==` and `!=` of two structs compare the class, which is one for each application of a generic struct, and then every field; of two lists, tuples and dictionaries, part by part; a value that holds a NaN is not equal to itself
- `<`, `<=`, `>`, `>=` and `<=>` of two `date`s, `time`s, `datetime`s, `civil_datetime`s or `duration`s, in time order
- Two zoned datetimes by core's candidates of `eq_` and `ne_`, with `utc` given as a value, a parameter, a generic value or an input of a node, as configuration of a lifted node, and to a lifted call by name as an input that is false while invalid

## OPS-3

analyser: partial

- `a <=> b` and `compare_(a, b)` through core's candidates in a value function are a `CmpResult`
- Left: temporal bodies

## OPS-4

analyser: partial

- `convert<T>(value)` through core's candidates in a value function: the conversions core declares, and no other
- Left: a list of structs to a list of their parent; temporal bodies

transpiler: partial

- hgraph: `convert<T>(value)` is the provider's function for the candidate, named with what it gives, `convert_i64_to_f64`, since core's candidates take one type and give different ones; given a connection it is lifted (FN-6)
- Left: a list of structs to a list of their parent

## OPS-5

analyser: partial

- The Boolean connectives and `!` take bools, in a value function
- Left: temporal bodies

## OPS-6

analyser: done

- The type each conversion yields; `convert<list<B>>(values)` of a `list<A>`, `A` inheriting from the abstract `B`, is `list<B>` with no option

transpiler: partial

- hgraph: an `i64` to an `f64`; a number to a `bool` and back; a `bool` to and from text; text to and from UTF-8 bytes; a `date` to the start of it in UTC, and a `datetime` to its date; an `i64` as its decimal digits and an `f64` as the shortest text that reads back, with an exponent from 1e16 up and below 1e-4, `nan`, `inf` and `-inf`; text as a number where it is a literal of the type after one `-`
- Left: a list of structs

## OPS-7

analyser: done

- A conversion's options, and the `result` its checked error policy returns

transpiler: done

- hgraph: an `f64` to an `i64` fails for a NaN with no `nan` value, an infinity, and a number outside `i64` once rounded; text that is no `bool` literal and bytes that are not UTF-8 fail
- Text that is no number of the type asked for, or whose number is outside `i64` or rounds to an infinity, fails
- A failure is an `err` under `.error`, and escapes under `.fail`

## OPS-8

analyser: partial

- The options of `convert` through core's candidates in a value function: `invalid` selects `result<T, error>` or T and is given a compile-time constant, `rounding` and `nan` are parameters, and an option is admitted only for the conversions that declare it
- Left: temporal bodies

transpiler: done

- hgraph: `invalid` is given as its member's name, as division's `zero` is; `rounding`, each of its six members, and `nan` are arguments
- An `err` carries `convert.invalid`, and so does the error that escapes, which `assert raises("convert.invalid")` takes

## OPS-9

analyser: done

- In a graph `invalid: .identity`, by symbol or by name, selects core's temporal candidate of `add_` or `sub_` for two operands of one type, where the value operations it requires exist for that type (GEN-16); without the option the value candidate is selected and lifted
- Core's two candidates are nodes, and are read as any node is

transpiler: partial

- hgraph: core's temporal candidates of `+` and `-` under `{invalid: .identity}` are written from core's own bodies, for each type they are called with: one valid operand is forwarded, right-only subtraction negates it, and both invalid invalidate the output
- Core's identity arithmetic cases pass on hgraph 0.9.8

## OPS-10

transpiler: done

- hgraph: a duration times an `f64`, and a duration divided by an `i64` or an `f64`, is the exact result rounded to a microsecond, a tie to the even one; a duration result outside the signed 64-bit range of microseconds, a duration times or divided by an infinity or a NaN, and an instant result outside the years 1 to 9999, is `numeric.out_of_range`; a `date` with a duration is the `civil_datetime` that far from the start of the date; a `zoned_datetime` with a duration is its instant moved, in its zone (OPS-1)

## LIST-1

analyser: done

- `append` is core's native function, typed as any call; its list is a `var` parameter (FN-15)

transpiler: done

- hgraph: `append(values, item)` is the provider's, which puts the item after the list's last entry and gives the list back; the call assigns the place
- A list inside the value a `var` holds, `append(book.orders, order)`, is grown where it is: the provider goes in to it (`_core.given_at`)

## LIST-2

analyser: done

- `extend` is core's native function, typed as any call; its list is a `var` parameter (FN-15)

transpiler: done

- hgraph: `extend(values, items)` is the provider's, which puts each entry of `items` after the list's last, in order, and gives the list back

## LIST-3

transpiler: done

- hgraph, by the owner's rule (issue 151): a `var`, a `state` and a `cache` change the list or the dictionary they hold where it is, with no check and no copy, and an argument is a reference
- A copy is made where a reference is made to what a place holds and the place then changes, a `let` or a loop whose place is changed while the name is in scope; where a call gives a place to a `var` parameter and what it holds to another; and where a reference is kept: assigned to a place or into one, put into a place by a native function, returned, published or put in the store
- A value written out and a function's own result are copied nowhere; a result is the function's own only where every value it may be is
- An ordinary value of a graph takes the same copies, and what a graph gives a node is a copy

## TEN-1

analyser: done

- The pairings are core's candidates, found as any operator's are: `date ± tenor`, `tenor ± tenor`, scaling in either order, negation, the order and `<=>`
- A tenor on the left of a date, an instant that is no date, and a division are `operator.no_candidate`

transpiler: done

- hgraph: a tenor's own candidates are the provider's functions, and `date ± tenor` core's HGL, written as any generic value function is
- The C++ target emits the shared hgraph scalar for tenor types and both forms of literal, including zero calendar and zero business days.

## TEN-2

transpiler: done

- hgraph: a sum, a difference, a negation and a scaling are count by count; the two forms have no sum (`tenor.mixed_units`), and a count outside the i64 range is `numeric.out_of_range`

## TEN-3

transpiler: done

- hgraph: the order is count by count; `<=>` gives `unordered` for two tenors that differ both ways or are of different forms, and `<`, `<=`, `>`, `>=` are false for those

## TEN-4

transpiler: partial

- hgraph: the provider's `moved`: the months first, to the last day of a shorter month, then the days; a date outside the years 1 to 9999 is `numeric.out_of_range`
- Left, reported: a move of a date written in the range's first or last year: the provider holds the date the months give to the range, where TEN-4 holds only the result

## TEN-5

analyser: done

- `calendar` is an option of `+` and `-`, an argument of the candidate that takes it (EXPR-10); a calendar is a value `business_day` has a candidate for, and one that has none finds no candidate

transpiler: done

- hgraph: core's functions are written for the calendar they are given, and ask its `business_day`, the program's own included; `Weekdays()` where none is given

## TEN-6

transpiler: done

- hgraph: core's HGL, as written: on to a business day, or back to one, and then the count

## TEN-7

analyser: done

- `rolling_rule` is an option of `+` and `-`, with `calendar` or alone; a rolling rule is a value `roll` has a candidate for

transpiler: done

- hgraph: the candidate that takes a rolling rule is written for the rule and the calendar, and gives `roll` the moved date

## TEN-8

transpiler: done

- hgraph: core's `roll` for `Rolling`, as written: each rule looks only the way it needs

## TEN-9

transpiler: done

- hgraph: `convert<str>(tenor)` is the provider's: the largest units, `0D` for no move, and `-` before the units of a count below zero

## END-1

transpiler: done

- hgraph: a scalar publication, false, zero, empty text and bytes, null, NaN and the infinities included, makes the input valid, and silence keeps it valid with its value

## END-2

transpiler: done

- hgraph: a publication marks the input modified for its cycle, an equal one included; an idle cycle leaves it unmodified with its value; an input bound to a node's output observes that output
- `modified(x)`, as a guard, admits a read of `x` as `valid(x)` does (TIME-4): the read stands under `(x.modified and x.valid)`
- A window of a duration is modified at an exit with nothing published (END-9), as hgraph 0.9.8's `TSW` is
- An input of a node in a branch started while the program runs, bound to an endpoint already valid, is modified in the cycle the branch starts and wakes its node, as hgraph samples an input it binds: of an `if`, a `switch` and a dictionary's child (CTRL-2, CTRL-7); core's conditional case passes
- Not written as a rule, since the pinned text leaves it out: a `ref` parameter, a collection's children, and `delta_value` and `last_modified` of such an input as its branch starts

## END-3

analyser: partial

- `delta_value(x)` is a `delta<T>` for an input of T, and for a child of one (END-8), the value itself for a scalar
- For a nullable scalar it is the optional TYPE-16 gives it, and for a window the V that arrived
- Where its place takes `result<delta<T>, error>` it is of that result (`CheckedDelta`, the language's ERR-14), of any endpoint, a generic body's `T` among them, as core's `record` reads it
- A child's `delta_value` needs the child's admission (TIME-1)
- Left: proof that `x` is valid and modified

transpiler: partial

- hgraph: `delta_value(input)` of a scalar input, and of a nullable scalar, whose delta is hgraph's own
- A fixed list's delta, `REMOVE` for a position that lost its value; an unsized list's, hgraph 0.9.8's `TSLDelta`: the new size where it moves, never a negative one, and on a shrink the dropped tail in `removed`, also for a smaller `size_modified` alone and a negative one, which the provider counts from what the list holds, to empty at most; growth by an entry after the last and past it, the positions between not valid
- A fixed list's entry past its bounds, and a `size_modified` that moves its size, fails the write
- A child's (`_core.child_delta`), which fails with `value.unset_read` where it is not valid
- A removed position the list does not hold lost no value, and is ignored: nothing fails and nothing is lost (the owner's decision on hhenson/hgl_spec#124, as for a dictionary's absent key)
- An entry at or past a `size_modified` written with it contradicts the delta and fails the write as the graph runs, with no execution code (the owner's decision on hhenson/hgl_spec#124: "a failure"); a constant one is not yet rejected when compiled (TYPE-16)
- A dictionary's delta removes a key with `REMOVE_IF_EXISTS`, so one it does not hold is ignored, and is no failure, a delta read from an input and published again among them (`_core.entries_published`); a set ignores a member it does not hold
- A delta that names no change, of a dictionary, a set or an unsized list, ticks only where it makes the endpoint valid, empty and modified, and its `delta_value` is then that delta (END-3, TIME-7), by a node, a source and a trace's cell, at the top, within a dictionary, a struct, a tuple or a fixed list, and beside an unsized list in a struct or a tuple; one of a struct, a tuple or a fixed list sets no child and is never a tick; so, as END-3 has it, agreed, is one whose every entry is an ignored removal. A trace's cell written whole is replayed as the delta that publishing it records (`_core.whole_delta`). hgraph ticks a `TSD` and a `TSS` for neither, and `clear` makes one that is not valid valid; it ticks what holds one given such a change, so the provider leaves that child out (`_core.entries_published`); `invalidate(out)` of a dictionary or a set empties it first, as hgraph keeps its keys and members (`_core.invalidated`)
- hgraph 0.9.8's bundle given a tuple ticks where any position is given, a change that a valid child is left as it was by among them, and fails the run for a dictionary's `REMOVE_IF_EXISTS` in it: a tuple that holds an unsized list is published as a `dict` of the positions it gives, as a struct is (`_core.published_into`)
- A dictionary or a set in a struct or a tuple that holds an unsized list is published as a node's delta of one is, by a node and by a source, the provider told where (`entries=`): an entry whose change names nothing, where it is valid, is left out
- Left: a dictionary's or a set's assigned to an entry of `out`; an unsized list's delta inside a dictionary's or a list's delta, which is reported (TYPE-16)

## END-4

transpiler: partial

- hgraph: `invalidate(out)` clears the output; one that holds an unsized list, a dictionary or a set, at any depth of a struct, a tuple and a fixed list, empties each first (`_core.invalidated`), since hgraph's `TSL` keeps its positions and its `TSD` and `TSS` their keys and members, which the next delta read would name removed and a delta that names no change would leave in what it makes valid (TIME-7: "it makes the value valid and empty")
- `invalidate(out)` of a `ref` result (FN-16: `out` of one takes the endpoint operations) leaves it designating nothing: the invalidation, for which hgraph 0.9.8 unbinds an ordinary reader and wakes it with its input not valid, and a `ref` parameter given it is not valid; the last action of the evaluation decides (`_core.Designation`); invalidated again while it is not valid, it wakes no reader, as hgraph has it
- An active input's transition to invalid wakes the node
- In a body or handler guard, `modified(input)` is false on invalidation, whether the input is active or passive
- Repeated scalar invalidation is silent and a scalar producer's last action decides its final validity
- Left: invalidation across unsupported structural shapes

## END-5

transpiler: partial

- hgraph: repeated writes to a node's own set in one evaluation are one delta
- Direct inputs can be activated and passivated
- `is_active(input)` reads the receiving input's current activity, including while invalid
- A passive input retains its value
- Its publication can satisfy a handler when another input schedules the node
- A delta that names no change, written where a dictionary, a set or an unsized list is valid, is no publication: "it marks nothing modified, moves no `last_modified` (END-6), and notifies no subscriber" (END-3, TIME-7)
- Nor is a collection given whole that changes nothing, a set given the members it holds or an empty list or dictionary given to a valid empty one; a dictionary's entries and a list's positions given again are publications: hgraph 0.9.8's `TSD` and `TSS` publish so, and the provider makes an empty list given to one that is not valid tick (TIME-7); core's three cases pass
- Left: coalescing across structural shapes

## END-6

analyser: done

- `last_modified` on an input or the node's output has type `datetime?`

transpiler: done

- hgraph: `last_modified` reads the endpoint's retained UTC publication time while valid and returns null while invalid
- It reads inputs and the injected output in handlers and in `stop`

## END-7

analyser: done

- `all_valid` is an endpoint operation on a list, tuple, struct, dictionary or window (TIME-9)

transpiler: partial

- hgraph: `valid` of a fixed list, a tuple or a struct is true once any child is, and `all_valid` once every one is
- A list's, fixed or unsized, and a value's that holds one, is the provider's (`_core.valid_of`): false again once every position has lost its value, true for a published empty list, where hgraph's `TSL` stays valid
- `all_valid` asks no optional child (TIME-9): of a struct or a tuple with one the provider asks the others, and of a value whose every child is optional it is its `valid`; in a whole value an optional child that is not valid is null

## END-8

analyser: partial

- A collection's child is an endpoint, valid or not: `valid`, `modified`, `delta_value` and `last_modified` take it, and `all_valid` where it is a collection; its value and delta are read where it is admitted (TIME-1); a read at an index known only when it runs is `CheckedChild` (ERR-14)
- A struct, a tuple or a fixed list read whole, a child among them, is admitted only where it is all valid by its own `all_valid`, which core's rejection case `quote_without_admission` checks; one at an index known only when it runs fails with `value.unset_read` where it is not all valid, or gives that `err` (ERR-14)

transpiler: partial

- hgraph: a child's endpoint is hgraph's child of the input or of `out`, `_core.position_of` of a fixed list and `_core.list_position` of an unsized one (hgraph 0.9.8's unbounded `TSL`), a bundle's field and `_core.entry_port`; a position an unsized list does not hold, before it publishes, while it is short or after a shrink, is a child that is not valid, as an absent key is; a child admitted is read through it, never through its siblings, and its delta as a direct input's of its type is (`language_delta`, a tuple's by `_core.positions`); a read at a run-time index fails with `value.unset_read` where the child is not valid, or gives `err` where a `result<T, error>` is expected

## END-9

analyser: partial

- `all_valid` takes a window, and its value is read in a hook, and given to a value function in composition, as a `list<V>` (TYPE-11, FN-12)
- `delta_value(x)` of a window is the V that arrived, read before it is all valid as after; of a window of a duration it is read where a `result` is taken, and a plain read is `payload.unguarded` (TYPE-11)
- `removed_value(x)` is core's declaration, given a window, read where a `result<V, error>` is taken, and a plain read is `payload.unguarded` for every window

transpiler: partial

- hgraph: a window is valid at its first value and all valid at its minimum, a count of ticks or its arrivals' span, the `TSW`'s own; in one of a duration a value leaves one smallest step after its age is the maximum, and the window ticks for the exit, modified with nothing published (END-2), as hgraph 0.9.8's `TSW` does; a minimum of zero is held by the first value, and an empty window is not all valid; one emptied by exits stays valid; proposed: `last_modified` is the exit's time
- `delta_value(x)` of a window of ticks is the `TSW`'s, the value that arrived, read plain or where a `result` is taken
- Its value is read in a hook, and given to a lifted helper where it is all valid, as the `list<V>` it holds, oldest first, and fails before with `value.unset_read` (`_core.window_values`), or gives that `err` where its place expects a `result<list<V>, error>` (`_core.window_result`); a lifted value function whose parameter has a default is not invoked where the window is valid and not all valid (FN-11); core's window cases pass but those of `removed_value` and of a window of a duration's delta read where a `result` is taken, listed in `crates/hgl_hgraph/ci/unsupported_cases.toml`
- Left, reported: a window of a value that is no scalar or enum (TYPE-11), and invalidating one (END-4); `removed_value`, and `delta_value` of a window of a duration read where a `result` is taken, which hgraph's `TSW` gives as `None` at an exit alone

## END-10

analyser: done

- A `ref` parameter of T is given a connection of T, and a `ref` result designates what its `return` names, a child of an input valid or not, read as `EndpointOf` or `ChildEndpointOf` with no payload read (FN-16)
- An assignment to a part of a `ref` result's `out` is `binding.immutable`, and `ref` on a `signal` parameter `ref.position` (FN-16, TYPE-12)

transpiler: partial

- hgraph: a `ref` parameter is a `REF` input, passive to its target, and modified where it is given a designation, which wakes a node active on it; `valid` of it is whether a reference is present; a reader of a `ref` result observes what it designates; a whole `ref` parameter returned is sampled, and the result keeps it until the node writes again; an ordinary input or a child path returned is followed through an active hidden `REF` input, a re-binding upstream included, and the node is not woken by that endpoint's ticks (the provider's `Designation`); core's reference cases pass
- Left, reported: what FN-16's entry lists

## CAP-1

analyser: done

- `inject` binds each capability but `out` to a local of the native type core declares for it; a property is read with its type and not assigned, and no call constructs the type (with TIME-8 and NATIVE-6)

transpiler: done

- hgraph: a property of `Clock`, `Logger` and `Scheduler` is read through the provider, `_core.Clock_evaluation_time`, `_core.Scheduler_is_scheduled`; `Alarm` and `GlobalState` have none

## CAP-2

analyser: done

- `log` and `log_debug`, `log_info`, `log_warning`, `log_error` are core's declarations, typed as any call; `logger.level` is a `LogLevel`

transpiler: done

- hgraph: `log` writes one record at its level through hgraph's logger, the message as text; `logger.level` is the member for the logger's effective level

## CAP-3

analyser: done

- A clock's four properties, each with its type

transpiler: done

- hgraph: each of the clock's four properties is read from hgraph's `EvaluationClock`

## CAP-4

analyser: done

- A scheduler's three properties, `is_scheduled_now` among them; `schedule` with a `duration` or a `datetime`, a `tag` and `on_wall_clock`; `un_schedule`, with a tag or without, and `reset`

transpiler: partial

- hgraph: as a node is entered each wake due then is taken from hgraph's scheduler (`_core.Wake`), which holds it until the node has run: it is delivered and no longer pending, so `is_scheduled`, `next_scheduled_time` (null where none is pending) and `un_schedule` with no tag see only the wakes to come, and `is_scheduled_now` holds through the evaluation, as `scheduled()` does
- `schedule` with a `duration` or a `datetime`, a tag and `on_wall_clock`, `un_schedule` with a tag or without, and `reset` are hgraph's; a tag is kept with a mark before it, so the empty tag is a tag
- A wake asked for at a `datetime` before the current evaluation time is dropped, with no failure; a zero duration in `start` wakes the node's first evaluation
- A duration not more than zero, but a zero in `start`, fails the call with `schedule.non_positive_duration` where the compiler does not see it (TIME-8, `_starting=`): hgraph 0.9.8 drops a zero or negative duration asked for in an evaluation, and a negative one in `start`, with no failure
- In simulation `on_wall_clock` changes nothing, the wake timed on evaluation time as hgraph times it, and core's case of it passes; in real time the wake is hgraph's on the wall clock, and that one already due comes in the next cycle is checked by no case
- Left, reported: a duration that may be zero asked for in a value function given the scheduler (TIME-8)
- Every node with no temporal input is given one that never wakes it, since hgraph takes no node without an input

## CAP-5

analyser: done

- An alarm has no property, and a node injects it or `scheduler` (TIME-8)
- `schedule` on it takes a `duration` or a `datetime`, and neither a tag nor a clock

transpiler: partial

- hgraph: an alarm's one wake is kept under a tag of its own in hgraph's scheduler, and asking again keeps the earlier of the one pending and the one asked for; the wake being delivered is taken as a scheduler's is (CAP-4)
- One asked for at a `datetime` before the current evaluation time is dropped, as a scheduler's is, and the one pending stays; a zero duration in `start` wakes the node's first evaluation
- A duration not more than zero, but a zero in `start`, fails the call as the scheduler's does (CAP-4)
- Left, reported: a duration that may be zero asked for in a value function given the alarm (CAP-4)

## CAP-6

analyser: partial

- `get`, `has` and `put` on the global store, through core's candidates: the key a `str` that is a constant expression (`argument.constant`, FN-1), and `get`'s type what the call writes, a nullable one whole (`get<i64?>(...)` is an `i64?`), or its place expects
- The run's own key, `"run.output"`, written out as a key is rejected (`store.reserved_key`)
- Left: one key's type across a program; the run's key where it is a `const` parameter, which is known when the graph is built; a nullable place, `let saved: i64? = get(...)`, which gives the candidate the `i64` under the `?` until hhenson/hgl_spec#165 decides it

transpiler: partial

- hgraph: `get`, `has` and `put` read and write hgraph's `GlobalState`, which wakes no node and orders none: a `put` is seen by what hgraph evaluates after it; an absent key fails the node
- Left: a key keeping the type of its first value; the run's own key given as a `const` argument, refused when the graph is built

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

- hgraph: an eval is a run of an entry the harness wires: core's `replay`, written from core's own source for the type of each temporal input the eval gives a trace, the target, and core's `record` for the type of its result; `const` arguments are given as they are
- It is a simulation of hgraph's, `evaluate_graph`, under the eval's configuration
- Left: the entry is wired by the harness where the test runs, and not generated as HGL

## CEVAL-2

runner: planned

- Typed replay entries for present publications; a dense placeholder adds no entry and no run bound

transpiler: done

- hgraph: present dense cells and timed pairs give replay one entry per publication at its absolute instant
- A placeholder adds no entry and an empty replay schedules nothing.

## CEVAL-3

transpiler: done

- hgraph: core's `replay` is a source written from its own body: each entry at its time, equal values among them, and nothing for a silent slot; a set's entry is its delta, and a nullable's a published null or a value
- A window's entries are its arrivals, each a V the replayed window takes, which takes its values out itself

## CEVAL-4

transpiler: partial

- hgraph: core's `record` is a node written from its own body: its recording starts empty, takes the evaluation time and the delta of each publication, read where a `result` is taken and `ok` of hgraph's delta (END-3), and is put in the store under its key at stop, an empty one included
- Left, reported: the record of a window of a duration, which skips a tick at which a value only leaves (END-9)

## CEVAL-5

runner: planned

- A fresh run, with its own node state and recording, for each eval; dense results built by TST-6

transpiler: done

- hgraph: each eval has its own graph, store and recording
- Its result retains the run start and the absolute time of each publication for dense and timed assertions (TST-6, TST-7)

## CRUN-1

runner: planned

- A run of one entry with its `const` arguments under one `RunConfig`: built, started, evaluated and stopped once

transpiler: done

- hgraph: the harness wires the entry's graph, and hgraph builds it, starts it, evaluates it and stops it, once for each run: an application's, and each eval's
- `eval` is a simulation and `run` in real time

## CRUN-2

transpiler: partial

- hgraph: a run's `RunConfig` is given to the harness, which runs the entry from its `start`, and where it gives none from the earliest start in simulation and the host clock's present time in real time, and ends it before its `end`, or before `length` from that start; a configuration with both an `end` and a `length` is `run.end_and_length`, raised before anything is built
- Left: a run that ends where it starts builds no graph, since hgraph has no such run

runner: planned

- `start` inclusive and `end` exclusive; what a null `start` and a null `end` mean in each mode

## CRUN-3

transpiler: partial

- hgraph: a run with an explicit start before `earliest_start` fails before the graph is built, including an empty run
- The epoch is `never`; the first evaluation time is one microsecond later

runner: planned

- Evaluation time constant in a cycle and later in each by a microsecond or more; the earliest start

## CRUN-4

runner: planned

- Simulation runs each cycle without waiting and ends when nothing is scheduled; the same run gives the same publications

transpiler: done

- hgraph: an eval, a test's and an application's, is hgraph's own simulation, which runs each cycle without waiting and ends when nothing is scheduled before the run's end

## CRUN-5

runner: planned

- Real time runs a cycle when the host clock reaches it

transpiler: done

- hgraph: an application's `run` is hgraph's own run in real time

## CRUN-6

runner: planned

- Every node that started is stopped, in reverse; a failure names its node and phase, and the run has no output

transpiler: partial

- hgraph: hgraph stops every node that started, however the run ends; an application's run that fails has no output, and `hgl run` says the HGL error it met
- Left: the failure's node and phase are not reported apart from hgraph's own account

## CRUN-7

runner: planned

- One store for a run, which holds what its application put there when the graph is built; nothing outside the program puts a value there

transpiler: done

- hgraph: each run, an eval's and an application's, has a global state of its own
- Before its graph is built an application's holds what the application put there, and an eval's is empty; a graph that injects the store puts values there as hgraph builds it

## CRUN-8

analyser: partial

- The run's own key written out as a key of the global store is rejected (`store.reserved_key`)
- Left: a key that is a `const` parameter

transpiler: partial

- hgraph: a run's output, an eval's and an application's, is what is under `"run.output"` in its store once it has stopped, a tuple for each publication; an entry with no result has none
- Left, reported: the output of an entry whose result is a window of a duration, which `record` reads into a `result` (CEVAL-4)

runner: planned

- The run's output, a `list<tuple<datetime, delta<T>>>` recorded under the run's own key

## CEVAL-6

runner: planned

- A failed run or a missing required recording fails the eval; neither is an empty result
