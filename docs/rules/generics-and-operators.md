# Generics and operators

`generics-and-operators.md`

## GEN-1

analyser: partial

- Generic scope covers the whole declaration; a repeated type parameter is one type at a call of a value function
- A repeated generic value parameter is one constant (UNIT-13) at a call of a value function and of a temporal one: `Scaled<1>` and `Scaled<1.00>` given to two `Scaled<k>` are one, and so are two NaNs, and `Scaled<0.0>` beside `Scaled<-0.0>`, or `list<i64, 2>` beside `list<i64, 3>`, is `type.mismatch`
- A generic used only as a complete, unqualified result can bind to no result from a supplied callback (FN-17). A nested type, ordinary input, `ref` result or constraint still needs a type.
- An instance with no result is called as a statement. A call in a value position and a written type in the body that needs the absent generic report `type.mismatch` (FN-7, GEN-7).
- Left: the other occurrence requirements in temporal bodies

## GEN-2

parser: done

- A default on a generic value parameter

analyser: partial

- A generic type in a type position or constructor is given all its arguments; a type pack is no single type; a value function's generics resolved at a call from what the call writes, then the type its place expects, then its arguments in declared parameter order; an abstract struct a later argument has widens what an earlier argument decided
- A graph instance fills unresolved inferred struct type arguments from settled fields in written order and checks the concrete application (GEN-3, GEN-7). An earlier unsettled field can defer a later field's binding. Explicit and expected type arguments and types needed to read a contextual field or generic default keep their source treatment.
- A graph instance can infer a named function's type arguments from settled operands and use the resulting parameter types to read context-dependent arguments (EXPR-9, ERR-2).
- A nullable type the call writes for an operator's result that is a type parameter is given whole to a generic candidate whose result is one: `get<i64?>(...)` gives core's candidate an `i64?`
- A nullable place leaves a bare result parameter for the inputs, a written type, or a solvable equality to decide (GEN-3, GEN-8). A fallible dictionary read resolves its success type from the dictionary. A plain value place can decide a store read's success type (CAP-6, ERR-15).
- A nullable temporal result preserves an unresolved bare call result when checking a publication. A call whose declared result has a delta shape can still take the publication's delta context (TIME-7).
- A plain result place can resolve the success type of a fallible generic call and let a bare parameter read the success of a result argument (ERR-15).
- A generic value parameter takes its default where nothing else says what it is: a function's and a candidate's at a call, a struct's where it is named bare or constructed, an operator's in the output requested
- Discarding a call leaves a generic result unresolved if nothing else supplies it (GEN-1).
- Left: packs; solvable equalities; temporal bodies

transpiler: partial

- Writes each demanded concrete value or temporal function instance and ordinary generic
  struct templates. Keeps references to instances in the declaring module.
- Left: An unresolved or unavailable instance is reported. Generic struct template and
  argument limits are under TYPE-3. A native argument outside the supported type and
  constant representations is reported.

## GEN-3

parser: done

- Category keywords `struct` and `enum` occur only after a membership's `in`.

analyser: partial

- Category membership tests the nominal kind, including generic structs, and excludes nullable types, other composites, native types and ordinary constants. Stated category facts are preserved through generic calls; negation and disjunction do not invent positive category evidence. The complete `examples.categories` illustration executes all three selections, with the separate `i64` call rejected.
- A category stated for a reflected pack member applies to that pack and key (GEN-7, GEN-17).
- A positive conjunctive `any` equality binds one unresolved generic from the first matching pack entry, with tentative bindings discarded on a failed trial and conjuncts revisited to a fixed point. Negation and disjunction only test supplied bindings.
- Proposed. A positive membership or equality of a dependent pack member supplies an unresolved input type to a bare callback, including inside `each` over the pack's keys (FN-17, GEN-17). Compare complete interpretations under GEN-12 after checking known input and result types, sibling equalities, and shared bindings. A less specific choice can fit where a more specific one conflicts. Distinct equally specific or incomparable best readings reject as ambiguous. Apply enclosing defaults and positive equalities to the callback and membership patterns. Each enclosing operator trial uses its candidate's template and checks both the public and candidate requirements (GEN-5, GEN-6). A bare operator callback also checks its own candidate signatures before choosing an interpretation. A winning alternative that binds nothing lets sibling arguments, callbacks or a required result settle the input.
- Bare bool generic terms and their negations are checked at calls and candidates; positive terms also state a bool bound in a generic body.
- Inequality constraints parse and test resolved operands, including named module constants, without binding.
- An operator declaration may name a capability type as a member of a type set, as in `requires C in {Scheduler, Alarm}` (TIME-8).

