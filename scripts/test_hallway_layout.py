#!/usr/bin/env python3
"""
Render two rooms connected by a hallway using the curated tile set.
"""
from pathlib import Path
from PIL import Image, ImageDraw, ImageFont

TILES_DIR = Path("assets/public/v1/imported/perspective_walls_template/tiles")
OUT_DIR = Path("assets/previews/runtime")
OUT_DIR.mkdir(parents=True, exist_ok=True)

try:
    font = ImageFont.truetype("/usr/share/fonts/truetype/dejavu/DejaVuSansMono-Bold.ttf", 12)
    small = ImageFont.truetype("/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf", 10)
except Exception:
    font = small = ImageFont.load_default()

CELL = 32
OVERHANG = 32

# Load curated tiles by semantic name
def load_tile(name):
    p = TILES_DIR / f"{name}.png"
    if p.exists():
        return Image.open(p).convert("RGBA")
    return None

TILE_CACHE = {}
def get_tile(name):
    if name not in TILE_CACHE:
        TILE_CACHE[name] = load_tile(name)
    return TILE_CACHE[name]

# Map wall role → curated tile name
ROLE_TILE = {
    "corner_top_left":     "tile_wall_corner_top_left",
    "corner_top_right":    "tile_wall_corner_top_right",
    "corner_bottom_left":  "tile_wall_corner_bottom_left",
    "corner_bottom_right": "tile_wall_corner_bottom_right",
    "edge_top":            "tile_wall_edge_top",
    "edge_bottom":         "tile_wall_edge_bottom",
    "edge_left":           "tile_wall_edge_left",
    "edge_right":          "tile_wall_edge_right",
    "cross":               "tile_wall_cross",
    "t_top":               "tile_wall_t_top",
    "t_bottom":            "tile_wall_t_bottom",
    "t_left":              "tile_wall_t_left",
    "t_right":             "tile_wall_t_right",
    "pillar":              "tile_wall_pillar",
    "terminal":            "tile_wall_terminal",
}

def pick_role(x, y, layout, W, H):
    def is_w(cx, cy):
        return 0 <= cx < W and 0 <= cy < H and layout[cy][cx] == '#'
    def is_f(cx, cy):
        return 0 <= cx < W and 0 <= cy < H and layout[cy][cx] in '._'
    n = is_w(x, y-1)
    e = is_w(x+1, y)
    s = is_w(x, y+1)
    w = is_w(x-1, y)
    cnt = sum([n, e, s, w])
    if cnt == 4: return "cross"
    if cnt == 3:
        if not n: return "t_top"
        if not s: return "t_bottom"
        if not w: return "t_right"
        if not e: return "t_left"
    if cnt == 2:
        if e and s: return "corner_top_left"
        if w and s: return "corner_top_right"
        if n and e: return "corner_bottom_left"
        if n and w: return "corner_bottom_right"
        if e and w:
            return "edge_top" if is_f(x, y+1) else "edge_bottom"
        if n and s:
            return "edge_left" if is_f(x+1, y) else "edge_right"
    if cnt == 1: return "terminal"
    return "pillar"

