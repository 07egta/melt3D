"""Furniture models for the YURACAD library (miniature, 60-100 mm). Run: python3 furniture.py

Writes ../furniture.json and renders previews to /tmp/lib_preview_furn/ (needs YURACAD_BIN).
Coordinates: mm, Z up, the front faces -Y. Box/Cylinder `pos` is the centre; size is the full
extent (Cylinder size = (radius, height)).
"""
import math

from lib import *

# ---- colours ------------------------------------------------------------------------------
WOOD = (0.66, 0.44, 0.24)
WOOD_DARK = (0.40, 0.25, 0.14)
WOOD_LIGHT = (0.88, 0.74, 0.54)
WOOD_PALE = (0.93, 0.84, 0.66)
TEAL = (0.20, 0.52, 0.52)
TEAL_LT = (0.36, 0.68, 0.66)
ORANGE = (0.90, 0.52, 0.22)
RED = (0.78, 0.22, 0.20)
BLUE = (0.25, 0.42, 0.72)
YELLOW = (0.92, 0.74, 0.25)
GREEN = (0.35, 0.60, 0.38)
LEATHER = (0.62, 0.36, 0.20)
WHITE = (0.95, 0.95, 0.93)
CREAM = (0.95, 0.91, 0.80)
METAL = (0.72, 0.74, 0.78)
DARK = (0.12, 0.12, 0.14)
GLASS = (0.72, 0.88, 1.0)
BRASS = (0.86, 0.68, 0.30)


def bx(name, x0, x1, y0, y1, z0, z1, color, **kw):
    """Box from min/max corners (mm)."""
    kw.setdefault("roughness", 0.55)
    kw.setdefault("blend", 0.5)
    return shape("Box", name, pos=((x0 + x1) / 2, (y0 + y1) / 2, (z0 + z1) / 2),
                 size=(x1 - x0, y1 - y0, z1 - z0), color=color, **kw)


def cy(name, x, y, z0, z1, r, color, **kw):
    """Upright cylinder from z0 to z1, radius r."""
    kw.setdefault("roughness", 0.55)
    kw.setdefault("blend", 0.5)
    return shape("Cylinder", name, pos=(x, y, (z0 + z1) / 2), size=(r, z1 - z0, 1), color=color, **kw)


def ball(name, x, y, z, r, color, **kw):
    kw.setdefault("roughness", 0.3)
    kw.setdefault("blend", 0.0)
    return shape("Sphere", name, pos=(x, y, z), size=(r, 1, 1), color=color, **kw)


items = []

# ---- いす: wooden chair with a red seat cushion and tall back posts ------------------------
items.append(item("いす", ["chair", "椅子", "furniture"], group("いす", [
    bx("座", -18, 18, -18, 18, 22, 26, WOOD, round=1.2, blend=1.0),
    cy("前左脚", -16, -16, 0, 23, 2.2, WOOD, blend=1.0),
    cy("前右脚", 16, -16, 0, 23, 2.2, WOOD, blend=1.0),
    cy("後左脚", -16, 14, 0, 64, 2.2, WOOD, blend=1.0),
    cy("後右脚", 16, 14, 0, 64, 2.2, WOOD, blend=1.0),
    bx("背上の板", -16, 16, 10, 16, 52, 60, WOOD, round=1.0, blend=0.8),
    bx("背中の板", -16, 16, 10, 16, 34, 40, WOOD, round=1.0, blend=0.8),
    bx("前のよこ木", -16, 16, -17, -15, 6, 9, WOOD, blend=0.8),
    bx("クッション", -16, 16, -16, 10, 26, 29, RED, round=2.5, blend=1.0),
])))

