// Rebuild with: typst compile examples/reader-sample.typ examples/readme-sample.pdf
// Distinct page labels make first/middle/last navigation easy to inspect.
#set page(paper: "us-letter", margin: 54pt)
#set text(font: "Libertinus Serif", size: 12pt)

#for number in range(1, 101) {
  align(right)[KVER / READER SAMPLE]
  v(90pt)
  text(size: 35pt, weight: "bold")[Page #number]
  v(20pt)
  [A small, hundred-page PDF for checking page navigation and zoom.
  Only the selected page should be rasterized. Move between pages with
  the left and right arrow keys, N and P, or the toolbar controls.]
  v(40pt)
  [This is page #number of 100. The first, middle, and last pages have
  different labels, making stale renders and off-by-one errors visible.]
  if number < 100 { pagebreak() }
}
