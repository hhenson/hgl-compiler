# Errors and diagnostics

`errors-and-diagnostics.md`

## ERR-1

diagnostics: done

- Every diagnostic has a category and a code; a warning has its code alone, is listed apart from the source codes, and does not reject

transpiler: planned

## ERR-2

analyser: partial

- `ok(value)` and `err(value)` take one value by position and no generic argument (`generic.argument_count`), and both types from the complete `result<T, E>` the place expects, a nullable one included; with none it is `result.context_type`
- The payload is read as its side's type
- Left: a result expected through a generic parameter still being inferred; equality

transpiler: planned

## ERR-3

transpiler: planned

analyser: partial

- `is_ok` and `is_err` take one result by position and give a `bool`
- A direct guard on an immutable result binding proves its tag: in the branch it holds for, across `!` and the Boolean connectives, and after a branch that returns
- `binding.value` and `binding.error` need the matching fact (`result.required_variant`), and have the T and the E
- In a graph a test of a connection gives no fact
- Left: a fact carried through a `switch` or a loop

## ERR-4

analyser: done

- `error(message: text, code: optional_code)`: each by its name, `message` a `str` and required, `code` a `str?`; `.message` is a `str` and `.code` a `str?`
- `.context` is an `error context?`, a type of this compiler's, whose `.node` and `.phase` are a `str` and `.evaluation_time` a `datetime`, read where a null test proves it present (`nullable.required_presence`) and never assigned (`binding.immutable`)

transpiler: planned

## ERR-5

analyser: partial

- `value?` has the T of a `result<T, E>`
- In a value function the enclosing result is a `result<U, E>` of the same E, and in a `when` the E is `error` (`result.propagation_type`, which what is no result, a nullable included, is too); in a graph, `start`, `stop`, a node's declarations and a source it is `result.propagation_phase`
- Left: an anonymous function

transpiler: planned

## ERR-6

analyser: partial

- A list is indexed by an `i64` and a dictionary by its key, for the element or the value
- `get(book, key)` is core's candidate, a `V?`
- Left: `at`, and `get` with a fallback (`lookup.default_type`, `nullable.required_presence`)

transpiler: planned

## ERR-7

analyser: partial

- `capture(callable, args...)` in a graph reads the call it names, from the same arguments less the name, as that call is read; in a value function or a hook, of what names no callable, and of a call that composes nothing it is `capture.phase`
- Bound with `let`, `.value` is the call's output and `.error` an `error`, both connections; `.value` of an outputless call is `capture.output_field`
- Left: a capture used other than through a `let` and its two connections is not judged

transpiler: planned

## ERR-8

transpiler: planned

## ERR-9

analyser: partial

- A type that is not known is reported once, and nothing is reported of its uses
- A module constant whose evaluation fails is rejected where it is declared (local `constant.failure`, UNIT-13), and nothing is reported of its uses
- A generic value argument a generic body works out from its own parameters, `times<(M * 2)>`, whose evaluation fails, or that this compiler does not work out, with what an instance gives them is `generic.argument_constant` at the call that makes the instance, as the same argument written in place is, and the callee's `requires` is then not told ("Failure during argument evaluation precedes later admission checks"); one fault is reported once at each such call
- Left: other required constant failures, a generic value argument's or a default's written in place, which is `generic.argument_constant`

optimiser: planned

- Folding keeps the failure phase of an executed operation

## ERR-10

parser: done

- `@suppress_warning("...")` before a named function

analyser: partial

- `@suppress_warning("node.no_activation")` before a function acknowledges its dormant-node warning
- Left: a string that is no warning's code; the warning `statement.no_effect`

## ERR-11

transpiler: planned

## ERR-12

analyser: done

- In a value function, a hook and a graph a direct test against null refines an immutable nullable parameter, `let` or loop binding to its payload's type: in the branch the test holds for, across `!` and the Boolean connectives, and after a branch that returns
- A nullable where its payload is required, as an operand no candidate takes, or read for a member or an element of its payload, is `nullable.required_presence`

## ERR-13

parser: done

- The result forms are calls of a name, and `?` a postfix suffix: `ok(value)`, `is_err(value)`, `capture(callable, args)`, `value?`

analyser: done

- `ok`, `err`, `is_ok`, `is_err` and `capture` as the callee of a call are the forms, looked up nowhere; no declaration, import alias, parameter or local takes one of the five names (`name.duplicate`)

## ERR-14

analyser: done

- A child read checked when it runs (`CheckedChild`) where the place expects `result<T, error>`, T the child's type, is of that result (EXPR-5); elsewhere it is the T
- So is a read of a window's value, of an input, `out` or a child named when compiled (`CheckedWindow`), where the place expects `result<list<V>, error>`, "T being the `list<V>` it reads as (TYPE-11)"
- So is `delta_value` of any endpoint, an input, `out` or a child, where the place expects `result<delta<T>, error>`, and core's `removed_value` of a window where it expects `result<V, error>` (`CheckedDelta`), the place saying what V is; where the place takes no `result` a window of a duration's delta and any `removed_value` are `payload.unguarded` (TYPE-11)

transpiler: planned

