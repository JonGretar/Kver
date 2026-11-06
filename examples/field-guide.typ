// Rebuild with: typst compile examples/field-guide.typ examples/field-guide.pdf
// A self-contained, fixed-break, 24-page illustrated PDF for manual reader checks.
#set page(
  paper: "us-letter",
  margin: 54pt,
  footer: context align(right, text(size: 9pt)[Kver field guide · #counter(page).display()]),
)
#set text(font: "Libertinus Serif", size: 11pt)
#set heading(numbering: "1.1")
#let blue = rgb("#316d83")
#let pale = rgb("#eaf1f1")

#v(60pt)
#text(size: 36pt, weight: "bold", fill: blue)[A field guide to the shoreline]
#v(10pt)
#text(size: 17pt)[Observations, maps, and measurements]
#v(22pt)
#image("field-guide-shore.svg", width: 100%)
#v(18pt)
This compact report stands in for an ordinary illustrated PDF. It contains
chapters, bookmarks, a contents page, figures, tables, a graph, and an
appendix. Measurements are illustrative rather than claims about a real
coast. Use it to check navigation, zoom, search, text extraction, and image
rendering in Kver. Fixed page breaks make the opening, middle, and ending
easy to locate and compare after changes to the reader.

#pagebreak()
#outline(title: [Contents], depth: 1)
#v(20pt)
*Using this sample* \
Follow a contents link or PDF bookmark to a chapter. Compare the two
landscapes, then inspect the table and graph at different zoom levels.

#let chapters = (
  (
    title: "Landscape",
    lead: "Dunes, tidal flats, and open water form a sequence of visual landmarks.",
    topics: (
      ("Reading the coast", "Look first for the dune crest, then follow the dark line of wet sand toward the water. The sketch separates these features by tone."),
      ("A walk from inland", "A path crosses grass and low shrubs before reaching the exposed beach. The distance between landmarks gives a useful sense of scale."),
      ("Light and weather", "Low light reveals texture on the dunes. A bright sky hides surface detail but makes the waterline easier to trace."),
      ("The changing edge", "The shore is not a fixed boundary. Wind and tide shift its apparent position over the course of a day."),
      ("What a map leaves out", "A map summarizes a place; it cannot show every ripple or passing cloud. Return to the picture when a symbol is ambiguous."),
    ),
  ),
  (
    title: "Field observations",
    lead: "A field log combines narrative notes with measurements that can be checked at a glance.",
    topics: (
      ("Morning transect", "The first walk records firm ground near the upper beach. Small pools remain between ridges where the sand is still damp."),
      ("Midday comparison", "The second walk follows the same route. The waterline is farther from the marked post, while cloud cover softens the shadows."),
      ("Surface texture", "Fine ripples run nearly parallel to the beach. Coarser patches collect around driftwood and shallow channels."),
      ("Notes on visibility", "Haze obscures the distant headland but not the closest markers. Nearby contrast is more dependable than a distant silhouette."),
      ("Evening return", "At the last visit the sand is cooler and the path is easier to see. The notes describe conditions without estimating a trend."),
    ),
  ),
  (
    title: "Measurements",
    lead: "Small datasets make sense when units, labels, and limitations appear together.",
    topics: (
      ("A simple series", "Five illustrative readings show how a caption distinguishes a measurement from an interpretation."),
      ("Comparing stations", "Two stations can share a scale without implying that conditions are identical. A table preserves their individual values."),
      ("Reading a graph", "The bars encode values from the appendix. Their order is fixed, and the baseline begins at zero."),
      ("Limits of precision", "A decimal place does not guarantee accuracy. Repeat observations and record the method before drawing conclusions."),
      ("A cautious summary", "This synthetic dataset demonstrates document layout and navigation; it is not environmental evidence."),
    ),
  ),
  (
    title: "Looking ahead",
    lead: "A useful report closes with questions and enough context to understand its figures later.",
    topics: (
      ("The next survey", "Repeat the route at a similar hour and note the weather. A matching procedure makes the next notes easier to compare."),
      ("What to photograph", "Include a wide view for context and a close view for texture. Put a scale marker in the frame when size matters."),
      ("Keeping records", "Give each observation a date, location, and short description. Keep original images beside the written log."),
      ("Questions worth asking", "Which features persist after a storm? Which landmarks remain reliable when the waterline has moved?"),
      ("Closing perspective", "The strongest reading combines the map, the picture, and the written account rather than relying on any one of them."),
    ),
  ),
)

