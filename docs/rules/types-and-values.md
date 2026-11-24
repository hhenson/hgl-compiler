# Types and values

`types-and-values.md`

## TYPE-1

analyser: partial

- Scalar type identities
- Left: capabilities

transpiler: partial

- Uses distinct C++ representations for bool, i64, f64, Unicode strings and byte
  sequences and the admitted ordinary collection types.
- Left: The unstable key restriction for f64 and types containing it is pending. An
  ordinary type with no C++ representation is reported.

## TYPE-2

lexer: done

- Literal dates keep years 1 to 9999; literal durations fit signed 64-bit microseconds

analyser: partial

- Each temporal scalar a type of its own
- A temporal constant is the value it denotes: two `datetime`s are one where they are one UTC instant (UNIT-13: "`@2026-09-03T09:30Z` and `@2026-09-03T10:30+01:00`, one instant"), and two `zoned_datetime`s where they are one instant in one zone ("zone aliases are distinct names"); a `zoned_time` is its parts
- Left: the domains of computed values

transpiler: partial

- Writes temporal scalar domains as distinct SDK value types, including civil time and
  zoned values, with provider-backed zone lookup.
- Left: Temporal connections of timezone and zoned time are outside the scalar family.
  Unsupported literal forms and types are reported.

## TYPE-3

analyser: partial

- Nominal identity by declaration and complete generic arguments, a value argument by its value (UNIT-13): `Scaled<1>` is `Scaled<1.00>`, and `Scaled<-0.0>` is not `Scaled<0.0>`; a parent is an abstract struct, listed once
- an abstract struct is not constructed in a value function
- Left: constructions elsewhere

transpiler: partial

- Uses nominal C++ struct types and abstract bases, generating a template for supported
  generic arguments.
- Left: Reports self-containing structs, abstract fields named after their type,
  unsupported fields or bases, and template applications that lose HGL atomic identity
  or would duplicate nullability. Struct parameter packs and value parameters outside
  the supported constant kinds are not emitted.

## TYPE-4

analyser: partial

- A field's type, and so its optionality, agrees between a struct and its parents, and between parents; `name = value` overrides a parent's field; one member to a name; a default of null only on an optional field
- A default is read as a value of its field's type, `name = value` as one of the parent's field (`type.mismatch`), and kept for the stages after, `Module::defaults`; each field a struct has says which default a construction takes
- Left: the default of an `atomic<...>` field, which is held to no type; a default whose field's type names a generic parameter

## TYPE-5

analyser: partial

- A construction in a value function names each field it gives, in any order, and gives every field that has no default and is not optional; a positional argument is rejected
- Once an inferred graph construction has concrete field types, its implied enum members, empty collections and null literals use those types, including in block tails (GEN-2, CTRL-10, TYPE-7, TYPE-14, EXPR-9).
- Left: constructions elsewhere

transpiler: partial

- Constructs concrete structs by name and in argument order, using inherited defaults
  and implicit nulls.
- Writes omitted field defaults in their defining scope, including statement blocks
  and defaults whose type is supplied by a generic struct application.
- Left: Reports field defaults whose specialization cannot be emitted, defaults
  that propagate failure, and defaults that recursively omit themselves.
  Unmapped fields and missing required constructor inputs have defensive reports.

## TYPE-6

analyser: partial

- No struct inherits from itself; a struct holds itself only through a field that is optional, `Node?`, `atomic<Node?>` or `atomic<Node>?`, or that holds it only as elements of a collection that may be empty; the field's default is `null` or `[]`, and the structs below keep a null one; the field passes each generic parameter on whole; an abstract-typed field holds each descendant; a collection is admitted inside an `atomic<...>` only
- Left: a constant argument built from a parameter

transpiler: partial

- Emits abstract parent values and accesses inherited fields through the concrete
  struct's generated implementation.
- Left: Reports recursive struct layouts, even where the analyser admits them through
  nullable fields or empty-capable containers.

## TYPE-7

parser: done

- `.member` read as an expression where an operand is due

analyser: done

- Numbering from zero or the number written; numbers are integers no two members share; overflow rejected
- A member is of its enum, and `.member` is given its enum by the type its place expects
- A call of the enum's name constructs a member from an `i64` or a `str`, one argument by position; a constant that is no member's number or name is `member.unknown`
- A construction from a constant that names a member is read as that member: a `switch` label, a `const` argument and a generic value argument take it as they take `Side.buy`

transpiler: partial

- Emits enum members and constructs members from names or numbers. Supplies enum
  payload, constant and trace forms.
- Left: An unavailable enum layout or unsupported construction source is reported. A
  failed construction lacks `enum.no_member`.

## TYPE-8

analyser: planned

- Typing of `any`

transpiler: planned

- The value representation for boxed `any` is pending.

## TYPE-9

analyser: partial

