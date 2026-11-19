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
- Left: the domains of computed values

transpiler: partial

- hgraph: a `duration` is a `timedelta`, a count of whole microseconds, as a value, as a constant and as what a connection ticks; its literal is written by its count
- A `date`, a `time`, a `datetime` and a `civil_datetime` are Python's `date`, `time` and `datetime`, written by the parts the literal has; an instant is a `datetime` in UTC that names no zone, as hgraph has them, and one written with another offset is moved to UTC
- A `timezone`, a `zoned_datetime` and a `zoned_time` are the provider's frozen dataclasses: a zone by its name, so aliases are distinct, and a zoned datetime its UTC instant and zone, equal where both are
- Each is a value, configuration and a temporal value
- Left: a literal instant outside the years 1 to 9999 in UTC, which is reported: a Python `datetime` holds no other

## TYPE-3

analyser: partial

- Nominal identity by declaration and complete generic arguments; a parent is an abstract struct, listed once
- an abstract struct is not constructed in a value function
- Left: constructions elsewhere

transpiler: partial

- hgraph: a struct is a class the provider makes, with every field the struct has, a parent's first, read by name
- A value of a generic struct is of the class for its arguments, so two applications are two types; two values are equal when they are of one and each field is
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

analyser: partial

- Numbering from zero or the number written; numbers are integers no two members share; overflow rejected
- in a value function a member is of its enum, and `.member` is given its enum by the type its place expects
- A call of the enum's name constructs a member from an `i64` or a `str`, one argument by position; a constant that is no member's number or name is `member.unknown`
- A construction from a constant that names a member is read as that member: a `switch` label, a `const` argument and a generic value argument take it as they take `Side.buy`
- Left: a `switch` label by its selector; temporal bodies

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
- An unsized list is a `TS` of a whole tuple of scalars or enums
- Left: an atomic tuple; an unsized list of composite elements, its delta and its children, and one as a child of a composite; a dictionary of composites read by key in a graph; a list's element at a position that ticks; a whole dictionary published, a whole fixed list with nullable or composite elements published, alone or as a part; a temporal abstract struct read whole; an ordinary fixed list or dictionary of composite elements as a graph branch's connection; a struct with no field as a part of a composite; an eval of a generic target given a trace with an atomic part, whose type is read back from its cells' delta

## TYPE-10

analyser: partial

- A size is a count that is not negative; `unbounded`; size in identity; tuples distinct from lists
- A size that names a constant of a module is that constant's value (UNIT-13)
- Left: sizes computed from other constants; an unsized pattern accepting a fixed size

transpiler: partial

- hgraph: a temporal list of a size is a `TSL` of that size; `list<T>` grows, and hgraph has no `TSL` that grows, so it is a `TS` of a whole tuple, whose tick is the complete list
- A temporal tuple is no list: a `TSB` with no name and a field for each position, each of its own type (TYPE-9)
- Left: `unbounded` and a size that is a generic parameter as a temporal shape; position-wise ticks of an unsized list

## TYPE-11

analyser: partial

- Size kind and bounds (`rolling_window.size_kind`, `rolling_window.size_bounds`) of literal sizes, generic value parameters, constants of a module, and `+`, `-`, `*` over them, a duration and an `i64` multiplied in either order among them; a size naming a constant of a type other than `i64` or `duration` is of the wrong kind; an omitted minimum is the maximum in identity
- Left: a size computed by a call, by `/`, `//` or `%`, or from a generic parameter, whose kind and bounds are not checked

transpiler: partial

- hgraph: a window of a scalar or an enum is a `TSW` of `WindowSize[n]` ticks or of a `timedelta`, one size where no minimum is written, and `TSW_OUT` for `out`
- A value published to it, and its `delta_value`, is the value that arrived (TYPE-16); `valid`, `modified` and `last_modified` are the `TSW`'s; an eval's replay and record are core's for the window
- A window within a struct, a tuple, a fixed list or a dictionary is that one's child, wired by its port in a graph, and that value's validity is its children's
- Core gives a window no readiness contract (FN-12), so a value that holds one, at any depth, is read as an input or as `out` only where each window it holds has a minimum of one tick, ready at its first value under any reading
- Left, reported: such a read where a window's minimum is more than one tick or a duration; what a window holds, which no core operation reads, whole or within another value; invalidating a value that holds one, which hgraph's `TSW` survives with its values; a window, alone or within, given to the graph of a branch or a loop, which hgraph's `switch_` and `map_` fail on; a window `out` written where the evaluation may publish to it again, as hgraph's `TSW` takes one value a cycle; a window of a nullable, a struct or a collection

