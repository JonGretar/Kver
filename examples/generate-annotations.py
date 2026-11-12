#!/usr/bin/env python3
"""Generate the imported-text-markup regression specimen, without a PDF writer."""
from pathlib import Path
import sys

objects = [
    "<< /Type /Catalog /Pages 2 0 R >>",
    "<< /Type /Pages /Kids [3 0 R 4 0 R] /Count 2 >>",
    "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 400 600] /Resources << /Font << /F1 5 0 R >> >> /Contents 6 0 R /Annots [10 0 R 8 0 R 9 0 R 7 0 R null 11 0 R 12 0 R 13 0 R] >>",
    "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 400 600] /CropBox [20 30 380 570] /Rotate 90 /Resources << /Font << /F1 5 0 R >> >> /Contents 14 0 R /Annots [15 0 R] >>",
    "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>",
]

def stream(text):
    return f"<< /Length {len(text.encode())} >>\nstream\n{text}\nendstream"

objects.append(stream(
    "BT /F1 16 Tf 40 550 Td (Imported text markup) Tj "
    "0 -70 Td (Highlight passage) Tj "
    "0 -60 Td (Underline passage) Tj "
    "0 -60 Td (Strikeout passage) Tj "
    "0 -60 Td (Squiggly passage) Tj ET"
))

def markup(kind, y, color, extra="", quads=None):
    if quads is None:
        quads = f"40 {y+16} 190 {y+16} 40 {y-4} 190 {y-4}"
    return (
        f"<< /Type /Annot /Subtype /{kind} /Rect [40 {y-4} 190 {y+16}] "
        f"/QuadPoints [{quads}] /C [{color}] "
        f"/Contents (Attached comment - never a quotation) {extra} >>"
    )

objects.extend([
    markup("Highlight", 480, "1 0.85 0", "/T (Fixture author) /NM (imported-highlight) /AP << /N 16 0 R >>"),
    markup("Underline", 420, "1 0 0", "/AP << /N 17 0 R >>"),
    markup("StrikeOut", 360, "0 0 1", "/AP << /N 18 0 R >>"),
    markup("Squiggly", 300, "0 0.7 0", "/AP << /N 19 0 R >>"),
    markup("Highlight", 180, "0.5"),  # No underlying text, still navigable.
    markup("Underline", 120, "0 1 0 0", quads="40 136 190 (invalid)"),
    "<< /Type /Annot /Subtype /Squiggly /QuadPoints [40 80 190 80 40 60 40 60] /C [2 -1 3] >>",
    stream("BT /F1 16 Tf 40 480 Td (Rotated passage) Tj ET"),
    markup("Highlight", 480, "1 0.85 0"),
])

for y, paint in [
    (480, "q /GS gs 1 0.85 0 rg 40 476 150 20 re f Q"),
    (420, "1 0 0 RG 1 w 40 418 m 190 418 l S"),
    (360, "0 0 1 RG 1 w 40 365 m 190 365 l S"),
    (300, "0 0.7 0 RG 1 w 40 298 m " +
        " ".join(f"{x} {298 + (2 if x % 8 else 0)} l" for x in range(44, 191, 4)) + " S"),
]:
    objects.append(stream(paint).replace(
        "<< /Length",
        f"<< /Type /XObject /Subtype /Form /BBox [40 {y-4} 190 {y+16}] "
        "/Resources << /ExtGState << /GS << /Type /ExtGState /ca 0.35 >> >> >> /Length"
    ))

many = "--many" in sys.argv
if many:
    objects[2] = objects[2].replace(
        "/Annots [10 0 R 8 0 R 9 0 R 7 0 R null 11 0 R 12 0 R 13 0 R]",
        "/Annots [" + "7 0 R " * 240 + "]",
    )

data = bytearray(b"%PDF-1.7\n")
offsets = [0]
for ix, obj in enumerate(objects, 1):
    offsets.append(len(data))
    data.extend(f"{ix} 0 obj\n{obj}\nendobj\n".encode())
xref = len(data)
data.extend(f"xref\n0 {len(offsets)}\n0000000000 65535 f \n".encode())
for offset in offsets[1:]:
    data.extend(f"{offset:010} 00000 n \n".encode())
data.extend(f"trailer\n<< /Size {len(offsets)} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n".encode())
Path(__file__).with_name("annotations-many.pdf" if many else "annotations.pdf").write_bytes(data)
