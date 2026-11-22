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
- A graph instance fills unresolved inferred struct type arguments from settled fields in written order and checks the concrete application (GEN-3, GEN-7). An earlier unsettled field can defer a later field's binding. Explicit and expected type arguments and types needed to read a contextual field or generic default keep their source treatment.
- A graph instance can infer a named function's type arguments from settled operands and use the resulting parameter types to read context-dependent arguments (EXPR-9, ERR-2).
- A nullable type the call writes for an operator's result that is a type parameter is given whole to a generic candidate whose result is one: `get<i64?>(...)` gives core's candidate an `i64?`
- A nullable place leaves a bare result parameter for the inputs to decide. A fallible dictionary read resolves its success type from the dictionary. A store read can infer it from its place (CAP-6).
- A plain result place can resolve the success type of a fallible generic call and let a bare parameter read the success of a result argument (ERR-15).
- A generic value parameter takes its default where nothing else says what it is: a function's and a candidate's at a call, a struct's where it is named bare or constructed, an operator's in the output requested
- Left: packs; solvable equalities; temporal bodies

transpiler: partial

- Writes each demanded concrete value or temporal function instance and ordinary generic
  struct templates. Keeps references to instances in the declaring module.
- Left: An unresolved or unavailable instance is reported. Generic struct template and
  argument limits are under TYPE-3. A native argument outside the supported type and
  constant representations is reported.

## GEN-3

analyser: partial

- A positive conjunctive `any` equality binds one unresolved generic from the first matching pack entry, with tentative bindings discarded on a failed trial and conjuncts revisited to a fixed point. Negation and disjunction only test supplied bindings.
- Bare bool generic terms and their negations are checked at calls and candidates; positive terms also state a bool bound in a generic body.
- Inequality constraints parse and test resolved operands without binding. Named module constant operands of `!=` remain for the F5 follow-up of #367.
- Naming a capability type in an operator declaration's type membership is planned in #562 (TIME-8).