def render_layout(layout, title, filename):
    W = len(layout[0])
    H = len(layout)
    cw = W * CELL + CELL + 48
    ch = 50 + H * CELL + OVERHANG + 16
    canvas = Image.new("RGBA", (cw, ch), (12, 16, 24, 255))
    d = ImageDraw.Draw(canvas)
    d.text((8, 8), title, font=font, fill=(255, 255, 255))
    ox, oy = 16, 50

    # Floor — paint under all cells that are part of the layout.
    # We paint floor for any cell that is not empty space, or is a wall.
    # For simplicity in this test, we paint floor for every cell that is in '._#'.
    floor = get_tile("tile_floor_clean")
    for ry in range(H):
        for rx in range(W):
            cell = layout[ry][rx]
            if cell in '._#':
                px = ox + rx * CELL
                py = oy + ry * CELL
                if floor:
                    canvas.paste(floor, (px, py), floor)
                else:
                    d.rectangle([px, py, px+CELL-1, py+CELL-1], fill=(100, 100, 100))

    # Walls — top to bottom for correct overlap
    for ry in range(H):
        for rx in range(W):
            if layout[ry][rx] != '#':
                continue
            role = pick_role(rx, ry, layout, W, H)
            tile_name = ROLE_TILE.get(role)
            if not tile_name:
                continue
            tile = get_tile(tile_name)
            if not tile:
                continue
            px = ox + rx * CELL
            py = oy + ry * CELL
            # Apply overhang for oversized tiles
            if tile.width > CELL or tile.height > CELL:
                py -= OVERHANG
            canvas.paste(tile, (px, py), tile)

    canvas.save(OUT_DIR / filename)
    print(f"Saved {filename}")

# --- Layouts ---

# Two rooms connected by a hallway
HALLWAY = [
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
render_layout(HALLWAY, "Two rooms + hallway (curated tiles)", "hallway_curated.png")

# Larger rooms with wider hallway
HALLWAY2 = [
    "##########     ##########",
    "#........#     #........#",
    "#........#     #........#",
    "#........#     #........#",
    "#........#######........#",
    "#.............._.........#",
    "#........#######........#",
    "#........#     #........#",
    "#........#     #........#",
    "#........#     #........#",
    "##########     ##########",
]
render_layout(HALLWAY2, "Large rooms + hallway (curated tiles)", "hallway_large_curated.png")

# Simple single room for reference
ROOM = [
    "##########",
    "#........#",
    "#........#",
    "#........#",
    "#........#",
    "#........#",
    "#........#",
    "##########",
]
render_layout(ROOM, "Single room 10x8 (curated tiles)", "room_curated.png")

# L-shaped room
L_SHAPE = [
    "##########      ",
    "#........#      ",
    "#........#      ",
    "#........#      ",
    "#........#######",
    "#..............#",
    "#..............#",
    "#..............#",
    "################",
]
render_layout(L_SHAPE, "L-shaped room", "l_shape_curated.png")

# Four rooms around a central hub
FOUR_ROOMS = [
    "#####   #####",
    "#...#   #...#",
    "#...#####...#",
    "#...........#",
    "#...#####...#",
    "#...#   #...#",
    "##.##   ##.##",
    "  .       .  ",
    "##.##   ##.##",
    "#...#   #...#",
    "#...#####...#",
    "#...........#",
    "#...#####...#",
    "#...#   #...#",
    "#####   #####",
]
render_layout(FOUR_ROOMS, "Four rooms + central corridors", "four_rooms_curated.png")

# Long corridor with side rooms
CORRIDOR = [
    "##### ##### ##### #####",
    "#...# #...# #...# #...#",
    "#...# #...# #...# #...#",
    "#...###...###...###...#",
    "#.....................#",
    "#...###...###...###...#",
    "#...# #...# #...# #...#",
    "#...# #...# #...# #...#",
    "##### ##### ##### #####",
]
render_layout(CORRIDOR, "Long corridor with side rooms", "corridor_curated.png")

# T-intersection
T_INTERSECT = [
    "     #####     ",
    "     #...#     ",
    "     #...#     ",
    "#####....######",
    "#.............#",
    "#####....######",
    "     #...#     ",
    "     #...#     ",
    "     #...#     ",
    "     #...#     ",
    "     #####     ",
]
render_layout(T_INTERSECT, "T-intersection", "t_intersect_curated.png")

# Large open chamber with pillars (using '#' as pillars in interior)
CHAMBER = [
    "################",
    "#..............#",
    "#..............#",
    "#...#....#.....#",
    "#..............#",
    "#..............#",
    "#..............#",
    "#...#....#.....#",
    "#..............#",
    "#..............#",
    "################",
]
render_layout(CHAMBER, "Chamber with pillars", "chamber_curated.png")
