# Customize keyboard shortcuts

Edit `~/Library/Application Support/com.JonGretar.Kver/config.toml` on macOS
to customize keys. Create the file if it does not exist; keep any existing
settings in it. Kver applies valid edits while it runs. Choose Standard or
Vim in **Kver → Settings → Keyboard**, or set `preset = "vim"` under `[keyboard]`
in the same file.

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
| `platform` | Optional; one of `"mac"`, `"linux"`, or `"windows"`. Without it, the entry applies on every platform. |

For example, add `platform = "mac"` to an entry to use that key only on macOS.
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
| Reader and navigation | `next-page`, `previous-page`, `navigate-back`, `navigate-forward`, `document-start`, `document-end`, `scroll-down`, `scroll-up`, `scroll-left`, `scroll-right`, `scroll-screen-down`, `scroll-screen-up`, `scroll-half-screen-down`, `scroll-half-screen-up` |
| Reader display | `zoom-in`, `zoom-out`, `fit-width`, `fit-page`, `single-page`, `continuous`, `use-default-reading-mode`, `toggle-pdf-dark-mode`, `copy` |
| Reading ruler | `toggle-reading-ruler`, `next-ruler-line`, `previous-ruler-line` |
| Find and sidebar | `show-find`, `next-match`, `previous-match`, `show-contents`, `toggle-sidebar`, `activate-outline` |
| Marks | `show-marks`, `set-local-mark`, `jump-local-mark`, `delete-local-mark` |
| Text markup | `apply-markup`, `highlight`, `underline`, `strikethrough`, `squiggly-underline`, `show-annotations`, `undo-annotation`, `redo-annotation`, `delete-highlight`, `recolor-highlight-yellow`, `recolor-highlight-green`, `recolor-highlight-blue`, `recolor-highlight-pink`, `recolor-highlight-red` |

The mark commands in this list open the corresponding mark chooser. The Vim
letter-specific mark sequences are supplied by the Vim preset; the TOML
command names do not accept a mark letter argument.

## Find and navigation presets

The Standard preset owns Command-F for Find, Command-B for the sidebar,
Command-G / Command-Shift-G for Find matches, and Command-T for Contents.
These shortcuts are not installed by Vim.

Vim distinguishes PDF pages from viewport-sized scrolling:

| Task | Vim shortcut |
| --- | --- |
| Scroll down / up (follow lines in ruler mode) | `j` / `k` |
| Toggle reading ruler | `r` |
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

Both presets retain Command-Z / Command-Shift-Z for annotation undo/redo and
Backspace/Delete for deletion in the reader or Annotations list. Vim additionally
uses `u` / Ctrl-R for annotation undo/redo and `x` for deletion. These commands
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