- An atomic payload is an ordinary value type
- A node publishes a V to a result of `atomic<V>`
- `atomic<S>` of a non-composite S is S, where the type is written and where a generic parameter is replaced, and an `atomic<T>` given a value of such a type says what `T` is
- A graph instance fills inferred types inside a retained `atomic<V>` wrapper and normalizes a field from its settled payload (GEN-7, EXPR-7).
- A field of an `atomic<S>` struct is an atomic of the field's type
- In a hook an atomic input's payload is its V
- Left: temporalization of composite types; an atomic connection given to a lifted value function of V

transpiler: partial

- Writes structural temporal lists, tuples, structs, sets and dictionaries, including
  atomic struct connections, and maps supported children to endpoints.
- Gives concrete struct applications with supported type arguments their own nominal
  schemas and maps their substituted fields to child endpoints.
- Left: Reports abstract temporal structs, structs with generic value arguments,
  bundles with unsupported field or type argument schemas, list parts that are
  sets, dictionaries, windows or atomic structs, nullable
  aggregate connections, non-scalar temporal keys, and dictionary values outside
  scalars, sets, dictionaries and tuples of supported keyed forms. The family dispatch
  reports unsupported schemas, reads, writes, deltas, constants and traces.

## TYPE-10

analyser: partial

- `list<T>` and `list<T, unbounded>` share one type, instance identity and API documentation type name. Fixed sizes are nonnegative counts and participate in list identity. Tuples have positional identity.
- A size is read by the one reader (`constant`) as an `i64`: a constant of a module, and a constant expression of those, is its value (UNIT-13); a list constant of another length than its place's size is of another type
- A size a generic value parameter gives is held where the parameter is given (`values::sizes`): by a struct application written in any type, a construction's and a parent's included, its argument written, defaulted, or worked out from a size expression, through the fields of the structs it applies; and by a call of a generic function, in its parameters and result. A negative count, or no count, is `list.size` there
- Ordinary and generic value functions accept a fixed-size list at an unsized `list<T>` parameter when the element type agrees.
- Left: fixed-size connections at unsized inputs, and a size inside a generic body, a local's `list<i64, N - 3>`, which GEN-7 judges once where the body is declared

transpiler: partial

- Writes growing lists (`list<T>` and `list<T, unbounded>`) and positive fixed-size lists in value code and as connections.
- Left: Zero-size and unresolved sizes have no C++ representation. Temporal element
  forms follow TYPE-9.

## TYPE-11

analyser: partial

- Size kind and bounds (`rolling_window.size_kind`, `rolling_window.size_bounds`) of sizes read by the one reader (`constant`): literals, generic value parameters, constants of a module and constant expressions of those, a duration and an `i64` multiplied in either order among them; a size of a type other than `i64` or `duration` is of the wrong kind; an omitted minimum is the maximum in identity
- `all_valid` takes a window (TIME-9), and a window's value read in a hook, of an input, of `out` or of a child of either, and given to a value function in composition, is a `list<V>` (FN-12, END-9); that list indexed is no child (TIME-9)
- Read where the place takes a `result<list<V>, error>`, returned, bound or given, it is of that result (`CheckedWindow`, or `CheckedChild` where it is a child known when the program runs), as ERR-14 has it
- Its delta is the V that arrived (TYPE-16), and of a nullable V the nullable's own delta, `delta<V>`, which its result takes as well (TIME-7). A duration window's delta takes a result or nullable place and rejects a plain place (`payload.unguarded`); a tick-count window's delta can also take a plain place.
- Core's `removed_value` is read where its place takes a `result<V, error>` (ERR-14); a read whose place is not a `result` is `payload.unguarded` for every window
- A window's result takes a V and nothing else (TIME-7); its trace's cells are V (TST-5), a generic target's once its arguments are known
- A size a generic value parameter gives is held to the same kind and bounds where the parameter is given, as TYPE-10's is (`values::sizes`): `Ticks<0>` for a `rolling_window<f64, N>` is `rolling_window.size_bounds`, and `Kinded<"x">` `rolling_window.size_kind`
- A generic hook applies the duration-window delta restriction when its instance supplies that input shape (GEN-7).
- Left: a size computed by a call; a size inside a generic body (GEN-7)

transpiler: partial

- Writes tick-count and duration windows with their analysed maximum and minimum, and
  emits their arrivals and supported reads.
- Left: Windows of nonscalar types, enums, timezone and zoned time are reported.
  Endpoint restrictions are under END-9.

## TYPE-12

parser: done

- `endpoint<T>` is a type of one argument; `ref` starts no type, so `ref<f64>`, `list<ref<f64>>` and `ref<ref<f64>>` are syntax errors

analyser: done

