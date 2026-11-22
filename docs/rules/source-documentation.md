# Source documentation

`source-documentation.md`

## SDOC-1

parser: done

- A documentation comment attaches to the declaration after it; repeated or unattached ones rejected

## SDOC-2

docs: done

- Documentation text: delimiters, indentation, line breaks

## SDOC-3

analyser: partial

- A key of an `Args:` or a `Type Args:` section, a name and a `:` four spaces in once SDOC-2's common indentation is removed, names a parameter or a generic parameter of what the comment documents; a declaration with none has none to name (`doc.unknown_key`)
- Left: `Requires:` and `Properties:` keys, which the chapter does not say how to write; `Args:` of a struct, whose fields are its construction's arguments

docs: partial

- Recognized headings, prose and Args/Type Args entries in JSON and HTML
- Fenced Markdown preserves literal text in headings and entries
- Left: key spelling of Requires, Properties and a struct’s Args

## SDOC-4

docs: done

- JSON and HTML keep interfaces, candidates and members distinct, with their own signatures, comments and source parts

## SDOC-5

formatter: planned

- Formatting keeps meaning, comments and line-break form, and is idempotent

## SDOC-6

analyser: done

- Runnable documentation examples enter the analysed program as tests only. They leave application emission, execution, overload resolution and provider selection unaffected.

## SDOC-7

parser: done

- Fenced Markdown protects literal headings and keys after normalization

docs: partial

- The JSON model carries Markdown text, preserving fenced blocks
- HTML renders CommonMark tables and displays authored HTML as text
- Left: render formulas and diagrams with the offline assets (#457)

## SDOC-8

docs: done

- HTML links a code span found in its comment's scope to the named declaration, when present in the site

## SDOC-9

parser: done

- An `hgl` fence in Examples is parsed as a named test at its source location

analyser: done

- Example tests use the containing module's ordinary named-test scope

runner: done

- Runnable examples are listed, selected and run through the named-test path on the C++ target

docs: done

- The JSON model and HTML mark runnable and shown HGL fences

## SDOC-10

docs: done

- Default output documents the exported interface; private declarations and tests are opt-in
