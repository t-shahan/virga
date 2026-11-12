#!/usr/bin/env python3
"""Build src/outline.bin, the coastlines and borders under the radar.

Reads Natural Earth's GeoJSON, which is in the public domain, and writes
the compact form `src/outline.rs` reads. Run it only to change the
source data or the format; the output is checked in, so a build needs
neither Python nor the network.

    scripts/build-outline.py DIR

DIR holds the files named in SOURCES, each with .geojson after it, from
https://github.com/nvkelso/natural-earth-vector/tree/master/geojson

Two levels of detail are packed. The 1:110m lines are for a view of a
continent or the world, where the 1:50m ones would put several points in
every dot and cost a draw the whole file. State and province lines are only
drawn close in, so they are packed at 1:50m alone.

Format, little-endian, one record per line until the file ends:

    u8       what the line is: its layer in the low four bits (COAST,
             COUNTRY, STATE, LAKE) and its detail in the high four
             (COARSE, FINE)
    u16      points in the line, at least 2
    i16 i16  the first point's longitude and latitude, in steps of STEP degrees
    i8 i8    each later point, as its difference from the one before

A step too long for an i8 is broken into shorter ones, and a line too long
for a u16 into several that share an end point.
"""

import json
import struct
import sys
from pathlib import Path

STEP = 0.02
COAST, COUNTRY, STATE, LAKE = range(4)
COARSE, FINE = range(2)
SOURCES = [
    ("ne_110m_coastline", COAST, COARSE),
    ("ne_110m_admin_0_boundary_lines_land", COUNTRY, COARSE),
    ("ne_110m_lakes", LAKE, COARSE),
    ("ne_50m_coastline", COAST, FINE),
    ("ne_50m_admin_0_boundary_lines_land", COUNTRY, FINE),
    ("ne_50m_admin_1_states_provinces_lines", STATE, FINE),
    ("ne_50m_lakes", LAKE, FINE),
]
MAX_POINTS = 65535


def lines_of(path):
    for feature in json.loads(path.read_text())["features"]:
        geometry = feature["geometry"]
        kind, coordinates = geometry["type"], geometry["coordinates"]
        if kind == "LineString":
            yield coordinates
        elif kind in ("MultiLineString", "Polygon"):
            yield from coordinates
        elif kind == "MultiPolygon":
            for polygon in coordinates:
                yield from polygon


def quantised(line):
    points = []
    for lon, lat, *_ in line:
        point = (round(lon / STEP), round(lat / STEP))
        if not points:
            points.append(point)
            continue
        # Walk to the point in steps an i8 can hold.
        while points[-1] != point:
            last = points[-1]
            dx = max(-127, min(127, point[0] - last[0]))
            dy = max(-127, min(127, point[1] - last[1]))
            points.append((last[0] + dx, last[1] + dy))
    return points


def records(points, tag):
    for start in range(0, len(points) - 1, MAX_POINTS - 1):
        piece = points[start : start + MAX_POINTS]
        if len(piece) < 2:
            continue
        yield struct.pack("<BHhh", tag, len(piece), *piece[0]) + b"".join(
            struct.pack("<bb", b[0] - a[0], b[1] - a[1])
            for a, b in zip(piece, piece[1:])
        )


def main():
    source = Path(sys.argv[1])
    out = Path(__file__).resolve().parent.parent / "src" / "outline.bin"
    data = b"".join(
        record
        for name, layer, detail in SOURCES
        for line in lines_of(source / f"{name}.geojson")
        for record in records(quantised(line), layer | detail << 4)
    )
    out.write_bytes(data)
    print(f"{out}: {len(data)} bytes")


if __name__ == "__main__":
    main()