#let notes = (
  "A single line can mean different things in a map and in a photograph. The pale curve marks a transition, not a surveyed property boundary.",
  "The inland trail is a useful reference point because it remains visible even when the tide covers the lower beach.",
  "The second illustration changes the palette but keeps the horizon and dune contours recognizable. Compare the two side by side.",
  "A shifting boundary is best described with a time and a viewing position. Without either, a precise-looking line can mislead.",
  "Scale bars and legends make a summary useful; an image supplies context that a simplified symbol cannot preserve.",
  "Record the starting marker before walking. A repeatable route is more valuable than a large number of unlocated observations.",
  "The visit table records categories rather than invented precision. It can be read without relying on the surrounding paragraph.",
  "Surface descriptions are qualitative. They belong beside, not in place of, measurements of depth or grain size.",
  "Contrast changes with weather. If a distant marker disappears, note the uncertainty instead of silently choosing a substitute.",
  "The final visit closes the log for the day. It does not imply that conditions will follow the same sequence tomorrow.",
  "A series is only as comparable as its collection method. Here the numbers exist to make a legible chart and table.",
  "The two rows are deliberately short. Check that both columns stay aligned when the page is magnified.",
  "Station four has the tallest bar, while station one has the shortest. Both remain labeled at ordinary reading size.",
  "Rounding can hide variability. Describe the instrument and its resolution before using extra digits in a report.",
  "The appendix repeats the five graph values so a reader can verify the visual encoding without guessing.",
  "A repeat visit should record any deviation from the original route. Similar weather alone does not make two walks equivalent.",
  "A wide picture gives orientation; a close picture captures details. Neither is a replacement for a written location.",
  "Use consistent names for files and locations. A short index is often enough to reconnect a photograph to its field note.",
  "Questions guide the next visit. Keep them separate from conclusions so new evidence can change the interpretation.",
  "Return to the opening sketch after reading the log. The report is a starting point for inquiry, not a finished survey.",
)

#for (chapter-index, chapter) in chapters.enumerate() {
  for (topic-index, topic) in chapter.topics.enumerate() {
    pagebreak()
    if topic-index == 0 {
      heading(level: 1, chapter.title)
      [#chapter.lead]
      v(18pt)
    }
    heading(level: 2, topic.at(0))
    v(12pt)
    [#topic.at(1)]
    v(18pt)
    if chapter-index == 0 and topic-index == 0 {
      figure(image("field-guide-shore.png", width: 85%), caption: [A raster illustration of the shoreline.])
    } else if chapter-index == 0 and topic-index == 2 {
      figure(image("field-guide-light.svg", width: 85%), caption: [A vector illustration at a different time of day.])
    } else if chapter-index == 1 and topic-index == 1 {
      table(
        columns: (1fr, 1fr, 1fr),
        table.header([*Visit*], [*Sky*], [*Surface*]),
        [Morning], [Clear], [Firm],
        [Midday], [Overcast], [Damp],
        [Evening], [Hazy], [Cool],
      )
    } else if chapter-index == 2 and topic-index == 2 {
      figure(
        align(center)[
          #for (i, value) in (18, 30, 24, 39, 27).enumerate() {
            box(width: 68pt, height: 160pt)[
              #align(bottom)[
                #rect(width: 42pt, height: value * 3pt, fill: blue)
                #linebreak()
                #text(size: 8pt)[Station #(i + 1)]
              ]
            ]
          }
        ],
        caption: [Illustrative readings (arbitrary units).],
      )
    } else if chapter-index == 2 and topic-index == 1 {
      table(
        columns: (1fr, 1fr, 1fr),
        table.header([*Station*], [*First*], [*Second*]),
        [North], [18], [24],
        [South], [30], [39],
      )
    } else if chapter-index == 3 and topic-index == 1 {
      figure(image("field-guide-light.svg", width: 85%), caption: [A wide view for context.])
    } else {
      block(fill: pale, inset: 18pt, width: 100%)[
        *Field note* \
        #notes.at(chapter-index * 5 + topic-index)
      ]
    }
    v(22pt)
    if ((chapter-index == 0 and topic-index == 0) or
       (chapter-index == 0 and topic-index == 2) or
       (chapter-index == 1 and topic-index == 1) or
       (chapter-index == 2 and (topic-index == 1 or topic-index == 2)) or
       (chapter-index == 3 and topic-index == 1)) {
      [#notes.at(chapter-index * 5 + topic-index)]
    }
  }
}

#pagebreak()
= Appendix: data and conventions
These values are invented to exercise a reader, not collected from a real
location. Pages are US Letter (612 × 792 points). Raster and vector artwork
is bundled locally.

#table(
  columns: (1fr, 1fr, 2fr),
  table.header([*Station*], [*Value*], [*Interpretation*]),
  [1], [18], [Low reference],
  [2], [30], [Intermediate],
  [3], [24], [Intermediate],
  [4], [39], [High reference],
  [5], [27], [Intermediate],
)

#pagebreak()
= About this specimen
*A field guide to the shoreline* is a synthetic document distributed with
Kver. It supports manual checks of loading, page jumping, outline navigation,
search, text selection, scrolling, and rendering at different zoom levels.

The illustrations were drawn for this example. No external photographs,
measurements, or live services are needed. Regenerate the PDF using the
Typst command at the start of this source file.
