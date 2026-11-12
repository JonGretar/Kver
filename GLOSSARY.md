# Kver

Kver is a keyboard-first PDF reader. These terms distinguish the document,
its presentation, and the reading state that can outlive a window.

## Language

**Document**:
A PDF identified by its local file, independent of how many windows display it.
_Avoid_: Tab, page

**Document window**:
An independent reading workspace with a document, navigation tools, and window
controls. Two windows can display the same document at different places.
_Avoid_: Document tab

**Reader view**:
The central PDF surface within a document window, not its sidebar or window
controls.
_Avoid_: Preview, entire window

**Reading position**:
A place within a document, identified by a page and a page-local offset in PDF
points, independent of zoom.
_Avoid_: Screen position, pixel offset

**Local mark**:
A document-specific lowercase letter identifying a saved reading position,
optionally with a display name. Naming a mark does not create a separate bookmark.
_Avoid_: Global mark, named bookmark

**Annotation**:
A persistent addition to a document, distinct from its underlying page content
and from temporary selection or search-result indicators.
_Avoid_: Local mark, search highlight

**Text markup**:
An annotation tied to a region of text, such as a highlight, underline, or
strikethrough.
_Avoid_: Text edit, highlight when referring to every markup type

**Highlight**:
A text markup that emphasizes text with a colored background.
_Avoid_: Search match, text selection, annotation when referring specifically to a highlight

**Reading history**:
The shared record of opened documents and their reading positions, reading-mode
choices, and local marks. Recording can be disabled while new state remains
available for the current session.
_Avoid_: Recent-file list when referring to all saved reading state

**Single Page mode**:
A reading mode that displays and scrolls within one selected page; turning pages
is a separate command.
_Avoid_: Document tabs, continuous scrolling

**Continuous mode**:
A reading mode in which the document's pages form one scrollable sequence.
_Avoid_: Single Page mode