# ---- ソファ: teal three-seat sofa with arms, back cushions and feet ----------------------
items.append(item("ソファ", ["sofa", "couch", "furniture"], group("ソファ", [
    bx("ベース", -35, 35, -15, 15, 6, 18, TEAL, round=4, blend=0.5),
    bx("背", -35, 35, 9, 16, 16, 46, TEAL, round=4, blend=2.0),
    bx("ひじ左", -35, -27, -15, 15, 6, 28, TEAL, round=3, blend=2.0),
    bx("ひじ右", 27, 35, -15, 15, 6, 28, TEAL, round=3, blend=2.0),
    bx("座面左", -27, -1, -14, 6, 18, 26, TEAL_LT, round=3, blend=1.5),
    bx("座面右", 1, 27, -14, 6, 18, 26, TEAL_LT, round=3, blend=1.5),
    bx("まくら左", -26, -8, 2, 10, 26, 42, ORANGE, round=3, blend=1.5),
    bx("まくら右", 8, 26, 2, 10, 26, 42, YELLOW, round=3, blend=1.5),
    cy("足左前", -31, -11, 0, 6, 2.2, WOOD_DARK, blend=0.0),
    cy("足右前", 31, -11, 0, 6, 2.2, WOOD_DARK, blend=0.0),
    cy("足左後", -31, 11, 0, 6, 2.2, WOOD_DARK, blend=0.0),
    cy("足右後", 31, 11, 0, 6, 2.2, WOOD_DARK, blend=0.0),
])))

# ---- テーブル: square table with four legs and a lower shelf -----------------------------
items.append(item("テーブル", ["table", "desk", "furniture"], group("テーブル", [
    bx("天板", -36, 36, -24, 24, 56, 60, WOOD_LIGHT, round=1.5, blend=1.0),
    bx("棚板", -31, 31, -19, 19, 14, 17, WOOD, round=0.8, blend=1.0),
    cy("脚左前", -31, -19, 0, 57, 2.6, WOOD, blend=1.0),
    cy("脚右前", 31, -19, 0, 57, 2.6, WOOD, blend=1.0),
    cy("脚左後", -31, 19, 0, 57, 2.6, WOOD, blend=1.0),
    cy("脚右後", 31, 19, 0, 57, 2.6, WOOD, blend=1.0),
])))

# ---- ベッド: bed frame, mattress, headboard, blanket and a pillow (head at +Y) -------------
items.append(item("ベッド", ["bed", "pillow", "寝具", "furniture"], group("ベッド", [
    bx("フレーム", -25, 25, -38, 38, 4, 14, WOOD, round=1.0, blend=0.5),
    bx("足板", -25, 25, -38, -34, 4, 16, WOOD, round=1.0, blend=0.5),
    bx("ヘッドボード", -25, 25, 32, 38, 4, 36, WOOD, round=1.5, blend=0.5),
    bx("マットレス", -23, 23, -34, 32, 14, 24, WHITE, round=3, blend=1.0),
    bx("まくら", -15, 15, 20, 31, 24, 31, WHITE, round=3, blend=1.5),
    bx("ふとん", -23, 23, -34, 10, 24, 28, RED, round=2, blend=1.5),
    cy("脚前左", -23, -35, 0, 4, 2.5, WOOD_DARK, blend=0.0),
    cy("脚前右", 23, -35, 0, 4, 2.5, WOOD_DARK, blend=0.0),
    cy("脚後左", -23, 35, 0, 4, 2.5, WOOD_DARK, blend=0.0),
    cy("脚後右", 23, 35, 0, 4, 2.5, WOOD_DARK, blend=0.0),
])))

# ---- ほんだな: open bookcase with two shelves, a back panel and books --------------------
shelf = []
shelf += [
    bx("左板", -25, -22, -10, 10, 0, 72, WOOD_LIGHT, round=0.5, blend=0.0),
    bx("右板", 22, 25, -10, 10, 0, 72, WOOD_LIGHT, round=0.5, blend=0.0),
    bx("上板", -25, 25, -10, 10, 69, 72, WOOD_LIGHT, round=0.5, blend=0.0),
    bx("下板", -25, 25, -10, 10, 0, 3, WOOD, round=0.5, blend=0.0),
    bx("たな1", -22, 22, -10, 10, 24, 26.5, WOOD_LIGHT, blend=0.0),
    bx("たな2", -22, 22, -10, 10, 48, 50.5, WOOD_LIGHT, blend=0.0),
    bx("裏板", -22, 22, 8, 10, 3, 69, WOOD_PALE, blend=0.0),
]
# books: (width, height, colour) placed left to right on each shelf gap
gaps = [
    (3.0, [(4.5, 17, RED), (3.8, 14, BLUE), (5.0, 16, YELLOW), (4.2, 12, GREEN)]),
    (26.5, [(4.0, 16, TEAL), (5.2, 13, ORANGE), (3.6, 15, RED), (4.8, 11, BLUE)]),
    (50.5, [(4.4, 15, GREEN), (3.4, 17, YELLOW), (5.0, 12, RED), (4.0, 14, TEAL)]),
]
for n, (floor, books) in enumerate(gaps):
    x = -20.5
    for k, (w, h, c) in enumerate(books):
        shelf.append(bx(f"本{n}-{k}", x, x + w, -8, 6, floor, floor + h, c, blend=0.0))
        x += w + 0.4
