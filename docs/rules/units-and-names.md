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

- Private unless exported, a native function and a native type among them; operators public; `impl fn` reached through its operator; import cycles rejected
- Left: export closure

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
- Left: the closure an imported default keeps

## UNIT-8

analyser: partial

- Any parameter but a pack may have a default; a pack's is rejected under FN-8
- A parameter that may be omitted follows every one that may not, and a value function's call may omit one written `T?`
- A default is read as a value of its parameter's type (`type.mismatch`) and kept for the stages after, `Module::defaults`
- Left: that a default is a required constant; the default of a window, an atomic or a reference, which is held to no type; a default whose parameter's type names a generic parameter

transpiler: partial

- hgraph: a call that leaves out a parameter with a default is written with the default in its place, for a value function and for a `const` parameter of a temporal one
- A temporal input written `T?` and left out is bound to hgraph's `nothing` of its type, never valid; a `signal?` to one of a `bool`
- Left: a temporal input's default, wired as a constant

## UNIT-9

analyser: planned

- Inherited defaults compared; a conflict needs an override

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
- It is read by its name wherever a value is, and is a constant expression (FN-1) and a compile-time constant (GEN-8)
- Where a size or a generic value argument needs its value, an `i64` or a `duration` written with literals, other constants and `+`, `-`, `*` is followed to it
- One whose value depends on itself is `constant.cycle`: through another constant, the definition a call selected, an operator's candidate a symbol or a name selected, or a default a call leaves to be read
- Left: the value of any other constant where one is needed; a `switch` label's value; a struct's field default counts for a construction that gives the field

transpiler: done

- hgraph: a constant is a function of no parameter that the provider runs once, and a read is a call of it, in value code, in a graph and from another module

## UNIT-14

parser: done

- `application_decl`: a file is headed `module name` or `application name`
- `run_statement`: `run(...)` or `eval(...)` where an item starts, with the arguments of an eval
- `store_statement`: `inject ...`, or a name and a call's arguments and no more, where an item starts

analyser: done

- An application is a unit of its own, holding what a module holds
- It exports nothing (`application.export`), no `use` names one (`application.import`), and it has one run statement (`application.run_count`), which follows its declarations and stands in no module (`run.position`)
- Between its declarations and its run statement it sets the run's store: `inject global_state`, and then calls of core's `put` on it, read together as the statements of one block are
- Any other capability or call there, one of these after the run statement or before a declaration, and either in a module, is `application.statement`
- Its path is no other file's: the first file of the path is the unit and each later one is reported (`application.duplicate_path`)

transpiler: done

- hgraph: what an application declares is written as a module's is, and its run statement after it as a function, `_application`, that runs its entry
- What sets the run's store is a function of the store, `_application_store`, which the run calls before it builds its graph
- An application that needs a form not written yet is blocked by it, as a test is
