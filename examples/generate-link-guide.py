#!/usr/bin/env python3
"""Generate link-guide.pdf without a PDF library.

Run from any directory: python3 examples/generate-link-guide.py
Typst emits relative PDF filenames as URI actions; this small specimen needs
GoToR actions to exercise Kver's another-PDF window behavior.
"""

from pathlib import Path

OUTPUT = Path(__file__).with_name("link-guide.pdf")
WIDTH, HEIGHT = 612, 792
INK = "0.13 0.20 0.24"
MUTED = "0.39 0.45 0.47"
TEAL = "0.10 0.38 0.43"
PAPER = "0.98 0.97 0.94"
CARD = "1 1 1"


def pdf_string(value):
    return "(" + value.replace("\\", "\\\\").replace("(", "\\(").replace(")", "\\)") + ")"


def fill(x, y, width, height, color):
    return f"{color} rg {x} {y} {width} {height} re f"


def label(x, y, value, size=11, font="F1", color=INK):
    return f"BT /{font} {size} Tf {color} rg 1 0 0 1 {x} {y} Tm {pdf_string(value)} Tj ET"


def card(commands, y, number, title, detail):
    commands.extend(
        [
            fill(46, y, 520, 88, CARD),
            fill(46, y, 4, 88, TEAL),
            label(66, y + 56, number, 10, "F2", TEAL),
            label(107, y + 53, title, 16, "F2"),
            label(107, y + 28, detail, 10, "F1", MUTED),
            label(530, y + 54, ">", 17, "F2", TEAL),
        ]
    )
    return (46, y, 566, y + 88)


def page_heading(number, title, intro):
    return [
        fill(0, 0, WIDTH, HEIGHT, PAPER),
        fill(0, 744, WIDTH, 48, TEAL),
        label(46, 762, "KVER   /   LINK WALK", 11, "F2", CARD),
        label(46, 682, number, 11, "F2", TEAL),
        label(46, 632, title, 28, "F2"),
        label(46, 600, intro, 11, "F1", MUTED),
        label(46, 46, "A small companion to A field guide to the shoreline", 9, "F1", MUTED),
    ]


def stream(commands):
    data = ("\n".join(commands) + "\n").encode("ascii")
    return b"<< /Length " + str(len(data)).encode() + b" >>\nstream\n" + data + b"endstream"


def main():
    objects = []

    def add(value):
        objects.append(value)
        return len(objects)

    catalog = add(None)
    pages = add(None)
    first = add(None)
    second = add(None)
    first_commands = page_heading(
        "01 / THE ROUTE",
        "Links between documents",
        "Choose a destination. Each field-guide link opens another window.",
    )
    first_links = [
        (card(first_commands, 482, "01", "Start at the cover", "Open field-guide.pdf at page 1"), "remote", 0),
        (card(first_commands, 380, "02", "Read the field log", "Open field-guide.pdf at page 8"), "remote", 7),
        (card(first_commands, 278, "03", "Inspect the appendix", "Open field-guide.pdf at page 23"), "remote", 22),
        (card(first_commands, 176, "04", "Continue this walk", "Jump to page 2 in this PDF"), "local", 2),
    ]
    first_commands.append(label(46, 113, "Return to this page to try another destination.", 10, color=MUTED))
    second_commands = page_heading(
        "02 / THE RETURN",
        "Keep your place",
        "The original window remains available when you visit the field guide.",
    )
    second_links = [
        (card(second_commands, 482, "05", "Open the field guide", "Start at its cover in a separate window"), "remote", 0),
        (card(second_commands, 380, "06", "Back to the route", "Return to page 1 in this PDF"), "local", 1),
    ]
    second_commands.extend(
        [
            label(46, 310, "Try switching between the two independent windows.", 11, "F2", TEAL),
            label(46, 288, "Each document keeps its own page and navigation position.", 11, color=MUTED),
        ]
    )
    first_stream = add(stream(first_commands))
    second_stream = add(stream(second_commands))

    def annotation(rect, kind, page):
        destination = (
            f"/A << /S /GoToR /F (field-guide.pdf) /D [{page} /Fit] >>"
            if kind == "remote"
            else f"/Dest [{first if page == 1 else second} 0 R /Fit]"
        )
        return add(
            f"<< /Type /Annot /Subtype /Link /Rect [{' '.join(map(str, rect))}] "
            f"/Border [0 0 0] {destination} >>"
        )

    first_annotations = [annotation(*link) for link in first_links]
    second_annotations = [annotation(*link) for link in second_links]
    regular = add("<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>")
    bold = add("<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica-Bold >>")
    resources = f"/Resources << /Font << /F1 {regular} 0 R /F2 {bold} 0 R >> >>"
    objects[catalog - 1] = f"<< /Type /Catalog /Pages {pages} 0 R >>"
    objects[pages - 1] = f"<< /Type /Pages /Kids [{first} 0 R {second} 0 R] /Count 2 >>"
    for page, content, annotations in [
        (first, first_stream, first_annotations),
        (second, second_stream, second_annotations),
    ]:
        references = " ".join(f"{index} 0 R" for index in annotations)
        objects[page - 1] = (
            f"<< /Type /Page /Parent {pages} 0 R /MediaBox [0 0 {WIDTH} {HEIGHT}] "
            f"{resources} /Contents {content} 0 R /Annots [{references}] >>"
        )

    data = bytearray(b"%PDF-1.4\n")
    offsets = []
    for index, obj in enumerate(objects, 1):
        offsets.append(len(data))
        body = obj.encode("ascii") if isinstance(obj, str) else obj
        data.extend(f"{index} 0 obj\n".encode() + body + b"\nendobj\n")
    xref = len(data)
    data.extend(f"xref\n0 {len(objects) + 1}\n0000000000 65535 f \n".encode())
    for offset in offsets:
        data.extend(f"{offset:010} 00000 n \n".encode())
    data.extend(
        f"trailer\n<< /Size {len(objects) + 1} /Root {catalog} 0 R >>\n"
        f"startxref\n{xref}\n%%EOF\n".encode()
    )
    OUTPUT.write_bytes(data)
    print(f"Wrote {OUTPUT}")


if __name__ == "__main__":
    main()
