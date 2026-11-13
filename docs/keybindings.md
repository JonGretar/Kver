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
| Reader display | `zoom-in`, `zoom-out`, `fit-width`, `fit-page`, `single-page`, `continuous`, `use-default-reading-mode`, `copy` |
| Find and sidebar | `show-find`, `next-match`, `previous-match`, `show-contents`, `toggle-sidebar`, `activate-outline` |
| Marks | `show-marks`, `set-local-mark`, `jump-local-mark`, `delete-local-mark` |
| Text markup | `highlight`, `underline`, `strikethrough`, `squiggly-underline`, `show-annotations`, `undo-annotation`, `redo-annotation`, `delete-highlight`, `recolor-highlight-yellow`, `recolor-highlight-green`, `recolor-highlight-blue`, `recolor-highlight-pink`, `recolor-highlight-red` |

The mark commands in this list open the corresponding mark chooser. The Vim
letter-specific mark sequences are supplied by the Vim preset; the TOML
command names do not accept a mark letter argument.

With text selected in the reader, Command-Shift-H (Ctrl-Shift-H outside macOS)
applies Highlight once and retains the selection. Underline, Strikethrough, and
Squiggly underline are available in the selection context menu, Edit menu, and
command palette; assign custom shortcuts with the command names above.
All types share annotation undo/redo and the last-used color per type for the
current document session. Despite their historical names, `delete-highlight` and
`recolor-highlight-*` operate on the selected page annotation of any supported type.
The legacy `recolor-highlight-purple` command remains accepted with its original
purple color; the five UI presets now offer Red instead of Purple.

Command-S (Ctrl-S) saves changes
to the current PDF. Command-Shift-S (Ctrl-Shift-S) opens Save As and suggests an
`-annotated.pdf` filename. Both presets share these shortcuts.
