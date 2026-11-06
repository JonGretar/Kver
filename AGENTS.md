# Kver — Agent Instructions

## Project Overview

Kver is a planned keyboard-first PDF reader in Rust. The first downloadable
release targets macOS; Linux and Windows have separate delivery gates.

It is inspired by Sioyek, but aims to be a general-purpose PDF reader rather than an academic-specific application.

Target platforms:

- macOS
- Linux
- Windows

Primary priorities:

1. Responsiveness and performance
2. Excellent keyboard navigation
3. Accurate PDF rendering
4. Simple, maintainable architecture
5. Cross-platform compatibility

## Technology

Preferred stack:

- Rust
- GPUI
- Hayro for PDF rendering; compare PDFium only if a concrete limitation
  warrants revisiting the backend
- SQLite for persistent state
- TOML for user configuration

Hayro is the chosen initial backend. SQLite and TOML are planned, not yet
implemented.

Do not introduce major frameworks or dependencies without explaining their purpose and tradeoffs.

## Architecture Principles

### Separate PDF processing from the UI

The PDF engine must be accessed through an internal abstraction.

Do not expose Hayro-specific types throughout the application.

The architecture should allow replacement of the PDF engine without rewriting the UI or navigation system.

### Keep the UI responsive

PDF rendering, text extraction, and other expensive operations must not block the UI thread.

Use background workers and appropriate caching.

Prioritize visible pages when scheduling rendering work.

### Keyboard-first interaction

Commands should be independent of physical keybindings.

For example, a "next page" action should be invokable by:

- Keyboard shortcut
- Command palette
- Menu or UI control

Avoid embedding keybinding logic directly into PDF rendering or view components.

### Preserve document coordinates

Persistent marks, links, highlights, and navigation positions must use document-space coordinates rather than screen pixels.

Screen coordinates are derived from document coordinates and viewport transformations.

### Avoid unnecessary complexity

Prefer straightforward implementations over premature abstractions.

Do not introduce:

- Plugin systems before they are needed
- Complex dependency injection
- Unnecessary asynchronous layers

## Rust Conventions

- Use idiomatic, stable Rust where possible.
- Prefer explicit types at important architectural boundaries.
- Avoid unnecessary cloning and allocations in rendering paths.
- Use `Result` for recoverable errors.
- Avoid panics in normal application code.
- Keep unsafe code isolated and documented.
- Prefer small modules with clear responsibilities.
- Format with `cargo fmt` and lint with Clippy using the command policy below.

### Local Rust commands

On macOS, when Corgi is installed, use Corgi for every build, check,
test, and lint operation it supports. This includes routine validation,
not only release builds and app packaging.

- Check Corgi's installed help for the appropriate command; do not assume
  Cargo's syntax or flags work unchanged.
- Use Cargo directly only when Corgi is unavailable, the operation is not
  supported by Corgi, or the task explicitly requires a Cargo command.
- Formatting with `cargo fmt` is fine.
- On Linux and in CI, use Cargo.
- If falling back to Cargo on a Mac with Corgi installed, explain why
  before running it.
- Do not run equivalent Cargo and Corgi checks merely to validate twice.

## Performance Considerations

PDF documents may contain hundreds or thousands of pages.

Design for:

- Virtualized page rendering
- Efficient bitmap/texture caching
- Background rendering
- Cancellation or invalidation of obsolete work
- Smooth scrolling and zooming
- Predictable memory usage

Avoid loading or rasterizing entire large documents eagerly.

## Testing

Prioritize testing of:

- Coordinate transformations
- Navigation history and marks
- Keyboard command parsing
- Page caching and invalidation
- PDF loading errors

Use representative real-world PDFs during development.

For visible UI changes that need review or approval, capture and show the
result in the Delta thread with the opt-in `tests/visual.rs` tool. See
`docs/agents/workflow.md` for the capture and cleanup workflow.

## Documentation

Keep `README.md` focused on the reader-facing project description and quick
start, not agent memory or work status. Record architecture changes in
`docs/architecture.md`, scope changes in `docs/features.md`, and release-gate
progress in `docs/roadmap.md`; GitHub Issues track live ticket status.

Document important architectural decisions and tradeoffs.

For ticketed work:

- Reference the issue number (`#N`) in commits. Use `Closes #N` in a PR only
  when merging it completes the issue; otherwise use `Refs #N`.
- In the landing PR, check off roadmap items only when fully complete, and add
  notable reader-facing changes to `CHANGELOG.md` under `Unreleased`.
- Create a release or tag only when explicitly requested.

For the full close-out and release process, see `docs/agents/workflow.md`.

## Agent skills

### Issue tracker

Publish project tickets to GitHub Issues. See `docs/agents/issue-tracker.md`.

### Domain docs

Use a single-context glossary and ADR layout. See `docs/agents/domain.md`.

### Feature workflow

For task suggestions or "what's next", start with `docs/roadmap.md` and the
linked GitHub issues; check live blockers and ownership before recommending
ready work. For specs, tickets, work status, parallel implementation, review,
or landing in Delta, see `docs/agents/workflow.md`. Use isolated Delta
worktrees for independent implementation; Git and `jj` are both acceptable
in this project.

Implementation threads leave PR creation to explicitly requested landing
unless told to open one. Orchestrators should not request PRs from them by
default.

## Scope

Do not implement features outside the current milestone without a clear reason.

The first objective is to validate the stack, then ship a buildable,
downloadable macOS PDF reader with smooth navigation and zooming. See
`docs/features.md` and `docs/roadmap.md` for later goals.
