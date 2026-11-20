// Rebuild: typst compile --ignore-system-fonts --creation-timestamp 0 examples/vim-tutor.typ examples/vim-tutor.pdf
#set document(title: "Kver Vim Tutor", author: "Kver")
#set page(
  paper: "a4",
  margin: (x: 52pt, y: 46pt),
  footer: context [
    #line(length: 100%, stroke: 0.6pt + rgb("#adb9b2"))
    #text(size: 9pt, tracking: 0.6pt)[KVER / VIM TUTOR #h(1fr) #counter(page).display()]
  ],
)
#set text(font: "Libertinus Serif", size: 13pt, fill: rgb("#202a2c"))
#set par(leading: 0.62em, spacing: 0.95em)
#let ink = rgb("#174c42")
#let accent = rgb("#93452e")
#let wash = rgb("#edf4ef")
#show heading: set text(fill: ink)
#show heading.where(level: 1): set text(size: 33pt)
#show heading.where(level: 2): set text(size: 17pt)
#show link: set text(fill: ink)
#show link: underline
#let tag(body) = text(size: 10pt, weight: "bold", tracking: 1.3pt, fill: accent, body)
#show raw: set text(fill: ink)
#let drill(body) = block(width: 100%, inset: 15pt, fill: wash, radius: 3pt, body)
#let keys(..rows) = table(
  columns: (auto, 1fr),
  inset: (x: 10pt, y: 7pt),
  stroke: none,
  fill: (_, y) => if calc.even(y) { wash },
  ..rows.pos(),
)

#tag[START HERE / INTERACTIVE WORKBOOK]
= Kver Vim Tutor <start>

*Learn the shortcuts by using them on this PDF.* You’ll move at different
scales, jump out and back, and mark up a passage without reaching for the mouse.
No Vim experience needed. Kver borrows Vim’s shortcuts; it isn’t a text editor.

== Switch to Vim first

+ Open *Kver → Settings → Keyboard*. You can also open Settings with
  `Cmd-,` on macOS or `Ctrl-,` on Linux and Windows.
+ Choose *Vim*, close Settings, and click the page to focus the reader.
  The preset applies to Kver, not just this PDF. Choose *Standard* in the
  same place whenever you want to switch back.
+ Press `:`. If the command palette opens, press `Esc` to close it.
  If nothing happens, check that Vim is selected and the page has focus.
  If you’ve customized your shortcuts, check the active ones in *Keyboard*.

== How to read the keys

`g g` means press `g` twice, one key at a time. `J` means `Shift-j`.
`Ctrl-d` means hold Ctrl and press `d`. `Esc` backs out of an unfinished
sequence or hint prompt. Bare-letter shortcuts depend on focus: click
the page again if a text field or another tool has the keyboard.

#drill[*First move:* press `J` to start the next lesson. If you’re
reading in another PDF viewer, use its page controls or click a lesson below.]

== Lesson map

#link(<move>)[01 Move and zoom] · #link(<find>)[02 Find and return] ·
#link(<select>)[03 Select text] · #link(<columns>)[04 Read columns] \
#link(<links>)[05 Follow links] · #link(<split>)[06 Split the view] ·
#link(<markup>)[07 Mark up and save] · #link(<finish>)[08 Put it together]

#pagebreak()
#tag[01 / NAVIGATION]
= Move and zoom <move>

Pick a distance before you move: a small adjustment, half a screen, one
screen, one PDF page, or the whole document. A screen is the visible area;
zooming changes how much of the PDF fits inside it.

#keys(
  [`j` / `k`], [Scroll down / up. With the ruler on, follow lines instead.],
  [`h` / `l`], [Pan left / right when the page is wider than the view.],
  [`Ctrl-d` / `Ctrl-u`], [Move down / up half a screen. `Space` / `Shift-Space` also work.],
  [`Ctrl-f` / `Ctrl-b`], [Move forward / back one screen. `Ctrl-f` is not Find in Vim.],
  [`J` / `K`], [Go to the next / previous PDF page.],
  [`g g` / `G`], [Go to the start / end of the document.],
  [`=` (or `+`) / `-`], [Zoom in / out.],
  [`w` / `W`], [Fit width / fit the whole page.],
  [`s` / `c`], [Use Single Page / Continuous reading.],
)

#drill[
  *Try it.* Press `s`, then `w`. Zoom in a few times with `=` until the
  page is wider than the pane. Pan with `l` and `h`. Press `Ctrl-d` to
  move down within this page, then `J` to turn the page. Press `K` to
  return, `W` to restore a whole-page view, and `c` to try scrolling
  across a page boundary.
]

*Check:* In Single Page mode, scrolling stays on the current PDF page;
`J` and `K` turn pages. In Continuous mode, scrolling can carry you
across a page boundary. When you’re done, go to the next lesson with
`J` from this page, or use #link(<find>)[Find and return].

