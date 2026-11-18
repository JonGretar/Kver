# Customize keyboard shortcuts

Edit `~/Library/Application Support/com.JonGretar.Kver/config.toml` on macOS
to customize keys. Create the file if it does not exist; keep any existing
settings in it. Kver applies valid edits while it runs. Choose Standard or
Vim in **Kver → Settings → Keyboard**, or set `preset = "vim"` under `[keyboard]`
in the same file.

Kver embeds two complete TOML presets in the executable. It selects one,
filters its entries for the current OS, then applies your bindings. These
are the only two policy layers; Vim does not inherit Standard, and Kver
does not look up preset files at runtime.

## Add a key sequence

Add this entry to go to the start of the document by pressing `g`, then `t`
while the reader has focus:

```toml
[[keyboard.bindings]]
keys = "g t"
context = "reader"
command = "document-start"
```

Each `[[keyboard.bindings]]` entry assigns one complete key sequence to one
command in one context. `g t` can coexist with Vim's `g g`; the two sequences
share a first key but neither complete sequence is a prefix of the other.
`document-start` means the beginning of the document, **not** the top of the
current page.

## Rebind a key or add an alias

The same key and context replace the inherited assignment. For example,
when using the Vim preset, this changes `right` from horizontal scrolling to
turning to the next page:

```toml
[[keyboard.bindings]]
keys = "right"
context = "reader"
command = "next-page"
```

A *different* key adds an alias without removing existing keys for the
command:

```toml
[[keyboard.bindings]]
keys = "ctrl-e"
context = "reader"
command = "next-page"
```

You can repeat `[[keyboard.bindings]]` for different keys. Do not assign the
same key and context twice for the same platform. Removing a default
assignment without choosing another command is not supported yet:
`[[keyboard.unbind]]` is rejected.

## Fields

| Field | Meaning |
| --- | --- |
| `keys` | Required. A key such as `"right"` or `"cmd-shift-p"`, or a space-separated sequence such as `"g t"`. Use `cmd`, `ctrl`, `alt`, and `shift` as modifier names. |
| `context` | Optional; defaults to `"global"`. Use `"reader"`, `"sidebar"`, `"contents"`, `"annotations"`, `"settings"`, or `"about"` to limit where the shortcut works. Contents and Annotations inherit Sidebar; a reader shortcut does not fire while typing in a dialog or palette. |
| `command` | Required. Lowercase, hyphenated command name; see the list below. |
| `platform` | Optional. One of `"mac"`, `"linux"`, or `"windows"`, or a nonempty array of those names. An array matches any listed platform. Omit it to apply on every platform. |
| `letter` | Optional for `set-local-mark`, `jump-local-mark`, and `delete-local-mark` only. One lowercase letter `"a"`–`"z"` targets that mark directly; without it, the command opens its chooser. |

For example, add `platform = "mac"` to an entry to use that key only on macOS.
Use `platform = ["linux", "windows"]` to share a binding on Linux and Windows:

```toml
[[keyboard.bindings]]
keys = "ctrl-e"
context = "reader"
command = "next-page"
platform = ["linux", "windows"]
```

The same filter syntax applies to embedded presets and your overrides.
Empty arrays, unknown names, and non-string values are rejected. Filtering
happens before duplicate and prefix checks: two entries for the same key
and context may target disjoint platforms, but must not overlap on the
current OS. An array's order does not establish precedence.

Use full key sequences: `g` alone conflicts with `g g` in the Vim reader,
whereas `g t` does not. Invalid keys, duplicate assignments, conflicting
prefixes, unknown fields, commands, contexts, and platforms are rejected
with diagnostics. Kver keeps the last valid keymap active until the file is
fixed. A valid edit also updates native menu shortcuts and command-palette
shortcut hints.

On macOS, Kver rejects a rebind if it would make a reader-only typing key
(including a Shift-only key) into a native menu accelerator. For example,
changing `cmd-alt-right` away from Next Page while `right` remains its first
reader key is unsafe. Rebind `right` too, or keep the modified shortcut.

