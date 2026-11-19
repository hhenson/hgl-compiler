# Functions and phases

`functions-and-phases.md`

## FN-1

analyser: partial

- A value function's parameters and result, and a `const` parameter, are ordinary value types; each signature recorded; a value function holds no `state`, `cache`, `inject`, `start`, `when` or `stop`
- A parameter of a capability's type, and `endpoint<T>` in a native value function, are admitted (TIME-8, TYPE-12)
- A `const fn` parameter marked `ref` is `ref.position` (FN-16)
- In a hook, a source and a value function a `const` parameter is given a constant expression (`argument.constant`); in a graph, no connection (`connection.as_constant`)
- Left: a `const` parameter that only an operator marks; roles of `const fn` and a temporal `fn` in bodies

## FN-2

analyser: done

- Each temporal function and temporal candidate is a graph or a node by the constructs its body holds, wherever it holds them; an `inject` of `global_state` alone makes no node
- A node's body is its declarations and then its hooks: any other statement is rejected, but in a source, which has its own shape (SOURCE-1)

## FN-3

analyser: partial

- A value function calls only value functions (with FN-4), and an operator through a value candidate (GEN-5)
- A node's hook is read as value code: a temporal parameter that is not `ref` is a value of its declared type, a `ref` one an endpoint (FN-16), and a temporal function called there is rejected (`call.role`)
- A graph's body calls any function: a temporal parameter and the result of a temporal or lifted call are connections, and a connection given to a `const` parameter is rejected (`connection.as_constant`)
- Left: what a composition body admits of a connection; what it calls; iteration and conditionals in composition

## FN-4

analyser: partial

- A temporal and a value definition may share a name
- a call in a value function takes the value definition, and a temporal one alone is rejected
- A call in a graph takes the temporal definition where there is one
- Left: selection in eval

## FN-5

parser: done

- `const` is a modifier only where one is admitted

analyser: done

- `const(argument)` resolves as an ordinary call, by the usual naming and role rules

## FN-6

analyser: partial

- In a graph a value function or a value candidate called with a connection among its arguments gives a connection, and one called with ordinary values alone an ordinary value
- Left: which arguments are inputs and which configuration, recorded for the lifted node; an outputless helper as a sink

transpiler: partial

- hgraph: a value function or a candidate of core given a connection in a graph is a node of its own, an input for each connection and a scalar parameter for each ordinary argument; any input wakes it, and each evaluation publishes its result
- A call given no connection is made where the graph is built
- An outputless helper is a sink
- A value function an eval names is lifted with each replay’s payload type, including nullable and set inputs
- Left: a pack, and types beyond the four scalars

## FN-7

analyser: partial

- A value function's `return`, arrow body and final expression are of its result type; a `return` with no value is rejected where a result is declared, and one with a value where none is; a function that declares a result returns on every path (`return.missing`)
- In a hook a `return` with no value is accepted where a result is declared, and one with a value is of the result's type
- Left: a composition return; an anonymous function's context

transpiler: partial

- hgraph: a `return` with a value publishes it and ends the evaluation, skipping later handlers; a body that ends with none publishes nothing
- Left: a `return` after a write through `out`

## FN-8

parser: done

- Pack parameter forms

analyser: done

- Packs follow fixed parameters, have no defaults and are not const; a named pack names a type pack of its declaration

## FN-9

analyser: partial

- An `inject` in a `const fn`, a value candidate, the contract of a native value function, or an anonymous function written in a node is rejected (`node.in_value_function`); a value function is given a capability by a parameter of its type (TIME-8)
- An application injects `global_state` alone, for the statements before its run statement (UNIT-14)
- A graph injects it alone too, and a call on it there is given no connection (`connection.as_constant`)
- Left: an anonymous function written in a graph; a capability used outside a hook or a source's body

## FN-10

parser: done

- Where an arrow body ends

## FN-11

analyser: planned

- Which lifted arguments need a valid input

transpiler: partial

- hgraph: an input whose parameter has a default is not required valid, and the default is bound while it is not; every other input is required valid
- Left: null bound for an input that admits null, which waits on nullable scalars

## FN-12

transpiler: partial

