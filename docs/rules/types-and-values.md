# Types and values

`types-and-values.md`

## TYPE-1

analyser: partial

- Scalar type identities
- Left: capabilities

transpiler: partial

- hgraph: `bool`, `i64`, `f64`, `str` and `bytes` are Python's own, each a type of its own, as values, configuration and temporal values; a `bytes` holds octets 0 to 255 and orders them unsigned
- Left: bytes made from octets, `value.byte_range`, which the analyser does not read yet

## TYPE-2

lexer: done

- Literal dates keep years 1 to 9999; literal durations fit signed 64-bit microseconds

analyser: partial

- Each temporal scalar a type of its own
- A temporal constant is the value it denotes: two `datetime`s are one where they are one UTC instant (UNIT-13: "`@2026-09-03T09:30Z` and `@2026-09-03T10:30+01:00`, one instant"), and two `zoned_datetime`s where they are one instant in one zone ("zone aliases are distinct names"); a `zoned_time` is its parts
- Left: the domains of computed values

transpiler: partial

- hgraph: a `duration` is a `timedelta`, a count of whole microseconds, as a value, as a constant and as what a connection ticks; its literal is written by its count
- A `date`, a `time`, a `datetime` and a `civil_datetime` are Python's `date`, `time` and `datetime`, written by the parts the literal has; an instant is a `datetime` in UTC that names no zone, as hgraph has them, and one written with another offset is moved to UTC
- A `timezone`, a `zoned_datetime` and a `zoned_time` are the provider's frozen dataclasses: a zone by its name, so aliases are distinct, and a zoned datetime its UTC instant and zone, equal where both are
- Two zoned datetimes compared are core's `eq_` and `ne_` for them (OPS-2): equal where the instants and the zones are, and with `utc` where the instants are. A set's members, a dictionary's keys, and what a list or a struct holds are told apart by instant and zone. A duration moves the instant across a change of offset
- Each is a value, configuration and a temporal value
- Left: a literal instant outside the years 1 to 9999 in UTC, which is reported: a Python `datetime` holds no other

## TYPE-3

analyser: partial

- Nominal identity by declaration and complete generic arguments, a value argument by its value (UNIT-13): `Scaled<1>` is `Scaled<1.00>`, and `Scaled<-0.0>` is not `Scaled<0.0>`; a parent is an abstract struct, listed once
- an abstract struct is not constructed in a value function
- Left: constructions elsewhere

transpiler: partial

- hgraph: a struct is a class the provider makes, with every field the struct has, a parent's first, read by name
- A value of a generic struct is of the class for its arguments, keyed by their values (UNIT-13), so two applications are two types; two values are equal when they are of one and each field is
- A struct's class and its bundle's schema are of the module that declares it, which hgraph knows them by with their names, so two modules' structs of one name are two types to it too
- As a temporal shape (TYPE-9) a generic struct's fields have the types its arguments give, under `atomic` it is a `TS` of the class for them, and an abstract struct's bundle has the fields it has itself: a connection of a struct below it is given where it is expected as a bundle of those fields
- Left: a generic struct made in a function generic in its arguments; a value of an abstract struct as configuration or under `atomic`, and a temporal one read whole, since its bundle does not say which struct below it the value is

## TYPE-4

analyser: partial

- A field's type, and so its optionality, agrees between a struct and its parents, and between parents; `name = value` overrides a parent's field; one member to a name; a default of null only on an optional field
- A default is read as a value of its field's type, `name = value` as one of the parent's field (`type.mismatch`), and kept for the stages after, `Module::defaults`; each field a struct has says which default a construction takes
- Left: the default of an `atomic<...>` field, which is held to no type; a default whose field's type names a generic parameter

## TYPE-5

analyser: partial

- A construction in a value function names each field it gives, in any order, and gives every field that has no default and is not optional; a positional argument is rejected
- Left: constructions elsewhere

transpiler: partial