The operating system gets first chance to handle some key combinations.
For example, macOS uses Command-Tab to switch apps and Command-Space for
Spotlight by default. A TOML entry cannot make Kver take those system
shortcuts over; choose keys the OS delivers to Kver. Native input and menu
behavior on Linux and Windows still belongs to their separate delivery
checks, not to the macOS or headless test results.

## Command names

Use the exact `command` value, not a menu label. These names are recognized
by the current keyboard configuration:

| Task | Commands |
| --- | --- |
| Application and windows | `open-pdf`, `save`, `save-as`, `show-recent`, `new-window`, `close-window`, `next-window`, `previous-window`, `quit`, `about-kver`, `check-for-updates`, `show-settings`, `close-settings`, `close-about`, `command-palette` |
| Reader and navigation | `next-page`, `previous-page`, `navigate-back`, `navigate-forward`, `document-start`, `document-end`, `read-down`, `read-up`, `scroll-down`, `scroll-up`, `scroll-left`, `scroll-right`, `scroll-screen-down`, `scroll-screen-up`, `scroll-half-screen-down`, `scroll-half-screen-up` |
| Reader display | `zoom-in`, `zoom-out`, `fit-width`, `fit-page`, `single-page`, `continuous`, `use-default-reading-mode`, `toggle-pdf-dark-mode`, `copy` |
| Split reading | `split-side-by-side`, `split-stacked`, `switch-pane`, `close-pane`, `keep-only-pane` |
| Reading ruler | `toggle-reading-ruler`, `next-ruler-line`, `previous-ruler-line` |
| Text selection | `visual-select`, `visual-select-line` |
| Links | `follow-link`, `follow-link-in-new-window` |
| Find and sidebar | `show-find`, `next-match`, `previous-match`, `show-contents`, `toggle-sidebar`, `activate-outline`, `select-down`, `select-up` |
| Marks | `show-marks`, `set-local-mark`, `jump-local-mark`, `delete-local-mark` |
| Text markup | `apply-markup`, `highlight`, `underline`, `strikethrough`, `squiggly-underline`, `show-annotations`, `undo-annotation`, `redo-annotation`, `delete-highlight`, `recolor-highlight-yellow`, `recolor-highlight-green`, `recolor-highlight-blue`, `recolor-highlight-pink`, `recolor-highlight-red` |

`f` starts **Follow link…** and `F` starts **Follow link in new window…**
in both presets. Type a visible link's two-letter hint to follow it; Backspace
corrects and Escape cancels. Same-document links navigate in place with `f`
and open in another window with `F`. Links to another local PDF always open
in a new window. HTTP(S) links open in the default browser with either
command, just as they do when clicked; Kver does not download them.

`read-down` and `read-up` follow lines with the reading ruler on and scroll
with it off. `select-down` and `select-up` move the focused sidebar list's
selection. The mark commands open their chooser unless `letter` is supplied.
For example, `command = "jump-local-mark"` with `letter = "a"` jumps directly
to local mark `a`. The Vim preset supplies its letter-specific sequences
using this same field.

## Platform and preset defaults

Both presets use Command on macOS and Ctrl on Linux/Windows for application
commands: Open, Open Recent, New/Close Window, Save/Save As, Settings, the
command palette, and the marks popup. `cmd` means the actual platform/meta key
(Command, Super, or Windows), not Control; use `ctrl` for Control in overrides.
`secondary` selects Command on macOS and Ctrl on Linux/Windows.

Some assignments deliberately differ rather than translating modifiers:

| Task | macOS | Linux / Windows |
| --- | --- | --- |
| Cycle Kver windows | Command-` / Command-Shift-` | Ctrl-F6 / Ctrl-Shift-F6 |
| Modified next / previous page, both presets | Command-Option-Right / Left | Ctrl-PageDown / PageUp |
| Standard Fit Page | Command-Option-F | Ctrl-0 |
| Standard Fit Width | Command-Option-W | Ctrl-Alt-W |
| Annotation redo, both presets | Command-Shift-Z | Ctrl-Shift-Z on Linux; Ctrl-Y on Windows |
| Quit application | Command-Q | Ctrl-Q on Linux; no default on Windows |

