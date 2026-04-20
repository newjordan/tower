#!/usr/bin/env python3
"""
Test: Extract tiles as 64x64 from the 256x256 source.
Ultima VII ortho-45 style: base cell is 32x32, full tile is 64x64.
Walls are 16px thick, doors 8px thick.
Grid: 7 columns x 7 rows at 32px intervals (tiles overlap by 32px).
Placed on 32px grid with 32px vertical overhang.
"""
from pathlib import Path
from PIL import Image, ImageDraw, ImageFont

SRC = Path("assets/public/v1/sources/perspective_walls_template/perspective_walls.png")
OUT_DIR = Path("assets/previews/runtime")
OUT_DIR.mkdir(parents=True, exist_ok=True)

source = Image.open(SRC).convert("RGBA")

TILE_SIZE = 64
GRID_STEP = 32

# Extract 64x64 tiles at 32px intervals: 7 columns x 7 rows
max_col = (source.width - TILE_SIZE) // GRID_STEP + 1
max_row = (source.height - TILE_SIZE) // GRID_STEP + 1
cells = {}
for r in range(max_row):
    for c in range(max_col):
        x = c * GRID_STEP
        y = r * GRID_STEP
        cells[(c, r)] = source.crop((x, y, x + TILE_SIZE, y + TILE_SIZE))

# First: render a grid of all tiles for visual identification
try:
    font = ImageFont.truetype("/usr/share/fonts/truetype/dejavu/DejaVuSansMono-Bold.ttf", 12)
    small = ImageFont.truetype("/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf", 10)
except:
    font = small = ImageFont.load_default()

scale = 2
card_w = TILE_SIZE * scale + 16
card_h = TILE_SIZE * scale + 24
grid_img = Image.new("RGBA", (max_col * card_w + 16, max_row * card_h + 50), (12, 16, 24, 255))
gd = ImageDraw.Draw(grid_img)
gd.text((8, 8), f"64x64 tile extraction ({max_col} cols x {max_row} rows @ 32px step) - scaled 2x", font=font, fill=(255, 255, 255))

for r in range(max_row):
    for c in range(max_col):
        tile = cells[(c, r)]
        scaled = tile.resize((TILE_SIZE * scale, TILE_SIZE * scale), Image.NEAREST)
        cx = 8 + c * card_w
        cy = 40 + r * card_h
        gd.rectangle([cx, cy, cx + card_w - 8, cy + card_h - 8], fill=(30, 34, 42), outline=(60, 70, 85))
        grid_img.paste(scaled, (cx + 8, cy + 16), scaled)
        gd.text((cx + 8, cy + 2), f"r{r}c{c}", font=small, fill=(200, 200, 200))

grid_img.save(OUT_DIR / "wall_tiles_64x64_grid.png")
print("Saved wall_tiles_64x64_grid.png")

# 64x64 tiles on 32px grid (Ultima VII ortho-45 style)
# Each tile: 32x32 base cell + 32px wall height + 32px horizontal depth
# Tiles overlap 32px with adjacent tiles (intentional for 3D wall effect)
#
# Source layout (32px grid → 7 cols x 7 rows):
#   c0 = LEFT wall pieces (face east)
#   c1 = TOP/BOTTOM wall pieces (face south)
#   c2 = RIGHT wall pieces (face west)
#   c3-c5 = inner corners
#   c4 = junctions (cross, T)
#   c6 = T-junction variants
#
ROLE_MAP = {
    # Outer corners and edges
    "corner_top_left":     (0, 0),
    "edge_top":            (1, 0),
    "corner_top_right":    (2, 0),
    "edge_left":           (0, 2),
    "edge_right":          (2, 2),
    "corner_bottom_left":  (0, 4),
    "edge_bottom":         (1, 4),
    "corner_bottom_right": (2, 4),
    # Inner corners
    "inner_corner_top_left":     (5, 2),
    "inner_corner_top_right":    (3, 2),
    "inner_corner_bottom_left":  (3, 4),
    "inner_corner_bottom_right": (5, 4),
    # Junctions
    "pillar":              (4, 0),
    "cross":               (4, 2),
    "terminal":            (4, 2),
    "t_top":               (4, 4),
    "t_bottom":            (4, 0),
    "t_left":              (6, 2),
    "t_right":             (4, 2),
}

def pick_role(x, y, layout, W, H):
    def is_w(x, y):
        return 0 <= x < W and 0 <= y < H and layout[y][x] == '#'
    def is_f(x, y):
        return 0 <= x < W and 0 <= y < H and layout[y][x] in '._'
    def oob_or_space(x, y):
        return x < 0 or x >= W or y < 0 or y >= H or layout[y][x] == ' '
    n, e, s, w = is_w(x,y-1), is_w(x+1,y), is_w(x,y+1), is_w(x-1,y)
    cnt = sum([n,e,s,w])
    if cnt == 4: return "cross"
    if cnt == 3:
        if not n: return "t_top"
        if not s: return "t_bottom"
        if not w: return "t_right"
        if not e: return "t_left"
    if cnt == 2:
        # Outer corners: wall neighbors form an L, open space outside
        if e and s: return "corner_top_left"
        if w and s: return "corner_top_right"
        if n and e: return "corner_bottom_left"
        if n and w: return "corner_bottom_right"
        # Edges
        if e and w:
            return "edge_top" if is_f(x, y+1) else "edge_bottom"
        if n and s:
            return "edge_left" if is_f(x+1, y) else "edge_right"
    if cnt == 1: return "terminal"
    return "pillar"

