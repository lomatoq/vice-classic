# STATUS M12 — technical production release candidate

Date: 2026-08-03.

M12 completes the technical productization scope from spec v1.3 section 28:

- the installed native CLI carries digest-pinned production configs and no
  longer depends on the current working directory;
- a small WASM adapter calls the same core entry point, and the browser UI can
  upload, classify, preview and download successful output;
- the legacy engine path is explicit-only, SHA-256 pinned, directly spawned,
  time/output bounded and always reported as non-Classic;
- Linux, Windows and macOS CI compare one exact structural release vector and
  build the real wasm32 release target;
- dependency inventory, security boundary and performance claims are written
  down without claiming an unmeasured named-CPU SLO;
- release status keeps public and commercial authorization false until the
  repository license, donor attestations and qualified patent/FTO review are
  completed by humans.

Technical verification commands:

```text
cargo test --locked --release --workspace
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo build --locked --release -p vice-wasm --target wasm32-unknown-unknown
cargo run --locked --release -p vice-cli -- release-status --check docs/M12_CROSS_PLATFORM_VECTORS.json
node --check web/app.js
```

The technical release candidate is not a public/commercial release
authorization. The remaining non-code sign-offs are enumerated in
`M12_LEGAL_FTO_REVIEW.md` and are machine-enforced by `release-status`.