- A parameter compared in a constraint is a const option or is rejected, and each constant it is compared with is a value of the option's type (`type.mismatch`, `member.unknown`, `enum.context_type`, at the declaration); a constant of a module among those is its value (UNIT-13)
- At a named operator call in a value function, a candidate's `T in {types}` and `T == type` are checked against what `T` stands for, and a named operator requirement is tested with the call's types (GEN-16)
- Where `T` stands for a generic parameter of the declaration the call is written in, which is not given there, such a condition is not told ("under negation/disjunction it only tests already-resolved data"; `declared::same_type`: `U` is not known to be `i64` or not, where it was taken to differ). GEN-7: "A generic body is checked under its stated requirements", so no candidate is known to admit the call, which is `operator.no_candidate` where the body is declared, by name and by symbol, in a value function and a graph (`const fn wrap<U>(value: U) -> i64 => kind_of(value)`); under `requires kind_of(U) -> i64` it is the operation the body requires (GEN-16)
- A positive conjunctive equality of a generic value parameter with a constant, `requires M == .slow`, binds it (`Typer::bound`): an option it gives is that constant where the body's calls select their candidates
- A membership of a type parameter, `T in {i64, f64}` (`T == i64` as one of one type), alone or joined by `&&`, "authorizes in the body (GEN-7) an operation that each listed type has a candidate for": a call that names the parameter is tried for every member, and with two parameters every combination, with what a call is held to once selected; each instance then selects its own candidate (`values::Selecting`, the instance walk, `Program::instance`), and "An operation that a listed type lacks rejects at the declaration", with a note naming it: the fragments `less` under `T in {i64, f64}` and, rejected, under `T in {i64, Point}`
- A membership of a generic value parameter, `M in {.fast, .slow}`, authorizes a call that gives it as an option where each listed value has a candidate that admits it, built as written: the fragment `ratio_in` under `z in {.ieee, .nan}`, and rejected under `z in {.ieee, .error}` and with no `requires`
- A call of a function, a construction of a struct, and a type written that applies one, whose `requires` does not hold with its generic arguments, is `requires.unmet`: a membership or an equality of a type, told in three answers (`declared::same_type`: `FBox<X>` is not known to be `FBox<2.0>` while `X` is not given), and a generic value parameter compared with constants and with another such parameter (`N == M`, `N in {M, 5}`) as the language's `==` compares them (`ConstantValue::equals`), not by their sameness (UNIT-13): `requires k == 0.0` admits `reciprocal<0.0>()` and `reciprocal<-0.0>()`; its value read as one of the parameter's type however it is written, after the callee's name, by name, in a type, or left to its default
- A call in a generic body of another generic function that passes on the body's own generic parameters, `one_only<Y>(v)` in `outer<const Y: f64>`, is judged where the body is declared, by what the body states (GEN-7: "It is judged once, where it is declared, by what its `requires` states"; hhenson/hgl_spec#144): it holds where a binding equality (`requires Y == 1.0`) or every member, and every combination of members, of a membership the body states makes the callee's `requires` hold, or where the body's own named operator requirement answers the callee's (GEN-16); otherwise `requires.unmet` at the call in the body, naming the member that fails, and `generic.argument_constant` there where a generic argument worked out for a member fails. A call of the body is held to the body's own `requires`. Not told is never taken to hold: a constant this compiler does not work out is `requires.unmet` at a call that gives every argument. The instance walk (`instances_hold`) keeps what only an instance decides: a struct a type in the declaration applies (a field's `Only<FBox<X>>`), a generic argument worked out from the body's parameters, the candidate a membership selects, and the bound below. Only the calls that reach a `requires`, or a generic argument worked out from the body's own parameters are followed, each instance once however many calls make it, by its arguments' values, and no more than `hgl_analyser::INSTANCES` (4096) instances of one declaration from one call, with arguments no longer than 65536 characters written out: a call past that is `generic.instance_limit` (GEN-13: "Where the instances of one declaration never end when the program is compiled, the program rejects"), at the call that makes the first of them, which says the instances of that declaration do not end within the bound, as `grow<N * 2>` in `grow<const N: i64>` and `climb<N + 1>` in `climb<const N: i64>` make
- At a call of a function with a type pack, a false requirement or one the analyser cannot judge is `requires.unmet`. The diagnostic distinguishes the two (GEN-10).
- Left: every other form `requires` admits; binding equalities solved to a fixed point; a membership of a generic parameter of the declaration a call is written in, which is not told (GEN-7); a binding equality of a type parameter, `T == i64`, read as a membership of one and not as a binding; a call settled by a membership whose argument is read as its place's type and is no constant (`[x]`)

## GEN-4

parser: partial

- An operator declaration can begin with `export` (UNIT-3). A candidate takes no `export`.
- An operator declaration and its candidates can declare configuration parameters (FN-18).

analyser: partial

- An `impl fn` is a temporal candidate and an `impl const fn` a value candidate; each binds an operator its module declares, its file imports, or the prelude binds, and one that names no operator is `impl.unknown_operator`; a value candidate's body is read as a value function's
- An operator declared in a named test has that test in its identity. A test-local candidate binds its operator in the test’s scope and participates only in selection within that test (TST-1).
- An operator's requirement `O in {...}` on its bare result parameter limits its candidates' results
- Operator identity in selection and other requirements of an operator are pending.
- A private operator is used only in its own module. Importing one rejects with `import.private` (UNIT-3). A candidate in another module needs a binding to an exported operator or reports `impl.unknown_operator`.
- A symbol call whose core identity is private rejects outside core with `operator.no_candidate`.

transpiler: done

- An HGL candidate has a separate name in its declaring module, using its declared parameter types and declaration identity (`emit::names::placed`). Its generic instances are named below that candidate's own namespace.

## GEN-5

analyser: partial

