# AGENTS.md

## Project at a glance

Stillkit is a Rust 2024 command-line application installed as `still`. It classifies files,
organizes photo collections by date, adjusts exposure, creates previews, and provides an
interactive terminal image-rating workflow.

Keep changes focused on this CLI. Do not introduce generic service, async, configuration, or
dependency-injection patterns unless a concrete feature requires them. This project is primarily
synchronous filesystem and image-processing code; CPU-heavy batch work is parallelized with
Rayon where it is safe and useful.

## Sources of truth

- `Cargo.toml` defines the package, binary, feature flags, and dependencies.
- `src/main.rs` defines the public command surface and dispatch.
- `README.md` documents supported user-facing behavior and external tool requirements.
- `PHOTO_SORT.md` is useful design history for `organize`, but current code and tests take
  precedence if it has drifted.

When CLI behavior, flags, output naming, supported formats, or required external tools change,
update `README.md` in the same change.

## Command flow and module structure

The current entry flow is:

```text
still
└── src/main.rs                 CLI construction and dispatch
    ├── classify  ────────────> src/sort.rs
    ├── organize  ────────────> src/file/mod.rs
    │                            ├── date.rs    date extraction and fallback policy
    │                            ├── group.rs   chronological clustering
    │                            ├── layout.rs  destination naming and placement
    │                            ├── scan.rs    existing-library discovery and matching
    │                            └── ops.rs     filesystem moves
    ├── exposure  ────────────> src/exposure.rs
    ├── previews  ────────────> src/preview/mod.rs
    └── rate      ────────────> src/rate/mod.rs
                                 └── import     rating import flow
```

There is also a supported legacy path, `still <directory>`, which dispatches to `classify`.
Preserve it unless the task explicitly includes a breaking CLI change.

For new modules and substantial refactors, prefer a layout that mirrors the CLI rather than a
generic `modules/` directory:

```text
src/
├── main.rs
├── commands/
│   ├── mod.rs
│   ├── classify.rs
│   ├── organize/
│   │   ├── mod.rs
│   │   ├── date.rs
│   │   ├── group.rs
│   │   ├── layout.rs
│   │   ├── scan.rs
│   │   └── ops.rs
│   ├── exposure/
│   │   └── mod.rs
│   ├── previews/
│   │   └── mod.rs
│   └── rate/
│       ├── mod.rs
│       └── import.rs
└── shared/                      Only code used by multiple subcommands
    ├── mod.rs
    └── image.rs                 Shared image detection/decoding when warranted
```

Treat this as the direction for touched code, not a mandate for broad file moves during an
unrelated task. Keep subcommand-specific types and helpers inside that subcommand. Promote code to
`shared/` only after at least two command modules genuinely share the same behavior.

Each command module should expose a small boundary, normally `subcommand()` for Clap construction
and `run()` for execution. Keep parsing/validation near that boundary, put domain decisions in pure
helpers where possible, and isolate filesystem or process side effects behind narrow functions.

## File and line-size limits

- Format Rust with rustfmt; use its default 100-column line width. Do not hand-format code against
  rustfmt.
- Keep production Rust files at or below **500 lines**, including inline unit tests. Split a file
  before a change would take it over the cap, using cohesive responsibilities rather than numbered
  or arbitrary fragments.
- Existing oversized files (`src/preview/mod.rs` and `src/rate/mod.rs`) are known debt. A small,
  scoped fix does not require wholesale refactoring, but substantial additions to either should
  first extract the relevant responsibility into a named child module.
- Prefer functions under roughly 50 lines. Longer orchestration functions are acceptable when the
  sequence remains linear and extraction would obscure rather than clarify it.
- Keep nesting shallow with early returns and small helpers. Comments should explain constraints,
  fallback order, or non-obvious intent—not restate the code.

## Rust conventions

- Use idiomatic ownership and borrowing. Accept `&Path` at filesystem API boundaries and return or
  store `PathBuf` only when ownership is needed.
- Use `Result` for recoverable failures and add actionable context that includes the relevant path
  or external command. Reserve `panic!`, `unwrap()`, and `expect()` for tests or invariants already
  guaranteed by Clap/type construction.
- Avoid `unsafe`. If it is unavoidable, keep it minimal, document its safety invariants, and add
  focused tests around the boundary.
- Prefer enums and typed configuration structs over boolean-heavy APIs or loosely related argument
  lists.
- Avoid adding dependencies for behavior that is clear and small with the standard library or an
  existing crate. If adding one, justify its maintenance, platform, binary-size, and license cost.
