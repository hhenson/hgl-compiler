# Types and values

`types-and-values.md`

## TYPE-1

analyser: partial

- Scalar type identities
- Left: Key, equality and ordering capabilities. Source checking does not reject
  `f64` keys or keys containing an `f64` (#956).

optimiser: done

- Splits a string append's `+` chain into ordered pieces where their reads are independent of the destination and evaluation order is preserved (EXPR-3).

transpiler: partial

- Uses distinct C++ representations for bool, i64, f64, Unicode strings and byte
  sequences and the admitted ordinary collection types.
- A declared `str` variable appends in place with `+=`, so each piece is copied amortised once as the destination grows. [Agreed TYPE-1 and EXPR-3 text](https://github.com/hhenson/hgl_spec/pull/243).
- Left: Ordinary types without a C++ representation are reported. Boxed `any`
  is pending under TYPE-8.

## TYPE-2

lexer: done

- Literal dates keep years 1 to 9999; literal durations fit signed 64-bit microseconds

analyser: partial

- Each temporal scalar a type of its own
- A temporal constant is the value it denotes: two `datetime`s are one where they are one UTC instant (UNIT-13: "`@2026-09-03T09:30Z` and `@2026-09-03T10:30+01:00`, one instant"), and two `zoned_datetime`s where they are one instant in one zone ("zone aliases are distinct names"); a `zoned_time` is its parts
- Left: The cold reader can leave a date computed through a call unevaluated.
  Calendar arithmetic checks its result's domain when it executes (#240).

transpiler: partial

- Writes temporal scalar domains as distinct SDK value types, including civil time and
  zoned values, with provider-backed zone lookup.
- Left: `timezone` and `zoned_time` connections are declined (#957). Unsupported
  literal forms and types are reported.

## TYPE-3

analyser: partial

- Nominal identity by declaration and complete generic arguments, a value argument by its value (UNIT-13): `Scaled<1>` is `Scaled<1.00>`, and `Scaled<-0.0>` is not `Scaled<0.0>`; a parent is an abstract struct, listed once
- Abstract constructors reject in value functions, graph composition and node hooks.

transpiler: partial

- Uses nominal C++ struct types and abstract bases, generating a template for supported
  generic arguments.
- An abstract field whose name matches its type can be constructed and read through
  a concrete subtype.
- Left: Reports self-containing structs (TYPE-6), unsupported fields or bases, and
  template applications that lose HGL atomic identity or would duplicate nullability.
  Struct parameter packs and value parameters outside the supported constant kinds
  are not emitted.

## TYPE-4

analyser: partial

- A field's type, and so its optionality, agrees between a struct and its parents, and between parents; `name = value` overrides a parent's field; one member to a name; a default of null only on an optional field
- A default is read as a value of its field's type, `name = value` as one of the parent's field (`type.mismatch`), and kept for the stages after, `Module::defaults`; each field a struct has says which default a construction takes
- Left: An atomic composite field's default is not fitted to its payload type
  (#958). A generic field's incompatible specialized default can pass (#728).

## TYPE-5

analyser: partial

- Constructors in value functions, graph composition and node hooks name the
  supplied fields. A graph also rejects a positional constructor argument.
- A known concrete struct type admits a nonempty keyed literal with bare field names. Required fields, defaults and source evaluation order follow the named constructor. Field names alone cannot select a schema.
- Once an inferred graph construction has concrete field types, its implied enum members, empty collections and null literals use those types, including in block tails (GEN-2, CTRL-10, TYPE-7, TYPE-14, EXPR-9).

transpiler: partial

- Constructs concrete structs by name and in argument order, using inherited defaults
  and implicit nulls.
- Writes omitted field defaults in their defining scope, including statement blocks
  and defaults whose type is supplied by a generic struct application.
- Recursive omission is diagnosed in source as `default.cycle` (UNIT-8).
- Left: Reports defaults whose specialization cannot be emitted (#728) and
  constructors assembled directly from live graph connections. Cold evaluation
  of failures is tracked separately (#240).

## TYPE-6

analyser: partial

- No struct inherits from itself; a struct holds itself only through a field that is optional, `Node?`, `atomic<Node?>` or `atomic<Node>?`, or that holds it only as elements of a collection that may be empty; the field's default is `null` or `[]`, and the structs below keep a null one; the field passes each generic parameter on whole; an abstract-typed field holds each descendant; a collection is admitted inside an `atomic<...>` only
- Left: A recursive field can pass a computed argument such as `N + 1`
  where the rule requires the original generic parameter (#959).

transpiler: partial

- Emits abstract parent values and accesses inherited fields through the concrete
  struct's generated implementation.
- Left: Reports recursive struct layouts, including a nullable recursive field
  terminating in a null default (#960).

## TYPE-7

parser: done

- `.member` read as an expression where an operand is due

analyser: done

- Numbering from zero or the number written; numbers are integers no two members share; overflow rejected
- A member is of its enum, and `.member` is given its enum by the type its place expects
- A call of the enum's name constructs a member from an `i64` or a `str`, one argument by position; a constant that is no member's number or name is `member.unknown`
- A construction from a constant that names a member is read as that member: a `switch` label, a `const` argument and a generic value argument take it as they take `Side.buy`
- Core's `convert` candidates select explicit conversions to a member's name or number and from its number (OPS-4, OPS-7).

transpiler: partial

- Emits enum members and constructs members from names or numbers. Supplies enum
  payload, constant and trace forms.
- Supplies the enum's source names and numbers to Core's conversion natives.
- Writes scalar and nullable ports for empty enums. In a lifted conversion
  a number gives an Err in a checked result, raises `convert.invalid` with
  `invalid: .fail`, or gives null in a nullable result (OPS-7, proposed).
- Left: A dynamic enum constructor fails on an unknown number, but its
  runtime error lacks `enum.no_member` (#961).

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
- Left: An atomic struct connection is rejected as the ordinary payload of a
  lifted value helper (FN-12, #962).

transpiler: partial

- Writes structural temporal lists, tuples, structs, sets and dictionaries, including
  atomic struct connections, and maps supported children to endpoints.
- Boolean membership reads typed SDK proxies as Boolean values, including temporal
  set inputs/results and a dictionary's Boolean key set (TIME-1, SET-1).
- Gives concrete struct applications with supported type and value arguments (`i64`,
  `bool`, finite `f64`, enum, and `Type`) their own nominal schemas and maps their
  substituted fields to child endpoints.
- Left: Reports abstract temporal structs, structs with unsupported generic value
  argument kinds (such as `str`, non-finite `f64`, or packs),
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
- Left: A fixed-size connection is rejected at an unsized input (#591). A live
  local list extent can pass source checking (#929). A local extent such as
  `list<i64, N - 3>` can stay unresolved even in a concrete instance (#963).

transpiler: partial

- Writes growing lists (`list<T>` and `list<T, unbounded>`) and positive fixed-size
  lists in value code and as connections. A zero-size list works in value code.
- Left: Zero-size list connections are declined (#829), as are unresolved local
  sizes (#963). Temporal element forms follow TYPE-9.

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
- Left: A size computed by a call can stay unevaluated (#240). A local window
  annotation can escape the bounds check after its generic size is supplied (#963).

transpiler: partial

- Writes tick-count and duration windows with their analysed maximum and minimum, and
  emits their arrivals and supported reads.
- Left: Windows of enums and ordinary structs are declined (#964).
  Window traces with `timezone` or `zoned_time` also lack the scalar connection
  representation (TYPE-2, #957). Endpoint restrictions are under END-9.

## TYPE-12

parser: done

- `endpoint<T>` is a type of one argument; `ref` starts no type, so `ref<f64>`, `list<ref<f64>>` and `ref<ref<f64>>` are syntax errors

analyser: done

- Native value declarations accept an endpoint as the element of a positional or named endpoint pack under FN-8. Endpoint types in ordinary storage and results reject.

- `ref` is no type but a mark of a node's parameter and result, whose type is the T after it (FN-16); a graph gives a `ref` parameter of T a connection of T
- A `signal` parameter marked `ref` is `ref.position` (FN-16)
- `rolling_window`, `atomic` and `signal` kept out of keys, payloads, window values, constant configuration and value functions; `signal` only as a temporal function's input; `endpoint<T>` only as the whole type of a native value function's parameter (`endpoint.position`)
- A borrowed endpoint default is checked as an endpoint binding and rejects an ordinary value (TIME-9, UNIT-8).
- A `signal` takes no default but `null` (`type.mismatch`). A hook gives it to an endpoint operation and reads its payload nowhere (`type.not_ordinary`), including from a constant-key pack entry whose type is supplied by an instance.

transpiler: partial

- Maps a mode-marked node parameter or result to reference wiring while preserving the
  parameter's payload type in ordinary code. Endpoint arguments are borrowed by
  supported natives.
- Maps a temporal parameter or configuration of `signal` or `signal?` to hgraph's
  `SIGNAL`. A supplied connection is observed for publications without reading its
  payload. An omitted optional signal never ticks (UNIT-8, END-2).
- Writes reference designations of ordinary struct fields, including nested
  fields (FN-16, TIME-10).
- Left: Other reference targets, including tuple positions and children of a
  `ref` parameter, follow FN-16. Borrowed endpoint call limits are under TIME-9.

## TYPE-13

analyser: done

- A `result` and an `error` are their own atomic and their own delta; a result's payloads are ordinary value types; `null` is no result, and a nullable result is tag-tested only where proved present

## TYPE-14

parser: done

- `?` after a type. The parser rejects postfix expression `?`.

analyser: partial

- What `?` is applied to is an ordinary value type throughout, but on the whole type of a temporal input or a struct field; a set's element and a dictionary's key take none
- a literal `null` is no value of a type with no `?`, and is rejected where nothing gives it a type
- A conditional combines a null branch with an ordinary result to give `T?`,
  including when the binding leaves its type unwritten (CTRL-10).

transpiler: partial

- Uses optionals for ordinary `T?`, preserves null in admitted collection types and
  compares a nullable value with its T. Writes nullable scalar and enum connections and
  lifts T to a supported `T?` connection. A refined read in a temporal branch
  uses a connection of the payload's type (ERR-12).
- Left: Nullable struct connections are declined (#965). See TYPE-3 for generic
  applications that would nest nullability.

## TYPE-15

parser: done

- The suffix covers the whole preceding type.

analyser: partial

- `list<f64?>` and `list<f64>?` are two types
- A published scalar null takes the null branch. Invalidation and silence in
  `demo.nullable_branches` exercise separate observations (TIME-1, ERR-12).

transpiler: partial

- Keeps null distinct from invalidity and silence in nullable scalar and enum
  connections. A supported bundle's absent optional field reads as null.
- A graph's comparison of a nullable scalar connection with null publishes a
  Boolean on each input publication, including a published null.
- Left: Nullable aggregate connections are declined (#965). Unsupported optional
  field forms follow TYPE-9.

## TYPE-16

analyser: partial

- `delta<T>` is what the table gives for T, where it is written and where a generic parameter is replaced: T itself, the V of an atomic or a window (of a window of a nullable V, `delta<V>`), a tuple of each position's delta under a `?`, and a type of its own for a nullable, a list, a set, a dictionary and a struct
- Proposed: a `Type`, or a tuple, collection, nullable or struct that stores one, has no delta (`delta.unsupported_shape`, TYPE-18). Original delta operands are retained until fields resolve, including applications that atomic, result or window normalization erases. Generic function and struct instances project those operands through the existing bounded GEN-7 traversal and reject forbidden substituted payloads. Ordinary struct layouts omit their auxiliary delta layout.
- A `signal` has none (`delta.unsupported_shape`); a `ref` parameter's `delta_value` is not read (`ref.read`, FN-16)
- Each delta that is a type of its own has its properties: a set's `added` and `removed`, a list's `updated`, `removed` and `size_modified`, a dictionary's `updated` and `removed`, a nullable's `value`, a struct's one for each field; none is assigned (`binding.immutable`)
- It is constructed by them, by name, each a value of the property's type; a name that is none and one given twice are rejected (`delta.argument_name`, `delta.duplicate_argument`)
- A generic graph instance fills inferred types inside a retained `delta<T>` wrapper, derives a property's type from its settled base and fits a constructor's property value (GEN-7, EXPR-5, EXPR-7).
- A delta that is another type the language has, a scalar's, a tuple's or a window's, has no constructor: each property its constructor names, or the constructor where it names none, is `delta.argument_name`
- A `delta<V>` is a value where a `delta<V?>` is expected, and null is not
- A set's, a dictionary's or a list's delta that names one constant entry both as added or updated and as removed is `delta.overlap`, constants read by the one reader (`constant`) as values of the key's type: `-1`, an enum's member however written, `60s` and `1m`, an integer literal as an `f64` key. Two are compared by the language's `==` (`ConstantValue::equals`), as TYPE-16 has it, "and not by their sameness as constants (UNIT-13)": `0.0` and `-0.0` are one key, and two NaNs two
- A constant position of a list's delta that is negative or past a fixed list's bounds is `delta.index_bounds`; a negative `size_modified` is a count to reduce by
- A tuple's delta is written as a tuple of nullable entries. Even an empty
  constructor for that delta rejects with `delta.argument_name`.

transpiler: partial

- Writes scalar, nullable, whole atomic, tuple, list, set, dictionary and struct deltas
  with the supported properties and temporal adapters.
- Left: Trace and publication support depend on TYPE-9. A compound window's delta
  is limited by its declined window representation (#964, END-9).

## TYPE-17

analyser: done

- `tenor` is a scalar type of its own, and a tenor literal is one: its count of months and of days, or of business days, so `1Y` and `12M` are one value
- Its operators are core's (TEN-1)

transpiler: done

- Uses the shared SDK tenor scalar and preserves its two forms and checked counts in
  emitted values.

## TYPE-18

parser: done

- `type(e)` contains one expression; the separate `requires type(N)` form is unchanged.

analyser: partial

- `Type` is an intrinsic ordinary type without generic parameters. A query retains the expression's checked type as a compiled constant, without its operand's calls, input payload reads or local flow effects. A declared but unassigned local supplies its type; a later ordinary read still requires assignment (EXPR-3). Generic instances project the queried type.
- The spelling is reserved only for type declarations and type parameters (`name.reserved` at the declared name). Ordinary value, callable, parameter and member names remain valid. GEN-13 inferred value parameters remain valid; a deferred type instance rejects at its declared parameter name with making-call context.
- A cold operand retains the call-role rules of its source place (FN-3). A graph query of a temporal call retains its payload type and wires no call.
- Generic cold queries use the existing instance source reread with concrete inputs, so a newly selected operator result replaces the provisional type (GEN-7). Operand diagnostics still reject at the making call; no operand calls enter runtime HIR, including recursive calls and node/graph queries.
- An omitted generic parameter default containing a cold query is read again in its defining scope with the concrete generic arguments and bound type packs (UNIT-7, GEN-10). Written types and call arguments use the existing pack projection, and the default worklist distinguishes pack instances. It retains no operand calls and no formal argument in scope; explicit arguments bypass the default.
- Omitted generic value arguments specialize type descriptors inside lists and tuples after ordinary call inference. Forwarded calls retain the concrete descriptor identity and any ordinary constant elements (GEN-2, GEN-8).
- An inherited operator default reconstructs the public operator’s bound pack from its tuple argument, including for a selected candidate with no generic parameters (GEN-6). Its query agrees with a body query, and explicit configuration bypasses the default.
- Type constants compare and form collection keys by canonical type identity (GEN-13), independently of their text. The type of a value annotated `any` is `any`.
- `Type`-valued generic arguments (in calls, written struct applications, signatures, struct fields/parents, and defaults) defer `type(...)` evaluation past `read` so same-module calls, constructors, annotated or inferred locals, declared unassigned locals, and module `Type` constants derived from them resolve with full lexical context while preserving operand diagnostics (GEN-8, GEN-13). Delayed declaration dependencies resolve to a fixed point. In a generic body, the concrete instance determines an overload-dependent query used as an argument.
- Proposed: an explicit delta of Type or a value that stores one rejects with `shape/delta.unsupported_shape` (TYPE-16).
- Left: Type-containing connections lack the proposed source rejection (#907); nested forbidden-delta diagnostics may duplicate or cascade (#908). A cold query of its own declared constant reports a cycle (#910).

transpiler: partial

- Emits ordinary Type values, equality, inequality, set elements and dictionary keys. Core's native conversion returns canonical diagnostic spelling (FMT-5). Queries in constants, parameter and field defaults, generic functions, node handlers and graph composition execute without evaluating the operand.
- Emits `Type`-valued generic struct parameters and applications as C++23 structural `hglrt::type_tag` non-type template arguments carrying canonical type identity, preserving distinct cross-module declaration identities and generic forwarding (GEN-13).
- Ordinary structs holding Type retain their value layout and omit auxiliary delta layouts under the proposed TYPE-16 rule. Storage dependencies are computed once after fields resolve and reused across emitted modules. Indexed dependencies retain their source keys and use concrete generic/pack projection at each application; unselected Type members do not count as stored. The existing GEN-13 instance/type-size bounds produce a checked limit result and a source diagnostic or clean target decline.
- Enum generic arguments use their declared enum name in text; qualified identity remains separate, including inside compound constant arguments.
- Left: Type-containing connections (#907), primitive type-call operands left untyped
  by their checker stub (#874), and unresolved anonymous-function query operands (#906)
  decline before native compilation. A query block with `return 5` can already
  reject at source against its enclosing `Type` result (#906). Generic-type diagnostics
  still preserve written duration and float arguments rather than canonical Type text (#909).