- In a value function, a call of an operator, by name or by the symbol that stands for it, selects among its value candidates: the requested output first, then the inputs; an integer literal selects as an `i64` before it is widened, which the candidates' types settle before their conditions, and candidates that tie in which literals they widen give way to the one that widens every one; no candidate and a tie rejected
- A generic candidate is admitted with its parameters resolved from the call, and the most specific admitted one is selected (GEN-12). A fallible candidate can meet a T or `T?` place. The selected candidate determines the result before its form is taken (ERR-15).
- Operand forms are tried at integer precision before widening literals. A nullable candidate result can meet a place for its payload under ERR-12. [Specification decision #182](https://github.com/hhenson/hgl_spec/pull/182)
- In a graph a call selects among temporal and value candidates, and a temporal candidate outranks a value one of the same parameter types
- In value code, a test's outside an `eval` or a `run` and an application's statements, a call selects among both too, a value candidate outranking a temporal one of the same parameter types; a temporal candidate it selects is `call.role`, the code proposed (TST-3, UNIT-14)
- A packed candidate matches each captured argument and checks the pack's bounds. Equally specific candidates stay tied when their admitted cardinalities overlap.
- Left: an argument that is itself a call only its requested output selects; composition, where a temporal candidate outranks a value one of the same parameter types, and runtime bodies

transpiler: done

- Named and symbol calls use the selected candidate's module, declaration and instance to name its value function or temporal wiring. C++ sees the selected candidate's own name (`emit::names::placed`).

## GEN-6

analyser: partial

- In a value function a named call is matched against each value candidate's own signature
- Each value or temporal candidate supplies the function type of a fixed callback, including its phase (FN-17).
- An option that selects is a generic value parameter, given by name or by its default, and the candidate's `requires` is checked against its value; one that does not select is a parameter, by name or position
- An option's name is one kind across an operator's candidates
- A candidate declares its operator's configuration parameters. Named and symbolic calls bind those parameters in braces, and a candidate may use the operator's default (FN-18, EXPR-10).
- A value candidate may mark a parameter `var`, which takes no part in selecting it (FN-15)
- An option given a generic value parameter of the enclosing function, `step(v, mode: M)` in `go<const M: Mode>`, admits a candidate where the function's `requires` binds the parameter, `requires M == .slow` (GEN-3); where it states nothing of it, a default of it included, no candidate is known to admit the call, which is `operator.no_candidate` where the function is declared (GEN-7)
- A fixed parameter in an operator pack's place reports the proposed `impl.pack` code at the candidate parameter name. A pack with cardinality one is accepted. Bare Boolean requirements await #367.
- Left: a candidate's shape against its operator, the names and types of the operator's parameters and its generic relationships; candidates that stay resolvable together; temporal candidates

## GEN-7

analyser: partial

- The proposed checking of passes over the keys of a generic pack awaits #368.

- In a body with generic parameters, a call of an operator, by name or by symbol, with an argument whose type names one of them is admitted where a requirement of the declaration names that operator with those types, each given by its position or by the operator's parameter name, and is of the type its arrow states. A candidate that takes every admitted type can also admit the body.
- The call records the position of the requirement it is (`Callee::Required`), so that a call of the declaration says which candidate it calls
- "It is judged once, where it is declared, by what its `requires` states, and never again for the generic arguments a call of it gives": a call of an operator in it is admitted where a candidate takes every generic argument the requirements admit, where a named operator requirement states it, or under a membership where each listed type has one (GEN-3), and otherwise the declaration rejects (`operator.no_candidate`, with a note), by name and by symbol, in a value function and a graph: the fragments `less` and `wrap` with no `requires`
- Each instance selects again for an operator call whose operand types use the enclosing declaration's generic parameters, including when a generic candidate admitted the body. The new selection uses the instance's operands and the operator binding at the call (GEN-5), including private operators, symbols, and calls in nodes and graphs.
- A generic body's value parameter, including a fixed list's size `N` (GEN-13), is specified to be read as a constant, such as in `0..<N` or an index. #368 tracks ranges.
- Left: a requirement met by widening an argument, and checking misuse of a type or size parameter as a runtime value

## GEN-8

parser: done

- `_` as an `instantiate` argument

analyser: partial

- A call's generic arguments, all or the first of them, for a value function and for the output requested of an operator's candidates; a call that leaves a parameter unresolved rejected
- A generic value parameter given by name or after the callee's name, and one a type gives, read as a value of the parameter's type (EXPR-5, `type.mismatch`): a literal, an enum member however it is written (TYPE-7), a constant, a generic value parameter in scope, and a constant expression of those, of numbers, durations, tenors, strings, bools, lists and tuples (`constant`); one that is no compile-time constant, and one whose value this compiler does not work out, is `generic.argument_constant`
- An operator's generics are resolved from what the call writes, then its place, then its arguments, then a default, and the output so requested is matched as far as it is known
- A known `set<str>` supplies a generic value from a declaration default, a call's configuration, or a named constant (FN-8). Its members give it one canonical identity in any written order (UNIT-13).
- A call that writes a positional type pack supplies its whole type sequence. An unwritten positional or named pack infers each type from its own argument or eval trace. A graph instance completes its deferred inferred types in capture order (GEN-7, GEN-10). An unresolved type rejects, or a sequence that requires context gives `sequence.context_type` (EXPR-9).
- One a generic body works out from its own generic parameters is worked out for each instance, and where its evaluation fails there (NUM-2: an `i64` out of range; a division of two `i64` by zero) or is not worked out, the call that makes the instance is `generic.argument_constant` (ERR-9)
- Two generic value arguments are one where they are the same constant (`ConstantValue::same`), which for an `f64` is not `==`: `Scaled<-0.0>` is not `Scaled<0.0>`, and any two NaNs are one constant (UNIT-13, GEN-13); hgraph writes each as an instance of its own. A comparison of a NaN constant is the language's: false, but `!=` (NUM-5)
- Left: an operator's own generic value parameter given by name; `instantiate`; what an operator's other generic arguments say of its operands; temporal bodies; an operation the reader does not work out, a symbol given configuration and a date moved by a tenor

## GEN-9

analyser: done

- Property domains are ordinary concrete value types. A generic domain rejects with local `property.domain`.
- Law queries read the public operator's metadata for the concrete operand type; an ordinary or anonymous function declares no guarantee. Metadata performs no arithmetic transformation.

optimiser: planned

- A recorded law licenses no reassociation across overflow or float effects

## GEN-10

parser: partial

- A cardinality after a pack parameter or a generic pack parameter; `each` and `any` quantifiers. `any` remains a scalar type outside the quantifier form.

analyser: partial

- A cardinality bounds the arguments a generic type's pack takes, and has its upper bound at least its lower
- Function and operator calls check the cardinality of captured arguments against the value parameter and its generic type pack. Explicit positional type sequences also constrain the argument count. The HIR records the resolved sequence of types for each captured type pack.
- A function's `each T in types(Ts)` checks its condition for every resolved type in the pack. An empty capture satisfies it. Equality and membership can test the bound type by name. A verdict the analyser cannot give rejects as untold (GEN-3).
- GEN-17 adds quantification over pack keys. `any` succeeds for at least one entry and rejects an empty pack. Type and key bindings project within nested type expressions.
- Left: named operator requirements bound by a quantifier.
- An ordinary value context rejects the borrowed view from `schemas(pack)`.

transpiler: partial

- C++ uses the analysed argument order, names and types when it writes each pack's
  parameters, including an empty capture.
- Quantifier verdicts and bindings are settled in the analyser before C++ emission.
- Left: reading a pack as a whole and `schemas(pack)`.

## GEN-11

analyser: partial

- In a graph, a connection given to a function's parameter binds its generic parameters from the connection's type, at a `ref` parameter as at any other, and a `ref` result is a connection of its type (FN-16): no type holds a reference, and inference removes none
- A window given to a value function's parameter binds as the `list<V>` it holds (TYPE-11, FN-1)
- Left: an operator's call; a requested output's nested variables

## GEN-12

analyser: partial

- Among the candidates a named call admits in a value function, each of the operator's parameters is compared with the other candidate's part for part: a concrete type or a structure over a bare variable in the same place; a candidate as specific everywhere and more in one place outranks
- A candidate's generics are bound in declared parameter order
- A named function call preserves declared parameter order when an earlier graph argument settles in an instance (GEN-7).
- A `ref` mark adds no priority: candidates are compared by their types, which hold none (FN-16)
- Left: a repeated variable counting once; temporal bodies

## GEN-13

analyser: partial

- A plain parameter is a type or a value by its uses in the signature; uses that disagree are rejected, and a struct's parameter nothing uses; a value parameter or a pack is not named as a type; a generic value parameter's default, a callable's or a struct's, is of its declared type, as far as how the default is written says what it is, and of the type a call resolves where that names a type parameter: an implied member, `.slow`, is held to the enum the call resolves, so is no `i64` and no member of an enum that lacks it (`type.mismatch`, TYPE-7)
- A default that names a generic parameter before its own is of what that one takes, at a call and in a struct written with no arguments (`W2` of `<const A: i64 = 1, const B: i64 = A>` is `W2<1, 1>`); one that names its own, or one after it, gives it no value, so a call that leaves it is `generic.unresolved` (GEN-8); a call in the function's own body that gives a parameter its own, `rec<N>(v)`, gives the default that one and a struct written with no arguments `generic.argument_count`
- Generic arguments that are the same, each type canonical and each value one constant (UNIT-13), make one type or one instance, and arguments that differ another: of the worked fragment, `Scaled<1>`, `Scaled<1.0>` and `Scaled<1.00>` are one type, and a `Scaled<-0.0>` where a `Scaled<0.0>` is expected is `type.mismatch`
- Calls and symbolic operations whose candidates become known in a graph instance contribute to the same bounded traversal of instances and their requirements (GEN-3, GEN-7).
- "Where the instances of one declaration never end when the program is compiled, the program rejects": no more than `hgl_analyser::INSTANCES` (4096) instances of one declaration are followed from one call, by the analyser and by `hgl_lower`, and a call past that is `generic.instance_limit`, at the call that makes the first of them: the worked fragment's `climbed`; through constants worked out from the body's own parameters, and through a type that grows, `nest<list<T>>`
- Left: a default that names anything else; the kind a constraint gives; `type(N)`; a call that supplies no type for a function's parameter its signature does not settle

## GEN-14

parser: done

- `<` after a name read as generic arguments or comparison; marked undecided when both parse

analyser: partial

- Decides an undecided constructor from what its name resolves to, a generic type, function or operator; a generic type before a comparison's `<` rejected; each argument against its parameter's kind
- Left: a generic function or operator before a comparison's `<`

## GEN-15

parser: partial

- Negation binds tighter than conjunction, which binds tighter than disjunction. The new `any` form and named Boolean terms await #367.

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

- Lowers a generic instance's supplied answer to a named operator requirement to its
  resolved candidate. Instances with test-local answers are kept distinct even
  when the generic arguments are identical (TST-1).
- Left: An operator requirement left without an executable answer is reported. Function
  law predicates are settled by the analyser (GEN-18).

## GEN-17

parser: partial

- Reads a function type's named pack and `arg_keys` and member types in requirements. `Ps[arg]` has a type form, with an index tree in positions that also allow values.
- Left: pack key membership in a body, alongside pack values and unrolled loops (#368).

analyser: partial

- Reflects the selected callback's fixed suffix and its supplied positional and named pack entries. A dependent pack keeps the callback parameter keys and the types written at the call.
- Evaluates `each` over `arg_keys`, constant key membership, and keyed type projection. Candidate requirements can compare bare pack members. A missing member of a known pack reports `pack.name`.
- A generic call that receives another function parameter checks its pack requirements when the caller supplies the reflected types.
- An operator callback reflects the public declaration's parameter names and the selected candidate's types (FN-17).

## GEN-18

parser: done

- Reads law-predicate calls and bare Boolean generic terms, including their negations and Boolean combinations.

analyser: done

- `associative(f)` and `commutative(f)` query a supplied function's public operator and concrete domain, in requirements and bool generic defaults. Explicit options override defaults.
- A wrapper is checked at declaration under its own stated positive or negative law (GEN-7). With no guarantee it rejects even if two candidates cover both values. A bool generic explicitly given onward under a membership in `{true, false}` selects its candidate in each instance (GEN-3). The corrected #367 ruling is tested.

- C++ receives the already-resolved bool options and selected candidates; this rule needs no runtime law query or target change.
