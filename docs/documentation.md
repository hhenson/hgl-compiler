# API documentation

`hgl doc <ROOT>... --format json [--out DIR] [--private]` writes one
UTF-8 JSON document for the analysed program. Source discovery and
diagnostics follow `hgl check`. The compiler writes to standard output
or to `DIR/documentation.json` when `--out` is given. Rejected source
produces diagnostics and no model.

The model includes every module and its parts, including the supplied
core prelude. Each interface lists its exported declarations. An
operator lists its candidates with their own signatures, locations and
comments, including candidates declared in another module. A candidate
has `"exported": false` because it is reached through its operator
(UNIT-3). A member records the export status of its containing type.
`--private` also lists private declarations and named tests. A declaration
without a comment has `"documentation": null`.

## JSON model version 1

The [JSON Schema](documentation.schema.json) specifies the fields of
`schemaVersion: 1`. Consumers can select the model contract using
`schemaVersion`.

- `modules` gives module paths in lexical order. Each module lists
  `parts` in part order, starting with the unnamed part when present.
  A part gives the location and documentation of its module header.
- `declarations` gives declarations in part and source order.
  `canonicalName` is `module.path::name`. A member appends `.member`;
  a candidate has its operator's canonical name. Role, signature and
  source position distinguish declarations that share a canonical name.
  Members and candidates are nested under their owner. Candidates are
  ordered by the implementing module, part and source position.
- Interfaces give analysed types and roles, generic and configuration
  parameters, ordinary parameters, result and any requires clause.
  Each parameter list preserves declaration order. Types are HGL-like
  display strings. Where a generic parameter, parameter or result has
  a `written` field, it gives that source fragment, including any
  qualification, modifiers and defaults. Defaults, requires and
  properties are source text. An enum member's `value` is its analysed
  number as a decimal string to preserve the full signed integer range.
  A constant's `value` is its written expression. Empty arrays and
  `null` indicate inapplicable or absent metadata.
- `source` gives the declaring module and part, relative `file`, and
  one-based `line` and `column` at the name. File paths use `/`
  separators. A file under a supplied directory is relative to the
  innermost such root. An individual file outside those directories
  uses its basename. The embedded prelude uses `<core>/` paths.
- `documentation.text` is all the comment's text, after SDOC-2 removes
  delimiters, outer blank lines and common indentation. `summary` is
  the initial prose. `sections` follows the source order, with SDOC-3
  headings including their colons. Unheaded prose after a section,
  including an unknown heading, has a null heading. Section text
  removes its four-space margin. In `Args:` and `Type Args:`, each
  `entries` record gives its key and description, with a missing
  description represented by an empty string. `Requires:`, `Properties:`
  and a struct's `Args:` retain their text while the specification settles
  how their keys are written.

The JSON is indented with two spaces and ends with one LF. Source
fragments and comments use LF for each LEX-15 line break. Generated
metadata contains no timestamps or absolute filesystem paths. Comment
text is Markdown (CommonMark with tables). The model carries the authored
markup, formulas, diagrams, references and examples as text after
SDOC-2 normalization.
