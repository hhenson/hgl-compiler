# Generics and operators

`generics-and-operators.md`

## GEN-1

analyser: partial

- Generic scope covers the whole declaration; a repeated type parameter is one type at a call of a value function
- A repeated generic value parameter is one constant (UNIT-13) at a call of a value function and of a temporal one: `Scaled<1>` and `Scaled<1.00>` given to two `Scaled<k>` are one, and so are two NaNs, and `Scaled<0.0>` beside `Scaled<-0.0>`, or `list<i64, 2>` beside `list<i64, 3>`, is `type.mismatch`
- Left: occurrence requirements; temporal bodies

## GEN-2

parser: done

- A default on a generic value parameter

analyser: partial

- A generic type in a type position or constructor is given all its arguments; a type pack is no single type; a value function's generics resolved at a call from what the call writes, then the type its place expects, then its arguments in declared parameter order; an abstract struct a later argument has widens what an earlier argument decided
- A nullable type the call writes for an operator's result that is a type parameter is given whole to a generic candidate whose result is one: `get<i64?>(...)` gives core's candidate an `i64?`
- A generic value parameter takes its default where nothing else says what it is: a function's and a candidate's at a call, a struct's where it is named bare or constructed, an operator's in the output requested
- Left: packs; solvable equalities; temporal bodies; what a nullable place says of a result that is a type parameter and nothing more (hhenson/hgl_spec#165, open), taken for now to be what is under the `?`

transpiler: planned

## GEN-3

analyser: partial

- A parameter compared in a constraint is a const option or is rejected, and each constant it is compared with is a value of the option's type (`type.mismatch`, `member.unknown`, `enum.context_type`, at the declaration); a constant of a module among those is its value (UNIT-13)
- At a named operator call in a value function, a candidate's `T in {types}` and `T == type` are checked against what `T` stands for, and a named operator requirement is tested with the call's types (GEN-16)
- Where `T` stands for a generic parameter of the declaration the call is written in, which is not given there, such a condition is not told ("under negation/disjunction it only tests already-resolved data"; `declared::same_type`: `U` is not known to be `i64` or not, where it was taken to differ). GEN-7: "A generic body is checked under its stated requirements", so no candidate is known to admit the call, which is `operator.no_candidate` where the body is declared, by name and by symbol, in a value function and a graph (`const fn wrap<U>(value: U) -> i64 => kind_of(value)`); under `requires kind_of(U) -> i64` it is the operation the body requires (GEN-16)
- A positive conjunctive equality of a generic value parameter with a constant, `requires M == .slow`, binds it (`Typer::bound`): an option it gives is that constant where the body's calls select their candidates
- A membership of a type parameter, `T in {i64, f64}` (`T == i64` as one of one type), alone or joined by `&&`, "authorizes in the body (GEN-7) an operation that each listed type has a candidate for": a call that names the parameter is tried for every member, and with two parameters every combination, with what a call is held to once selected; each instance then selects its own candidate (`values::Selecting`, the instance walk, `Program::instance`), and "An operation that a listed type lacks rejects at the declaration", with a note naming it: the fragments `less` under `T in {i64, f64}` and, rejected, under `T in {i64, Point}`
- A membership of a generic value parameter, `M in {.fast, .slow}`, authorizes a call that gives it as an option where each listed value has a candidate that admits it, built as written: the fragment `ratio_in` under `z in {.ieee, .nan}`, and rejected under `z in {.ieee, .error}` and with no `requires`
- A call of a function, a construction of a struct, and a type written that applies one, whose `requires` does not hold with its generic arguments, is `requires.unmet`: a membership or an equality of a type, told in three answers (`declared::same_type`: `FBox<X>` is not known to be `FBox<2.0>` while `X` is not given), and a generic value parameter compared with constants and with another such parameter (`N == M`, `N in {M, 5}`) as the language's `==` compares them (`ConstantValue::equals`), not by their sameness (UNIT-13): `requires k == 0.0` admits `reciprocal<0.0>()` and `reciprocal<-0.0>()`; its value read as one of the parameter's type however it is written, after the callee's name, by name, in a type, or left to its default
- A call in a generic body of another generic function that passes on the body's own generic parameters, `one_only<Y>(v)` in `outer<const Y: f64>`, is judged where the body is declared, by what the body states (GEN-7: "It is judged once, where it is declared, by what its `requires` states"; hhenson/hgl_spec#144): it holds where a binding equality (`requires Y == 1.0`) or every member, and every combination of members, of a membership the body states makes the callee's `requires` hold, or where the body's own named operator requirement answers the callee's (GEN-16); otherwise `requires.unmet` at the call in the body, naming the member that fails, and `generic.argument_constant` there where a generic argument worked out for a member fails. A call of the body is held to the body's own `requires`. Not told is never taken to hold: a constant this compiler does not work out is `requires.unmet` at a call that gives every argument. The instance walk (`instances_hold`) keeps what only an instance decides: a struct a type in the declaration applies (a field's `Only<FBox<X>>`), a generic argument worked out from the body's parameters, the candidate a membership selects, and the bound below. Only the calls that reach a `requires`, or a generic argument worked out from the body's own parameters are followed, each instance once however many calls make it, by its arguments' values, and no more than `hgl_analyser::INSTANCES` (4096) instances of one declaration from one call, with arguments no longer than 65536 characters written out: a call past that is `generic.instance_limit` (GEN-13: "Where the instances of one declaration never end when the program is compiled, the program rejects"), at the call that makes the first of them, which says the instances of that declaration do not end within the bound, as `grow<N * 2>` in `grow<const N: i64>` and `climb<N + 1>` in `climb<const N: i64>` make
- Left: every other form `requires` admits; binding equalities solved to a fixed point; a membership of a generic parameter of the declaration a call is written in, which is not told (GEN-7); a function with a type pack or a pack parameter; a binding equality of a type parameter, `T == i64`, read as a membership of one and not as a binding; a call settled by a membership whose argument is read as its place's type and is no constant (`[x]`)

