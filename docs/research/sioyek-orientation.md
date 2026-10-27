# Kver and Sioyek: orientation for design discussions

This is a historical reading note, not a current project-status report, backend
decision, or feature commitment. Sioyek's documentation and source links below
refer to its `development` branch or published documentation; behavior may
differ between releases.

## Where Kver stands

- Kver aims to be a general-purpose, keyboard-first **but mouse-usable** PDF
  reader. macOS leads; Linux and Windows have separate delivery gates. The
  proposed stack includes Rust, GPUI, an undecided PDF backend, SQLite, and
  TOML ([README](../../README.md), [roadmap](../roadmap.md)).
- At the time of this note, there was no Rust application in the repository.
  Phase 0 called for a GPUI window, render and text-geometry experiments, and
  comparison of PDFium with Hayro / `gpui-pdf` before choosing a backend
  ([roadmap](../roadmap.md),
  [architecture](../architecture.md)).
- The intended boundaries are UI, commands, reader state, rendering, PDF backend,
  and persistence. PDF processing should not block input; document-space positions
  must survive viewport changes ([architecture](../architecture.md#high-level-architecture),
  [coordinate systems](../architecture.md#coordinate-systems)).
- The visual ruler is a wanted reading feature, not an MVP blocker. Its design
  calls for line-aware movement where text geometry permits, positional
  movement on image-only pages, and an overlay separate from page rasterization
  ([architecture](../architecture.md#visual-ruler), [features](../features.md#visual-ruler)).

## What Sioyek actually offers

- Sioyek describes itself as a PDF viewer focused on textbooks and research
  papers. Its documented interaction model maps configurable key sequences to
  named commands and offers a searchable command menu; it supports counts such
  as `42gg` ([project README](https://github.com/ahrm/sioyek/blob/development/README.md),
  [usage](https://sioyek-documentation.readthedocs.io/en/latest/usage.html#basic-movement),
  [configuration](https://sioyek-documentation.readthedocs.io/en/latest/configuration.html#syntax-of-keys-config-files)).
- Its *visual mark* follows text lines with `j`/`k`, can highlight below or
  around the line, and can move with the wheel in visual-scroll mode
  ([usage](https://sioyek-documentation.readthedocs.io/en/latest/usage.html#visual-mark),
  [configuration](https://sioyek-documentation.readthedocs.io/en/latest/configuration.html#ruler-mode)).
- Location history works across documents. Lowercase marks are document-local,
  uppercase marks global; bookmarks have descriptive names
  ([history](https://sioyek-documentation.readthedocs.io/en/latest/usage.html#history-navigation),
  [marks](https://sioyek-documentation.readthedocs.io/en/latest/usage.html#marks),
  [bookmarks](https://sioyek-documentation.readthedocs.io/en/latest/usage.html#bookmarks)).
- Its research-oriented layer includes linkless SmartJump to figures or
  references, non-disruptive destination overviews, and portals that show a
  linked destination in a helper window as the reader moves
  ([project README](https://github.com/ahrm/sioyek/blob/development/README.md),
  [overview](https://sioyek-documentation.readthedocs.io/en/latest/usage.html#overview-and-smartjump),
  [portals](https://sioyek-documentation.readthedocs.io/en/latest/usage.html#portals)).
  These are unscheduled Kver goals
  ([features](../features.md#goals-without-a-fixed-release-date),
  [roadmap](../roadmap.md#goals-to-schedule-after-evidence)).
- Sioyek's source is C++/Qt with MuPDF rendering rather than Kver's proposed
  Rust/GPUI stack and undecided PDF backend. Its renderer uses worker threads,
  queued requests, cached responses, and discards queued requests at obsolete zoom
  levels; this is useful behavioral evidence, not an architecture to copy
  wholesale ([development README](https://github.com/ahrm/sioyek/blob/development/README.md#development-branch-faq),
  [renderer source](https://github.com/ahrm/sioyek/blob/development/pdf_viewer/pdf_renderer.cpp)).
- Sioyek keeps path-dependent data in `local.db` and portable reading data in
  `shared.db`, and separates default from user key/preference files
  ([data](https://sioyek-documentation.readthedocs.io/en/latest/usage.html#data-and-synchronization),
  [configuration](https://sioyek-documentation.readthedocs.io/en/latest/configuration.html)).

## Takeaways for Kver

- Validate PDF rendering, positioned text, coordinate transforms, worker
  behavior, and packaging before choosing the backend. The ruler can remain
  unimplemented while its prerequisites are checked
  ([roadmap](../roadmap.md#0-validate-the-stack)).
- Keep persistent locations in document space so restoration, navigation,
  and later overlays do not depend on screen dimensions
  ([architecture](../architecture.md#coordinate-systems)).
- Sioyek's visual mark illustrates line-aware reading; Kver still needs to
  decide how to order lines in multi-column layouts when it builds its ruler
  ([Sioyek visual mark](https://sioyek-documentation.readthedocs.io/en/latest/usage.html#visual-mark),
  [Kver ruler design](../architecture.md#visual-ruler)).

No engine benchmark, platform prototype, or hands-on Sioyek test was performed
for this note; those remain validation work, not established conclusions.
