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
both notices and writes them into the generated site. The release archive
also includes both notices beside its other licenses.