## GEN-4

analyser: partial

- An `impl fn` is a temporal candidate and an `impl const fn` a value candidate; each binds an operator its module declares, its file imports, or the prelude binds, and one that names no operator is `impl.unknown_operator`; a value candidate's body is read as a value function's
- An operator's requirement `O in {...}` on its bare result parameter limits its candidates' results
- Left: operator identity in selection; every other requirement of an operator

transpiler: planned

## GEN-5

analyser: partial

- In a value function, a call of an operator, by name or by the symbol that stands for it, selects among its value candidates: the requested output first, then the inputs; an integer literal selects as an `i64` before it is widened, which the candidates' types settle before their conditions, and candidates that tie in which literals they widen give way to the one that widens every one; no candidate and a tie rejected
- A generic candidate is admitted with its parameters resolved from the call, and the most specific admitted one is selected (GEN-12)
- In a graph a call selects among temporal and value candidates, and a temporal candidate outranks a value one of the same parameter types
- In value code, a test's outside an `eval` or a `run` and an application's statements, a call selects among both too, a value candidate outranking a temporal one of the same parameter types; a temporal candidate it selects is `call.role`, the code proposed (TST-3, UNIT-14)
- Left: a candidate with a pack; an operator with a pack, and so the Boolean connectives and `!`; an argument that is itself a call only its requested output selects; composition, where a temporal candidate outranks a value one of the same parameter types, and runtime bodies

## GEN-6

analyser: partial

- In a value function a named call is matched against each value candidate's own signature
- An option that selects is a generic value parameter, given by name or by its default, and the candidate's `requires` is checked against its value; one that does not select is a parameter, by name or position
- An option's name is one kind across an operator's candidates
- A value candidate may mark a parameter `var`, which takes no part in selecting it (FN-15)
- An option given a generic value parameter of the enclosing function, `step(v, mode: M)` in `go<const M: Mode>`, admits a candidate where the function's `requires` binds the parameter, `requires M == .slow` (GEN-3); where it states nothing of it, a default of it included, no candidate is known to admit the call, which is `operator.no_candidate` where the function is declared (GEN-7)
- Left: a candidate's shape against its operator, the names and types of the operator's parameters and its generic relationships; candidates that stay resolvable together; temporal candidates

## GEN-7

analyser: partial

- In a body with generic parameters, a call of an operator, by name or by symbol, with an argument whose type names one of them is admitted where a requirement of the declaration names that operator with those types, each given by its position or by the operator's parameter name, and is of the type its arrow states; with none it selects among the candidates as any call does, so one that takes any type is still taken
- The call records the position of the requirement it is (`Callee::Required`), so that a call of the declaration says which candidate it calls
- "It is judged once, where it is declared, by what its `requires` states, and never again for the generic arguments a call of it gives": a call of an operator in it is admitted where a candidate takes every generic argument the requirements admit, where a named operator requirement states it, or under a membership where each listed type has one (GEN-3), and otherwise the declaration rejects (`operator.no_candidate`, with a note), by name and by symbol, in a value function and a graph: the fragments `less` and `wrap` with no `requires`
- Left: a requirement met by widening an argument; a type or size parameter used as a runtime value

## GEN-8