Windows retains the OS's Alt-F4 window-close behavior; Ctrl-W closes a Kver
window in either preset. Kver does not bind Super/Windows shortcuts by default.
Ctrl-Alt arrows are not page shortcuts: Linux desktops may reserve them for
workspace switching.

Standard uses Command-F for Find, Command-B for the sidebar,
Command-G / Command-Shift-G for Find matches, and Command-T for Contents on
macOS. Linux/Windows use Ctrl for these commands and also F3 / Shift-F3 for
Find matches. Vim does not install these Standard commands: Ctrl-F / Ctrl-B
remain reader scrolling, not Find/sidebar.

Standard history uses Alt-Left / Alt-Right in Reader and Contents. macOS also
retains Control-O / Control-I. On Linux/Windows, Standard Ctrl-O opens a PDF,
including from Contents; it never means Back.

Vim retains Control-O / Control-I for Back / Forward in Reader and Contents on
every OS. Use `o` to open a PDF or `O` for Open Recent from the reader. On
Linux/Windows, Ctrl-O opens a PDF elsewhere, but the more-specific
Reader/Contents history binding takes precedence there. On macOS, Command-O
continues to open from either context. Effective shortcut lists omit a
shadowed Open assignment.

Vim distinguishes PDF pages from viewport-sized scrolling:

| Task | Vim shortcut |
| --- | --- |
| Scroll down / up (follow lines in ruler mode) | `j` / `k` |
| Toggle reading ruler | `r` |
| Select a word range using hints | `v` |
| Select a whole-line range using hints | `V` |
| Next / previous PDF page | `J` / `K` |
| Forward / backward one screen | Ctrl-F / Ctrl-B |
| Down / up half a screen | Ctrl-D / Ctrl-U or Space / Shift-Space |
| Next / previous Find match | `n` / `N` |
| Open Find | `/` |
| Open Contents | `g c` |
| Start / end of document | `g g` / `G` |

These Vim keys belong to the reader, not text inputs or sidebar lists.
`p` no longer turns pages and has no default binding. Escape dismisses the
sidebar in either preset. Native menu commands remain available in Vim without
turning bare reader keys into macOS shortcuts that intercept input text.

## Split reading

Split commands appear in the menu and command palette. A window shows one
PDF in at most two panes; changing orientation rearranges the existing views.

| Command | Vim | Standard macOS | Standard Linux / Windows |
| --- | --- | --- | --- |
| Split side by side | `g w v` | Command-\\ | Ctrl-\\ |
| Split top and bottom | `g w s` | Command-Shift-\\ | Ctrl-Shift-\\ |
| Switch pane | `g w w` or Ctrl-Tab | Ctrl-Tab | Ctrl-Tab |
| Close pane | `g w c` | Unbound | Unbound |
| Keep only this pane | `g w o` | Unbound | Unbound |

Reader commands target the active pane. Each pane keeps its own location,
zoom, layout, selection, ruler, and Back/Forward trail. The sidebar and Find
query remain shared: choosing a destination moves only the intended pane;
switching panes alone never jumps to a Find result.

Inputs, sidebar lists, menus, and dialogs keep their own keyboard behavior.
The modified Switch pane shortcut also works during unfinished word, line,
or link targeting; switching cancels that targeting, not a completed selection.
Scrolling and zoom gestures follow the pointer without changing the keyboard
target. Closing a pane preserves the surviving view and never means Close
Window.

## Reading ruler

Choose **Toggle reading ruler** from the View menu or command palette, or
Option/Alt-click text to place the guide.

| Task | Standard | Vim |
| --- | --- | --- |
| Toggle reading ruler | Command-Shift-R on macOS; Ctrl-Shift-R on Linux/Windows | `r` |
| Next / previous line with ruler on; scroll down / up with ruler off | Down / Up | Down / Up or `j` / `k` |