## TYPE-12

parser: done

- `endpoint<T>` is a type of one argument, written as `ref<T>` is

analyser: partial

- `ref` and `signal` kept out of keys, payloads, window values, constant configuration and value functions; `signal` only as a temporal function's input; `endpoint<T>` only as the whole type of a native value function's parameter (`endpoint.position`)
- In a graph a function's input of `ref<T>` takes a connection of `T`, one of `T` a `ref<T>` (TIME-10), as a graph's result does, and a `signal` any connection
- A `signal` takes no default but `null` (`type.mismatch`), and a hook gives it to an endpoint operation and reads it nowhere else (`type.not_ordinary`); a hook gives a `ref` to no value function (`type.not_ordinary`)
- Left: a `ref` in a tuple or a struct a hook builds; a reference given to an operator's call in a graph

transpiler: partial

- hgraph: `ref<T>` is a `REF` of the temporal T, whose value a node returns to re-point what follows it, and `signal` a `SIGNAL`
- A graph passes a connection across a reference as it is, and a lifted input given a `ref<T>` is one of T
- Left: a reference as a child of a temporal composite; a pack of signals; a struct connection given across a reference to a struct it inherits from

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

## TYPE-16

analyser: partial

- `delta<T>` is what the table gives for T, where it is written and where a generic parameter is replaced: T itself, the V of an atomic or a window, a tuple of each position's delta under a `?`, and a type of its own for a nullable, a list, a set, a dictionary and a struct
- A `ref` and a `signal` have none (`delta.unsupported_shape`)
- Each delta that is a type of its own has its properties: a set's `added` and `removed`, a list's `updated`, `removed` and `size_modified`, a dictionary's `updated` and `removed`, a nullable's `value`, a struct's one for each field; none is assigned (`binding.immutable`)
- It is constructed by them, by name, each a value of the property's type; a name that is none and one given twice are rejected (`delta.argument_name`, `delta.duplicate_argument`)
- A `delta<V>` is a value where a `delta<V?>` is expected, and null is not
- A set's, a dictionary's or a list's delta that names one constant entry both as added or updated and as removed is `delta.overlap`, constants compared by value: `-1`, an enum's member, an `i64` widened to an `f64` key
- A constant position of a list's delta that is negative or past a fixed list's bounds is `delta.index_bounds`; a negative `size_modified` is a count to reduce by
- Left: a constructor written for a tuple's delta, which is not judged

transpiler: partial

- hgraph: a set's delta, made from its properties, is hgraph's own set delta, in a node, in a trace and as a value
- A nullable's is hgraph's own too: `Null` for a published null, and the value otherwise; `delta<i64?>()` and `delta<i64?>(value: v)` make one, and its `value` is read through the provider
- A struct's is hgraph's delta of a bundle, a `dict` of the fields that changed, made from its properties and read by them through the provider
- A tuple's is a tuple, null where a position did not change, which hgraph's bundle takes as it is; hgraph's `dict` of each position that changed is read as that tuple through the provider, in a node and in a trace
- A list's is the provider's `ListDelta`, hgraph's `dict` of each position updated, with `size_modified`: a fixed list's is hgraph's own; an unsized list's is applied to what the node last published: a shrink by `size_modified`, a negative one reducing it by that count and to empty at most, or by a `removed` tail, and growth by an entry at the position after the last
- A fixed list's delta made as the graph runs fails, with no code, where a position is past its bounds (`_core.list_within`)
- A dictionary's is hgraph's delta of a `TSD`, a `dict` of each key updated with its delta and of each removed with `REMOVE`, made from `updated` and `removed` and read by them through the provider
- Left, reported: an unsized list's delta read; a fixed list's `removed`, written or read, and its `size_modified` written (hgraph's bounded `TSL` reports neither); an unsized list's position removed short of its end fails as the graph runs; so does growth past the position after the last, whose positions between are not valid (TYPE-16, agreed on hhenson/hgl_spec#124): a whole tuple has no position that is not valid, and hgraph 0.9.3's unbounded `TSL` gives exactly the gap, so building it is moving `list<V>` onto that `TSL`
- The delta of a list or a dictionary that holds a tuple, read, is reported

## TYPE-17

analyser: done

- `tenor` is a scalar type of its own, and a tenor literal is one: its count of months and of days, or of business days, so `1Y` and `12M` are one value
- Its operators are core's (TEN-1)

transpiler: done

- hgraph: a tenor is the provider's `Tenor`, a frozen dataclass of its counts, as a value, configuration and a temporal input or result, a nullable one included; its literal is written by its counts