- A parameter compared in a constraint is a const option or is rejected, and each constant it is compared with is a value of the option's type (`type.mismatch`, `member.unknown`, `enum.context_type`, at the declaration); a constant of a module among those is its value (UNIT-13)
- At a named operator call in a value function, a candidate's `T in {types}` and `T == type` are checked against what `T` stands for, and a named operator requirement is tested with the call's types (GEN-16)
- Where `T` stands for a generic parameter of the declaration the call is written in, which is not given there, such a condition is not told ("under negation/disjunction it only tests already-resolved data"; `declared::same_type`: `U` is not known to be `i64` or not, where it was taken to differ). GEN-7: "A generic body is checked under its stated requirements", so no candidate is known to admit the call, which is `operator.no_candidate` where the body is declared, by name and by symbol, in a value function and a graph (`const fn wrap<U>(value: U) -> i64 => kind_of(value)`); under `requires kind_of(U) -> i64` it is the operation the body requires (GEN-16)
- A positive conjunctive equality of a generic value parameter with a constant, `requires M == .slow`, binds it (`Typer::bound`): an option it gives is that constant where the body's calls select their candidates
- A membership of a type parameter, `T in {i64, f64}` (`T == i64` as one of one type), alone or joined by `&&`, "authorizes in the body (GEN-7) an operation that each listed type has a candidate for": a call that names the parameter is tried for every member, and with independent parameters every combination, with what a call is held to once selected; each instance then selects its own candidate (`values::Selecting`, the instance walk, `Program::instance`), and "An operation that a listed type lacks rejects at the declaration", with a note naming it: the fragments `less` under `T in {i64, f64}` and, rejected, under `T in {i64, Point}`
- A membership of a generic value parameter, `M in {.fast, .slow}`, authorizes a call that gives it as an option where each listed value has a candidate that admits it, built as written: the fragment `ratio_in` under `z in {.ieee, .nan}`, and rejected under `z in {.ieee, .error}` and with no `requires`
- A call of a function, a construction of a struct, and a type written that applies one, whose `requires` does not hold with its generic arguments, is `requires.unmet`: a membership or an equality of a type, told in three answers (`declared::same_type`: `FBox<X>` is not known to be `FBox<2.0>` while `X` is not given), and a generic value parameter compared with constants and with another such parameter (`N == M`, `N in {M, 5}`) as the language's `==` compares them (`ConstantValue::equals`), not by their sameness (UNIT-13): `requires k == 0.0` admits `reciprocal<0.0>()` and `reciprocal<-0.0>()`; its value read as one of the parameter's type however it is written, after the callee's name, by name, in a type, or left to its default
- A call in a generic body of another generic function that passes on the body's own generic parameters, `one_only<Y>(v)` in `outer<const Y: f64>`, is judged where the body is declared, by what the body states (GEN-7: "It is judged once, where it is declared, by what its `requires` states"; hhenson/hgl_spec#144): it holds where a binding equality (`requires Y == 1.0`) or every member, and every combination of members, of a membership the body states makes the callee's `requires` hold, or where the body's own named operator requirement answers the callee's (GEN-16); otherwise `requires.unmet` at the call in the body, naming the member that fails, and `generic.argument_constant` there where a generic argument worked out for a member fails. A call of the body is held to the body's own `requires`. Not told is never taken to hold: a constant this compiler does not work out is `requires.unmet` at a call that gives every argument. The instance walk (`instances_hold`) keeps what only an instance decides: a struct a type in the declaration applies (a field's `Only<FBox<X>>`), a generic argument worked out from the body's parameters, the candidate a membership selects, and the bound below. Only the calls that reach a `requires`, or a generic argument worked out from the body's own parameters are followed, each instance once however many calls make it, by its arguments' values, and no more than `hgl_analyser::INSTANCES` (4096) instances of one declaration from one call, with arguments no longer than 65536 characters written out: a call past that is `generic.instance_limit` (GEN-13: "Where the instances of one declaration never end when the program is compiled, the program rejects"), at the call that makes the first of them, which says the instances of that declaration do not end within the bound, as `grow<N * 2>` in `grow<const N: i64>` and `climb<N + 1>` in `climb<const N: i64>` make
- At a call of a function with a type pack, a false requirement or one the analyser cannot judge is `requires.unmet`. The diagnostic distinguishes the two (GEN-10).
- A known outer-type mismatch settles a binding `any` as false even when a nested generic remains unresolved; the call reports `requires.unmet` before the missing result binding (GEN-10). Negation and disjunction still contribute no binding.
- Left: every other form `requires` admits; binding equalities solved to a fixed point; a membership of a generic parameter of the declaration a call is written in, which is not told (GEN-7); a binding equality of a type parameter, `T == i64`, read as a membership of one and not as a binding; a call settled by a membership whose argument is read as its place's type and is no constant (`[x]`)

