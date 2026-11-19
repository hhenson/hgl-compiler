# Generics and operators

`generics-and-operators.md`

## GEN-1

analyser: partial

- Generic scope covers the whole declaration; a repeated type parameter is one type at a call of a value function
- Left: equal constants; occurrence requirements; temporal bodies

## GEN-2

parser: done

- A default on a generic value parameter

analyser: partial

- A generic type in a type position or constructor is given all its arguments; a type pack is no single type; a value function's generics resolved at a call from what the call writes, then the type its place expects, then its arguments in declared parameter order; an abstract struct a later argument has widens what an earlier argument decided
- A generic value parameter takes its default where nothing else says what it is: a function's and a candidate's at a call, a struct's where it is named bare or constructed, an operator's in the output requested
- Left: packs; solvable equalities; temporal bodies

transpiler: partial

- hgraph: a temporal function with generic parameters, and a temporal candidate with them, is written once for each set of generic arguments the program calls it with, in the declaration's module, and a call names the instance for its arguments; one the program calls with none is not written
- So is a value function or candidate whose body calls an operation it requires or hands a requirement on (GEN-16), reads a generic value as a value (GEN-13), or calls one written by instance with its own generic parameters; any other value function with generic parameters is written once
- A generic value read as a value is the constant the call gives (GEN-13)
- Each instance is a piece of its own: one with a form that is not written is left out, and a test that reaches it is not run
- Left: a call whose generic arguments are not known, an enum member given to a function's generic value among them; an application's run statement is not read for the instances it asks for

## GEN-3

analyser: partial

- A parameter compared in a constraint is a const option or is rejected
- At a named operator call in a value function, a candidate's `T in {types}` and `T == type` are checked against what `T` stands for, and a named operator requirement is tested with the call's types (GEN-16)
- A call of a function, a construction of a struct, and a type written that applies one, whose `requires` does not hold with its generic arguments, is `requires.unmet`: a membership or an equality of a type, a generic value parameter compared with constants, however its value is written (GEN-8), after the callee's name or by name, or left to its default, and a named operator requirement
- Left: every other form `requires` admits; binding equalities solved to a fixed point; a membership of a generic parameter of the declaration a call is written in, which is not told (GEN-7); a function with a type pack or a pack parameter; a generic value that is an expression of constants other than integer or duration arithmetic, a `!` or a comparison, which is not folded

## GEN-4

analyser: partial

- An `impl fn` is a temporal candidate and an `impl const fn` a value candidate; each binds an operator its module declares, its file imports, or the prelude binds, and one that names no operator is `impl.unknown_operator`; a value candidate's body is read as a value function's
- An operator's requirement `O in {...}` on its bare result parameter limits its candidates' results
- Left: operator identity in selection; every other requirement of an operator

transpiler: partial

- hgraph: a candidate with a body is written in its module, named by the operator and the candidate's parameter types, in a program and in core: a value candidate as a function, and a temporal candidate as a node or a graph, which a call in a graph that selects it calls
- The value candidate of a temporal and a value pair of the same parameter types is named apart
- One with generic parameters is written by instance (GEN-2)
- Left: two candidates of one module and one role with the same parameter types; a native candidate a program declares

## GEN-5

analyser: partial

- In a value function, a call of an operator, by name or by the symbol that stands for it, selects among its value candidates: the requested output first, then the inputs; an integer literal selects as an `i64` before it is widened, which the candidates' types settle before their conditions, and candidates that tie in which literals they widen give way to the one that widens every one; no candidate and a tie rejected
- A generic candidate is admitted with its parameters resolved from the call, and the most specific admitted one is selected (GEN-12)
- In a graph a call selects among temporal and value candidates, and a temporal candidate outranks a value one of the same parameter types
- Left: a candidate with a pack; an operator with a pack, and so the Boolean connectives and `!`; an argument that is itself a call only its requested output selects; composition, where a temporal candidate outranks a value one of the same parameter types, and runtime bodies

## GEN-6

analyser: partial

- In a value function a named call is matched against each value candidate's own signature
- An option that selects is a generic value parameter, given by name or by its default, and the candidate's `requires` is checked against its value; one that does not select is a parameter, by name or position
- An option's name is one kind across an operator's candidates
- A value candidate may mark a parameter `var`, which takes no part in selecting it (FN-15)
- Left: a `requires` on a generic value parameter of the enclosing function; a candidate's shape against its operator, the names and types of the operator's parameters and its generic relationships; candidates that stay resolvable together; temporal candidates

## GEN-7

analyser: partial

