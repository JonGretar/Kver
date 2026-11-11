// Rebuild: typst compile --creation-timestamp 0 examples/reading-order.typ examples/reading-order.pdf
// A durable reading/selection specimen, not a claim of semantic layout support.
// Pages 1–3 have ordinary column flows. Page 4 deliberately stresses tables,
// math, notes, and references: inspect output rather than assuming linear prose.
#set document(title: "Margins & Methods", author: "Kver")
#set page(
  paper: "a4",
  margin: (x: 46pt, y: 44pt),
  footer: context [
    #line(length: 100%, stroke: 0.4pt + rgb("#81928c"))
    #v(5pt)
    #text(size: 8pt)[KVER / MARGINS & METHODS #h(1fr) #counter(page).display()]
  ],
)
#set text(font: "Libertinus Serif", size: 10pt)
#set par(justify: true, leading: 0.65em)
#let green = rgb("#254d43")
#let rust = rgb("#a34f35")
#let pale = rgb("#edf1eb")
#show heading: set text(fill: green)
#show link: underline
#let label(body) = block(above: 0pt, below: 8pt, text(size: 8pt, weight: "bold", fill: rust, body))
#let prose = [
  At the edge of the estuary, a painted post marks the start of a repeatable
  walk. The observer records the time, cloud cover, and water level before
  crossing the sand. These details matter more than the apparent precision
  of a line on a map: the same shoreline can look different an hour later.
]
#let repeat-note = [
  A useful record separates observation from interpretation. Dark sand may
  indicate moisture, shade, or a change in grain size. The notebook keeps the
  description beside the conditions under which it was made. A later visitor
  can then compare like with like instead of silently filling gaps in the evidence.
]

#label[01 / FIELD MAGAZINE]
#v(8pt)
#text(size: 38pt, weight: "bold", fill: green)[Where the water was]
#v(6pt)
#text(size: 14pt)[A shoreline essay about observation, uncertainty, and the spaces between columns.]
#v(16pt)
#columns(2, gutter: 24pt)[
  *Left column begins.* #prose

  #repeat-note

  == Looking twice
  #prose

  #block(inset: 12pt, fill: pale, width: 100%)[
    #text(size: 17pt, style: "italic", fill: green)[“A boundary needs a time as well as a place.”]
  ]

  #repeat-note

  *Left column ends.*
  #colbreak()
  *Right column begins.* Follow the previous column to its end before
  reading this sentence. A drag between the two columns should follow that
  same path; a drag entirely inside the first must not include this one.

  #figure(
    image("field-guide-light.svg", width: 100%),
    caption: [Evening light changes contrast without moving the reference post.],
  )

  #repeat-note

  == A return route
  #prose

  #repeat-note

  Visit #link("https://example.org/shoreline")[the field notebook] for an
  example external link. The address is illustrative, not a data source.

  *Right column ends.*
]

#pagebreak()
#label[02 / THREE-COLUMN DISPATCH]
#text(size: 29pt, weight: "bold", fill: green)[Three ways to read a coast]
#v(12pt)
#columns(3, gutter: 18pt)[
  *First column begins.*
  == Walk
  #prose

  #repeat-note

  A short final paragraph leaves an uneven column edge. White space is not
  an instruction to jump sideways into the next line of another column.

  *First column ends.*
  #colbreak()
  *Second column begins.*
  == Measure
  #repeat-note

  #prose

  #repeat-note

  *Second column ends.*
  #colbreak()
  *Third column begins.*
  == Compare
  #prose

  #repeat-note

  #prose

  *Third column ends.*
]
#v(20pt)
#line(length: 100%, stroke: 0.5pt + green)
#text(size: 11pt)[*Across the page.* This full-width closing note follows all three columns, not their first row.]

#pagebreak()
#label[03 / RESEARCH ARTICLE]
#align(center)[
  #text(size: 22pt, weight: "bold")[Repeatable shoreline observations]

  #text(size: 10pt)[E. Strand and M. Reed · Synthetic Field Methods Laboratory]
]
#v(8pt)
#block(fill: pale, inset: 12pt, width: 100%)[
  *Abstract.* This synthetic article examines the difference between a
  repeatable procedure and a precise-looking result. Two columns, section
  headings, mathematical notation, and citations stand in for an ordinary
  scientific PDF. No environmental conclusions should be drawn from its values.
]
#v(14pt)
#columns(2, gutter: 24pt)[
  *Article left begins.*
  == 1. Introduction
  #prose

  #repeat-note

  == 2. Methods
  Each station is observed at a fixed interval. Distance $d$ is expressed
  in metres and elapsed time $t$ in seconds. A simple rate is
  $ v = d / t. $
  The formula is a layout example, not a physical model of tidal change.

  #repeat-note

  A café, a naïve estimate, and Þingvellir exercise mapped Unicode text.
  Hyphenated terms and justified spacing should remain readable when copied.

  *Article left ends.*
  #colbreak()
  *Article right begins.*
  == 3. Results
  The mean of $n$ observations is written
  $ overline(x) = 1/n sum_(i=1)^n x_i. $
  Equations may have a different PDF drawing order from surrounding prose.
  Their presence does not imply that copying reconstructs mathematical source.

  #prose

  == 4. Discussion
  #repeat-note

  #prose

  The procedure is described in the #link(<supplement>)[supplement on the next page].
  Clicking that link should navigate; a selection dragged from surrounding
  prose across its text should not navigate.

  *Article right ends.*
]

#pagebreak()
#label[04 / COMPLEX-LAYOUT STRESS CASES]
= Supplement: data and interpretation <supplement>
This page is intentionally not a prose-only reading-order oracle. Tables
need row and cell semantics, equations need mathematical structure, and
footnotes need an association with their callouts. Use it to identify limits,
not to infer that geometric column detection solves these structures.

#figure(
  table(
    columns: (1.3fr, 1fr, 1fr, 2fr),
    inset: 7pt,
    table.header([*Station*], [*Distance (m)*], [*Time (s)*], [*Observation*]),
    [North], [18], [30], [Wet, fine sand],
    [Central], [24], [45], [Shallow channel],
    [South], [39], [60], [Coarse grains],
  ),
  caption: [Invented station values. Read rows across cells, not as four prose columns.],
)
#v(10pt)
#columns(2, gutter: 24pt)[
  == Notes and uncertainty
  A confidence interval is not a guarantee about an individual visit.
  The illustrative report uses $x plus.minus sigma$ to distinguish a
  central estimate from variability.#footnote[This footnote belongs to the
  uncertainty sentence; its placement alone does not establish copy order.]

  #repeat-note

  #block(inset: 10pt, fill: pale)[
    *Editorial sidebar.* Sidebars can interrupt or supplement a story.
    Their intended insertion point cannot always be recovered from geometry.
  ]
  #colbreak()
  == References
  [1] Strand, E. (2025). _Keeping a repeatable field notebook._
  Fictional Methods Review, 12(2), 18–24.

  [2] Reed, M. (2026). _What a shoreline sketch leaves out._
  Imaginary Coastal Studies, 4(1), 39–45.

  == Selection checks
  Select forward and backward within a column, then across columns.
  Compare the highlighted text with a paste into a plain-text editor.
  Repeat after zooming and scrolling. Headers, captions, pull quotes,
  references, tables, and footnotes deserve separate inspection.
]
