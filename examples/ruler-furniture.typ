// Rebuild: typst compile --creation-timestamp 0 examples/ruler-furniture.typ examples/ruler-furniture.pdf
// Synthetic regression specimen: only isolated, repeated margin text is furniture.
#set document(title: "Ruler furniture specimen", author: "Kver")
#set page(
  width: 600pt, height: 800pt, margin: (x: 50pt, y: 70pt),
  header: context text(size: 9pt)[Coastal observations],
  header-ascent: 28pt,
  footer: context {
    let n = counter(page).get().first()
    align(center, text(size: 9pt, if n <= 5 { [Page #n of 15] }
      else if n <= 10 { [#n] } else { [42] }))
  },
  footer-descent: 28pt,
)
#set text(font: "Libertinus Serif", size: 11pt)
#for n in range(1, 16) [
  #if n > 1 { pagebreak() }
  #text(size: 18pt, weight: "bold")[Chapter #n]
  #v(18pt)
  Across the page before the columns.
  #v(12pt)
  #columns(2, gutter: 24pt)[
    Left column begins.

    Body text stays readable and remains selectable.
    Automatic movement follows this column first.

    Left column ends.
    #colbreak()
    Right column begins.

    Body text in the other column is not page furniture.
    A repeated sentence in the body must remain.

    Right column ends.
  ]
  #v(18pt)
  Figure #n: A caption spanning both columns stays in the reading flow.
  #v(1fr)
  #text(size: 9pt)[#n. A unique footnote on this page retains its original text.]
  #parbreak()
  #text(size: 9pt)[The next footnote line is also part of the passage.]
]
