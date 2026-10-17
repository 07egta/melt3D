"""Furniture models for the YURACAD library (miniature, 60-100 mm). Run: python3 furniture.py

Writes ../furniture.json and renders previews to /tmp/lib_preview_furn/ (needs MELTCAD_BIN).
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

# ---- Household everyday furniture (shared 1:15 miniature proportions) -----------------
# ダイニングチェア / 9 primitives
items.append(item("ダイニングチェア", ["いす","椅子","chair","dining chair","食卓","furniture"], group("ダイニングチェア", [
    bx("木製座枠", -15, 15, -16, 15, 27, 29, WOOD_LIGHT, round=0.8),
    cy("前左脚", -12, -12, 0, 28, 1.9, WOOD),
    cy("前右脚", 12, -12, 0, 28, 1.9, WOOD),
    cy("後左脚", -12, 12, 0, 56, 1.9, WOOD),
    cy("後右脚", 12, 12, 0, 56, 1.9, WOOD),
    bx("背もたれ上段", -13, 13, 10.5, 14.5, 49, 55, WOOD_LIGHT, round=1),
    bx("背もたれ中段", -13, 13, 10.5, 14.5, 38, 43, WOOD_LIGHT, round=1),
    bx("座面クッション", -13.5, 13.5, -14, 11, 29, 31.5, CREAM, round=1, blend=0.8),
    bx("前脚補強材", -11.5, 11.5, -12, -10.5, 8, 10, WOOD, round=0.2),
])))

# スツール / 8 primitives
items.append(item("スツール", ["stool","chair","腰掛け","丸椅子","furniture"], group("スツール", [
    cy("木の座面", 0, 0, 28.5, 31.5, 14, WOOD_LIGHT, round=1),
    cy("座面クッション", 0, 0, 31, 33, 13.5, LEATHER, round=1, blend=0.5),
    cy("脚左前", -9.5, -9.5, 0, 29.5, 1.8, WOOD),
    cy("脚右前", 9.5, -9.5, 0, 29.5, 1.8, WOOD),
    cy("脚左後", -9.5, 9.5, 0, 29.5, 1.8, WOOD),
    cy("脚右後", 9.5, 9.5, 0, 29.5, 1.8, WOOD),
    bx("前後の足置き", -10, 10, -1, 1, 9, 11, WOOD, round=0.2),
    bx("左右の補強", -1, 1, -10, 10, 9, 11, WOOD, round=0.2),
])))

# ダイニングテーブル / 9 primitives
items.append(item("ダイニングテーブル", ["テーブル","table","dining table","食卓","furniture"], group("ダイニングテーブル", [
    bx("木目の天板", -50, 50, -26, 26, 47, 50, WOOD_LIGHT, round=1.2, blend=0.5),
    bx("前幕板", -44, 44, -22, -19, 40, 47.5, WOOD, round=0.3),
    bx("後幕板", -44, 44, 19, 22, 40, 47.5, WOOD, round=0.3),
    bx("左幕板", -46, -43, -19, 19, 40, 47.5, WOOD, round=0.3),
    bx("右幕板", 43, 46, -19, 19, 40, 47.5, WOOD, round=0.3),
    bx("前左脚", -46, -42, -22, -18, 0, 47.5, WOOD, round=0.35),
    bx("前右脚", 42, 46, -22, -18, 0, 47.5, WOOD, round=0.35),
    bx("後左脚", -46, -42, 18, 22, 0, 47.5, WOOD, round=0.35),
    bx("後右脚", 42, 46, 18, 22, 0, 47.5, WOOD, round=0.35),
])))

# ローテーブル / 6 primitives
items.append(item("ローテーブル", ["low table","coffee table","リビング","ちゃぶ台","furniture"], group("ローテーブル", [
    bx("低い天板", -38, 38, -22, 22, 24, 27, WOOD_LIGHT, round=1.5, blend=0.6),
    bx("下段の棚", -32, 32, -18, 18, 11, 13, WOOD, round=0.5),
    bx("前左脚", -33, -29, -18, -14, 0, 25, WOOD_DARK, round=0.35),
    bx("前右脚", 29, 33, -18, -14, 0, 25, WOOD_DARK, round=0.35),
    bx("後左脚", -33, -29, 14, 18, 0, 25, WOOD_DARK, round=0.35),
    bx("後右脚", 29, 33, 14, 18, 0, 25, WOOD_DARK, round=0.35),
])))

# 3人掛けソファ / 14 primitives
items.append(item("3人掛けソファ", ["ソファ","sofa","couch","three seater","三人掛け","furniture"], group("3人掛けソファ", [
    bx("ソファの土台", -60, 60, -24, 24, 8, 25, TEAL, round=3, blend=1),
    bx("高めの背もたれ", -60, 60, 12, 25, 21, 55, TEAL, round=3, blend=0.8),
    bx("左ひじ掛け", -60, -52, -24, 24, 9, 37, TEAL, round=2.8, blend=0.9),
    bx("右ひじ掛け", 52, 60, -24, 24, 9, 37, TEAL, round=2.8, blend=0.9),
    bx("座面クッション左", -51, -18, -22, 8, 25, 31, TEAL_LT, round=2, blend=0.3),
    bx("座面クッション中央", -16.5, 16.5, -22, 8, 25, 31, TEAL_LT, round=2, blend=0.3),
    bx("座面クッション右", 18, 51, -22, 8, 25, 31, TEAL_LT, round=2, blend=0.3),
    bx("背クッション左", -50, -18, 7, 16, 30, 49, CREAM, round=2, blend=0.2),
    bx("背クッション中央", -16, 16, 7, 16, 30, 49, CREAM, round=2, blend=0.2),
    bx("背クッション右", 18, 50, 7, 16, 30, 49, CREAM, round=2, blend=0.2),
    cy("前左脚", -55, -19, 0, 8.5, 2.4, WOOD_DARK),
    cy("前右脚", 55, -19, 0, 8.5, 2.4, WOOD_DARK),
    cy("後左脚", -55, 19, 0, 8.5, 2.4, WOOD_DARK),
    cy("後右脚", 55, 19, 0, 8.5, 2.4, WOOD_DARK),
])))

# シングルベッド / 13 primitives
items.append(item("シングルベッド", ["ベッド","bed","single bed","寝具","sleep","furniture"], group("シングルベッド", [
    bx("ベッドフレーム", -33, 33, -65, 65, 9, 21, WOOD, round=1),
    bx("左サイドフレーム", -33, -30, -65, 65, 10, 24, WOOD_DARK, round=0.5),
    bx("右サイドフレーム", 30, 33, -65, 65, 10, 24, WOOD_DARK, round=0.5),
    bx("ヘッドボード", -33, 33, 60, 65, 9, 65, WOOD_LIGHT, round=1.2),
    bx("フットボード", -33, 33, -65, -62, 9, 27, WOOD_LIGHT, round=0.7),
    bx("マットレス", -30, 30, -60, 57, 21, 31, WHITE, round=2.5, blend=0.6),
    bx("掛け布団", -29, 29, -58, 21, 31, 34, BLUE, round=1.5, blend=0.5),
    bx("枕左", -27, -2, 37, 54, 31, 37, CREAM, round=2.2, blend=0.5),
    bx("枕右", 2, 27, 37, 54, 31, 37, CREAM, round=2.2, blend=0.5),
    cy("足元左脚", -28, -59, 0, 10, 3, WOOD_DARK),
    cy("足元右脚", 28, -59, 0, 10, 3, WOOD_DARK),
    cy("頭側左脚", -28, 58, 0, 10, 3, WOOD_DARK),
    cy("頭側右脚", 28, 58, 0, 10, 3, WOOD_DARK),
])))

# 本棚 / 20 primitives
items.append(item("本棚", ["ほんだな","bookshelf","bookcase","書棚","furniture"], group("本棚", [
    bx("左側板", -30, -27, -12, 12, 0, 120, WOOD_LIGHT, round=0.4),
    bx("右側板", 27, 30, -12, 12, 0, 120, WOOD_LIGHT, round=0.4),
    bx("天板", -30, 30, -12, 12, 117, 120, WOOD_LIGHT, round=0.4),
    bx("底板", -30, 30, -12, 12, 0, 3, WOOD_DARK, round=0.4),
    bx("下段棚板", -27, 27, -12, 12, 29, 31, WOOD_LIGHT, round=0.2),
    bx("中段棚板", -27, 27, -12, 12, 58, 60, WOOD_LIGHT, round=0.2),
    bx("上段棚板", -27, 27, -12, 12, 87, 89, WOOD_LIGHT, round=0.2),
    bx("裏板", -27, 27, 10, 12, 3, 117, WOOD_PALE),
    bx("本1", -25, -14.5, -9, 7, 3, 24, BLUE, round=0.3),
    bx("本2", -12.5, -2, -9, 7, 3, 19, GREEN, round=0.3),
    bx("本3", 0, 10.5, -9, 7, 3, 22, CREAM, round=0.3),
    bx("本4", 12.5, 23, -9, 7, 3, 25, RED, round=0.3),
    bx("本5", -25, -14.5, -9, 7, 31, 50, WOOD_DARK, round=0.3),
    bx("本6", -12.5, -2, -9, 7, 31, 46, TEAL, round=0.3),
    bx("本7", 0, 10.5, -9, 7, 31, 53, YELLOW, round=0.3),
    bx("本8", 12.5, 23, -9, 7, 31, 48, ORANGE, round=0.3),
    bx("本9", -25, -14.5, -9, 7, 60, 80, BLUE, round=0.3),
    bx("本10", -12.5, -2, -9, 7, 60, 83, GREEN, round=0.3),
    bx("本11", 0, 10.5, -9, 7, 60, 78, RED, round=0.3),
    bx("本12", 12.5, 23, -9, 7, 60, 76, CREAM, round=0.3),
])))

# 勉強机 / 11 primitives
items.append(item("勉強机", ["study desk","writing desk","学習机","つくえ","desk","furniture"], group("勉強机", [
    bx("広い天板", -40, 40, -21, 21, 47, 50, WOOD_LIGHT, round=0.8),
    bx("前左脚", -37, -33, -19, -15, 0, 48, WOOD, round=0.3),
    bx("前右脚", 33, 37, -19, -15, 0, 48, WOOD, round=0.3),
    bx("後左脚", -37, -33, 15, 19, 0, 48, WOOD, round=0.3),
    bx("後右脚", 33, 37, 15, 19, 0, 48, WOOD, round=0.3),
    bx("背面補強", -34, 34, 18, 20, 36, 47, WOOD, round=0.3),
    bx("引出し収納箱", 15, 37, -18, 18, 17, 45, WOOD_PALE, round=0.4),
    bx("上の引出し", 16.2, 36, -19.5, -18, 35, 43.3, WOOD_LIGHT, round=0.3),
    bx("下の引出し", 16.2, 36, -19.5, -18, 25, 33.3, WOOD_LIGHT, round=0.3),
    bx("上の取っ手", 23, 30, -21, -19.4, 38.4, 40, BRASS, round=0.4, metallic=0.7, roughness=0.3),
    bx("下の取っ手", 23, 30, -21, -19.4, 28.4, 30, BRASS, round=0.4, metallic=0.7, roughness=0.3),
])))

# タンス / 14 primitives
items.append(item("タンス", ["たんす","箪笥","chest of drawers","drawer","収納","furniture"], group("タンス", [
    bx("木製収納本体", -29, 29, -15, 15, 5, 70, WOOD, round=0.8),
    bx("天板", -30, 30, -16, 16, 70, 73, WOOD_DARK, round=0.8),
    cy("脚1", -25, -11, 0, 5.5, 2.3, WOOD_DARK),
    cy("脚2", 25, -11, 0, 5.5, 2.3, WOOD_DARK),
    cy("脚3", -25, 11, 0, 5.5, 2.3, WOOD_DARK),
    cy("脚4", 25, 11, 0, 5.5, 2.3, WOOD_DARK),
    bx("引出し1", -27, 27, -16.7, -14.8, 7, 20.6, WOOD_LIGHT, round=0.5),
    bx("取っ手1", -6, 6, -18.4, -16.5, 12.7, 14.9, BRASS, round=0.6, roughness=0.3, metallic=0.8),
    bx("引出し2", -27, 27, -16.7, -14.8, 22.2, 35.8, WOOD_LIGHT, round=0.5),
    bx("取っ手2", -6, 6, -18.4, -16.5, 27.9, 30.1, BRASS, round=0.6, roughness=0.3, metallic=0.8),
    bx("引出し3", -27, 27, -16.7, -14.8, 37.4, 51, WOOD_LIGHT, round=0.5),
    bx("取っ手3", -6, 6, -18.4, -16.5, 43.1, 45.3, BRASS, round=0.6, roughness=0.3, metallic=0.8),
    bx("引出し4", -27, 27, -16.7, -14.8, 52.599999999999994, 66.19999999999999, WOOD_LIGHT, round=0.5),
    bx("取っ手4", -6, 6, -18.4, -16.5, 58.3, 60.49999999999999, BRASS, round=0.6, roughness=0.3, metallic=0.8),
])))

# オフィスチェア / 21 primitives
items.append(item("オフィスチェア", ["office chair","desk chair","事務椅子","回転椅子","キャスター","furniture"], group("オフィスチェア", [
    bx("座面のフレーム", -20, 20, -17, 16, 29, 33, DARK, round=2, blend=0.5),
    bx("厚手の座面クッション", -19, 19, -16, 15, 33, 36, BLUE, round=2, blend=0.4),
    bx("背もたれ本体", -19, 19, 12, 19, 37, 74, DARK, round=2, blend=0.5),
    bx("背クッション", -16, 16, 10.5, 13.5, 43, 70, BLUE, round=2, blend=0.3),
    bx("背を支える板", -3, 3, 12, 19, 30, 44, METAL, round=0.6),
    bx("左ひじ掛け", -22, -17, -11, 13, 43, 45.5, DARK, round=1.5),
    bx("右ひじ掛け", 17, 22, -11, 13, 43, 45.5, DARK, round=1.5),
    bx("左ひじの支柱", -21, -18, 8, 11, 33, 44, METAL, round=0.3),
    bx("右ひじの支柱", 18, 21, 8, 11, 33, 44, METAL, round=0.3),
    cy("昇降支柱", 0, 0, 6, 30, 2.8, METAL, metallic=0.75, roughness=0.3),
    cy("脚の中央", 0, 0, 4.8, 10, 5, DARK, round=0.8),
    bx("五本脚1", 1.8000000000000007, 19.8, -1.5, 1.5, 3.9, 7.1, DARK, round=1, rot=(0, 0, 0)),
    bx("五本脚2", -5.662616460750567, 12.337383539249434, 8.771410375987658, 11.771410375987658, 3.9, 7.1, DARK, round=1, rot=(0, 0, 72)),
    bx("五本脚3", -17.737383539249432, 0.2626164607505679, 4.848080724758711, 7.848080724758711, 3.9, 7.1, DARK, round=1, rot=(0, 0, 144)),
    bx("五本脚4", -17.737383539249436, 0.2626164607505661, -7.848080724758709, -4.848080724758709, 3.9, 7.1, DARK, round=1, rot=(0, 0, 216)),
    bx("五本脚5", -5.66261646075057, 12.33738353924943, -11.77141037598766, -8.77141037598766, 3.9, 7.1, DARK, round=1, rot=(0, 0, 288)),
    ball("キャスター1", 20, 0, 2.2, 2.2, DARK, roughness=0.4),
    ball("キャスター2", 6.180339887498949, 19.02113032590307, 2.2, 2.2, DARK, roughness=0.4),
    ball("キャスター3", -16.180339887498945, 11.755705045849465, 2.2, 2.2, DARK, roughness=0.4),
    ball("キャスター4", -16.180339887498953, -11.75570504584946, 2.2, 2.2, DARK, roughness=0.4),
    ball("キャスター5", 6.180339887498945, -19.021130325903073, 2.2, 2.2, DARK, roughness=0.4),
])))

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




# A single miniature scale (about 1:15) is used across the ten everyday objects.
# Older generic names stay searchable through tags; no near-duplicate is registered.
if __name__ == "__main__":
    assert len(items) == 15, len(items)
    assert len({it["name"] for it in items}) == 15
    for it in items:
        n = len(it["group"]["shapes"])
        assert 4 <= n <= 24, (it["name"], n)
    write_category("furniture.json", "かぐ", items)
    if os.environ.get("MELTCAD_BIN") and os.path.isfile(os.environ["MELTCAD_BIN"]):
        preview(items, outdir="/tmp/lib_preview_furn")
