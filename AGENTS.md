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
- PDFium through `pdfium-render` (provisional)
- SQLite for persistent state
- TOML for user configuration

The PDF engine choice is not finalized.

Do not introduce major frameworks or dependencies without explaining their purpose and tradeoffs.

## Architecture Principles

### Separate PDF processing from the UI

The PDF engine must be accessed through an internal abstraction.

Do not expose PDFium-specific types throughout the application.

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
- Large framework hierarchies
- Complex dependency injection
- Unnecessary asynchronous layers
- Academic-specific features into the initial MVP

## Rust Conventions

- Use idiomatic, stable Rust where possible.
- Prefer explicit types at important architectural boundaries.
- Avoid unnecessary cloning and allocations in rendering paths.
- Use `Result` for recoverable errors.
- Avoid panics in normal application code.
- Keep unsafe code isolated and documented.
- Prefer small modules with clear responsibilities.
- Use `cargo fmt` and `cargo clippy`.

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

## Visual Ruler

The visual ruler is a desired feature, not an MVP release gate. Validate
positioned text and document coordinates early so it remains feasible.

It should:

- Be controlled through keyboard commands
- Move between text lines when text geometry is available
- Support a coordinate-based fallback for scanned PDFs
- Be rendered independently from PDF page rasterization
- Remain correctly positioned through zoom and scrolling

Keep ruler logic separate from PDF engine implementations.

## Testing

Prioritize testing of:

- Coordinate transformations
- Navigation history and marks
- Keyboard command parsing
- Text-line navigation
- Page caching and invalidation
- PDF loading errors

Use representative real-world PDFs during development.

## Documentation

Keep these documents updated when decisions change:

- `README.md`
- `docs/architecture.md`
- `docs/features.md`
- `docs/roadmap.md`

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

## Scope

Do not implement features outside the current milestone without a clear reason.

The first objective is to validate the stack, then ship a buildable,
downloadable macOS PDF reader with smooth navigation and zooming. The ruler
and full keyboard customization are later goals, not MVP blockers.

See `docs/roadmap.md`.
