"""Generate a one-page, dependency-free PDF from the README introduction."""

from pathlib import Path
import re
import textwrap
import unicodedata


ROOT = Path(__file__).resolve().parents[1]
OUTPUT = ROOT / "examples" / "readme-sample.pdf"


def plain_text(line: str) -> str:
    line = re.sub(r"\[([^]]+)\]\([^)]+\)", r"\1", line)
    line = line.replace("**", "").replace("`", "").replace("_", "")
    return unicodedata.normalize("NFKD", line).encode("ascii", "replace").decode()


def pdf_object(data: bytes, number: int) -> bytes:
    return f"{number} 0 obj\n".encode() + data + b"\nendobj\n"


def make_pdf(lines: list[str]) -> bytes:
    instructions = ["BT /F1 12 Tf 50 750 Td 17 TL"]
    for line in lines:
        escaped = line.replace("\\", "\\\\").replace("(", "\\(").replace(")", "\\)")
        instructions.append(f"({escaped}) Tj T*")
    instructions.append("ET")
    stream = "\n".join(instructions).encode("ascii")
    objects = [
        b"<< /Type /Catalog /Pages 2 0 R >>",
        b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
        b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] "
        b"/Resources << /Font << /F1 5 0 R >> >> /Contents 4 0 R >>",
        f"<< /Length {len(stream)} >>\nstream\n".encode() + stream + b"\nendstream",
        b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>",
    ]
    pdf = bytearray(b"%PDF-1.4\n")
    offsets = [0]
    for number, obj in enumerate(objects, start=1):
        offsets.append(len(pdf))
        pdf.extend(pdf_object(obj, number))
    xref = len(pdf)
    pdf.extend(f"xref\n0 {len(offsets)}\n0000000000 65535 f \n".encode())
    for offset in offsets[1:]:
        pdf.extend(f"{offset:010} 00000 n \n".encode())
    pdf.extend(
        f"trailer\n<< /Size {len(offsets)} /Root 1 0 R >>\n"
        f"startxref\n{xref}\n%%EOF\n".encode()
    )
    return bytes(pdf)


def main() -> None:
    readme = (ROOT / "README.md").read_text()
    intro = readme.split("## Goals", maxsplit=1)[0]
    goals = readme.split("## Goals", maxsplit=1)[1].split("## Technology", maxsplit=1)[0]
    lines = ["Kver - a keyboard-friendly PDF reader", ""]
    for paragraph in intro.split("\n\n")[1:]:
        text = plain_text(" ".join(paragraph.splitlines()))
        lines.extend(textwrap.wrap(text, width=78))
        lines.append("")
    lines.append("Goals")
    for goal in goals.splitlines():
        if goal.startswith("- "):
            lines.extend(textwrap.wrap(plain_text(goal), width=78, subsequent_indent="  "))
    if len(lines) > 38:
        raise ValueError("README excerpt no longer fits on one page")
    OUTPUT.write_bytes(make_pdf(lines))
    print(f"Wrote {OUTPUT} ({len(lines)} lines)")


if __name__ == "__main__":
    main()