## GEN-4

parser: partial

- An operator declaration can begin with `export` (UNIT-3). A candidate takes no `export`.
- An operator declaration and its candidates can declare configuration parameters (FN-18).

analyser: partial

- An `impl fn` is a temporal candidate and an `impl const fn` a value candidate; each binds an operator its module declares, its file imports, or the prelude binds, and one that names no operator is `impl.unknown_operator`; a value candidate's body is read as a value function's
- An operator declared in a named test has that test in its identity. A test-declared candidate binds its operator in the test’s lexical scope and participates in selection throughout its module's test runs (TST-1).
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
- A fixed-length tuple pattern outranks a tuple pattern with a pack that admits it. The comparison also applies inside nested and nullable tuple types.
- Exact parameter matches are compared before structural specificity (GEN-12). A candidate that widens fewer arguments in this comparison beats one that widens those same arguments and another. Opposite widenings leave the call ambiguous. Widenings include `T?`, `delta<V?>` (TYPE-16), an inherited parent struct, and `any`.
- Keeping the value of an anonymous callback body with no written result counts as an exact match (FN-17). Proposed: the same comparison applies to a callback input shared with another argument, so the inferred input type beats its nullable widening. [Specification #257](https://github.com/hhenson/hgl_spec/pull/257)
- Left: when both candidates take an ordinary argument as `R?`, a plain `R` can leave their inferred callback inputs tied (#879).
- Operand forms are tried at integer precision before widening literals. A literal taken as `f64` is compared as `f64` for exact matching (EXPR-5). A nullable candidate result can meet a place for its payload under ERR-12. [Specification decision #182](https://github.com/hhenson/hgl_spec/pull/182)
- In a graph a call selects among temporal and value candidates. With the same parameter types, a temporal candidate is preferred for a call with a connection and a value candidate is preferred for an all-scalar call (FN-4, FN-6).
- In value code, a test's outside an `eval` or a `run` and an application's statements, a call selects among both too, a value candidate outranking a temporal one of the same parameter types; a temporal candidate it selects is `call.role`, the code proposed (TST-3, UNIT-14)
- A packed candidate matches each captured argument and checks the pack's bounds. Equally specific candidates stay tied when their admitted cardinalities overlap.
- Left: an argument that is itself a call only its requested output selects

transpiler: done

- Named and symbol calls use the selected candidate's module, declaration and instance to name its value function or temporal wiring. C++ sees the selected candidate's own name (`emit::names::placed`).

## GEN-6

analyser: partial

- In a value function a named call is matched against each value candidate's own signature
- Outside a generic body, literals in named and symbolic operator calls are fitted independently by each candidate, with or without a selecting option. A typed argument still fixes its generic in a requested result; another literal argument does not erase that type. Public generic defaults, result requirements and reflected-pack checks keep their complete inference; inherited configuration defaults use the fitted input type (GEN-2, GEN-8, GEN-13, UNIT-8). Generic-dependent literals use the selected instance's type (GEN-7); fully concrete calls lexically inside a generic declaration and generic configuration-default metadata retain their existing handling.
- Each value or temporal candidate supplies the function type of a fixed callback, including its phase (FN-17).
- An option that selects is a generic value parameter, given by name or by its default, and the candidate's `requires` is checked against its value; one that does not select is a parameter, by name or position
- An option's name is one kind across the operator's candidates in each view
  (TST-1). `check`, `test` and `build --tests` report `impl.option_kind` for
  conflicts in a module's test view. Plain `build` and `run` still diagnose
  test bodies. Their option-kind check uses the released candidate set,
  which excludes test-declared candidates.
- A candidate declares its operator's configuration parameters. Named and symbolic calls bind those parameters in braces, and a candidate may use the operator's default (FN-18, EXPR-10).
- Candidate-only options participate in candidate selection after the operator's public packs are reflected. A candidate whose required input narrows the operator's nullable input cannot inherit a written null default (UNIT-8).
- A value candidate may mark a parameter `var`, which takes no part in selecting it (FN-15)
- Candidates keep the operator's written parameter modifiers. Omitting `const` on a parameter or its callback type reports the proposed `impl.parameter` code where the candidate declares it. A candidate can add `const` to a parameter, and a value candidate can take `fn(...)` as `const fn(...)`.
- An option given a generic value parameter of the enclosing function, `step(v, mode: M)` in `go<const M: Mode>`, admits a candidate where the function's `requires` binds the parameter, `requires M == .slow` (GEN-3); where it states nothing of it, a default of it included, no candidate is known to admit the call, which is `operator.no_candidate` where the function is declared (GEN-7)
- A fixed parameter in an operator pack's place reports the proposed `impl.pack` code at the candidate parameter name. A pack with cardinality one is accepted. Bare Boolean requirements await #367.
- A candidate's own result or callback result can be absent when the operator permits a result-only generic to bind to none (GEN-1). A candidate whose result disagrees with its operator reports `impl.result` at the result type or at the candidate's name when it has no result.
- A fixed public result rejects a candidate that adds null or failure. A candidate may guarantee the value of a nullable or fallible result. A complete fallible place takes that value as a success. An independent output parameter can bind to the candidate's own result.
- The types and values bound by a candidate's declared parameters can show that the operator's `requires` does not hold. The candidate's name then reports `impl.requires`.
- The declaration check warns at each candidate when a positional call with a common concrete result leaves several candidates equally preferred. It checks fixed scalar slots and bare type parameters, including calls that omit trailing arguments with candidate or operator defaults, through the ordinary candidate matcher. Each admitted candidate's filled defaults supply the omitted input types. Public generic defaults are resolved before checking the public requirement. In slots where both declarations use bare type parameters, each trial uses one common type from `i64`, `f64`, `bool` and `str`. Requirement resolution uses the same production or test candidate view as the call (TST-1). A declaration may acknowledge its own warning (ERR-10).
- Left: other checks of a candidate's shape against its operator, the names and types of the operator's parameters and its generic relationships; tie witnesses that need named or configured inputs, structured input patterns, packs or dependent results, or test candidate views that reach the operator only through a selected candidate body; temporal candidate selection

## GEN-7

analyser: partial

- A structurally admitted fallback can prove a call in a generic body. A condition on an exact candidate can select that candidate when the instance provides the needed type.
- Inline pack-key passes are checked when an instance supplies its keys. Only the branch a constant condition selects is read. A failing pass reports at the call that made the instance and names the failing body and any nested instances (FN-19, CTRL-16).

- In a body with generic parameters, a call of an operator, by name or by symbol, with an argument whose type names one of them is admitted where a requirement of the declaration names that operator with those types, each given by its position or by the operator's parameter name, and is of the type its arrow states. A candidate that takes every admitted type can also admit the body.
- The call records the position of the requirement it is (`Callee::Required`), so that a call of the declaration says which candidate it calls
- "It is judged once, where it is declared, by what its `requires` states, and never again for the generic arguments a call of it gives": a call of an operator in it is admitted where a candidate takes every generic argument the requirements admit, where a named operator requirement states it, or under a membership where each listed type has one (GEN-3), and otherwise the declaration rejects (`operator.no_candidate`, with a note), by name and by symbol, in a value function and a graph: the fragments `less` and `wrap` with no `requires`
- Each instance selects again for an operator call whose operand types use the enclosing declaration's generic parameters, including when a generic candidate admitted the body. The new selection uses the instance's operands and the operator binding at the call (GEN-5), including private operators, symbols, and calls in nodes and graphs.
- An instance with no result reads an arrow body as a statement. An explicit `return` with a value reports `type.mismatch` at the call and names the instance (FN-7).
- The instance projects reflected pack-member types in operator operands and written generic arguments before checking and emitting the selected candidate (GEN-8, GEN-17).
- A positive equality or finite membership for a reflected member can prove an operator call at the declaration (GEN-3, GEN-7, GEN-17). An equality can use the finite types stated for the member or the type parameter it names. Explicitly equated keys take the same trial type. Independent keys take independent trial types. Each instance selects using the actual reflected member.
- A nullable's delta retains the identity of an inner collection delta in reflected membership bounds. A list and `delta<delta<list<i64>>?>` are disjoint (TYPE-16).
- The same finite evidence proves a callee's `requires` at a generic function call, including membership stated with `each` over a reflected pack's keys. Positive bounds on the same reflected key intersect regardless of conjunction order. A reflected member and all type parameters equated to that key must satisfy their finite memberships, including when the callee uses only one of the parameters. Each common type must admit the call. Empty overlap supplies no proof. An existential, an inequality, an equality between unknown members, or evidence that holds on only one side of `||` does not by itself prove the member type. An unproved callee requirement reports `requires.unmet` at the declaration (GEN-7, GEN-10, GEN-17).
- A named operator checks written arguments in their function slots beside a computed spread. Pack cardinality and candidate selection, including a braced selecting option, use the completed entries in each instance (FN-17, FN-19, GEN-6).
- Repeated positive memberships and concrete equalities on an ordinary parameter intersect. Positive equality chains between parameters and specific reflected keys share one finite set of types. A trial gives the same choice to all equated slots used by the call. An empty domain supplies no evidence for a call using that parameter or member. Other calls can use their own finite domains.
- A positive `key in arg_keys(Ps)` supplies a member to which the declaration’s universal bounds for `Ps` apply. Those bounds can narrow an equated ordinary parameter. A universal alone also admits an empty pack.
- Left: finite sets containing open type parameters and reflected evidence in generic function-call replay (#618).
- The covered function, node and graph calls fit integer, float and negative literals to each selected instance's candidate, with or without an operator option. Membership admission records a provisional result type; instance rechecks reach consumers that the declaration already admits with the literal's provisional `i64`. The supplied sum of calls using a typed seed and a literal seed gives `4.5` in its `f64` instance. Literal-free membership-admitted operator chains retain their result consumers too. A contextual result precedes input inference (GEN-2, EXPR-5). Unsupported or unstated memberships still reject the declaration.
- Left: a literal-call result used as another call's argument, or multiplied by another literal-call result, can still reject where the generic body is declared although each admitted instance has fitting candidates (#734).
- Instance checks reach named function references, calls in anonymous bodies, and concrete calls produced by pack forwarding, including forwarding reached through a reference. A forwarding instance is read with the ordinary const inputs its body reads at compile time as well as its entries, so a const pattern given beside packs is checked there (FN-1, FMT-3, #706). A concrete body given const inputs it reads is followed as an instance of its own, and so is a candidate a deferred operator call in a walked instance can select. A call given a pack entry not yet read makes no instance where the forwarding body is declared; each instance finds it with the entry's type. An instance two calls make, one through another instance, is reported once at each. They enforce a selected candidate's `var` place (FN-15), `const` argument (FN-1, FN-3), and configuration (EXPR-14). A rejection keeps its ordinary code at the call that makes the instance, with the instance and failing body call named in related sites. Nested instances report at each outermost making call and name each instance in the chain.
- An instance selects with the call's written or contextual output request. It rechecks uses when the selected result changes type, including inferred locals, calls, indexing and loops. Reflected results use the instance's concrete pack members before checking their consumers. A failed use retains its ordinary diagnostic code and gives the selected operator call as the failing site in its body (GEN-7).
- A generic body reads a fixed list's inferred size `N` as an `i64` constant (GEN-13, TYPE-10). The C++ target executes it in arithmetic, range bounds, subscripts and local initializers in value code, node handlers and graph composition.
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
- Each domain in a clause gets its laws and expects its own identity type (EXPR-5). A repeated domain in one or more clauses, or a repeated property in one clause, rejects at the later domain or property (`property.duplicate`).
- Law queries read the public operator's metadata for the concrete operand type; an ordinary or anonymous function declares no guarantee. Metadata performs no arithmetic transformation.

optimiser: planned

- A recorded law licenses no reassociation across overflow or float effects

## GEN-10

parser: partial

- A cardinality after a pack parameter or a generic pack parameter; `each` and `any` quantifiers. `any` remains a scalar type outside the quantifier form.

analyser: partial

- `tuple<Ts>` captures and expands a type pack in element order, including nested types, explicit arguments, cardinalities and repeated captures. Ordinary type parameters remain one element even when their type is itself a tuple. Tuple captures share the existing specialization path with ordinary calls and independently typed callbacks. Forwarded sequences retain their splice position among fixed tuple elements, and must prove the callee's category requirements under the wrapper's stated bounds. Every independent source pack survives specialization. A decisive existential witness or false conjunction needs a guaranteed element: declared bounds and an exact `tuple<Us>` parameter can supply that minimum; a pack beside fixed tuple elements cannot. Ordinary value-pack forwarding remains checked in the concrete instance (GEN-7). Tuple indices select a guaranteed member by its pack key, never a sequence marker; positions whose member cannot be determined for every admitted length reject at the declaration. The same effective minimum proves call cardinality. Tuple partition equations reuse settled argument types to a fixed point, tuple operands expand in requirements, and a pack cannot stand beneath an element's `?`.
- A cardinality bounds the arguments a generic type's pack takes, and has its upper bound at least its lower
- Function and operator calls check the cardinality of captured arguments against the value parameter and its generic type pack. Explicit positional type sequences also constrain the argument count. The HIR records the resolved sequence of types for each captured type pack.
- A function's `each T in types(Ts)` checks its condition for every resolved type in the pack. An empty capture satisfies it. Equality and membership can test the bound type by name. A verdict the analyser cannot give rejects as untold (GEN-3).
- GEN-17 adds quantification over pack keys. `any` succeeds for at least one entry and rejects an empty pack. Type and key bindings project within nested type expressions.
- An `any` equality can infer a const generic nested in a type, such as a fixed list length, from either equality operand; an explicitly supplied value is preserved (GEN-3).
- Tuple candidate ranking retains the declared tuple form and compares components using declared variables for captured entries (GEN-5, GEN-12). Explicit callback packs keep their supplied function types, and a template mismatch is reported at the argument (FN-17, GEN-8).
- In a declaration contract, a homogeneous positional candidate pack with a known nonempty arity supplies the elements of a public tuple-pack result (GEN-6, GEN-10). Concrete result mismatches use `impl.result`.
- Guaranteed indexed tuple members retain their pack's universal category facts only where the effective minimum guarantees that key. Exact tuple-pack occurrences inside parameter containers also prove a positive minimum. Tuple equations are retained inside lists, dictionary values, nullable types and nested tuples without changing argument reading.
- Left: key-preserving universal finite-membership and named-call inheritance for indexed members (#723). The direct reflected-member operator proof uses separate alternatives per key (GEN-7).
- Left: positive existential constraints do not yet prove a nonempty pack for tuple indexing (#724).
- A named operator requirement under `each` or `any` checks the value candidates for the current concrete type or key, including nested written types (GEN-16). The candidate's return value does not determine the verdict.
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
- A callback result explicitly absent is more specific than one that can bind to none (GEN-1), except when GEN-5 prefers keeping the value of an anonymous callback's body. The inputs still take part in the comparison.
- A `ref` mark adds no priority: candidates are compared by their types, which hold none (FN-16)
- Left: a repeated variable counting once; temporal bodies

## GEN-13

analyser: partial

- Type constants share canonical identity for equality and collection keys; named declarations remain distinct across modules and constant argument spellings normalize (TYPE-18).
- Local, same-module call/constructor, and module-constant `type(...)` expressions work as generic value arguments on functions, operators, and structs, and emit on the C++ target via structural `hglrt::type_tag` template arguments (TYPE-18).

- A plain parameter is a type or a value by its uses in the signature; uses that disagree are rejected, and a struct's parameter nothing uses; a value parameter or a pack is not named as a type; a generic value parameter's default, a callable's or a struct's, is of its declared type, as far as how the default is written says what it is, and of the type a call resolves where that names a type parameter: an implied member, `.slow`, is held to the enum the call resolves, so is no `i64` and no member of an enum that lacks it (`type.mismatch`, TYPE-7)
- A default that names a generic parameter before its own is of what that one takes, at a call and in a struct written with no arguments (`W2` of `<const A: i64 = 1, const B: i64 = A>` is `W2<1, 1>`); one that names its own, or one after it, gives it no value, so a call that leaves it is `generic.unresolved` (GEN-8); a call in the function's own body that gives a parameter its own, `rec<N>(v)`, gives the default that one and a struct written with no arguments `generic.argument_count`
- Generic arguments that are the same, each type canonical and each value one constant (UNIT-13), make one type or one instance, and arguments that differ another: of the worked fragment, `Scaled<1>`, `Scaled<1.0>` and `Scaled<1.00>` are one type, and a `Scaled<-0.0>` where a `Scaled<0.0>` is expected is `type.mismatch`
- A result-only parameter bound to none denotes no result and identifies a separate instance (GEN-1).
- Calls and symbolic operations whose candidates become known in a graph instance contribute to the same bounded traversal of instances and their requirements (GEN-3, GEN-7).
- "Where the instances of one declaration never end when the program is compiled, the program rejects": no more than `hgl_analyser::INSTANCES` (4096) instances of one declaration are followed from one call, by the analyser and by `hgl_lower`, and a call past that is `generic.instance_limit`, at the call that makes the first of them: the worked fragment's `climbed`; through constants worked out from the body's own parameters, and through a type that grows, `nest<list<T>>`
- The forwarding walk uses the same argument size and nesting limits. It also reports `generic.instance_limit` at the initiating call after more than 64 increases in the pack width of one declaration along a path.
- A const input that grows through a forwarded call, `grow(p + "x", ...args)`, makes a new forwarding instance at each call, and the walk reports `generic.instance_limit` at the initiating call past the same instance bound, or where its text passes the argument size (#706). A walk that has followed an instance for a call does not follow it again for that call, and a call where any walk meets the bound is reported once.
- Left: a default that names anything else; the kind a constraint gives; `type(N)`; a call that supplies no type for a function's parameter its signature does not settle

## GEN-14

parser: done

- `<` after a name read as generic arguments or comparison; marked undecided when both parse

analyser: partial

- Decides an undecided constructor from what its name resolves to, a generic type, function or operator; a generic type before a comparison's `<` rejected; each argument against its parameter's kind
- A contextual `Type` argument supplies the descriptor in a known type slot and uses ordinary value lookup in a value slot. Callable signatures and constraints settle their declared slots before call arguments are read, including operator and pack slots. Where the call supplies the kind, module/imported constants and local value bindings take precedence over the descriptor fallback. Value-only lookup diagnostics are retained only on the value path; GEN-8 still requires a constant.
- With no value named `Type`, a known value slot rejects the descriptor as the wrong argument kind, including struct annotations and constructors.
- Function pairs defer the contextual spelling until the existing FN-4/6 role selection supplies the receiving signature. Declaration order cannot choose a type or value interpretation.
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
- Reads `key in arg_keys(Ps)` at comparison precedence in a body.
- Left: pack-key iteration outside the computed spread slice (#368).

analyser: partial

- Reflects the selected callback's fixed suffix and its supplied positional and named pack entries. A dependent pack keeps the callback parameter keys and the types written at the call.
- Evaluates `each` over `arg_keys`, constant key membership, and keyed type projection. At a concrete call, equality and membership inside `each` or `any` use the current key in a written type's value arguments, including `Field<k>` (GEN-3, GEN-10). Candidate requirements can compare bare pack members. A missing member of a known pack reports `pack.name`.
- In a pack comprehension, `arg_keys` names a declared type pack and the iteration name is a compile-time key. In a concrete inline spread, a membership condition tests that pass's key against the resolved type pack and gives a constant `bool` (GEN-7).
- A generic call that receives another function parameter checks its pack requirements when the caller supplies the reflected types. The receiver's fixed positions determine which original inputs its pack reflects.
- An operator callback reflects the public declaration's parameter names and the selected candidate's types (FN-17).
- An inherited configuration default retains the public operator's reflected pack keys, fixed prefix and forwarding metadata, separately from the selected candidate's packs (GEN-6, TYPE-18).
- Specializes a pack-member type written as a function call's generic argument (GEN-8), including within another type. A missing key reports `pack.name`. A positive member-type equality, including one stated for each key of a pack, can admit a constrained call under GEN-7. A call with an untold member type remains unsupported in the target.

## GEN-18

parser: done

- Reads law-predicate calls and bare Boolean generic terms, including their negations and Boolean combinations.

analyser: done

- `associative(f)` and `commutative(f)` query a supplied function's public operator and concrete domain, in requirements and bool generic defaults. Explicit options override defaults.
- A built-in law requirement must name a formal parameter that takes a function; other operands reject at declaration. An operator bound to the same callee name retains ordinary GEN-16 callability checking, including through a selective import, independently of its bool result.
- A wrapper is checked at declaration under its own stated positive or negative law (GEN-7). With no guarantee it rejects even if two candidates cover both values. A bool generic explicitly given onward under a membership in `{true, false}` selects its candidate in each instance (GEN-3). The corrected #367 ruling is tested.

- C++ receives the already-resolved bool options and selected candidates; this rule needs no runtime law query or target change.
