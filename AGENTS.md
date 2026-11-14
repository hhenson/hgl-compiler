# Compiler rules

- Before every push, check the branch's PR state on GitHub. If its previous PR
  is merged, fetch `origin/main`, start a fresh `codex/` branch from it, and carry
  forward only the new changes. Do not push follow-up commits to a merged PR's
  branch; open a new PR for the fresh branch.
- Keep documents, discussions, issues, and reviews terse. Avoid extended dialogue.
- The [language specification](https://github.com/hhenson/hgl_spec) and
  [core](https://github.com/hhenson/hgl_core) are the authority. Implement their
  rules; do not define language behaviour here. Report a gap or contradiction
  to the owning repository's issue tracker instead of deciding it in code.
- Behaviour of an earlier implementation is not a rule. Require no historical
  repository to understand this one.
- Keep the stages separate: lexer, parser, analyser, optimiser, transpiler.
  A stage reads only the previous stage's output and the diagnostics sink.
  Target knowledge lives only in a transpiler.
- Cite the specification rule (for example `LEX-8`) where code implements one
  and where a test checks one. A diagnostic uses the catalogued category and
  code when the specification has one, and is marked local when it has not.
- Keep `docs/rules.md` current. It lists each rule of the pinned specification
  and core against the stage that implements it. Mark a row `done` in the
  change that implements the rule and tests it, or `partial`, saying what is
  left, in one that implements some of it.
- Document the code for a reader new to Rust: every crate and module says what
  it is for and where it sits in the pipeline; every public item has a doc
  comment; a non-obvious Rust idiom gets one explanatory line.
- Prefer an established, well-maintained tool or crate to hand-written code.
  Drop one that cannot do the job the way the design needs: the design
  decides, not the tool. List each dependency and its job in
  `docs/architecture.md`.
- Stay current. `rust-toolchain.toml` pins the latest stable Rust; merge the
  scheduled toolchain and dependency update pull requests promptly. Declare no
  minimum supported Rust version.
- `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`
  and `cargo test --workspace` pass before a push.
- Report the exact tests run, skipped scope, and unsupported rules. Passing
  implementation tests is not conformance.
- Keep host details, credentials, local paths, and private projects out of commits.
