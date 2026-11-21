# Source documentation

`source-documentation.md`

## SDOC-1

parser: done

- A documentation comment attaches to the declaration after it; repeated or unattached ones rejected

## SDOC-2

docs: unscheduled

- Documentation text: delimiters, indentation, line breaks

## SDOC-3

analyser: partial

- A key of an `Args:` or a `Type Args:` section, a name and a `:` four spaces in once SDOC-2's common indentation is removed, names a parameter or a generic parameter of what the comment documents; a declaration with none has none to name (`doc.unknown_key`)
- Left: `Requires:` and `Properties:` keys, which the chapter does not say how to write; `Args:` of a struct, whose fields are its construction's arguments

docs: unscheduled

- Section headings and their contents

## SDOC-4

docs: unscheduled

- Interface text kept with its identity, signature and part

## SDOC-5

formatter: planned

- Formatting keeps meaning, comments and line-break form, and is idempotent

## SDOC-6

analyser: done

- Documentation is recorded nowhere in the analysed program: two programs that differ in it alone analyse alike