- hgraph: a lifted helper is one invocation with the whole value; a fixed list given to one is required `valid`, as any list is
- Left: what the helper is given for a position not yet valid

## FN-13

transpiler: done

- hgraph: an argument is a reference the function cannot change, and nothing else runs while it runs; one read from what the same call gives a `var` parameter is a copy (the owner's rule, issue 151), in a graph as in a value function
- What a function returns of an argument, or of what may be one, what a place keeps, what is published and what a graph gives a node are copies, so a retained result is independent of later changes, of a list and of a struct
- No argument is a view: hgraph gives a list made of its tuple, a struct made of its bundle, or a `TS`'s value, which nothing changes where it is

## FN-14

analyser: done

- What an anonymous function may use from around it

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
- The call is held to what an assignment of the place is: a property of a native value or of a delta is `binding.immutable` (NATIVE-6, TYPE-16), and in a graph a `var` of an ordinary value in a branch selected at runtime is `binding.category` (CTRL-14) and one declared outside a loop that makes a child for each member `loop.carried` (CTRL-15)
- Left: a native implementation part's `var` marks matched with its declaration's (NATIVE-2)

transpiler: partial

- hgraph: a function, and a candidate of an operator, with `var` parameters returns its result and then what each holds, at every way out; a call that is the whole of a statement, or of what a `let`, an assignment or a `return` takes, is bound to `_given` and assigns each place after it, in the order of the parameters, a place inside a value as an assignment does (EXPR-3)
- An index or a key of a place is evaluated once, before the call, and held, so that the place read is the place assigned; every argument of such a call is then held too, each evaluated in the order written (EXPR-2), and what an index is of is read before the index is evaluated; the assignment reads it again, so two places inside one root are both assigned
- A `var` parameter is the caller's place: its list, its dictionary or its struct of more than two scalar fields is changed where it is
- A function that fails gives back what it raised, `_core.Failed`, beside what each `var` parameter holds then, and the call assigns each place before it raises it again: no change is undone, of a parameter of any type, and of one given a value of its own
- An argument that fails is evaluated before the call, which assigns nothing
- Left: such a call inside an expression, which is reported

## FN-16

parser: done

- `ref` before a parameter's name and before a declared function's result type, a token of the parameter or of the result; no `const`, `var` or `...` after it, and none on an anonymous function's parameter or result

analyser: partial

- The mark is recorded on the parameter and on the signature, and is no part of the type (`Parameter::reference`, `Signature::reference`): a caller, a graph among them, gives a `ref` parameter a connection of its type, and a `ref` result is a connection of its type
- A mark on a value function, a value candidate, a graph, an operator, or a native implementation part that is a graph (`{}`) is `ref.position`, a code whose row is proposed; a node, a temporal candidate that is one, and a native temporal declaration with no body take it
- In a hook a `ref` parameter, a child of one and `out` of a `ref` result are endpoints: the endpoint operations take them, and a read of the value or the `delta_value` of any of them is `ref.read`, a code whose row is proposed; an atomic projection, an iteration, a comparison and a call that reads one among them
- A `return` to a `ref` result, and `out =` of one (proposed), names a temporal input of the node, `ref` or not, or a child of one: it is read as the designation it is (`EndpointOf`, `ChildEndpointOf`), of the result's type (EXPR-5), reads no payload and needs no admission; a value there is `type.mismatch`
- Left: a native implementation part's marks matched with its declaration's (NATIVE-2)

transpiler: partial

- hgraph: a `ref` parameter and a `ref` result are hgraph's `REF` of the type, bound to and by connections of it as they are; `valid`, `modified`, `last_modified` and activity are the `REF`'s own
- `return low` of a `ref` parameter is the reference it holds; `return value` of an input that is no `ref` parameter, and `return items[idx]` of a fixed list's position, read a hidden `REF` input the node is given beside it through a graph of the function's name; `out =` designates alike
- A reference the result holds already is not published again (TIME-10, END-10)
- Left, reported: a child of a `ref` parameter or result as an endpoint, and `all_valid` of one; a designation of a dictionary's entry, a struct's field, a tuple's position, an unsized list's position or a child deeper than one step; an input designated whole and by a position in one node; a `ref` result of a source
