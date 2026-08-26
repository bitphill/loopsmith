#!/usr/bin/env python3
"""Copy the logo into the CLI crate with its flat background keyed to alpha.

Run via tools/sync-logo.sh, which explains why the copy exists at all.

Flood-filling inward from the corners rather than thresholding on lightness is
the whole trick: a global "near-white is background" rule punches holes in the
figure's own white highlights. Only background the fill can actually reach is
removed, so an enclosed light region stays.

Pure stdlib. This machine has neither Pillow nor ImageMagick, and `sips` cannot
key alpha.
"""
import collections
import pathlib
import struct
import sys
import zlib

ROOT = pathlib.Path(__file__).resolve().parent.parent
SRC = ROOT / "assets/loopsmith-logo-256.png"
DST = ROOT / "runtime/crates/loopsmith-cli/templates/loopsmith-mark.png"

# How far a pixel may drift from the sampled corner colour and still count as
# background. Loose enough to swallow the mottling in a flat field, tight enough
# to stop at the figure's outline.
TOLERANCE = 46


def read_png(path):
    data = path.read_bytes()
    if data[:8] != b"\x89PNG\r\n\x1a\n":
        sys.exit(f"{path} is not a PNG")
    pos, idat, ihdr = 8, b"", None
    while pos < len(data):
        (length,) = struct.unpack(">I", data[pos : pos + 4])
        ctype = data[pos + 4 : pos + 8]
        body = data[pos + 8 : pos + 8 + length]
        if ctype == b"IHDR":
            ihdr = struct.unpack(">IIBBBBB", body)
        elif ctype == b"IDAT":
            idat += body
        elif ctype == b"IEND":
            break
        pos += 12 + length
    return ihdr, zlib.decompress(idat)


def unfilter(raw, width, height, bpp):
    """Undo PNG's per-scanline filtering."""
    stride = width * bpp
    out = bytearray(width * height * bpp)
    prev = bytearray(stride)
    pos = 0
    for y in range(height):
        ft = raw[pos]
        pos += 1
        line = bytearray(raw[pos : pos + stride])
        pos += stride
        for i in range(stride):
            a = line[i - bpp] if i >= bpp else 0
            b = prev[i]
            c = prev[i - bpp] if i >= bpp else 0
            x = line[i]
            if ft == 1:
                x += a
            elif ft == 2:
                x += b
            elif ft == 3:
                x += (a + b) >> 1
            elif ft == 4:
                p = a + b - c
                pa, pb, pc = abs(p - a), abs(p - b), abs(p - c)
                x += a if (pa <= pb and pa <= pc) else (b if pb <= pc else c)
            line[i] = x & 0xFF
        out[y * stride : (y + 1) * stride] = line
        prev = line
    return out


def chunk(tag, body):
    return (
        struct.pack(">I", len(body))
        + tag
        + body
        + struct.pack(">I", zlib.crc32(tag + body) & 0xFFFFFFFF)
    )


def main():
    ihdr, raw = read_png(SRC)
    w, h, depth, color, _comp, _filt, interlace = ihdr
    if (depth, color, interlace) != (8, 2, 0):
        sys.exit(f"expected 8-bit non-interlaced RGB, got depth={depth} colour={color}")

    px = unfilter(raw, w, h, 3)

    def at(x, y):
        i = (y * w + x) * 3
        return px[i], px[i + 1], px[i + 2]

    bg = at(0, 0)
    alpha = bytearray(b"\xff" * (w * h))
    seen = bytearray(w * h)
    queue = collections.deque()
    for sx, sy in ((0, 0), (w - 1, 0), (0, h - 1), (w - 1, h - 1)):
        queue.append((sx, sy))
        seen[sy * w + sx] = 1

    while queue:
        x, y = queue.popleft()
        r, g, b = at(x, y)
        if max(abs(r - bg[0]), abs(g - bg[1]), abs(b - bg[2])) > TOLERANCE:
            continue
        alpha[y * w + x] = 0
        for dx, dy in ((1, 0), (-1, 0), (0, 1), (0, -1)):
            nx, ny = x + dx, y + dy
            if 0 <= nx < w and 0 <= ny < h and not seen[ny * w + nx]:
                seen[ny * w + nx] = 1
                queue.append((nx, ny))

    # Feather the boundary. Without this the anti-aliased edge pixels the fill
    # could not reach stay fully opaque and the mark wears a white halo on dark.
    feathered = bytearray(alpha)
    for y in range(h):
        for x in range(w):
            if alpha[y * w + x] == 0:
                continue
            if any(
                0 <= x + dx < w and 0 <= y + dy < h and alpha[(y + dy) * w + (x + dx)] == 0
                for dx, dy in ((1, 0), (-1, 0), (0, 1), (0, -1))
            ):
                r, g, b = at(x, y)
                d = max(abs(r - bg[0]), abs(g - bg[1]), abs(b - bg[2]))
                feathered[y * w + x] = max(0, min(255, int(d * 255 / TOLERANCE)))
    alpha = feathered

    body = bytearray()
    for y in range(h):
        body.append(0)  # filter type 0; zlib does the real work on art this flat
        for x in range(w):
            i = (y * w + x) * 3
            body += bytes((px[i], px[i + 1], px[i + 2], alpha[y * w + x]))

    png = (
        b"\x89PNG\r\n\x1a\n"
        + chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, 6, 0, 0, 0))
        + chunk(b"IDAT", zlib.compress(bytes(body), 9))
        + chunk(b"IEND", b"")
    )
    DST.parent.mkdir(parents=True, exist_ok=True)
    DST.write_bytes(png)

    opaque = sum(1 for a in alpha if a == 255)
    print(f"wrote {DST.relative_to(ROOT)}  {w}x{h}  {len(png) / 1024:.1f} KB  {100 * opaque // (w * h)}% opaque")


if __name__ == "__main__":
    main()