items.append(item("ほんだな", ["bookshelf", "bookcase", "本棚", "furniture"], group("ほんだな", shelf)))

# ---- たんす: chest of four drawers with brass pulls and feet ------------------------------
chest = [
    bx("本体", -26, 26, -16, 16, 5, 66, WOOD, round=1.2, blend=0.5),
    bx("天板", -28, 28, -18, 18, 66, 69, WOOD_DARK, round=1.5, blend=0.5),
    cy("足左前", -23, -13, 0, 6, 2.5, WOOD_DARK, blend=0.5),
    cy("足右前", 23, -13, 0, 6, 2.5, WOOD_DARK, blend=0.5),
    cy("足左後", -23, 13, 0, 6, 2.5, WOOD_DARK, blend=0.5),
    cy("足右後", 23, 13, 0, 6, 2.5, WOOD_DARK, blend=0.5),
]
for k in range(4):
    z0 = 6 + k * 14.5
    z1 = z0 + 13.5
    chest.append(bx(f"引き出し{k}", -24, 24, -17.2, -16, z0, z1, WOOD_LIGHT, round=0.6, blend=0.0))
    chest.append(bx(f"取っ手{k}", -6, 6, -18.9, -17.0, (z0 + z1) / 2 - 1.1, (z0 + z1) / 2 + 1.1,
                    BRASS, round=0.8, blend=0.0, metallic=0.9, roughness=0.25))
items.append(item("たんす", ["chest", "drawers", "箪笥", "furniture"], group("たんす", chest)))

# ---- スツール: round seat, four legs and a ring stretcher ---------------------------------
stool = [
    shape("Cylinder", "座面", pos=(0, 0, 54.5), size=(14, 5, 1), round=2.0, color=LEATHER,
          roughness=0.7, blend=0.5),
    shape("Torus", "ぼう", pos=(0, 0, 14), size=(10, 1.4, 1), color=WOOD, roughness=0.5, blend=0.5),
]
for k, ang in enumerate((45, 135, 225, 315)):
    a = math.radians(ang)
    stool.append(cy(f"脚{k}", 10 * math.cos(a), 10 * math.sin(a), 0, 56, 1.8, WOOD, blend=1.0))
items.append(item("スツール", ["stool", "chair", "腰掛け", "furniture"], group("スツール", stool)))

# ---- ランプ: floor lamp with a glowing bulb inside a translucent shade -------------------
items.append(item("ランプ", ["lamp", "light", "灯", "glow", "furniture"], group("ランプ", [
    cy("台座", 0, 0, 0, 3, 11, METAL, round=1.0, blend=0.5, roughness=0.3, metallic=0.8),
    cy("支柱", 0, 0, 2, 56, 1.2, METAL, blend=1.0, roughness=0.3, metallic=0.8),
    shape("Revolve", "かさ", pos=(0, 0, 0), size=(1, 1, 1), params=(360, 0, 0, 0),
          outline=[[19, 36], [12, 60], [10.8, 60], [17.5, 36]], color=CREAM,
          translucency=0.6, roughness=0.8, blend=0.0, glow=0.35),
    ball("でんきゅう", 0, 0, 47, 5.0, (1.0, 0.92, 0.62), glow=3.5, roughness=0.3),
])))

