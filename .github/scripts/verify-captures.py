"""Check headless capture output without installing an image-processing dependency."""

import pathlib
import struct
import sys


def png_dimensions(path: pathlib.Path) -> tuple[int, int]:
    data = path.read_bytes()
    if data[:16] != b"\x89PNG\r\n\x1a\n\x00\x00\x00\rIHDR":
        raise ValueError(f"{path} is not a PNG")
    width, height = struct.unpack(">II", data[16:24])
    if width == 0 or height == 0 or len(data) < 100:
        raise ValueError(f"{path} is an empty image")
    return width, height


directory = pathlib.Path(sys.argv[1])
before = directory / "before.png"
after = directory / "after.png"
size = png_dimensions(before)
if png_dimensions(after) != size:
    raise ValueError("Zoom changed the screenshot canvas size")
if before.read_bytes() == after.read_bytes():
    raise ValueError("Zoom did not change the captured frame")
print(f"Captured distinct before/after frames at {size[0]}x{size[1]}")
