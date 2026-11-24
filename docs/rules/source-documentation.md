# Source documentation

`source-documentation.md`

## SDOC-1

parser: done

- A documentation comment attaches to the declaration after it; repeated or unattached ones rejected
- Native properties follow the attachment rules for struct fields, and the documentation model carries their own sections

## SDOC-2

docs: done

- Documentation text: delimiters, indentation, line breaks

## SDOC-3

analyser: partial

- A key of an `Args:` or a `Type Args:` section, a name and a `:` four spaces in once SDOC-2's common indentation is removed, names a parameter or a generic parameter of what the comment documents; a declaration with none has none to name (`doc.unknown_key`)
- Left: `Requires:` and `Properties:` keys, which the chapter does not say how to write; `Args:` of a struct, whose fields are its construction's arguments

docs: partial

- Recognized headings, including `Options:`, `Rules:` and `Status:`, in JSON and HTML
- Authored headings with an indented body keep their headings in HTML and remain unrecognized in JSON, including when the body starts after a blank line
- `Args:` and `Type Args:` descriptions remove their common continuation indent for Markdown while preserving relative indentation
- Fenced Markdown preserves literal text in headings and entries
- Left: key spelling of Requires, Properties and a struct’s Args

## SDOC-4

docs: done

- JSON and HTML keep interfaces, candidates and members distinct, with their own signatures, comments and source parts
- HTML signatures include block initializers of constants and fields
- HTML parameter tables show argument modes and pack marks

## SDOC-5

formatter: planned

- Formatting keeps meaning, comments and line-break form, and is idempotent

## SDOC-6

analyser: done

- Runnable documentation examples enter the analysed program as tests only. They leave application emission, execution, overload resolution and provider selection unaffected.

## SDOC-7

parser: done

- Fenced Markdown protects literal headings and keys after normalization

docs: done

- The JSON model carries Markdown text, preserving fenced blocks
- HTML renders CommonMark tables and displays authored HTML as text
- HTML shows Markdown link and image destinations as text, with navigation on HTTP and HTTPS link labels
- HTML places section titles and Markdown headings beneath their containing module, part or declaration
- Inline and fenced formulas and Mermaid fences render with local assets, with source text remaining visible on rendering failure

## SDOC-8

docs: done

- HTML links a code span found in its comment's scope to the named declaration, when present in the site
- Declaration links use the declared name, with a suffix for repeated names. Module filenames use the module name, including when the name needs a shorter filename.

## SDOC-9

parser: done

- An `hgl` fence in Examples is parsed as a named test at its source location

analyser: done

- Example tests use the containing module's ordinary named-test scope

runner: done

- Runnable examples are listed, selected and run through the named-test path on the C++ target
- Examples are checked and run when their module's source is given (TST-14).

docs: done

- The JSON model and HTML mark runnable and shown HGL fences

## SDOC-10

docs: done

- Default output documents the exported interface; private declarations and tests are opt-in
