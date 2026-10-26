# Features

## Product Direction

Kver is a general-purpose, keyboard-first PDF reader.

The primary audience includes developers, technical users, researchers, and people who frequently read long documents.

The application must remain usable with conventional mouse and keyboard interactions.

## Core MVP — P0

These features are required before considering the initial reader complete.

### PDF Viewing

- Open local PDF documents
- Handle password-protected PDFs
- Continuous vertical scrolling
- Page navigation
- Zoom in/out
- Fit width
- Fit page
- Smooth scrolling
- Efficient rendering and caching
- Handle large documents without excessive memory use

### Keyboard Navigation

- Vim-style `j` and `k` scrolling
- `gg` and `G` navigation
- Numeric command counts
- Page-wise navigation
- Configurable keybindings
- Navigation backward/forward

### Visual Ruler

The visual ruler is a core feature.

Requirements:

- Toggle ruler visibility
- Move ruler using keyboard commands
- Line-aware positioning where possible
- Fallback movement for image-only documents
- Configurable ruler appearance
- Correct positioning while scrolling and zooming

### Search

- Search text within the current document
- Navigate next/previous match
- Visually highlight matches
- Keyboard-accessible search interface
- Incremental search if practical

### Basic Text Interaction

- Select text with the mouse
- Copy selected text
- Follow internal links
- Open external links safely

## Everyday Usability — P1

These are important for a practical replacement for existing readers.

### Navigation

- Document table of contents
- Jump to a page number
- Named marks
- Recently opened documents
- Restore previous reading position
- Restore appropriate zoom state
- Document navigation history

### Desktop Experience

- Native file opening
- Drag-and-drop PDF files
- Dark/light application themes
- Keyboard command palette
- Useful error messages
- Configurable preferences

### Multiple Documents

- Open multiple documents
- Switch between documents quickly
- Preserve independent state per document

Tabs are desirable but not mandatory for the first working prototype.

## Advanced Features — P2

These are valuable but should not delay the initial release.

### Annotation

- Text highlighting
- Named bookmarks
- Annotation navigation
- Notes
- Export or persistence of annotations

### Advanced Viewing

- Split views
- Two-page layouts
- Presentation mode
- Document comparison
- Multiple windows

### Navigation Enhancements

- Link hints
- Search across previously opened documents
- Enhanced mark management
- Keyboard-driven text selection

## Academic / Research Features — P3

These are inspired by Sioyek and may be implemented incrementally.

### Smart Jump

Recognize references to figures, tables, equations, and bibliography entries even when PDF hyperlinks are missing.

### Overview Windows

Display referenced content without leaving the current reading position.

### Portals

Connect document locations and dynamically display referenced material in another viewport.

### Bibliography Navigation

Identify references and navigate between citations and their corresponding bibliography entries.

## Design Principles

### Keyboard-first, not keyboard-only

All important actions should be keyboard-accessible, but mouse interaction must remain natural.

### Responsiveness

Rendering should not interrupt navigation or input handling.

### Minimal UI

Avoid permanent UI elements that unnecessarily reduce document viewing space.

### Configurability

Power users should be able to customize keyboard bindings and relevant reading behavior.

### Persistence

A document should reopen where the user left off.

### General-purpose usability

Do not assume PDFs are academic papers.

Documents may include:

- Manuals
- Technical specifications
- Programming books
- Reports
- Invoices
- Research papers
- Scanned documents
- Presentations

## Out of Scope for MVP

- Editing PDF contents
- Creating PDFs
- OCR
- PDF form editing
- Digital signatures
- Cloud document libraries
- Collaborative editing
