# Errors and diagnostics

`errors-and-diagnostics.md`

## ERR-1

diagnostics: done

- Every diagnostic has a category and a code; a warning has its code alone, is listed apart from the source codes, and does not reject

transpiler: done

- Reports escaping failures with their code and HGL statement location. Test assertions,
  unsupported target forms and ordinary result data have separate outcomes.

## ERR-2

analyser: partial

- `ok(value)` and `err(value)` take one value by position and no generic argument (`generic.argument_count`), and both types from the complete `result<T, E>` the place expects, a nullable one included; with none it is `result.context_type`
- The payload is read as its side's type. This includes a named function argument whose result parameter type is completed by another argument in the graph instance (GEN-2, GEN-7). An inferred constructor field also keeps its scoped payload
  until an earlier field completes the result type.
- Left: a result expected through a generic parameter still being inferred; equality

transpiler: done

- Emits tagged successes and errors with one evaluated payload. A temporal result is one
  publication of that tagged value.

## ERR-3

transpiler: done

- Emits the result tag test and the corresponding success or error payload read after
  the analyser proves the tag.

analyser: partial

- `is_ok` and `is_err` take one result by position and give a `bool`. A graph instance checks the argument when its type first settles (GEN-7).
- An instance checks a direct tag test or a test of an inferred `let` against the operator's selected result. A written result place still guides selection (GEN-5, GEN-7).
- A direct guard on an immutable result binding proves its tag: in the branch it holds for, across `!` and the Boolean connectives, and after a branch that returns
- `binding.value` and `binding.error` need the matching fact (`result.required_variant`), and have the T and the E
- A tag proof for a child held by an inferred `let` applies to that binding. The read still has the child admission and validity checks (ERR-14, TIME-1).
- In a graph a test of a connection gives no fact. A deferred projection checks its
  saved lexical fact against the settled category of the result (GEN-7).
- Left: a fact carried through a `switch` or a loop

## ERR-4

analyser: done

- `error(message: text, code: optional_code)`: each by its name, `message` a `str` and required, `code` a `str?`; `.message` is a `str` and `.code` a `str?`
- A graph instance refits newly known `message` and `code` values, including any required read of the message (GEN-7, ERR-12).
- `.context` is an `error context?`, a type of this compiler's, whose `.node` and `.phase` are a `str` and `.evaluation_time` a `datetime`, read under ERR-12 and never assigned (`binding.immutable`)

transpiler: partial

- Emits error construction, message and code reads, equality and nullable provenance.
  The runtime retains the first failure location on propagation.
- Left: An error property outside the supported HIR forms is reported. Construction
  provenance is tracked separately under CRUN-6.

## ERR-5

analyser: partial

- A place that needs the value of a result records where a failure goes. A value function returning a result requires the same error type. In other phases only `error` can be raised (`result.propagation_type`).

transpiler: partial

- C++ returns the original error from a fallible value function. In a `when`, `start`,
  `stop` or source body, at graph build, and in test or application statements, it
  raises the error at the HGL read.
- Left: A propagation that needs statements in a loop condition, default or value-only
  expression is reported. Ordinary fallible computation inside a wiring-time branch body
  is also reported (CTRL-2). Unsupported placements identify the recorded raise phase.

## ERR-6

analyser: partial

- Brackets read an ordinary dictionary by its key. Core's two-argument `get` gives `result<V, error>` and the place selects its form (ERR-15).
- Checking a fallback against the exact dictionary value type (`lookup.default_type`) is pending. A literal `null` fallback selects the plain read.

transpiler: partial

- C++ brackets perform a strict read. Two-argument `get` returns the whole result to the
  recorded ERR-15 form. A temporal dictionary child is an endpoint read (ERR-14).
- Left: Evaluation of the fallback on a key hit and preservation of a nullable fallback
  value are pending.

## ERR-7

analyser: partial

- `capture(callable, args...)` in a graph reads the call it names, from the same arguments less the name, as that call is read; in a value function or a hook, of what names no callable, and of a call that composes nothing it is `capture.phase`
- Bound with `let`, `.value` is the call's output and `.error` an `error`, both connections; `.value` of an outputless call is `capture.output_field`
- Left: a capture used other than through a `let` and its two connections is not judged

transpiler: partial

- Wires an error boundary around a resolved temporal function, a temporal
  HGL operator candidate, or a lifted helper and exposes its value and error
  connections.
- Left: A target with no supported call or recording form, an unavailable packed
  instance and other capture reads are reported.

## ERR-8

transpiler: partial

- The capture emits the original failure event and keeps the child live for later work.
  Start and stop failures escape; re-raising preserves the first provenance.
- Left: Capture support follows the callable and publication forms listed under ERR-7
  and TYPE-9.

## ERR-9

analyser: partial

