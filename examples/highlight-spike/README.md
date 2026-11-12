# PDF highlight writer spike

Standalone, disposable-file experiment for standard PDF highlights. This is
not Kver's annotation implementation or a production-safe save API. Its own
manifest/lockfile keep `lopdf` out of the reader's dependencies.

Retained as fixture and interoperability prior art for
[spec #123](https://github.com/JonGretar/Kver/issues/123). These opt-in assertion
checks are not part of the application test suite or a new CI requirement.
Replace them with production regression coverage as the feature is implemented.

From the repository root on macOS:

```sh
corgi --manifest-path examples/highlight-spike/Cargo.toml run -- target/highlight-spike
swift examples/highlight-spike/pdfkit-check.swift target/highlight-spike
corgi --manifest-path examples/highlight-spike/Cargo.toml run -- roundtrip \
  target/highlight-spike/pdfkit-edited.pdf target/highlight-spike/native-roundtrip
```

On Linux, use `cargo run --locked --manifest-path
examples/highlight-spike/Cargo.toml -- target/highlight-spike` for the Rust
experiment. The Swift check requires macOS PDFKit.

Use an ignored output directory: the commands overwrite their generated
artifacts. They never save over the checked-in example PDFs.

The Rust program generates a four-page specimen with a nonzero crop origin,
0/90/180/270-degree page rotations, left/right text columns, an existing
highlight, and an existing link on every page. It adds a three-region highlight
with a normal appearance stream, reopens and recolors it, and removes it.
Each edit appends a PDF incremental revision to a disposable output file.

Assertions check annotation geometry/color/identity, unchanged page content,
unchanged other annotation dictionaries, and preservation of each prior file
as an exact byte prefix. Hayro uses the same version and 1x rendering settings
as Kver; adding/recoloring must change its render and deletion must restore the
baseline render. The existing linked field guide is separately exercised for
page/catalog/content/annotation preservation, not text-selection accuracy.

The Swift program independently parses and renders the specimen with Apple's
PDFKit, checks geometry/colors and existing annotations, then recolors and
saves a Rust-created highlight. The `roundtrip` command reopens that native
save with Rust and recolors/deletes its highlights while preserving the other
page content/annotations. PDFKit checks do not establish Preview's interactive
editing behavior: open the generated PDFs there to check that.

For geometry extracted entirely through Hayro:

```sh
mkdir -p target/highlight-spike/hayro-geometry
cp target/highlight-spike/original.pdf target/highlight-spike/hayro-geometry/original.pdf
corgi --manifest-path examples/highlight-spike/Cargo.toml run -- hayro-control \
  target/highlight-spike/hayro-geometry
```

The two generated controls use glyph ink bounds and reader-style selection
bounds with actual glyph advances, respectively, without PDFKit or stored
native-selection coordinates. The probe deliberately matches known fixture
text runs; it is not a replacement for the reader's layout/selection system.
The padded initial specimen remains an intentional interoperability failure
case, not a recommended geometry policy.

For the consolidated Preview check, use the Hayro-derived PDFs and inspect all
three regions after interactive recoloring, not just annotation counts/colors.
Do not regenerate files while somebody is editing them. The one-off native
comparison/debug probes have been removed; their implementation-relevant
conclusions are in the [research report](../../docs/research/pdf-highlighting.md#writer-feasibility).

Limitations:

- Fixture coordinates are known, not derived from an actual selection gesture.
- Page rectangles rotate; this does not test arbitrarily slanted text.
- Incremental deletion hides the annotation in the current revision. Older
  revisions remain in the bytes; this is not secure erasure or redaction.
- Recoloring an imported specimen uses its stored quads, not the fixture's
  initial rectangle constants. The fixture note-text fallback handles Preview
  dropping `NM`; real annotation identity must not depend on matching comments.
- Safe in-place replacement, permissions, concurrent external edits, signatures,
  encryption, malformed files, and large-document costs are not covered.
- A handful of fixtures is not a general annotation-compatibility guarantee.