- Keep visibility as narrow as possible: private by default, `pub(crate)` for genuine cross-module
  use, and `pub` only for an intentional public boundary.
- Preserve deterministic ordering before assigning exposure ramps, planning moves, or presenting
  files in the UI. Parallel execution must not make observable results nondeterministic.

## Filesystem safety

These commands operate on users' original media, so correctness and recoverability take priority
over convenience.

- Validate source and destination paths before creating directories, renaming files, or invoking
  external programs.
- Never silently overwrite an existing file. Honor the command's explicit `--force`, `--overwrite`,
  duplicate-detection, or prompt semantics.
- Keep `--dry-run` side-effect free: no directory creation, moves, metadata writes, or external
  conversions that produce output.
- Define collision behavior explicitly and test it. A same-name file is not necessarily a duplicate.
- Use `Path`/`PathBuf` operations rather than constructing paths with string concatenation.
- Do not assume UTF-8 filenames. Use lossy conversion only for display; keep filesystem operations
  on `OsStr`/`Path` values.
- Be deliberate about symlinks and recursive traversal. Avoid following a path back into an output
  directory or processing newly generated output as fresh input.
- When practical, write generated output to a temporary file in the destination filesystem and
  rename it into place only after successful encoding.

## Images, metadata, and platform fallbacks

- Keep extension matching case-insensitive and centralize supported-extension lists instead of
  duplicating them across functions.
- Preserve image metadata by default where current behavior promises it. `--clear-metadata` must
  reliably remove EXIF, XMP, and IPTC metadata.
- Preserve the documented fallback order for HEIC/HEIF/HIF processing. On macOS, `sips` is the
  accelerated path; native decoding is feature-gated; ImageMagick is a fallback where supported.
- External processes must be invoked with `std::process::Command`, never via a shell-composed
  command string. Check exit status and include useful stderr in failures without exposing
  unrelated data.
- `magick`, `exiftool`, and macOS-only `sips` are environmental capabilities, not unconditional test
  prerequisites. Tests that need them should detect availability and skip cleanly when absent.
- The `native-heic` feature uses the `heic` crate and has licensing implications. Do not enable it
  by default or change distribution assumptions without explicit review.
- Bound parallel work and memory use conceptually: full-resolution images are expensive. Avoid
  cloning pixel buffers or retaining decoded images longer than necessary.

## CLI and user experience

- Keep command names, flags, defaults, conflicts, help text, and README examples aligned.
- Prefer Clap validation for local argument constraints; use runtime validation for filesystem and
  cross-argument conditions that Clap cannot express clearly.
- Send normal results to stdout and warnings/errors to stderr. Error messages should say what
  failed, identify the relevant file when safe, and suggest a remedy when one is known.
- Progress bars and prompts must not hide final failures. Preserve non-interactive behavior for
  commands that support automation.
- Treat output filenames and date-directory formats as user-facing compatibility surfaces.

## Testing

Put small unit tests beside the code they exercise. Use `tests/` for command-level or multi-module
behavior when such coverage is added. Test behavior and edge cases rather than private
implementation details.

For filesystem tests:

- Create isolated temporary directories; never mutate files under `test/` in place.
- Cover mixed-case extensions, extensionless files, non-UTF-8 names where the platform permits,
  collisions, recursive traversal, and dry-run behavior when relevant.
- Verify both the resulting filesystem and the files deliberately left untouched.

For CLI changes, test Clap parsing for the happy path plus required arguments, defaults, conflicts,
and backwards-compatible forms. For naming and date logic, favor table-driven pure unit tests.

Run the checks appropriate to the change:

```sh
cargo fmt --all -- --check
cargo check --all-targets
cargo test
cargo clippy --all-targets -- -D warnings
```

Also run `cargo check --all-targets --features native-heic` when changing feature-gated HEIC code,
provided the local toolchain can build that optional dependency. State clearly when an external-tool
or platform-specific path could not be exercised.

## Change discipline

- Inspect the current implementation before editing; this repository may contain uncommitted user
  work. Preserve unrelated changes.
- Keep patches scoped. Avoid renaming modules, reformatting unrelated code, or refactoring multiple
  commands unless the requested change needs it.
- Add regression tests with bug fixes and focused tests with new behavior.
- Do not weaken error handling or tests merely to make a check pass.
- Do not commit build artifacts, generated previews, adjusted photos, or temporary media.
- Before finishing, review the diff for accidental media changes, verify documentation matches the
  implementation, and report checks that ran as well as checks that could not run.
