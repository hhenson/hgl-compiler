# Functions and phases

`functions-and-phases.md`

## FN-1

analyser: partial

- A value function's parameters and result, and a `const` parameter, are ordinary value types; each signature recorded; a value function holds no `state`, `cache`, `inject`, `start`, `when` or `stop`
- A value function cannot `yield` (FN-2, SOURCE-1), with `yield.outside_source`.
- A parameter of a capability's type, and `endpoint<T>` in a native value function, are admitted (TIME-8, TYPE-12)
- A `const fn` parameter marked `ref` is `ref.position` (FN-16)
- A generic parameter of a value function that a call's place, or what the call writes, binds to a type that holds a window is `type.not_ordinary`; one given a window binds to the `list<V>` it holds (GEN-1, TYPE-11)
- In a hook, a source and a value function a `const` parameter is given a constant expression (`argument.constant`); in a graph, no connection (`connection.as_constant`)
- Left: a `const` parameter that only an operator marks; roles of `const fn` and a temporal `fn` in bodies

## FN-2

analyser: done

- Each temporal function and temporal candidate is a graph or a node by the constructs its body holds, wherever it holds them; an `inject` of `global_state` alone makes no node
- A node's body is its declarations and then its hooks: any other statement is rejected, but in a source, which has its own shape (SOURCE-1)

## FN-3

analyser: partial

- A value function calls only value functions (with FN-4), and an operator through a value candidate (GEN-5)
- So does value code, a test's outside an `eval` or a `run` and an application's statements, where a temporal function called is `call.role`, the code proposed (TST-3, UNIT-14)
- A node's hook is read as value code: a temporal parameter that is not `ref` is a value of its declared type, a `ref` one an endpoint (FN-16), and a temporal function called there is rejected (`call.role`)
- A graph's body calls any function: a temporal parameter and the result of a temporal or lifted call are connections, and a connection given to a `const` parameter is rejected (`connection.as_constant`)
- What each expression is, a connection or an ordinary value, is recorded on it for the stages after the analyser, and on an instance's body with its candidates in place: a symbol, `&&`, `||` and `!` among them, and a construction, a result form, an element or a field of a connection is one
- Left: what a composition body admits of a connection; what it calls; iteration and conditionals in composition

transpiler: partial

- Emits calls in their analysed phase and wires supported ordinary values as constant
  connections. A supported fixed-size list publishes its positions together at graph start
  (TIME-7), whether written as a literal or made by a loop.
- Left: A read of a connection outside an admitted phase, an ordinary call of a temporal
  function, unsupported connection expressions and constant connections of sets or
  dictionaries are reported.

## FN-4

analyser: partial

- A temporal and a value definition may share a name
- A call in a graph takes the temporal definition of a pair when given a connection, and the value definition when all arguments are ordinary (FN-6, GEN-5)
- A temporal function name, or an anonymous function that captures a connection or calls a temporal function, selects the temporal definition (FN-17, GEN-6). A callable both roles admit directly leaves the choice to the other arguments. The selected definition checks its template's input types.
- The selected definition checks the call's signature
- A call in value code takes the value definition. An eval's target takes the temporal definition where it exists.
- Left: named-pair selection where an operand's category settles only after instance selection or after it receives a contextual type

## FN-5

parser: done

- `const` is a modifier only where one is admitted

analyser: done

- `const(argument)` resolves as an ordinary call, by the usual naming and role rules

## FN-6

analyser: partial

- In a graph a value function or a value candidate called with a connection among its arguments gives a connection, and one called with ordinary values alone an ordinary value
- Each call records whether it is lifted: in a graph's body, of a value function or a value candidate that marks no parameter `var` (FN-15), given a connection; its arguments that are connections are the node's inputs, and the rest its configuration
- Left: an outputless helper as a sink

transpiler: partial

- Wires a helper's temporal inputs and binds ordinary arguments and omitted defaults as
  configuration. Includes the value-function eval path.
