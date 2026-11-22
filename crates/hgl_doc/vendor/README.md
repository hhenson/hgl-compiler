# Documentation renderers

Each file comes from the official npm tarball. The archive integrity values
are the registry's SHA-512 values and match the downloaded bytes.

| Package | Files copied from the package | npm integrity |
| --- | --- | --- |
| `katex@0.19.1` | `package/dist/katex.min.js`, `package/LICENSE` | `sha512-jG8uKntjSEcm2EpQOh0XturpPuuC9r3Sq5HLHdsFdWZT27E672SF19QEi+of0/YeOy15lDgRhKvr5614uRrEfA==` |
| `mermaid@12.3.0` | `package/dist/mermaid.min.js`, `package/LICENSE` | `sha512-4113TdXeLAsMruJ79OpFL5O8+LTFXhQqZ260rF3tgUmiNAtRm4Uume04I+65PRN6yZ69J4f8rVYWW8zRk65UYw==` |

The site uses KaTeX's MathML output, which needs no KaTeX CSS or fonts.
Mermaid's classic browser bundle handles diagram rendering from a local
file. Both packages are MIT-licensed. The compiler embeds the bundles and
the notices and writes them into the generated site. The release archive
also includes the notices beside its other licenses.

## File hashes

SHA-256 values relative to this directory.

```text
fa140498b8cfc8302cbe5e7cb1bdbc1f952bc38b3fa258ede09e73edbf9a74bc  katex/katex.min.js
766ccc1f306c885aa45542a9846bbd0a505b27a0374f146778171c2254ce18e3  katex/LICENSE
336df803d87a38b70a01e736e666c8f7601d2ce42f5e47c8fafe5d721074b9a3  mermaid/mermaid.min.js
ec9fb67dcb25eccc416ed56e1aab819222c805a2a4bfe4cb19e7556bf2ffde80  mermaid/LICENSE
a75556387898a9d0658ade7f9205e46d4d0d67b49f787972f88810ae19c26d20  mermaid/THIRD-PARTY-NOTICES.txt
```

## Mermaid bundle notices

The source inventory comes from `package/dist/mermaid.min.js.map` in the
published tarball. Some parser chunks are already bundled there. Their
build source comments identify additional exact dependencies. The Mermaid
12.3.0 release's pnpm lockfile verifies the external package versions and
tarball integrity. The internal parser package version comes from
`packages/parser/package.json` at that release.

`mermaid/THIRD-PARTY-NOTICES.txt` records the npm archive, member name,
SHA-256 and original notice text for each entry. FastDom publishes its MIT
notice in the README. The lockfile also contains dependencies used by the
build, such as `uuid` under Istanbul and SockJS, which do not occur in the
embedded browser source inventory.

| Embedded package | Upstream license |
| --- | --- |
| `@braintree/sanitize-url@7.1.2` | MIT |
| `@chevrotain/cst-dts-gen@13.2.0` | Apache-2.0 |
| `@chevrotain/gast@13.2.0` | Apache-2.0 |
| `@chevrotain/regexp-to-ast@13.2.0` | Apache-2.0 |
| `@chevrotain/utils@13.2.0` | Apache-2.0 |
| `@iconify/utils@3.0.2` | MIT |
| `@mermaid-js/parser@2.0.1` | MIT |
| `@upsetjs/venn.js@2.0.0` | MIT |
| `chevrotain@13.2.0` | Apache-2.0 |
| `chevrotain-allstar@0.5.0` | MIT |
| `cose-base@1.0.3` | MIT |
| `cose-base@2.2.0` | MIT |
| `cytoscape@3.34.0` | MIT |
| `cytoscape-cose-bilkent@4.1.0` | MIT |
| `cytoscape-fcose@2.2.0` | MIT |
| `d3@7.9.0` | ISC |
| `d3-array@2.12.1` | BSD-3-Clause |
| `d3-array@3.2.4` | ISC |
| `d3-axis@3.0.0` | ISC |
| `d3-brush@3.0.0` | ISC |
| `d3-chord@3.0.1` | ISC |
| `d3-color@3.1.0` | ISC |
| `d3-contour@4.0.2` | ISC |
| `d3-delaunay@6.0.4` | ISC |
| `d3-dispatch@3.0.1` | ISC |
| `d3-drag@3.0.0` | ISC |
| `d3-dsv@3.0.1` | ISC |
| `d3-ease@3.0.1` | BSD-3-Clause |
| `d3-fetch@3.0.1` | ISC |
| `d3-force@3.0.0` | ISC |
| `d3-format@3.1.0` | ISC |
| `d3-geo@3.1.1` | ISC |
| `d3-hierarchy@3.1.2` | ISC |
| `d3-interpolate@3.0.1` | ISC |
| `d3-path@1.0.9` | BSD-3-Clause |
| `d3-path@3.1.0` | ISC |
| `d3-polygon@3.0.1` | ISC |
| `d3-quadtree@3.0.1` | ISC |
| `d3-random@3.0.1` | ISC |
| `d3-sankey@0.12.3` | BSD-3-Clause |
| `d3-scale@4.0.2` | ISC |
| `d3-scale-chromatic@3.1.0` | ISC |
| `d3-selection@3.0.0` | ISC |
| `d3-shape@1.3.7` | BSD-3-Clause |
| `d3-shape@3.2.0` | ISC |
| `d3-time@3.1.0` | ISC |
| `d3-time-format@4.1.0` | ISC |
| `d3-timer@3.0.1` | ISC |
| `d3-transition@3.0.1` | ISC |
| `d3-zoom@3.0.0` | ISC |
| `dagre-d3-es@7.0.14` | MIT |
| `dayjs@1.11.21` | MIT |
| `dompurify@3.4.17` | (MPL-2.0 OR Apache-2.0) |
| `elkjs@0.12.0` | EPL-2.0 OR GPL-3.0-or-later |
| `es-toolkit@1.45.1` | MIT |
| `fastdom@1.0.12` | MIT |
| `internmap@2.0.3` | ISC |
| `js-yaml@4.3.2` | MIT |
| `katex@0.18.11` | MIT |
| `khroma@2.1.0` | MIT (package/license) |
| `langium@4.4.0` | MIT |
| `layout-base@1.0.2` | MIT |
| `layout-base@2.0.1` | MIT |
| `lodash-es@4.18.1` | MIT |
| `marked@16.3.0` | MIT |
| `roughjs@4.6.6` | MIT |
| `stylis@4.3.6` | MIT |
| `ts-dedent@2.2.0` | MIT |
| `vscode-jsonrpc@9.0.3` | MIT |
| `vscode-languageserver-protocol@3.18.4` | MIT |
| `vscode-languageserver-textdocument@1.0.15` | MIT |
| `vscode-languageserver-types@3.18.4` | MIT |
| `vscode-uri@3.1.0` | MIT |