- A type that is not known is reported once, and nothing is reported of its uses
- A module constant whose evaluation fails is rejected where it is declared (`constant.failure`, UNIT-13), and nothing is reported of its uses
- Known required-constant failures in parameter and field defaults use `constant.failure` (UNIT-8)
- A generic value argument a generic body works out from its own parameters, `times<(M * 2)>`, whose evaluation fails, or that this compiler does not work out, with what an instance gives them is `generic.argument_constant` at the call that makes the instance, as the same argument written in place is, and the callee's `requires` is then not told ("Failure during argument evaluation precedes later admission checks"); one fault is reported once at each such call
- Left: required failures the constant reader cannot establish, including calls it cannot evaluate (#240); generic value arguments retain `generic.argument_constant`

optimiser: planned

- Folding keeps the failure phase of an executed operation

## ERR-10

parser: done

- `@suppress_warning("...")` before a named function

analyser: partial

- `@suppress_warning("node.no_activation")` before a function acknowledges its dormant-node warning
- Left: a string that is no warning's code; the warning `statement.no_effect`

## ERR-11

transpiler: done

- Uses native numeric outcomes for infinities, NaNs, unordered comparisons and checked
  result data. Only an escaping failure fails execution (NUM-3, NUM-6, NUM-10).

## ERR-12

analyser: done

- In a value function, a hook and a graph a direct test against null refines an immutable nullable parameter, `let` or loop binding to its payload's type: in the branch the test holds for, across `!` and the Boolean connectives, and after a branch that returns
- The proof of an inferred nullable child applies to value reads of that alias and retains the child's separate admission check (ERR-14, TIME-1).
- A generic graph keeps a non-null proof for an ordinary binding and fills incomplete payload types, including in reads that already record a possible null failure (GEN-7).
- A nullable read in a place that expects its payload records a possible `value.null` failure and its phase. A written null still needs a nullable type (TYPE-14).

transpiler: done

- C++ reads the payload at the recorded expression and reports `value.null` in its recorded phase. A null publication on a required connection fails its consumer that cycle, before handler or lifted-call admission. This includes an input bound through a pack.
- A refined connection in a temporal branch projects its present payload. That projection follows its source's validity when the branch changes.

## ERR-13

parser: done

- The result forms use calls: `ok(value)`, `is_err(value)`, and `capture(callable, args)` among them.

analyser: done

- `ok`, `err`, `is_ok`, `is_err` and `capture` as the callee of a call are the forms, looked up nowhere; no declaration, import alias, parameter or local takes one of the five names (`name.duplicate`)

## ERR-14

analyser: done

- An inferred `let` given a child holds the endpoint. Endpoint operations
  name it and a value use reads it under TIME-1 admission. A hook's `var`
  reads the child at initialization. A graph's `var` given a child denotes
  its connection (EXPR-4).
- A runtime-checked child read, or an admitted whole read of a dictionary, an unsized list or a window, gives T, `result<T, error>`, or `T?` according to its place. The nullable form discards the error as ERR-15 does.
- Comparing a runtime-checked child with `null` takes the nullable form when its concrete type is nonnullable. If the child has a nullable type, the comparison reads its payload and an unset child still fails. An operator candidate can keep the whole read result, including from a child bound in a generic hook (GEN-7, ERR-15).
- `delta_value` takes the same forms. TYPE-11 restricts a duration-window delta to a result or nullable place, and `removed_value` to a result place for every window.

transpiler: partial

- Holds the position or key of an inferred child binding through pack expansion
  and static value-function captures (FN-8, FN-14, FN-17). A payload read happens at
  the use, and a failed plain read names the child and reports the use's HGL location. A child bound
  inside a value helper borrows its captured parent's endpoint.
- Writes checked child, dictionary, unsized-list and window reads in the analysed
  value, result or nullable form. A dictionary omits wholly unset entries and a
  partial valid entry fails its whole read.
- Left: Deltas and window reads outside the supported endpoint forms are
  reported (END-9). List children and nested dictionary entries have the
  EXPR-7 limits.

## ERR-15

analyser: partial

- A call or held result keeps its tag in a result place, gives its success or raises in a T place, and gives its success or null in a `T?` place. An unannotated `let` keeps the result. A nullable success type is itself the T form.
- A `result<U?, error>` given where a U is expected raises its original error on failure and reads the success under ERR-12. The owner confirmed the two reads while the specification update was in review. [Compiler issue #345](https://github.com/hhenson/hgl-compiler/issues/345#issuecomment-7060418814)
- In composition the analyser records whether a conversion belongs to the call's node or to a reader of a held connection. A field projection waits for an incomplete result type, then takes the success before reading the field (GEN-7, EXPR-7).
- A candidate keeps operands whole where possible. Equal minimum arrangements of result reads are `operator.ambiguous`. [Compiler issue #356](https://github.com/hhenson/hgl-compiler/issues/356)
- Among admitted operator candidates, fewer result reads take precedence over GEN-5 ranking. Equal counts use normal candidate ranking. [Compiler issue #429](https://github.com/hhenson/hgl-compiler/issues/429)
- A checked child supplies its payload type to ordinary parameter inference. A candidate with a result slot can keep the whole read, and ranking counts each checked child consumed as an ordinary operand (ERR-14).

transpiler: done

- C++ follows the recorded whole, success or nullable form. A lifted call or node publishes the chosen form. A held failure taken as a value fails on its publication cycle, before the consumer runs a handler. Consumers that need child tracking use a projected endpoint. [Compiler issue #355](https://github.com/hhenson/hgl-compiler/issues/355)
- The SDK cases check that generic `i64` and `f64` instances read checked division into a nullable return in value functions, node handlers and graphs. [Compiler issue #465](https://github.com/hhenson/hgl-compiler/issues/465)