# Simple room test
CELL = 32
OVERHANG = 32  # vertical overhang (wall height above cell)
H_OVERHANG = 32  # horizontal overhang (tile extends 32px past cell width)

for room_w, room_h, name in [(8, 6, "8x6"), (12, 9, "12x9")]:
    layout = []
    for ry in range(room_h):
        row = ""
        for rx in range(room_w):
            if rx == 0 or rx == room_w-1 or ry == 0 or ry == room_h-1:
                row += "#"
            else:
                row += "."
        layout.append(row)

    W, H = room_w, room_h
    cw = W * CELL + H_OVERHANG + 32
    ch = 50 + H * CELL + OVERHANG + 16
    canvas = Image.new("RGBA", (cw, ch), (12, 16, 24, 255))
    draw = ImageDraw.Draw(canvas)
    draw.text((8, 8), f"64x64 tiles on 32px grid - Room {room_w}x{room_h}", font=font, fill=(255,255,255))
    draw.text((8, 24), "Ultima VII ortho-45 style: 32px base, 64px tile", font=small, fill=(180,180,180))

    ox, oy = 16, 50
    # Floor
    for ry in range(H):
        for rx in range(W):
            if layout[ry][rx] == '.':
                px, py = ox + rx*CELL, oy + ry*CELL
                draw.rectangle([px, py, px+CELL-1, py+CELL-1], fill=(100, 100, 100))

    # Walls - render top-to-bottom so lower walls overlap correctly
    for ry in range(H):
        for rx in range(W):
            if layout[ry][rx] != '#': continue
            role = pick_role(rx, ry, layout, W, H)
            cell_pos = ROLE_MAP.get(role)
            if cell_pos is None: continue
            tile = cells.get(cell_pos)
            if tile is None: continue
            px = ox + rx * CELL
            py = oy + ry * CELL - OVERHANG
            canvas.paste(tile, (px, py), tile)

    canvas.save(OUT_DIR / f"wall_test_64x64_{name}.png")
    print(f"Saved wall_test_64x64_{name}.png")

# Two rooms + hallway test
HALL_LAYOUT = [
    "#######   #######",
    "#.....#   #.....#",
    "#.....#   #.....#",
    "#.....#####.....#",
    "#......._.......#",
    "#.....#####.....#",
    "#.....#   #.....#",
    "#.....#   #.....#",
    "#######   #######",
]

def render_layout(layout, title, filename):
    W = len(layout[0])
    H = len(layout)
    cw = W * CELL + H_OVERHANG + 48
    ch = 50 + H * CELL + OVERHANG + 16
    canvas = Image.new("RGBA", (cw, ch), (12, 16, 24, 255))
    d = ImageDraw.Draw(canvas)
    d.text((8, 8), title, font=font, fill=(255, 255, 255))
    ox, oy = 16, 50

    # Floor first
    for ry in range(H):
        for rx in range(W):
            ch2 = layout[ry][rx]
            px, py = ox + rx * CELL, oy + ry * CELL
            if ch2 in '._':
                d.rectangle([px, py, px + CELL - 1, py + CELL - 1], fill=(100, 100, 100))

    # Walls - render top-to-bottom so lower walls overlap upper ones correctly
    for ry in range(H):
        for rx in range(W):
            if layout[ry][rx] != '#':
                continue
            role = pick_role(rx, ry, layout, W, H)
            cell_pos = ROLE_MAP.get(role)
            if cell_pos:
                tile = cells.get(cell_pos)
                if tile:
                    px = ox + rx * CELL
                    py = oy + ry * CELL - OVERHANG
                    canvas.paste(tile, (px, py), tile)

    # Debug: annotate roles
    for ry in range(H):
        for rx in range(W):
            if layout[ry][rx] != '#':
                continue
            role = pick_role(rx, ry, layout, W, H)
            short = role.replace("corner_", "c").replace("edge_", "e").replace("inner_corner_", "i")
            short = short.replace("top_left", "TL").replace("top_right", "TR")
            short = short.replace("bottom_left", "BL").replace("bottom_right", "BR")
            short = short.replace("top", "T").replace("bottom", "B").replace("left", "L").replace("right", "R")
            px = ox + rx * CELL
            py = oy + ry * CELL
            d.text((px + 2, py + 2), short, font=small, fill=(255, 255, 0, 180))

    canvas.save(OUT_DIR / filename)
    print(f"Saved {filename}")

render_layout(HALL_LAYOUT, "32x64 tiles - Two rooms + hallway", "wall_test_32x64_hallway.png")

# L-shaped room test (tests inner corners)
# Pad all rows to same width with spaces
L_LAYOUT = [
    "########   ",
    "#......#   ",
    "#......#   ",
    "#......####",
    "#.........#",
    "#.........#",
    "###########",
]
render_layout(L_LAYOUT, "32x64 tiles - L-shaped room", "wall_test_32x64_L.png")

# U-shaped room test
U_LAYOUT = [
    "#####   #####",
    "#...#   #...#",
    "#...#   #...#",
    "#...#####...#",
    "#...........#",
    "#...........#",
    "#############",
]
render_layout(U_LAYOUT, "32x64 tiles - U-shaped room", "wall_test_32x64_U.png")