parser: done

- `_` as an `instantiate` argument

analyser: partial

- A call's generic arguments, all or the first of them, for a value function and for the output requested of an operator's candidates; a call that leaves a parameter unresolved rejected
- A generic value parameter given by name or after the callee's name, and one a type gives, read as a value of the parameter's type (EXPR-5, `type.mismatch`): a literal, an enum member however it is written (TYPE-7), a constant, a generic value parameter in scope, and a constant expression of those, of numbers, durations, tenors, strings, bools, lists and tuples (`constant`); one that is no compile-time constant, and one whose value this compiler does not work out, is `generic.argument_constant`
- An operator's generics are resolved from what the call writes, then its place, then its arguments, then a default, and the output so requested is matched as far as it is known
- One a generic body works out from its own generic parameters is worked out for each instance, and where its evaluation fails there (NUM-2: an `i64` out of range; a division of two `i64` by zero) or is not worked out, the call that makes the instance is `generic.argument_constant` (ERR-9)
- Two generic value arguments are one where they are the same constant (`ConstantValue::same`), which for an `f64` is not `==`: `Scaled<-0.0>` is not `Scaled<0.0>`, and any two NaNs are one constant (UNIT-13, GEN-13); hgraph writes each as an instance of its own. A comparison of a NaN constant is the language's: false, but `!=` (NUM-5)
- Left: an operator's own generic value parameter given by name; `instantiate`; packs; what an operator's generic arguments say of its operands; temporal bodies; an operation the reader does not work out, a symbol given configuration and a date moved by a tenor

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
- Function calls check the cardinality of captured arguments against both the value parameter and its generic type pack. The HIR records the resolved sequence of types for each captured type pack.
- Explicitly written types check the corresponding arguments of a single trailing type pack. Pack reflection and the remaining explicit type-pack cases are tracked separately. [Compiler issue #336](https://github.com/hhenson/hgl-compiler/issues/336)

transpiler: planned

- `schemas(pack)` as a runtime-only borrowed view

## GEN-11

analyser: partial

- In a graph, a connection given to a function's parameter binds its generic parameters from the connection's type, at a `ref` parameter as at any other, and a `ref` result is a connection of its type (FN-16): no type holds a reference, and inference removes none
- A window given to a value function's parameter binds as the `list<V>` it holds (TYPE-11, FN-1)
- Left: an operator's call; a requested output's nested variables

## GEN-12

analyser: partial

- Among the candidates a named call admits in a value function, each of the operator's parameters is compared with the other candidate's part for part: a concrete type or a structure over a bare variable in the same place; a candidate as specific everywhere and more in one place outranks
- A candidate's generics are bound in declared parameter order
- A `ref` mark adds no priority: candidates are compared by their types, which hold none (FN-16)
- Left: a repeated variable counting once; temporal bodies

## GEN-13

analyser: partial

- A plain parameter is a type or a value by its uses in the signature; uses that disagree are rejected, and a struct's parameter nothing uses; a value parameter or a pack is not named as a type; a generic value parameter's default, a callable's or a struct's, is of its declared type, as far as how the default is written says what it is, and of the type a call resolves where that names a type parameter: an implied member, `.slow`, is held to the enum the call resolves, so is no `i64` and no member of an enum that lacks it (`type.mismatch`, TYPE-7)
- A default that names a generic parameter before its own is of what that one takes, at a call and in a struct written with no arguments (`W2` of `<const A: i64 = 1, const B: i64 = A>` is `W2<1, 1>`); one that names its own, or one after it, gives it no value, so a call that leaves it is `generic.unresolved` (GEN-8); a call in the function's own body that gives a parameter its own, `rec<N>(v)`, gives the default that one and a struct written with no arguments `generic.argument_count`
- Generic arguments that are the same, each type canonical and each value one constant (UNIT-13), make one type or one instance, and arguments that differ another: of the worked fragment, `Scaled<1>`, `Scaled<1.0>` and `Scaled<1.00>` are one type, and a `Scaled<-0.0>` where a `Scaled<0.0>` is expected is `type.mismatch`
- "Where the instances of one declaration never end when the program is compiled, the program rejects": no more than `hgl_analyser::INSTANCES` (4096) instances of one declaration are followed from one call, by the analyser and by `hgl_lower`, and a call past that is `generic.instance_limit`, at the call that makes the first of them: the worked fragment's `climbed`; through constants worked out from the body's own parameters, and through a type that grows, `nest<list<T>>`
- Left: a default that names anything else; the kind a constraint gives; `type(N)`; a call that supplies no type for a function's parameter its signature does not settle

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

transpiler: planned