- Left: Reports an unavailable lifted or packed target, a fallible default of an omitted
  nullable parameter, and defaults that run statements or propagate a failure. Constant
  inputs require a supported temporal schema, excluding sets and dictionaries.
  Contextual connection reads have no ordinary value-code path.

## FN-7

analyser: partial

- A value function's `return`, arrow body and final expression are of its result type; a `return` with no value is rejected where a result is declared, and one with a value where none is; a function that declares a result returns on every path (`return.missing`)
- In a hook a `return` with no value is accepted where a result is declared, and one with a value is of the result's type
- No value of a type is a function: a call of one, `plus_one(x)()`, `[x][0]()` or `(1 + 2)()`, is `type.mismatch`
- The analyser checks an anonymous function's body with its written parameter and result types. A direct call supplies unwritten parameter types from its arguments and the unwritten result type from the call's context.
- Composition returns remain pending. Accepting an anonymous function as an argument is tracked in [#362](https://github.com/hhenson/hgl-compiler/issues/362).

transpiler: partial

- Graph returns choose a connection, hook returns publish and exit the evaluation, and
  ordinary functions return values.
- Left: Restrictions on returns in temporal branches and value expressions are under
  CTRL-13, CTRL-14 and CTRL-10. A valued return in a source, a valued return where the
  graph or hook cannot return one, and a return in a test have defensive reports.

## FN-8

parser: done

- Reads positional and named packs and the `for` and `without` parts of a pack declared for a function.

analyser: partial

- Packs follow fixed parameters, have no defaults and are not const; a named pack names a type pack of its declaration
- Function and operator calls bind leftover positional and named arguments to their corresponding packs and check the known element type. The HIR keeps each argument's pack slot and position or source name.
- A second positional pack or a second named pack in one declaration reports `pack.duplicate` at its parameter name. A declaration may have one of each kind.
- A dependent call matches the supplied arguments to the selected callback, excluding the parameters named by `without`. A defaulted parameter is reflected and contributes no argument unless supplied.

transpiler: partial

- C++ writes a concrete parameter for each captured argument in value functions, nodes, graphs, and lifted calls.
- FN-19 describes how the body reads and forwards these entries.

## FN-9

analyser: partial

- An `inject` in a `const fn`, a value candidate, the contract of a native value function, or an anonymous function written in a node is rejected (`node.in_value_function`); a value function is given a capability by a parameter of its type (TIME-8)
- An application injects `global_state`, for the statements before its run statement (UNIT-14), and `run_config` (RUN-6), and no other
- A graph injects it alone too, and a call on it there is given no connection (`connection.as_constant`). An instance checks arguments whose categories were unsettled at the declaration (GEN-7).
- `scheduled()` in a value function or a graph, which has no schedule of its own (TIME-9), is `scheduled.outside_node`; so is one in an anonymous function written there, and `scheduled<T>()`
- Left: an anonymous function written in a graph; a capability used outside a hook or a source's body

transpiler: partial

- Executes the root graph's native store calls at construction in source order. Nodes
  and sources bind their admitted capabilities in the corresponding hooks.
- Left: A branch or loop child cannot see the surrounding graph's store or captured
  capabilities. Other graph injections have a defensive report.

## FN-10

parser: done

- Where an arrow body ends

## FN-11

analyser: planned

- Left: which lifted arguments need a valid input. The analyser records a nullable connection read as T at the reader (ERR-12).

transpiler: partial

- Uses default or null when a lifted input is invalid and gates required inputs. Wires
  an omitted optional input with no publication.
- Left: Unsupported constant schemas and fallible defaults follow FN-6 and UNIT-8.

## FN-12

analyser: partial

- A window's connection is given to a value function's parameter in composition as the `list<V>` it holds, which binds its generic parameters: the `T` of `list<T>` is V, and a bare `T` is `list<V>` (TYPE-11, FN-1); a `list<V>?` takes it too, "whatever its default or null contract" (FN-11)
- Left: which aggregates need `all_valid` (FN-11)

transpiler: partial

- Lifts whole list, dictionary, struct, tuple and window reads using the input's
  eligibility for a whole read.
- Left: Nested aggregate shapes follow TYPE-9. Fallible default expressions follow FN-6.

## FN-13

transpiler: done

- Lowering retains copies that must survive mutation, and passes stable ordinary
  arguments or local read-only data for the duration of the call.

## FN-14

analyser: done

- What an anonymous function may use from around it
- An enclosing `var` parameter follows the same capture rule as a `var` local (`name.captured`). An anonymous function may read a `let` holding the parameter's value.
- A call's callee is read as any expression is, so an anonymous function chosen by an `if` or taken from a list reads the node's inputs where it is written (TIME-1)
- An endpoint operation in one, `scheduled()` among them, is judged where the function is written, in a hook, an initialiser, a source, a value function, a graph or a test (TIME-9); the guards of TIME-4 and the phases of TIME-5 hold in it as around it

## FN-15

parser: done

- `var` before a parameter's name, a name and a type; no pack and no default is read after one

analyser: partial

- A `var` parameter is a `var` of its function's body
- A call gives it a place: a `var`, a `state` or a `cache`, or a field or an element of one; a `let`, any other parameter, an iteration binding and a constant are `binding.immutable`, and what is no place `argument.place`
- No argument of such a call is a connection (`connection.as_constant`), and the function is no target of an eval (`eval.target`)
- A value candidate of an operator may declare one too, and the candidate a call resolves to says which of its arguments are places (GEN-6); one of a temporal function, a temporal candidate or an operator, or of a capability's type or `endpoint<T>`, is `parameter.var`
- It is not left out though its type is nullable (`argument.missing`)
- A place is of the parameter's type exactly: one of a type the parameter's only admits, an `i64` place for an `i64?` parameter or a struct's for its abstract parent's, is `type.mismatch`
- The call is held to what an assignment of the place is: a property of a native value or of a delta is `binding.immutable` (NATIVE-6, TYPE-16), and in a graph a `var` of an ordinary value in a branch selected at runtime is `binding.category` (CTRL-14) and one declared outside a loop that still makes a child for each member local `loop.carried` (CTRL-15, #373)
- A native implementation part's `var` marks match its declaration's (NATIVE-2)

transpiler: partial

- Passes `var` places by mutable reference and holds ordinary arguments independently of
  changes a call makes. Holds indexes before resolving a place at the call.
- Left: Calls that assign in an expression, alias two `var` arguments to one place, or
  take an unsupported place are reported. This includes a `list<bool>` bit, a dictionary
  entry and an abstract field.

## FN-16

parser: done

- `ref` before a parameter's name and before a declared function's result type, a token of the parameter or of the result; no `const`, `var` or `...` after it, and none on an anonymous function's parameter or result

analyser: partial

- The mark is recorded on the parameter and on the signature, and is no part of the type (`Parameter::reference`, `Signature::reference`): a caller, a graph among them, gives a `ref` parameter a connection of its type, and a `ref` result is a connection of its type
- A mark on a value function, a value candidate, a graph, an operator, or a native implementation part that is a graph (`{}`) is `ref.position`; a node, a temporal candidate that is one, and a native temporal declaration with no body take it
- A mark on a `signal` parameter, `ref tick: signal` or one that may be left out, of any of them is `ref.position` too (with TYPE-12: "a signal has no payload to designate")
- A mark on a window parameter or a window result, one that may be left out among them, is `ref.position` too: "Nor does `ref` stand on a window"; a window inside what is marked, a struct's field, is not
- An assignment to a part of `out` of a `ref` result in a `when`, `out.bid = x`, `out[0] = x`, at any depth and compound, is `binding.immutable`: "A `ref` result designates one endpoint whole"; the place is named and not read, so no `ref.read` is reported beside it
- In a hook a `ref` parameter, a child of one and `out` of a `ref` result are endpoints: the endpoint operations take them, and a read of the value or the `delta_value` of any of them is `ref.read`, and so is the `removed_value` of a window that is a child of one; an atomic projection, an iteration, a comparison and a call that reads one among them
- A `return` to a `ref` result, and `out =` of one, names a temporal input of the node, `ref` or not, or a child of one: it is read as the designation it is (`EndpointOf`, `ChildEndpointOf`), of the result's type (EXPR-5), reads no payload and needs no admission; a value there is `type.mismatch`, and so is a source's `yield` to a `ref` result: a source has no temporal input to name (TIME-7: "A `ref` result takes no value")
- A native implementation part's parameter and result `ref` marks match its declaration's (NATIVE-2)

transpiler: partial

- Writes node `ref` inputs and results. A return can sample a ref-parameter designation,
  designate a whole ordinary input, a fixed-list position or a dictionary entry.
  Nodes with a scheduler or an alarm can make these designations.
- Left: Reports designations of other children and list forms, simultaneous whole and
  positional designation of one input, and unsupported assignments to a `ref` result.
  Graph and source `ref` forms have defensive reports.

## FN-17

parser: done

- Function types in parameter positions take fixed inputs and packs, a result type and the optional `const fn` phase. Explicit generic function references are read as arguments.

analyser: partial

- Check the fixed template's phase, input names and input and result types. A value target may lift in a temporal template. An anonymous body's phase comes from the accepting template (FN-7).
- When a graph instance completes a call's parameter types, an anonymous callback takes the completed template in its lexical scope. Its synthesized capture inputs are separate from the written arguments (GEN-2).
- Match a declared function or an operator's selected candidate, infer generic types in either direction, and accept explicit generic arguments on function and operator references. An operator reference uses the public signature, with the written arguments and the template together resolving its generic parameters. Static parameters may be called or passed onward. They hold no runtime value.
- A named operator call reads a fixed callback against each candidate's function template. The other arguments can resolve its input types, its template sets an anonymous callback's phase, and the supplied function can resolve its result type (GEN-2, GEN-6).
- An anonymous callback is read against the result type its template already knows, including one a later argument resolved (GEN-2).
- Applies the proposed plain-parameter restriction to the fixed inputs. Keeps the remaining callback slots in declared order with their modes, defaults and pack shape.
- A reflected operator callback selects from the resolved parameter types, including the dependent arguments. With no resolved parameter type, it reports `generic.unresolved` even for a lone candidate (FN-17).
- A reflected anonymous parameter beyond the fixed template needs a written type. Arguments of a homogeneous callback pack can determine its generic element type.
- A call through a function parameter gives arguments by position or by a name written in its template (FN-17).

transpiler: partial

- A static target specializes the called body. The C++ target emits ordinary value helpers and composed graph calls; anonymous functions carry their permitted lexical captures as inputs.
- An anonymous target's identity also carries the normalized const inputs of its enclosing instance when those inputs determined its code (FN-1, FMT-3).
- The C++ target bounds the size and structural depth of the bodies it copies when specializing static callable arguments, including each use of a named or anonymous target and the body of an anonymous target. It also bounds deeper requests of one declaration along a call chain. Exceeding a limit reports the request as unsupported.
- The C++ target specializes reflected templates by the static callback and the dependent argument shape. It does not yet write `valid` on a temporal input captured by a value callback (#397).

## FN-18

parser: done

- Reads configuration parameters in braces before the parentheses of functions, operators, candidates and native declarations.

analyser: partial

- Binds configuration only by name in the braces. Checks defaults, missing and misplaced arguments, naming conflicts, and forbidden parameter modes.
- Holds omitted operator configuration defaults to the selected candidate's parameter types in the operator's defining scope (GEN-6, UNIT-7, UNIT-8).
- Fits supplied configuration of a required operator call to the selected candidate's parameters when instantiating the caller (GEN-16, EXPR-5).
- Configuration and named generic options go in braces when a dependent pack takes the callback's arguments in parentheses (FN-8).
- A callback can have defaulted configuration. Its function type and reflected pack omit those slots.

transpiler: partial

- Passes scalar configuration to value functions, hooks and value candidates. Temporal declarations can receive connected configuration where their own parameter rules admit it (EXPR-14).
- Calls through a static function parameter use the callback declaration's configuration defaults (FN-17).

## FN-19

parser: partial

- Reads a pack spread among ordinary call arguments.
- Reads a pack comprehension as a primary expression.

analyser: partial

- Reads a parameter pack entry by a constant name or position. The instance supplies the entry type. A missing key reports `pack.name` using its written spelling, with an instantiated generic parameter shown as the value supplied (GEN-17, UNIT-13).
- In a hook, the entry follows ordinary input admission and lifecycle rules (TIME-1, TIME-4, TIME-5) and reads the whole payload of an atomic or the list held by a window (TYPE-9, TYPE-11).
- A checked whole read from a window, an unsized list or a dictionary takes the result or nullable form of its place (ERR-14, ERR-15).
- A child of a structural pack entry uses the ordinary child admission check. A guard proves a child of that entry only (TIME-1, TIME-4).
- Keeps an entry read pending when its constant key needs an unrolled range pass (CTRL-16) or a module constant the compiler has not evaluated (UNIT-13). C++ declines the pending read.
- Resolves spreads of parameter packs when the enclosing callable is instantiated. Written arguments bind first, then named entries, then positional entries fill the remaining parameter slots in order.
- A `const` parameter written among a forwarding call's arguments is followed into the instance with the call's entries, and is an input of the instance only where its body reads it at compile time (FN-1, FMT-3). A forwarding body that comes back on its own path with other entries takes the parameter as read there (GEN-13). What a function parameter is given goes to a plain parameter of its template and is no compile-time read (FN-17). A conditional input past the 64 KiB text budget ends the analyser's walk; lowering treats it as before.
- Checks function, operator and static callback arguments with the ordinary call diagnostics (EXPR-2) and uses the names written by a callback template (FN-17).
- A forwarded name that binds a generic value parameter or a selecting operator option reports `generic.argument_constant` at the runtime entry (GEN-6, GEN-8). Generic value names take priority over a callee's ordinary named pack.
- Follows concrete calls in parameter and field defaults and anonymous bodies. Struct fields bind by name, and positional entries reject (TYPE-5). An enum constructor with a spread reports the argument during analysis (TYPE-7).
- Checks where a pack comprehension is held and requires its iterable to name a type pack's keys. At the declaration it checks condition chains for runtime bindings, assignments to the iteration key, and ordinary reads of a type pack or callback. In an instance an inline computed spread visits the concrete keys, reads only each selected branch, and omits keys whose chain selects no entry (GEN-7, CTRL-16).
- Reads a declared function's written arguments, including its configuration, through their parameter types before the computed entries are known.
- Keeps the compile-time key comparison in anonymous bodies that capture a concrete key. A key differs from a constant of the other kind (FN-14, GEN-17).
- A computed entry has the type of its own pass (FN-19). The receiving parameter does not give a type to a bare implied enum member, `null`, or an empty sequence, and it does not widen an `i64` literal entry to `f64`.
- A `let`-bound pack keeps each selected entry under its key. An entry can be indexed or spread; a pack name in an ordinary value expression rejects. The homogeneous positional parameter pack in `examples.pack_forms::total` can be iterated in a node hook.

transpiler: partial

- Lowers forwarded entries and constant-key reads from the same fitted pack input slots, including captured values and connections. The SDK cases also exercise operator selection, static callbacks and constructors.
- Lowers the concrete arguments of an inline computed spread. An anonymous body is specialized for the enclosing pack keys even when each pass supplies a value of the same type. The SDK cases also cover mixed entry types, named and positional keys, selected nested conditions, and written pack-member types.
- Stores each `let`-bound entry once where its binding runs. Subsequent reads and spreads use that entry's value or connection. SDK cases cover all four forwarding examples, constructor fields, and reuse of the entries.

Left: C++ declines a spread whose pass condition does not supply a known constant `bool`. The packed signal payload diagnostic is tracked in #731, and guards on the pack entry itself in #732.