- hgraph: a construction is written with its arguments in the order the source has them, which is the order they are evaluated in, and then the default of each field left out, `None` for an optional field with none: a default is evaluated only where its field is omitted
- A struct of at most two scalar fields is a value, never changed; any other is changed where its place holds it, and what retains it retains a copy (the owner's rule, issue 151), so a retained one keeps its data
- A struct is the type of an input and an output (TYPE-9)
- Left: a construction in a graph; a struct as configuration

## TYPE-6

analyser: partial

- No struct inherits from itself; a struct holds itself only through a field that is optional, `Node?`, `atomic<Node?>` or `atomic<Node>?`, or that holds it only as elements of a collection that may be empty; the field's default is `null` or `[]`, and the structs below keep a null one; the field passes each generic parameter on whole; an abstract-typed field holds each descendant; a collection is admitted inside an `atomic<...>` only
- Left: a constant argument built from a parameter

## TYPE-7

parser: done

- `.member` read as an expression where an operand is due

analyser: done

- Numbering from zero or the number written; numbers are integers no two members share; overflow rejected
- A member is of its enum, and `.member` is given its enum by the type its place expects
- A call of the enum's name constructs a member from an `i64` or a `str`, one argument by position; a constant that is no member's number or name is `member.unknown`
- A construction from a constant that names a member is read as that member: a `switch` label, a `const` argument and a generic value argument take it as they take `Side.buy`

transpiler: partial

- hgraph: an enum is a Python `Enum`, each member with its number, and a scalar of an input, an output or a configuration
- A member is read by its name however it was written, and a construction from a number or a name gives the member or fails where it is evaluated
- Left: string and numeric conversion

## TYPE-8

analyser: planned

- Typing of `any`

transpiler: planned

- A boxed value keeps its exact type; equality, capability failure, independent copies

## TYPE-9

analyser: partial

- An atomic payload is an ordinary value type
- A node publishes a V to a result of `atomic<V>`
- `atomic<S>` of a non-composite S is S, where the type is written and where a generic parameter is replaced, and an `atomic<T>` given a value of such a type says what `T` is
- A field of an `atomic<S>` struct is an atomic of the field's type
- In a hook an atomic input's payload is its V
- Left: temporalization of composite types; an atomic connection given to a lifted value function of V

transpiler: partial

- hgraph: a temporal set of a scalar or an enum is a `TSS`, whose tick is the set's delta
- A temporal struct is a `TSB` over a schema made beside its class, each field temporalized in turn, a generic struct's with the types its arguments give and an abstract struct's of its own fields; a field is the field's port in a graph and in a hook
- A tuple is a `TSB` with no name, a field `_0`, `_1` for each position, each temporalized in turn; a position is its port in a graph and in a hook, under a constant index, and a tuple of connections in a graph is the tuple's connection
- A whole struct or tuple published sets each part, a struct or tuple part by its own parts and a set field as the members it has now
- A struct with no field is a `TS` of its class, which ticks with each publication and is then valid and all valid (TIME-9)
- A struct or a tuple read whole, and a field or a position read alone, needs each part of a required type valid: one that is not fails where the read runs, with `value.unset_read`, and an optional one is null
- That is an interim reading of the owner's decision on hhenson/hgl_spec#126, whose spelling is open; no value is made up
- An ordinary struct, tuple, list, set or dictionary in a graph's branch is a constant of its time-series type
- `atomic<V>` is a `TS[V]`, read in a hook as its whole V, and a field of an atomic struct in a graph hgraph's `getattr_`
- A fixed list is a `TSL`, valid once a position is and all valid once each is, and a dictionary a `TSD`; an element is the child in a graph, by a constant position or by a key that may tick and that fails where absent (ERR-6), and a whole value is read in a hook and by a lifted helper
- An unsized list is hgraph 0.9.8's unbounded `TSL`, `Size[-1]`, of any element, as an input, an output, `out`, a struct's field, a tuple's position, a dictionary's value and a connection, a position its own child, valid or not; a whole value of a struct that holds one gives it as a tuple, and an empty one ticks through its port of the node's output
- A list read whole, in a hook, as a lifted value function's argument and as a trace cell's held value, is one function's (`_core.list_value`), as TIME-1's table has it (hhenson/hgl_spec#137, ahead of the pinned text): a position of an unsized list that holds no value, a gap or one removed, reads NaN for an `f64` and fails with `value.unset_read` for an `i64`; an optional element reads null, of a fixed list too (TYPE-15); every other element type fails, the table's one proposed row, and so does a fixed list that is not all valid; a graph's loop over an unsized one is hgraph's `map_` over the `TSL`
- An element of an unsized list's connection in a graph is the provider's `position_in`, by a position known where the graph is built or one that ticks: a connection that is not valid while the list does not hold the position or is not valid, invalidated whole among them; a dictionary's key selected (`entry_of`) is so too, where hgraph's `getitem_` fails the run
- Left: an atomic tuple; an element of an unsized list of composites selected in a graph, reported; an empty unsized list assigned to an entry or a list's position of `out`, which fails as the graph runs; a dictionary of composites read by key in a graph; a list's element at a position that ticks; a whole dictionary published, a whole fixed list with nullable or composite elements published, alone or as a part; a temporal abstract struct read whole; an ordinary fixed list or dictionary of composite elements as a graph branch's connection; a struct with no field as a part of a composite; an eval of a generic target given a trace with an atomic part, whose type is read back from its cells' delta

## TYPE-10

analyser: partial

- A size is a count that is not negative; `unbounded`; size in identity, by value; tuples distinct from lists
- A size is read by the one reader (`constant`) as an `i64`: a constant of a module, and a constant expression of those, is its value (UNIT-13); a list constant of another length than its place's size is of another type
- A size a generic value parameter gives is held where the parameter is given (`values::sizes`): by a struct application written in any type, a construction's and a parent's included, its argument written, defaulted, or worked out from a size expression, through the fields of the structs it applies; and by a call of a generic function, in its parameters and result. A negative count, or no count, is `list.size` there
- Left: an unsized pattern accepting a fixed size; a size inside a generic body, a local's `list<i64, N - 3>`, which GEN-7 judges once where the body is declared

transpiler: partial

- hgraph: a temporal list of a size is a `TSL` of that size; `list<T>` grows: hgraph 0.9.8's unbounded `TSL`, `Size[-1]`, which ticks by position, a child for each position it holds
- hgraph 0.9.8 does not tick that `TSL` for a publication that changes nothing it sees, an empty list first among them: for an empty whole value, and a delta that names no change, where the list is not valid (TIME-7), the provider grows it by a position and drops it again in the same evaluation, which leaves nothing in the delta read
- A temporal tuple is no list: a `TSB` with no name and a field for each position, each of its own type (TYPE-9)
- Left: `unbounded` and a size that is a generic parameter as a temporal shape

## TYPE-11

analyser: partial

- Size kind and bounds (`rolling_window.size_kind`, `rolling_window.size_bounds`) of sizes read by the one reader (`constant`): literals, generic value parameters, constants of a module and constant expressions of those, a duration and an `i64` multiplied in either order among them; a size of a type other than `i64` or `duration` is of the wrong kind; an omitted minimum is the maximum in identity
- `all_valid` takes a window (TIME-9), and a window's value read in a hook, of an input, of `out` or of a child of either, and given to a value function in composition, is a `list<V>` (FN-12, END-9); that list indexed is no child (TIME-9)
- Read where the place takes a `result<list<V>, error>`, returned, bound or given, it is of that result (`CheckedWindow`, or `CheckedChild` where it is a child known when the program runs), as ERR-14 has it
- Its delta is the V that arrived (TYPE-16), and of a nullable V the nullable's own delta, `delta<V>`, which its result takes as well (TIME-7), read before the window is all valid as after; a read of the delta of a window of a duration, an input's, a child's or `out`'s, whose place is not a `result` is `payload.unguarded`, and one of a window of a tick count is not
- Core's `removed_value` is read where its place takes a `result<V, error>` (ERR-14); a read whose place is not a `result` is `payload.unguarded` for every window
- A window's result takes a V and nothing else (TIME-7); its trace's cells are V (TST-5), a generic target's once its arguments are known
- A size a generic value parameter gives is held to the same kind and bounds where the parameter is given, as TYPE-10's is (`values::sizes`): `Ticks<0>` for a `rolling_window<f64, N>` is `rolling_window.size_bounds`, and `Kinded<"x">` `rolling_window.size_kind`
- Left: a size computed by a call; a size inside a generic body (GEN-7); the delta of a window of a duration that a generic parameter stands for, which is read in an instance

transpiler: partial

- hgraph: a window of a scalar or an enum is a `TSW` of `WindowSize[n]` ticks or of a `timedelta`, one size where no minimum is written, and `TSW_OUT` for `out`
- A value published to it, and its `delta_value`, is the value that arrived (TYPE-16); `valid`, `modified` and `last_modified` are the `TSW`'s; an eval's replay and record are core's for the window
- A window within a struct, a tuple, a fixed list or a dictionary is that one's child, wired by its port in a graph, and that value's validity is its children's
- A window is valid at its first value and all valid at its minimum, of ticks or of its arrivals' span, the `TSW`'s own; in one of a duration a value leaves one smallest step after its age is the maximum, whether or not a value arrives, and the window ticks for the exit, which wakes its readers; a minimum of `0s` is written as it is, held by the first value; an empty window is not all valid, and one emptied by exits stays valid, as hgraph 0.9.8 does (hhenson/hgraph#1818)
- `delta_value` of a window of ticks, read plain or where a `result` is taken, is the `TSW`'s, the value that arrived; core's `record` of one reads it so (ERR-14)
- A window's value, of an input, of `out`, of a child where it is admitted or checked when it runs, in a `when` or `stop`, and given to a lifted helper, is the `list<V>` it holds, oldest first (`_core.window_values`), which fails with `value.unset_read` before the window is all valid, where hgraph 0.9.8 reads what it holds
- Where its place takes a `result<list<V>, error>` it is `ok` of that list, or before the window is all valid `err` of an `error` coded `value.unset_read` (`_core.window_result`, and `_core.checked_child_result` of a child known when it runs); given to a lifted parameter with a default, FN-11's entry says what binds
- Left, reported: the value of a window that a generic type parameter stands for, which the body, checked once (GEN-7), reads as a `T`; a value that holds a window read whole; invalidating a value that holds one, which hgraph's `TSW` survives with its values; a window, alone or within, given to the graph of a branch or a loop, which hgraph's `switch_` and `map_` fail on; multiple writes where the node does not explicitly invalidate its direct window, or reads the output while writes are held; a window of a nullable, a struct or a collection; the delta of a value that holds a window of a duration, whose field's delta where a value only leaves the rules do not give, and hgraph gives none; `delta_value` of a window of a duration and `removed_value`, read where a `result` is taken, and so core's `record` of a window of a duration

## TYPE-12

parser: done

- `endpoint<T>` is a type of one argument; `ref` starts no type, so `ref<f64>`, `list<ref<f64>>` and `ref<ref<f64>>` are syntax errors

analyser: done

- `ref` is no type but a mark of a node's parameter and result, whose type is the T after it (FN-16); a graph gives a `ref` parameter of T a connection of T
- A `signal` parameter marked `ref` is `ref.position` (FN-16)
- `rolling_window`, `atomic` and `signal` kept out of keys, payloads, window values, constant configuration and value functions; `signal` only as a temporal function's input; `endpoint<T>` only as the whole type of a native value function's parameter (`endpoint.position`)
- A `signal` takes no default but `null` (`type.mismatch`), and a hook gives it to an endpoint operation and reads it nowhere else (`type.not_ordinary`)

transpiler: partial

- hgraph: `signal` is a `SIGNAL`, and a `ref` parameter or result a `REF` of its type (FN-16)
- Left: a pack of signals

## TYPE-13

analyser: done

- A `result` and an `error` are their own atomic and their own delta; a result's payloads are ordinary value types; `null` is no result, and a nullable result is tag-tested only where proved present

## TYPE-14

parser: done

- `?` after a type

analyser: partial

- What `?` is applied to is an ordinary value type throughout, but on the whole type of a temporal input or a struct field; a set's element and a dictionary's key take none
- a literal `null` is no value of a type with no `?`, and is rejected where nothing gives it a type
- Left: null from other expressions

transpiler: partial

- hgraph: a temporal `T?` of a scalar or an enum is `_hg.TS[T | None]`, hgraph's value that holds one `T` or is unset, and a `const` one `T | None`; a null is Python's `None` as a value, and hgraph's `Null` where it is published
- A `bytes`, a tenor and the zoned types included
- Needs hgraph 0.9.1
- Left: a nullable set or struct

## TYPE-15

parser: done

- The suffix covers the whole preceding type; `?` in a type and `?` in an expression kept apart

analyser: partial

- `list<f64?>` and `list<f64>?` are two types
- Left: a present null is data, distinct from an invalid input

transpiler: partial

- hgraph: in a value read whole, in a hook, by a lifted helper and as a trace cell's held value, an optional child that is not valid is null, and `valid` of the child tells it from a published null; `all_valid` does not ask it (TIME-9)

## TYPE-16

analyser: partial

- `delta<T>` is what the table gives for T, where it is written and where a generic parameter is replaced: T itself, the V of an atomic or a window (of a window of a nullable V, `delta<V>`), a tuple of each position's delta under a `?`, and a type of its own for a nullable, a list, a set, a dictionary and a struct
- A `signal` has none (`delta.unsupported_shape`); a `ref` parameter's `delta_value` is not read (`ref.read`, FN-16)
- Each delta that is a type of its own has its properties: a set's `added` and `removed`, a list's `updated`, `removed` and `size_modified`, a dictionary's `updated` and `removed`, a nullable's `value`, a struct's one for each field; none is assigned (`binding.immutable`)
- It is constructed by them, by name, each a value of the property's type; a name that is none and one given twice are rejected (`delta.argument_name`, `delta.duplicate_argument`)
- A delta that is another type the language has, a scalar's, a tuple's or a window's, has no constructor: each property its constructor names, or the constructor where it names none, is `delta.argument_name`
- A `delta<V>` is a value where a `delta<V?>` is expected, and null is not
- A set's, a dictionary's or a list's delta that names one constant entry both as added or updated and as removed is `delta.overlap`, constants read by the one reader (`constant`) as values of the key's type: `-1`, an enum's member however written, `60s` and `1m`, an integer literal as an `f64` key. Two are compared by the language's `==` (`ConstantValue::equals`), as TYPE-16 has it, "and not by their sameness as constants (UNIT-13)": `0.0` and `-0.0` are one key, and two NaNs two
- A constant position of a list's delta that is negative or past a fixed list's bounds is `delta.index_bounds`; a negative `size_modified` is a count to reduce by
- Left: a constructor written for a tuple's delta, which is not judged

transpiler: partial

- hgraph: a set's delta, made from its properties, is hgraph's own set delta, in a node, in a trace and as a value
- A nullable's is hgraph's own too: `Null` for a published null, and the value otherwise; `delta<i64?>()` and `delta<i64?>(value: v)` make one, and its `value` is read through the provider
- A struct's is hgraph's delta of a bundle, a `dict` of the fields that changed, made from its properties and read by them through the provider
- A tuple's is a tuple, null where a position did not change, which hgraph's bundle takes as it is; hgraph's `dict` of each position that changed is read as that tuple through the provider, in a node and in a trace
- A tuple's delta that holds an unsized list, a dictionary or a set, at any depth, is given hgraph as a `dict` of the positions it gives (`_core._by_position`): hgraph 0.9.8's bundle given a tuple fails the run (`std::bad_cast`) for a key removed from a dictionary in it, which a `dict` takes; from a `return`, `out =`, a `yield` and a trace's cell, inside a struct and a dictionary too
- A list's is hgraph 0.9.8's `TSLDelta`, a `dict` of each position updated and of each removed with `REMOVE`, with `size_modified`, made from its properties and read by them, for a fixed list and an unsized one alike
- An unsized list's published is counted from the size the list holds, on the node's `_output` (`_core.list_applied`): a smaller `size_modified` alone shrinks it, a negative one reduces it by that count, to empty at most, an entry after the last extends it, and one further past leaves the positions between not valid (agreed on hhenson/hgl_spec#124); `removed` alone leaves the size; the delta read is hgraph's, the dropped tail in `removed`
- A fixed list's delta made as the graph runs fails, with no code, where a position is past its bounds or a `size_modified` moves its size (`_core.list_within`); its `removed` makes the position not valid
- A dictionary's is hgraph's delta of a `TSD`, a `dict` of each key updated with its delta and of each removed with `REMOVE_IF_EXISTS`, made from `updated` and `removed` and read by them through the provider
- A `removed` that names a key a dictionary does not hold, or a member a set does not hold, is ignored and is no failure (`_core.entries_published`, END-3)
- Published, a delta that names no change, `delta<set<i64>>()`, ticks only where it makes the value valid (TIME-7)
- A struct's and a tuple's delta that holds an unsized list's delta is published through the node's output, each list's size counted from what it holds (`_core.published_into`), by a node and by a source when it is due
- A removed position an unsized list does not hold is ignored, as a dictionary's absent key is (the owner's decision on hhenson/hgl_spec#124)
- An entry at or past a `size_modified` written with it contradicts the delta and fails the write that publishes it as the graph runs, with no execution code (the owner's decision there: "a failure"; hhenson/hgl_spec#137)
- Left: a constant one whose size is no reduction, which hhenson/hgl_spec#137 rejects when compiled with `delta.index_bounds`: the analyser's delta checks read the size with the constant reader hhenson/hgl-compiler#218 is rewriting, so it waits on that
- Left, reported: an unsized list's delta inside a list's or a dictionary's delta the program makes, which hgraph applies with no negative size and whose position or key it may add
- The delta of a list or a dictionary that holds a tuple, read, is reported

## TYPE-17

analyser: done

- `tenor` is a scalar type of its own, and a tenor literal is one: its count of months and of days, or of business days, so `1Y` and `12M` are one value
- Its operators are core's (TEN-1)

transpiler: done

- hgraph: a tenor is the provider's `Tenor`, a frozen dataclass of its counts, as a value, configuration and a temporal input or result, a nullable one included; its literal is written by its counts