# ---- テレビ: flat-screen TV on a stand, screen lit ------------------------------------------
items.append(item("テレビ", ["tv", "television", "monitor", "画面", "furniture"], group("テレビ", [
    bx("本体", -32, 32, -3, 3, 30, 70, DARK, round=2.5, blend=0.0, roughness=0.4),
    bx("画面", -28, 28, -3.8, -2.5, 34, 66, (0.10, 0.22, 0.42), round=1.0, blend=0.0,
       roughness=0.1, coat=0.5, glow=0.6),
    bx("スタンド首", -3, 3, -2, 2, 3, 30, DARK, blend=1.0),
    bx("スタンド台", -15, 15, -7, 7, 0, 3, DARK, round=1.0, blend=1.0),
])))

# ---- れいぞうこ: white fridge with a freezer door split and two handles -------------------
items.append(item("れいぞうこ", ["fridge", "refrigerator", "冷蔵庫", "furniture"], group("れいぞうこ", [
    bx("本体", -20, 20, -19, 19, 3, 82, WHITE, round=2.5, blend=0.0, roughness=0.3),
    bx("ひょうめん", -18, 18, -17, 17, 0, 3, DARK, round=0.5, blend=0.0),
    bx("とびら", -22, 22, -20, -18, 55.5, 56.5, DARK, op="Subtract", blend=0.0),
    bx("とって上", 12, 14.5, -22, -19, 64, 74, METAL, round=1.0, blend=0.0, metallic=0.8, roughness=0.25),
    bx("とって下", 12, 14.5, -22, -19, 36, 50, METAL, round=1.0, blend=0.0, metallic=0.8, roughness=0.25),
])))

# ---- ドア: a painted door in a white frame, standing on a threshold -----------------------
items.append(item("ドア", ["door", "扉", "戸", "furniture"], group("ドア", [
    bx("敷居", -28, 28, -4, 4, 0, 1.5, WOOD_DARK, round=0.5, blend=0.0),
    bx("かまち左", -28, -23, -4, 4, 0, 78, WHITE, blend=0.0),
    bx("かまち右", 23, 28, -4, 4, 0, 78, WHITE, blend=0.0),
    bx("よこ木", -23, 23, -4, 4, 74, 78, WHITE, blend=0.0),
    bx("とびら板", -21.5, 21.5, -2, 2, 1.5, 74, BLUE, round=0.5, blend=0.0, roughness=0.6),
    bx("上の鏡板", -13, 13, -3, -2, 42, 66, BLUE, round=1.0, blend=0.0, roughness=0.6),
    bx("下の鏡板", -13, 13, -3, -2, 8, 36, BLUE, round=1.0, blend=0.0, roughness=0.6),
    ball("ドアノブ", 16, -3.4, 38, 2.2, BRASS, metallic=0.9, roughness=0.2),
])))

# ---- まど: a window with glass, cross bars and a sill, cut into a wall ---------------------
items.append(item("まど", ["window", "窓", "glass", "ガラス", "furniture"], group("まど", [
    bx("かべ", -35, 35, -3, 3, 0, 70, CREAM, blend=0.0, roughness=0.8),
    bx("まどの穴", -22, 22, -5, 5, 13, 57, CREAM, op="Subtract", blend=0.0),
    bx("ガラス", -19, 19, -0.8, 0.8, 16, 54, GLASS, glass=1.0, blend=0.0, roughness=0.05),
    bx("わく上", -22, 22, -2.5, 2.5, 53, 57, WHITE, blend=0.0),
    bx("わく下", -22, 22, -2.5, 2.5, 13, 17, WHITE, blend=0.0),
    bx("わく左", -22, -18, -2.5, 2.5, 13, 57, WHITE, blend=0.0),
    bx("わく右", 18, 22, -2.5, 2.5, 13, 57, WHITE, blend=0.0),
    bx("たて桟", -1, 1, -2, 2, 17, 53, WHITE, blend=0.0),
    bx("よこ桟", -18, 18, -2, 2, 34, 36, WHITE, blend=0.0),
    bx("まどだい", -26, 26, -6, 4, 10, 13, WHITE, round=0.8, blend=0.5),
])))


if __name__ == "__main__":
    assert len(items) == 12, len(items)
    for it in items:
        n = len(it["group"]["shapes"])
        assert 4 <= n <= 22, (it["name"], n)
    write_category("furniture.json", "かぐ", items)
    preview(items, outdir="/tmp/lib_preview_furn")