#pagebreak()
#tag[02 / SEARCH, CONTENTS, AND MARKS]
= Find and return <find>

Use *Find* for a phrase, *Contents* for a section, and a *mark* for a place
you choose yourself. Use history to retrace a jump without losing your place.

== Search for a word

#drill[
  Press `/` and type `tidemark`. Press `Enter` to go to a result, then
  `Esc` to return to the reader. Press `n` for the next occurrence and
  `N` for the previous one. Look for another occurrence in the link
  destination on page 9. Press `Ctrl-o` to go back along your jump
  history and `Ctrl-i` to go forward.
]

== Use the PDF outline

Press `g c` to open Contents. This PDF has an outline; not all PDFs do.
In Contents, `j` and `k` move through the entries. Press `Enter` on
*Find and return*, then `Esc` if the sidebar is still open.

== Leave a mark

Practice on this page, or open a local copy if Kver doesn’t offer marks in
your web preview. Marks are reader bookmarks, not PDF highlights.

#keys(
  [`m a`], [Set mark a at the current reading position. Pick another lowercase letter if a is taken.],
  [`' a`], [Jump to mark a. The first key is an apostrophe.],
  [`d m a`], [Delete mark a. Confirm if prompted.],
  [`Cmd-Shift-m` / `Ctrl-Shift-m`], [Open Marks on macOS / Linux and Windows.],
)

*Try it:* Set a mark, press `G`, then jump back to it. Delete the practice
mark when you’re done. All three sequences work with letters a–z.

#pagebreak()
#tag[03 / WORD AND LINE HINTS]
= Select text <select>

Hints are two-letter labels Kver places on visible text. Read the labels
that appear in your own window; they can change with zoom and position.
You choose both ends of the selection, not a moving Vim caret.

== Select a phrase with `v`

#drill[
  The blue notebook stays beside the window. \
  The last train leaves before sunrise. \
  A small lantern marks the end of the path.
]

+ With the practice text visible, press `v`. Type the hint on *blue*,
  then the hint on *notebook*. Both words should be selected, but not
  the words on either side.
+ Press `y` to yank (copy) the selected text to the clipboard. Paste into
  a text field to check that it says *blue notebook*. `y` also works
  after selecting a whole line or dragging over text with the mouse.
+ Repeat with one word’s hint twice. Then try the two endpoints in
  reverse order: you should get the same range.

== Select whole lines with `V`

Press `V`. Type the hints at the start of the line about the train and
the line about the lantern. Both endpoint lines are included in full,
even if some text is clipped. Use the same hint twice for one whole line.

*Recover a hint:* `Backspace` erases a typed hint letter. If nothing is
typed at the second endpoint, it returns to the first choices. `Esc`
cancels without replacing the previous selection. If there are too many
visible words, zoom in and start again. Endpoints must be on the same
PDF page. Selecting text does not edit the PDF.

#pagebreak()
#tag[04 / READING RULER]
= Read columns <columns>

Use the ruler to hold your place in dense prose. In this specimen the
reading order is down the left column, then down the right, not across
the gutter at each row. Complex tables and sidebars may not read as neatly.

*Try it.* Get both columns in view (`W` fits the page). Option-click
*Left begins* on macOS, or Alt-click it on Linux and Windows, to place
the ruler. Then use `j` to step through the passage and `k` to retrace it.
The `r` key toggles the ruler without the mouse.

#v(16pt)
#columns(2, gutter: 44pt)[
  Left begins. A boat leaves at dawn. \
  Its crew follows the shore. \
  They pass a low stone wall. \
  Beyond it, the water narrows. \
  Left ends at the old pier.
  #colbreak()
  Right begins. A bell rings twice. \
  The crew ties up at the quay. \
  The tide covers the mud. \
  The town is waking up. \
  Right ends at the market.
]
#v(16pt)

#drill[
  *Check the handoff:* From *Left ends*, press `j` once. The ruler should
  move to *Right begins*. Press `k` to step back. Then toggle the ruler
  off with `r` so `j` and `k` scroll again.

  *Try a cross-column range:* Press `v` and select from *pier* to *bell*.
  The range should run from the end of the left column into the start
  of the right column, not sweep across every row.
]

#pagebreak()
#tag[05 / LINK HINTS]
= Follow links <links>

`f` labels visible links rather than words. Type the two letters for the
link you want. `Backspace` corrects a letter; `Esc` cancels. No link moves
until you finish the hint.

#keys(
  [`f`], [Follow a link. A destination in this PDF replaces the current view.],
  [`F`], [Open a destination in this PDF in a new window.],
  [`g w f`], [Open a destination in this PDF in the other pane.],
)