Standard never binds bare `r`. Vim's Ctrl-R remains annotation redo.
These shortcuts belong to the focused reader: inputs, menus, dialogs, Find,
and focused Contents/Annotations keep their own navigation. In Vim, `j`/`k`
also navigate the focused Contents or Annotations list.
Escape leaves ruler mode after dismissing a more-specific interaction.

Assign `toggle-reading-ruler`, `next-ruler-line`, and `previous-ruler-line`
in the `reader` context to use your preferred keys in either preset.
Explicit line commands do nothing while the ruler is off; explicit
`scroll-down` / `scroll-up` always scroll, even with the ruler on.
Line movement follows reading order across columns and pages.
The guide belongs to the current window; its remembered location follows
reading-history privacy settings and is never saved in the PDF.

## Visual select

Choose **Visual select…** in the command palette, or press `v` with the Vim
reader focused. Type the two-letter label of the first word, then the label
of the last word. Both words are included; choosing them in reverse selects
the same passage, and choosing one word twice selects just that word.
For whole lines, choose **Visual select line…** (`V` in Vim). Each visible line
is labelled at its first visible eligible word. Both endpoint lines are included
in full, including clipped text; the same label twice selects one full line.
Endpoints must be on the same page. The second stage keeps the original labels
and marks the chosen start in red with underlined letters.

Backspace corrects a prefix. With no endpoint prefix, Backspace returns to the
start choices. Escape or the ribbon’s × button exits without replacing the
previous selection.
Scrolling, zooming, resizing, changing the reading layout, or moving focus to
another tool exits targeting; invoke Visual select again rather than reuse old
labels. If there are too many visible words, zoom in and retry.

The result is ordinary text selection: copy it or apply any text-markup type.
Selection alone does not edit the PDF. This is not a caret or persistent Vim
visual mode, and it does not select across pages.

Standard has no bare-letter default. To assign a shortcut in either preset:

```toml
[[keyboard.bindings]]
keys = "v"
context = "reader"
command = "visual-select"
```

Use `command = "visual-select-line"` to bind whole-line targeting instead.

## Annotation shortcuts

Creation shortcuts apply once to the reader's current text selection, retain
that selection, and remember the chosen type in that window. They do not arm a
persistent tool or work while typing in an input. `apply-markup` repeats the
window's remembered type, initially Highlight.

| Command | Standard on macOS | Vim |
| --- | --- | --- |
| Apply remembered type | Command-Enter | `a a` |
| Highlight | Command-Shift-H | `a h` |
| Underline | Command-Shift-U | `a u` |
| Strikethrough | Command-Shift-X | `a s` |
| Squiggly underline | Command-Option-U | `a w` |
| Open Annotations | Command-Shift-A | `g a` |

The Standard shortcuts use Ctrl instead of Command outside macOS, and Alt
instead of Option. The modified creation shortcuts are not installed by Vim.
In Vim, `a` is a sequence prefix, not a mode; Escape cancels a pending sequence.
Both presets also expose creation through the toolbar, selection context menu,
Edit > Annotation menu, and command palette.

Both presets use Command-Z for annotation undo on macOS and Ctrl-Z on
Linux/Windows. Redo uses Command-Shift-Z on macOS, Ctrl-Shift-Z on Linux, and
Ctrl-Y on Windows. Backspace/Delete removes markup in the reader or Annotations
list. Vim additionally uses `u` / Ctrl-R for annotation undo/redo and `x` for deletion. These commands
operate only in their reader/list contexts; text inputs keep their own editing.
List deletion targets the selected list entry, not a previous page target.

All types share annotation undo/redo and the last-used color per type for the
current document session. No individual colors have default shortcuts.
Despite their historical names, `delete-highlight` handles supported markup of
any type; `recolor-highlight-*` recolors the annotation selected in the focused
page or Annotations list.
The legacy `recolor-highlight-purple` command remains accepted with its original
purple color; the five UI presets now offer Red instead of Purple.

Command-S (Ctrl-S) saves changes
to the current PDF. Command-Shift-S (Ctrl-Shift-S) opens Save As and suggests an
`-annotated.pdf` filename. Both presets share these shortcuts.
