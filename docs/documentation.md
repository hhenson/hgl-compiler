# API documentation

`hgl doc <ROOT>... [--out DIR] [--private]` writes an HTML reference.
Open `index.html` in that directory. The default directory is
`hgl-doc`. Use `--format json` for the JSON model below.

The index links to one page per module. Each module page shows its
parts, their source comments and its declarations grouped by kind.
Declarations show their written signatures, file and line,
documentation, and parameter and type-parameter tables. The argument and
configuration tables show modes beside parameter names and pack marks
beside types. An operator lists its candidates
with their own signatures and documentation. Exported declarations
appear by default. Use `--private` to include the rest and named tests.

HTML output goes into an empty directory or a site marked by `hgl doc`.
An update removes generated files the new site no longer uses, including
pages from a previous `--private` build. It preserves unrelated files.
Choose an empty directory when regenerating an older, unmarked site.
New sites use an empty `.hgl-doc-site-v2` marker. Sites with the original
`.hgl-doc` marker also rebuild.

Comments are rendered as CommonMark with tables (SDOC-7). Authored HTML
is shown as text. HGL fences are highlighted using the lexer and labeled
as runnable or shown examples (SDOC-9). A code span naming a declaration
or member links to it when it resolves in the comment's source scope and
appears in the site (SDOC-8). Markdown link and image destinations appear
as text after their labels. HTTP and HTTPS link labels are clickable.
Image descriptions appear as text. The pages and stylesheet
use relative paths and open offline. The index searches declaration names
using a local search index. Inline formulas and `math` fences use the
bundled KaTeX renderer. `mermaid` fences use the bundled Mermaid renderer.
If a formula or diagram cannot be rendered, its source stays visible.
A site includes each renderer and its notices when one of its pages
uses that renderer.

KaTeX emits MathML with `trust: false` and strict errors. It limits macro
expansion to 1,000 steps and caps supported sizes at 20 em. When a generated
MathML dimension or offset exceeds 20 em in magnitude, the formula
stays visible as source. Mermaid uses strict security and SVG text labels,
including text spans for journey diagrams. The site fixes these settings,
text placement, theme CSS, theme variables and fonts for every diagram.

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

## Runnable examples

`hgl check` checks `hgl` fences under `Examples:` as named tests and
reports errors on the original comment lines. `hgl test --target cpp`
runs them alongside ordinary tests. A fence marked `hgl no-run` or
outside `Examples:` is shown only. Backtick and tilde fences both work.
These commands include examples from modules under the given source roots.
Give Core's source as a root to check or run its examples.

The listing identifies an example as `module.path::declaration#example-N`.
Pass that same name to `--test`, for example `--test 'demo::twice#example-1'`.
Count HGL fences under the comment's `Examples:` headings from one,
including `hgl no-run`. A field or member uses `Type.member`; the module
header uses `[module]`, and a named part's header uses `[part name]`.
When several documented declarations have the same label, the second and
later use `~2`, `~3`, and so on before `#example-N`, in part and source
order. Ordinary test selectors keep their `module.path::name` spelling.

## JSON model version 1

The [JSON Schema](documentation.schema.json) specifies the fields of
`schemaVersion: 1`. Consumers can select the model contract using
`schemaVersion`.

- `modules` gives module paths in lexical order. Each module lists
  `parts` in part order, starting with the unnamed part when present.
  A part gives the location and documentation of its module header.
- `declarations` gives declarations in part and source order.
  `canonicalName` is `module.path::name`. A member appends `.member`;
  a module candidate has its operator's canonical name. A declaration
  private to a named test includes the test's name, as in
  `module.path::test_name::type_name`. Role, signature and
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
  delimiters, outer blank lines and common indentation. `summary` is the
  first paragraph of introductory prose, or an empty string if absent.
  `sections` follows source order. After the summary, unheaded prose
  has a null heading and `recognized: false`. SDOC-3 headings include
  their colons and have `recognized: true`. Other column-zero lines
  ending in a colon and followed by a four-space-indented line keep
  their heading and use `recognized: false`. A headed section's text
  removes its four-space margin. Fenced Markdown keeps its delimiters
  and literal contents, including heading-like lines and keys.
- A comment with HGL fences includes an `examples` array in source order.
  Each entry has its `code`, its one-based `position` under `Examples:`
  (null outside those headings), and a `runnable` flag.
- Only recognized `Args:` and `Type Args:` sections carry `entries` with
  keys and descriptions. A missing description is an empty string.
  `Requires:`, `Properties:` and a struct's `Args:` retain their text
  while the specification settles how their keys are written.

The JSON is indented with two spaces and ends with one LF. Source
fragments and comments use LF for each LEX-15 line break. Generated
metadata contains no timestamps or absolute filesystem paths. Comment
text is Markdown (CommonMark with tables). The model carries the authored
markup, formulas, diagrams, references and examples as text after
SDOC-2 normalization.