#drill[
  Press `f`, then type the hint on #link(<destination>)[Signal station].
  Look for the signal report on page 9. Press `Ctrl-o` to return here.
  Try `F` with the same link: compare the new window with the original,
  then close the new window with `Cmd-w` on macOS or `Ctrl-w` elsewhere.
]

The #link("https://github.com/JonGretar/Kver")[Kver project page] is an
external link, not a destination in this PDF. It opens in your browser.
External links are not candidates for the same-document split command.

Next: #link(<split>)[Keep two places in view].

#pagebreak()
#tag[06 / TWO PANES, ONE PDF]
= Split the view <split>

Keep your place while you check a reference. The two panes show the same
document, but each has its own position, zoom, layout, selection, ruler,
and jump history. Reader shortcuts act on the active pane.

#keys(
  [`g w v` / `g w s`], [Split side by side / top and bottom. If split already, change its orientation.],
  [`g w w` / `Ctrl-Tab`], [Switch the active pane.],
  [`g w f`], [Show link hints for this PDF. Follow one in the other pane, creating a split if needed.],
  [`g w c`], [Close the active pane and keep the other one.],
  [`g w o`], [Keep the active pane and close the other one.],
)

#drill[
  Start with one pane. Press `g w f` and type the hint on
  #link(<destination>)[Signal station]. The new pane should show the
  destination and take focus, while the original stays on this lesson.
  Press `w` to fit the active pane’s width, then `g w w` to return focus
  to the lesson. Press `g w s` to stack the panes, then `g w o` with
  this lesson active to keep it on its own.
]

*Try the other close command:* Use `g w v` to duplicate the current view.
Go to another page in the active pane with `J` or `K`, then use `g w c`.
The pane that stayed at this lesson should remain. Neither close command
closes the PDF window.

#pagebreak()
#tag[07 / TEXT MARKUP]
= Mark up and save <markup>

Select text first, then choose a markup. The `a` in `a h` is a sequence
prefix, not a mode. The command acts once and keeps the text selected.

#drill[
  Use `v` to select the first four words of this sentence: \
  Check the red marker before you cross the bridge.

  Press `a h` to highlight the selection. Press `u` to undo, then
  `Ctrl-r` to redo. The highlight should disappear and reappear.
]

#keys(
  [`a h` / `a u`], [Highlight / underline.],
  [`a s` / `a w`], [Strikethrough / squiggly underline.],
  [`a a`], [Apply the last-used markup type (initially Highlight).],
  [`g a`], [Open Annotations. Use `j` and `k` to move through the list.],
  [`u` / `Ctrl-r`], [Undo / redo annotation changes in the reader or list.],
  [`x`], [Delete the targeted markup. In the list, this targets the selected entry.],
)

*Try it:* Open Annotations with `g a`, select your practice highlight in
the list and press `x` to remove it. Confirm if prompted. Undo if you want
it back. `Esc` dismisses the sidebar. To change a highlight’s color, use the
annotation controls; there are no default keys for individual colors.

*Keep a copy:* Use *Save As* (`Cmd-Shift-s` on macOS, `Ctrl-Shift-s` on
Linux and Windows) to save your work without changing the original file.
For a web preview, save locally to keep your annotations. Ordinary *Save*
is `Cmd-s` / `Ctrl-s`.

#pagebreak()
#tag[08 / PRACTICE AND REFERENCE]
= Put it together <finish>

== Signal station: the link destination <destination>

#drill[
  *Signal report.* The tidemark is below the red post. \
  Keep the original observation in view while you compare the report. \
  Return to your source before making a note.
]

== One last drill

+ Press `g g` to start at the cover. Use `f` and the hint for *Put it
  together* to return to this page. Use `Ctrl-o` to go back to the cover.
+ Use `/` to find *Signal report*. Close Find with `Esc`, then use `V`
  on the first line of the report. Underline it with `a u`, undo with
  `u`, and redo with `Ctrl-r`.
+ Follow #link(<columns>)[Read columns] in a split with `g w f` and keep
  this report in view. Switch panes with `g w w`. Close an unneeded pane
  with `g w c`.

== Beyond the lessons

#keys(
  [`:`], [Open the command palette. Search by command name to see its active shortcut.
    `Cmd-Shift-p` / `Ctrl-Shift-p` also opens it.],
  [`o` / `O`], [Open a local PDF / Open Recent from the reader.],
  [`Cmd-n` / `Ctrl-n`], [Open a new window on macOS / Linux and Windows.],
)

Vim’s key sequences save the most time when you combine them: find a
passage, open its reference beside it, then select and annotate without
leaving the reader. For platform aliases, overrides, or commands without
default keys, open *Settings → Keyboard* or see
#link("https://github.com/JonGretar/Kver/blob/main/docs/keybindings.md")[Customize keyboard shortcuts].
