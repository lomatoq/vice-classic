# Security policy

## Supported state

The current tree is an M12 technical release candidate, not a public or
commercial release. Security fixes target `main`. Report vulnerabilities with
a private GitHub Security Advisory for this repository; do not include secrets
or private customer images in a public issue.

## Trust boundaries

- PNG, JPEG and WebP inputs are untrusted. Decode enforces a 64 MiB encoded
  limit, 8192 px per side and 32 million decoded pixels before large work.
- Core search, render and candidate stages have explicit time/work/memory
  budgets. M10 and M11 add bounded intersection, sampling, proposal and working
  set policies rather than relying on allocator failure.
- Production configs are embedded and SHA-256 trust-anchored. Explicit config
  files are accepted only when their exact bytes and model identity match.
- The browser adapter adds a 64 MiB boundary before crossing into the core and
  contains no filesystem, process or network API.
- Classic SVG is emitted only after independent serialized render and seal
  verification. The browser previews it as an image Blob under a restrictive
  Content Security Policy.

## Legacy engines

`vicec vectorize` never calls a legacy engine. `legacy-vectorize` is an
explicit operator action and requires an exact executable SHA-256. It uses
direct argv with no implicit shell, null stdin, bounded wall time and a combined
64 MiB output/log limit; executable identity is checked before and after the
run. Its report always says `classic_success: false`.

A pinned legacy executable is still trusted native code. The wrapper is not an
OS sandbox and cannot prevent a deliberately malicious engine from accessing
the invoking user's files or spawning descendants. Run third-party engines in
an OS sandbox/container with least privilege.

## Release blockers

Public/commercial distribution stays disabled until a repository license is
selected, donor non-use/license attestations are signed, dependency notices are
rechecked for the distribution bundle, and qualified human counsel completes
the patent/FTO review. Technical status must not be read as legal approval.
