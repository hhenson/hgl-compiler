# Units and names

`units-and-names.md`

## UNIT-1

parser: done

- One `module` line, with an optional `part`, opens a file

analyser: done

- Assembles a module's files: one scope, one unnamed file, unique part names

## UNIT-2

analyser: done

- Imports precede declarations and bind in their file; they and `alias::name` reach public declarations only

## UNIT-3

analyser: partial

- Private unless exported, a native function and a native type among them; operators still public; `impl fn` reached through its operator; import cycles rejected
- Left: export closure and private operators (#364).

## UNIT-4

analyser: done

- Module-qualified nominal identity; an alias adds none

## UNIT-5

analyser: done

- Duplicate functions of one role, parameters, parts and locals rejected

## UNIT-6

analyser: done

- Forward reference to declarations; locals visible after declaration; test-only declarations kept out of production

## UNIT-7

analyser: partial

- A default reads no parameter of its declaration and no field of its struct
- Left: the closure an imported default keeps; defaults that read a function's laws (#367).

## UNIT-8

analyser: partial

- Any parameter but a pack may have a default; a pack's is rejected under FN-8
- A parameter that may be omitted follows every one that may not, and a value function's call may omit one written `T?`
- A default is read as a value of its parameter's type (`type.mismatch`) and kept for the stages after, `Module::defaults`
- Configuration parameter ordering awaits #363, and law predicates in defaults await #367.
- Left: that a default is a required constant; the default of a window, an atomic or a reference, which is held to no type; a default whose parameter's type names a generic parameter

transpiler: planned

## UNIT-9

analyser: planned

- Inherited defaults compared; a conflict needs an override
- Compared by bound expression and canonical type, a literal by its value however it is written (UNIT-13): `= 1` and `= 1.0` of an `f64` field are one default
- Not built: two parents' defaults of one field are not compared, so a struct that has the field from both is accepted whatever they are, `= 1` beside `= 1.0` as one default and `= 1` beside `= 2` too

## UNIT-10

analyser: partial

- Every name in a signature, body, type and constraint resolved to its one binding, and one that nothing binds is `name.unknown`
- Left: capability receivers; activation forms; `types(Ts)` and `schemas(pack)`, which are forms no pass reads yet and are not reported

## UNIT-11

analyser: done

- When the program has the module `core`, a name nothing nearer binds is its public declaration of that name
- `hgl` supplies the pinned core's module to a program whose source roots hold none
- A name neither the program nor core binds is UNIT-10's to reject

## UNIT-12

analyser: done

- No declaration, import, local, parameter or generic parameter takes a name already in scope, a prelude name included for a declaration, an import alias and a local

## UNIT-13

parser: done

- `constant_decl`: `const name: T = expression`, with `export` before it
- `const` before a name begins a constant, and before `fn` a function
- The type and the value are both written, the value on the declaration's line

analyser: partial

- A constant has the type it is written with, an ordinary value type, and its expression is read as a value of it with no parameter in scope
- It is read by its name wherever a value is, and is a constant expression (FN-1) and a compile-time constant (GEN-8); in a `requires` compared with a generic value parameter or an option, `N == C`, `C == N`, `N in {C, 5}`, `mode == FAST`, it is that value and no type is read there (beside a type parameter, `T == C`, it is still `type.not_a_type`)
- Where its value is needed, a size, a generic value argument, a `requires` operand, a default, a `switch` label, a delta's key, a tuple's index, an enum's number, it is followed to it by the one reader (`constant`), through the constants it names, in its expression and its type, each worked out once as a value of its own type: an `i64` constant is no `f64` (EXPR-5)
- One computed by a call is held by its name, the same as itself and known beside no other
- Two are one where their values are the same, however each is written (`ConstantValue::same`): `1`, `1.0` and `1.00` of an `f64`, `60s` and `1m`, two `datetime`s at one instant; `-0.0` and `0.0` are two, though `-0.0 == 0.0`, and any two NaNs one, `0.0 / 0.0` among them. That sameness is a type's identity (TYPE-3) and a call's instance (GEN-13); an equality the program writes, in a `requires` as elsewhere, is the language's `==` (`ConstantValue::equals`, GEN-3): `Flag<(-0.0 == 0.0)>` is `Flag<true>`
- How one is written is kept beside its value (`Constant::text`), for a message, and is never compared
- One whose evaluation fails, an `i64` out of range or a division of two `i64` by zero, is local `constant.failure` where it is declared ("a failure in evaluating it is a required-constant failure (ERR-9)"), and says nothing more where it is read
- One the reader does not work out, a datetime moved by a duration, is no value a generic value argument is given (`generic.argument_constant`), as the same expression written in place is not
- An expression is read in a loop along a chain of operators, `1 + 1 + ... + 1`, of any length the parser builds a tree for
- One whose value depends on itself is `constant.cycle`: through another constant, the definition a call selected, an operator's candidate a symbol or a name selected, or a default a call leaves to be read
- Left: the value of a constant computed by a `const fn`; a struct's field default counts for a construction that gives the field; a formatter's writing a constant back as written, as there is no formatter yet

transpiler: planned

## UNIT-14

parser: done

- `application_decl`: a file is headed `module name` or `application name`
- `run_statement`: `run` or `eval` where an item starts, with an eval's configuration and call
- `store_statement`: `inject ...`, or a name and a call's arguments and no more, where an item starts

analyser: done

- An application is a unit of its own, holding what a module holds
- It exports nothing (`application.export`), no `use` names one (`application.import`), and it has one run statement (`application.run_count`), which follows its declarations and stands in no module (`run.position`)
- Between its declarations and its run statement it injects `run_config`, giving its run's configuration defaults, and sets the run's store: `inject global_state`, and then calls of core's `put` on it, read together as the statements of one block are; a value given to `put` may read `run_config` (RUN-6)
- Any other capability or call there, one of these after the run statement or before a declaration, and either in a module, is `application.statement`
- Those statements, the defaults among them, and the arguments of the run statement's call are value code: a call of a node, a graph or a source there is `call.role`, the code proposed, an operator selects among all its candidates and a temporal one selected is `call.role` too (GEN-5), and nothing is lifted; the call of the entry is composition (RUN-1)
- The entry arguments can read an earlier `inject run_config` (RUN-6).
- Its path is no other file's: the first file of the path is the unit and each later one is reported (`application.duplicate_path`)

transpiler: planned