- `ref` is no type but a mark of a node's parameter and result, whose type is the T after it (FN-16); a graph gives a `ref` parameter of T a connection of T
- A `signal` parameter marked `ref` is `ref.position` (FN-16)
- `rolling_window`, `atomic` and `signal` kept out of keys, payloads, window values, constant configuration and value functions; `signal` only as a temporal function's input; `endpoint<T>` only as the whole type of a native value function's parameter (`endpoint.position`)
- A `signal` takes no default but `null` (`type.mismatch`), and a hook gives it to an endpoint operation and reads it nowhere else (`type.not_ordinary`)

transpiler: partial

- Maps a mode-marked node parameter or result to reference wiring while preserving the
  parameter's payload type in ordinary code. Endpoint arguments are borrowed by
  supported natives.
- Left: a `signal` input has no supported temporal schema. Reference designations and
  borrowed endpoint calls are limited to the forms under FN-16 and TIME-9.

## TYPE-13

analyser: done

- A `result` and an `error` are their own atomic and their own delta; a result's payloads are ordinary value types; `null` is no result, and a nullable result is tag-tested only where proved present

## TYPE-14

parser: done

- `?` after a type. The parser rejects postfix expression `?`.

analyser: partial

- What `?` is applied to is an ordinary value type throughout, but on the whole type of a temporal input or a struct field; a set's element and a dictionary's key take none
- a literal `null` is no value of a type with no `?`, and is rejected where nothing gives it a type
- Left: null from other expressions

transpiler: partial

- Uses optionals for ordinary `T?`, preserves null in admitted collection types and
  compares a nullable value with its T. Writes nullable scalar and enum connections and
  lifts T to a supported `T?` connection.
- Left: Temporal nullable composites and generic applications where nullability would
  incorrectly nest are unsupported (TYPE-9, TYPE-3).

## TYPE-15

parser: done

- The suffix covers the whole preceding type.

analyser: partial

- `list<f64?>` and `list<f64>?` are two types
- Left: a present null is data, distinct from an invalid input

transpiler: partial

- Keeps null distinct from invalidity and silence in nullable scalar and enum
  connections. A supported bundle's absent optional field reads as null.
- A graph's comparison of a nullable scalar connection with null publishes a
  Boolean on each input publication, including a published null.
- Left: Nullable temporal aggregate types and unsupported optional field forms follow
  TYPE-9.

## TYPE-16

analyser: partial

- `delta<T>` is what the table gives for T, where it is written and where a generic parameter is replaced: T itself, the V of an atomic or a window (of a window of a nullable V, `delta<V>`), a tuple of each position's delta under a `?`, and a type of its own for a nullable, a list, a set, a dictionary and a struct
- A `signal` has none (`delta.unsupported_shape`); a `ref` parameter's `delta_value` is not read (`ref.read`, FN-16)
- Each delta that is a type of its own has its properties: a set's `added` and `removed`, a list's `updated`, `removed` and `size_modified`, a dictionary's `updated` and `removed`, a nullable's `value`, a struct's one for each field; none is assigned (`binding.immutable`)
- It is constructed by them, by name, each a value of the property's type; a name that is none and one given twice are rejected (`delta.argument_name`, `delta.duplicate_argument`)
- A generic graph instance fills inferred types inside a retained `delta<T>` wrapper, derives a property's type from its settled base and fits a constructor's property value (GEN-7, EXPR-5, EXPR-7).
- A delta that is another type the language has, a scalar's, a tuple's or a window's, has no constructor: each property its constructor names, or the constructor where it names none, is `delta.argument_name`
- A `delta<V>` is a value where a `delta<V?>` is expected, and null is not
- A set's, a dictionary's or a list's delta that names one constant entry both as added or updated and as removed is `delta.overlap`, constants read by the one reader (`constant`) as values of the key's type: `-1`, an enum's member however written, `60s` and `1m`, an integer literal as an `f64` key. Two are compared by the language's `==` (`ConstantValue::equals`), as TYPE-16 has it, "and not by their sameness as constants (UNIT-13)": `0.0` and `-0.0` are one key, and two NaNs two
- A constant position of a list's delta that is negative or past a fixed list's bounds is `delta.index_bounds`; a negative `size_modified` is a count to reduce by
- Left: a constructor written for a tuple's delta, which is not judged

transpiler: partial

- Writes scalar, nullable, whole atomic, tuple, list, set, dictionary and struct deltas
  with the supported properties and temporal adapters.
- Left: Reports unsupported delta constructors, properties and member-delta types. Trace
  and publication support depend on TYPE-9. A window's delta is available only through
  its supported endpoint forms (END-9).

## TYPE-17

analyser: done

- `tenor` is a scalar type of its own, and a tenor literal is one: its count of months and of days, or of business days, so `1Y` and `12M` are one value
- Its operators are core's (TEN-1)

transpiler: done

- Uses the shared SDK tenor scalar and preserves its two forms and checked counts in
  emitted values.