- In a body with generic parameters, a call of an operator, by name or by symbol, with an argument whose type names one of them is admitted where a requirement of the declaration names that operator with those types, each given by its position or by the operator's parameter name, and is of the type its arrow states; with none it selects among the candidates as any call does, so one that takes any type is still taken
- The call records the position of the requirement it is (`Callee::Required`), so that a call of the declaration says which candidate it calls
- Left: a requirement met by widening an argument; a type or size parameter used as a runtime value

## GEN-8

parser: done

- `_` as an `instantiate` argument

analyser: partial

- A call's generic arguments, all or the first of them, for a value function and for the output requested of an operator's candidates; a call that leaves a parameter unresolved rejected; a function's or a candidate's generic value parameter given by name, held to the parameter's type, and one given no compile-time constant rejected
- An enum member a call gives one, or a default names, is recorded as the member it comes to, however it is written (TYPE-7)
- An operator's generics are resolved from what the call writes, then its place, then its arguments, then a default, and the output so requested is matched as far as it is known
- Left: an operator's own generic value parameter given by name; `instantiate`; packs; what an operator's generic arguments say of its operands; temporal bodies

## GEN-9

analyser: partial

- A property domain is an ordinary value type
- Left: clauses applied to explicit concrete domains

optimiser: planned

- A recorded law licenses no reassociation across overflow or float effects

## GEN-10

parser: done

- A cardinality after a pack parameter or a generic pack parameter

analyser: partial

- A cardinality bounds the arguments a generic type's pack takes, and has its upper bound at least its lower
- Left: pack views; `each`; the arity of a call

transpiler: planned

- `schemas(pack)` as a runtime-only borrowed view

## GEN-11

analyser: partial

- In a graph, a connection given to a function's parameter: a pattern with no `ref` in it binds its generic parameters from the connection's type with each reference removed, at any depth, and a `ref<T>` pattern binds T to the type under the reference, or to the connection's type where it is none
- A stated resolution and a requested output keep their references
- Left: an operator's call; a requested output's nested variables; a structural projection that keeps a declared `ref`

## GEN-12

analyser: partial

- Among the candidates a named call admits in a value function, each of the operator's parameters is compared with the other candidate's part for part: a concrete type or a structure over a bare variable in the same place; a candidate as specific everywhere and more in one place outranks
- A candidate's generics are bound in declared parameter order
- Left: a repeated variable counting once; reference qualification; temporal bodies

## GEN-13

analyser: partial

- A plain parameter is a type or a value by its uses in the signature; uses that disagree are rejected, and a struct's parameter nothing uses; a value parameter or a pack is not named as a type; a generic value parameter's default, a callable's or a struct's, is of its declared type, as far as how the default is written says what it is, and of the type a call resolves where that names a type parameter
- Left: a default that names something; the kind a constraint gives; `type(N)`; a call that supplies no type for a function's parameter its signature does not settle

## GEN-14

parser: done

- `<` after a name read as generic arguments or comparison; marked undecided when both parse

analyser: partial

- Decides an undecided constructor from what its name resolves to, a generic type, function or operator; a generic type before a comparison's `<` rejected; each argument against its parameter's kind
- Left: a generic function or operator before a comparison's `<`

## GEN-15

parser: done

- Constraint precedence: negation, then conjunction, then disjunction

## GEN-16

parser: done

- One form for constraint calls, with an optional result arrow

analyser: partial

- A constraint call that names an operator is a requirement, and names a value operation
- A candidate that marks a parameter `var` answers none
- Where a call selects a candidate it is tested with the candidate's generic parameters in place, as the call it names would select: one most-specific value candidate admits the types supplied and gives the arrow's type where one is stated, the operator's defaults taking part (GEN-8)
- A call of a declaration that states requirements records what answers each (`hir::Answer`): the value candidate selected with the call's types, with its own requirements answered; or, in a generic body, the requirement of that body's declaration that states the same
- `Program::instance` gives a generic declaration's signature and body for one call: its generic arguments in place, and each operation it requires as the call of the candidate that answers there
- A function's requirement that does not hold at a call of it, and a struct's where it is constructed, is `requires.unmet` (GEN-3)
- Left: a predicate, and the arrow rejected on one; a requirement supplied with a value or a set; a requirement checked where it is declared

transpiler: partial

- hgraph: in an instance of a generic function or candidate, temporal or value, an operation the declaration requires is the call of the value candidate that answers the requirement for that instance's types: called in a value function and in a node's hook, and lifted in a graph
- Left: an operation for which the analyser could not tell what answers, which is reported where it is called
