"""Ready-made toy vehicles for YURACAD (category のりもの). Run from gen/: python3 vehicles.py

Conventions: mm, Z up, nose towards -Y, wheels touch z=0. Wheels are a Torus tyre plus a Cylinder
hub, both with their axle along X (rot 0,90,0). Revolve and Loft profiles are centred on `pos` in Z.
"""
from lib import *

TYRE = (0.08, 0.08, 0.09)
CHROME = (0.80, 0.82, 0.86)
GLASS = (0.55, 0.80, 0.95)
HEAD = (1.0, 0.94, 0.62)
TAIL = (0.95, 0.15, 0.10)
WHITE = (0.95, 0.95, 0.93)
DARK = (0.15, 0.16, 0.18)


def paint(kind, name, pos, size=None, color=(0.9, 0.2, 0.15), blend=2.0, **kw):
    """Shiny coated paint."""
    kw.setdefault("roughness", 0.25)
    kw.setdefault("coat", 1.0)
    return shape(kind, name, pos=pos, size=size, color=color, blend=blend, **kw)


def plain(kind, name, pos, size=None, color=DARK, blend=1.0, **kw):
    """Matt part (bumpers, chassis, tyres' neighbours)."""
    kw.setdefault("roughness", 0.7)
    return shape(kind, name, pos=pos, size=size, color=color, blend=blend, **kw)


def win(name, pos, size, blend=0.0, **kw):
    """Clear glass pane."""
    return shape("Box", name, pos=pos, size=size, color=GLASS, roughness=0.05, glass=1.0,
                 blend=blend, **kw)


def lamp(name, pos, color=HEAD, size=2.5, glow=1.5):
    return shape("Sphere", name, pos=pos, size=(size, 1, 1), color=color, roughness=0.2,
                 glow=glow, blend=0.5)


def wheel(tag, x, y, R=6.5, t=3.0, hub_r=None, hub_len=None, z=None):
    """A wheel with its axle along X: Torus tyre (major R, minor t) + Cylinder hub.
    Its centre sits at R + t so the tyre touches z = 0 (unless z is given)."""
    zc = (R + t) if z is None else z
    hub_r = (R - t + 0.4) if hub_r is None else hub_r
    hub_len = (2 * t * 0.9) if hub_len is None else hub_len
    return [
        shape("Torus", tag + "タイヤ", pos=(x, y, zc), size=(R, t, 1), rot=(0, 90, 0), blend=0.5,
              color=TYRE, roughness=0.9),
        shape("Cylinder", tag + "ホイール", pos=(x, y, zc), size=(hub_r, hub_len, 1), rot=(0, 90, 0),
              blend=0.5, color=CHROME, roughness=0.25, metallic=0.9),
    ]


def four_wheels(x, ys, R=6.5, t=3.0):
    out = []
    for y in ys:
        side = "左" if x < 0 else "右"
        end = "前" if y < 0 else "後"
        out += wheel(side + end, x, y, R=R, t=t)
    return out


def sedan_shapes(col, police=False):
    s = [
        paint("Box", "車体", (0, 0, 12), (36, 84, 12), col, blend=3, round=5),
        paint("Box", "キャビン", (0, 2, 24), (28, 40, 12), col, blend=4, round=6),
        win("フロントガラス", (0, -19.2, 25.5), (25, 1.5, 9), round=1),
        win("左窓", (-14.6, 2, 25), (1.5, 22, 8), round=1),
        win("右窓", (14.6, 2, 25), (1.5, 22, 8), round=1),
        win("リアガラス", (0, 21.2, 25.5), (24, 1.5, 8), round=1),
        lamp("左ヘッドライト", (-11, -41.5, 16)),
        lamp("右ヘッドライト", (11, -41.5, 16)),
        shape("Box", "左テールランプ", pos=(-11, 42.5, 16), size=(6, 1.5, 3), color=TAIL, glow=0.8,
              roughness=0.2, blend=0.5),
        shape("Box", "右テールランプ", pos=(11, 42.5, 16), size=(6, 1.5, 3), color=TAIL, glow=0.8,
              roughness=0.2, blend=0.5),
    ]
    if police:
        s += [
            shape("Box", "赤色灯", pos=(-3.5, 0, 31.5), size=(6, 4, 3), color=(1.0, 0.1, 0.05),
                  glow=2.0, roughness=0.2, blend=0.3),
            shape("Box", "青色灯", pos=(3.5, 0, 31.5), size=(6, 4, 3), color=(0.1, 0.35, 1.0),
                  glow=2.0, roughness=0.2, blend=0.3),
            shape("Box", "左ドアの帯", pos=(-18.2, 0, 13), size=(2, 70, 4), op="Paint",
                  color=(0.04, 0.04, 0.05)),
            shape("Box", "右ドアの帯", pos=(18.2, 0, 13), size=(2, 70, 4), op="Paint",
                  color=(0.04, 0.04, 0.05)),
        ]
    return s


def make_car():
    s = sedan_shapes((0.15, 0.45, 0.9))
    s += four_wheels(-17, [-25, 25]) + four_wheels(17, [-25, 25])
    return s


def make_police():
    s = sedan_shapes((0.96, 0.96, 0.94), police=True)
    s += four_wheels(-17, [-25, 25]) + four_wheels(17, [-25, 25])
    return s


def make_sports():
    red = (0.9, 0.08, 0.08)
    s = [
        paint("Box", "車体", (0, 0, 9), (38, 96, 10), red, blend=3, round=4),
        paint("Box", "キャビン", (0, 5, 19), (26, 30, 9), red, blend=4, round=5),
        win("左窓", (-13.4, 5, 19), (1.5, 22, 6), round=1),
        win("右窓", (13.4, 5, 19), (1.5, 22, 6), round=1),
        win("フロントガラス", (0, -10.5, 19.5), (24, 1.5, 7), round=1),
        # white racing stripe, painted on the top of the body
        shape("Box", "白いストライプ", pos=(0, 0, 14.2), size=(5, 90, 1.2), op="Paint", color=WHITE),
        paint("Box", "リアスポイラー", (0, 41, 20), (30, 5, 1.8), red, blend=0.5, round=0.5),
        plain("Box", "スポイラー支柱左", (-9, 40, 16), (2, 4, 6), color=DARK, blend=0.5),
        plain("Box", "スポイラー支柱右", (9, 40, 16), (2, 4, 6), color=DARK, blend=0.5),
        shape("Box", "左ヘッドライト", pos=(-10, -45, 11), size=(8, 2, 2.5), color=HEAD, glow=1.5,
              roughness=0.2, blend=0.5),
        shape("Box", "右ヘッドライト", pos=(10, -45, 11), size=(8, 2, 2.5), color=HEAD, glow=1.5,
              roughness=0.2, blend=0.5),
    ]
    s += four_wheels(-17.5, [-29, 27], R=6.0, t=2.8) + four_wheels(17.5, [-29, 27], R=6.0, t=2.8)
    return s


def make_bus():
    yellow = (0.98, 0.76, 0.12)
    s = [
        paint("Box", "車体", (0, 0, 31), (36, 104, 38), yellow, blend=2, round=4),
        win("左窓", (-18.4, 0, 38), (1.5, 84, 12), round=1),
        win("右窓", (18.4, 0, 38), (1.5, 84, 12), round=1),
        win("フロントガラス", (0, -52.2, 37), (30, 1.5, 22), round=2),
        plain("Box", "行先表示", (0, -52.4, 49), (26, 1, 4), color=DARK, blend=0.5),
        lamp("左ヘッドライト", (-12, -52.6, 16)),
        lamp("右ヘッドライト", (12, -52.6, 16)),
        plain("Box", "バンパー", (0, -53, 6), (30, 3, 5), color=DARK, blend=0.5),
    ]
    s += four_wheels(-18, [-34, 34], R=6.0, t=3.0) + four_wheels(18, [-34, 34], R=6.0, t=3.0)
    return s


def make_truck():
    blue = (0.14, 0.34, 0.82)
    s = [
        plain("Box", "シャシー", (0, 0, 10), (30, 92, 6), color=DARK, blend=1, round=1),
        paint("Box", "運転席", (0, -28, 27), (34, 28, 28), blue, blend=3, round=5),
        win("フロントガラス", (0, -42.6, 33), (28, 1.5, 12), round=1),
        win("左窓", (-17.4, -28, 33), (1.5, 14, 11), round=1),
        win("右窓", (17.4, -28, 33), (1.5, 14, 11), round=1),
        plain("Box", "グリル", (0, -43.2, 16), (20, 2, 6), color=CHROME, blend=0.5, roughness=0.3),
        lamp("左ヘッドライト", (-12, -43.5, 24), size=2.8),
        lamp("右ヘッドライト", (12, -43.5, 24), size=2.8),
        paint("Box", "荷台", (0, 16, 32), (34, 56, 38), WHITE, blend=1, round=1.5),
    ]
    s += four_wheels(-18, [-30, 26], R=7.0, t=3.0) + four_wheels(18, [-30, 26], R=7.0, t=3.0)
    return s


def make_plane():
    blue = (0.12, 0.38, 0.85)
    fus = (0.96, 0.96, 0.95)
    s = [
        # fuselage: a capsule lying along Y (Z-axis capsule turned by rot x 90)
        paint("Capsule", "機体", (0, 0, 16), (6, 72, 1), fus, blend=2, rot=(90, 0, 0)),
        paint("Box", "垂直尾翼", (0, 34, 28), (2.5, 14, 16), blue, blend=1.5, round=1),
        paint("Box", "水平尾翼", (0, 34, 18), (30, 10, 2), fus, blend=1.5, round=1),
        paint("Box", "主翼", (0, -2, 15), (84, 14, 2.5), fus, blend=1.5, round=1.5),
        win("コックピット", (0, -22, 21), (7, 10, 4), round=2),
        win("左窓", (-6.2, 0, 18), (1, 40, 2.5), round=1),
        win("右窓", (6.2, 0, 18), (1, 40, 2.5), round=1),
        # engines under the wings
        plain("Cylinder", "左エンジン", (-20, -4, 10.5), (3, 12, 1), rot=(90, 0, 0), color=CHROME,
              blend=1, roughness=0.3, metallic=0.8),
        plain("Cylinder", "右エンジン", (20, -4, 10.5), (3, 12, 1), rot=(90, 0, 0), color=CHROME,
              blend=1, roughness=0.3, metallic=0.8),
        # landing gear
        plain("Cylinder", "左脚", (-12, -2, 11), (1, 6, 1), color=CHROME, blend=0.5, roughness=0.3),
        plain("Cylinder", "右脚", (12, -2, 11), (1, 6, 1), color=CHROME, blend=0.5, roughness=0.3),
        plain("Cylinder", "前脚", (0, -30, 8), (0.9, 4, 1), color=CHROME, blend=0.5, roughness=0.3),
    ]
    s += wheel("左", -12, -2, R=3.0, t=1.5, hub_r=1.2, hub_len=2.6)
    s += wheel("右", 12, -2, R=3.0, t=1.5, hub_r=1.2, hub_len=2.6)
    s += wheel("前", 0, -30, R=3.0, t=1.5, hub_r=1.2, hub_len=2.6)
    return s


def make_heli():
    red = (0.85, 0.14, 0.10)
    s = [
        paint("Sphere", "機体", (0, -2, 20), (13, 1, 1), red, blend=3, scale=(1, 1.25, 1)),
        win("キャノピー", (0, -12, 21), (10, 1, 1), blend=1.0, scale=(1, 0.9, 0.95)),
        paint("Capsule", "尾部", (0, 28, 20), (3, 28, 1), red, blend=3, rot=(90, 0, 0)),
        paint("Box", "尾翼", (0, 42, 28), (1.5, 10, 12), red, blend=1, round=0.5),
        plain("Cylinder", "尾ローター", (2.5, 44, 22), (6, 1, 1), rot=(0, 90, 0), color=DARK,
              blend=0.5),
        plain("Cylinder", "マスト", (0, -1, 37), (1.5, 8, 1), color=DARK, blend=1),
        plain("Cylinder", "ローターハブ", (0, -1, 42), (2.5, 3, 1), color=DARK, blend=0.5),
        plain("Box", "メインローター1", (0, -1, 43.5), (72, 3, 1.2), color=DARK, blend=0.5, round=0.5),
        plain("Box", "メインローター2", (0, -1, 43.5), (3, 40, 1.2), color=DARK, blend=0.5, round=0.5),
        plain("Capsule", "左スキッド", (-9, -2, 1.5), (1.5, 44, 1), rot=(90, 0, 0), color=DARK,
              blend=0.5),
        plain("Capsule", "右スキッド", (9, -2, 1.5), (1.5, 44, 1), rot=(90, 0, 0), color=DARK,
              blend=0.5),
    ]
    for side, x in (("左", -9), ("右", 9)):
        for end, y in (("前", -8), ("後", 6)):
            s.append(plain("Cylinder", side + end + "の支柱", (x, y, 7.5), (1, 9, 1), color=DARK, blend=1.0))
    return s


def make_boat():
    hull_top = [[0, -46], [17, -16], [18, 36], [-18, 36], [-17, -16]]
    hull_bottom = [[0, -42], [8, -16], [9, 34], [-9, 34], [-8, -16]]
    s = [
        # hull: a loft from the narrow keel (z 0) up to the wide deck (z 12)
        shape("Loft", "船体", pos=(0, 0, 7), size=(14, 1, 1), outline=hull_bottom, outline2=hull_top,
              color=(0.86, 0.18, 0.14), roughness=0.25, coat=1.0, blend=0),
        paint("Box", "操縦室", (0, 6, 21), (16, 20, 12), WHITE, blend=2, round=2),
        win("風防", (0, -4.6, 22), (14, 1.5, 7), round=1),
        win("左窓", (-8.6, 6, 22), (1.5, 14, 6), round=1),
        win("右窓", (8.6, 6, 22), (1.5, 14, 6), round=1),
        plain("Cylinder", "マスト", (0, 8, 40), (0.9, 26, 1), color=CHROME, blend=0.5, roughness=0.3),
        paint("Box", "旗", (5, 8, 49), (10, 0.5, 5), (1.0, 0.8, 0.1), blend=0.3, roughness=0.6,
              coat=0.0),
    ]
    return s


def make_yacht():
    hull_top = [[0, -40], [13, -14], [14, 30], [-14, 30], [-13, -14]]
    hull_bottom = [[0, -36], [6, -14], [7, 28], [-7, 28], [-6, -14]]
    s = [
        shape("Loft", "船体", pos=(0, 0, 4), size=(8, 1, 1), outline=hull_bottom, outline2=hull_top,
              color=(0.96, 0.96, 0.94), roughness=0.25, coat=1.0, blend=0),
        paint("Box", "キャビン", (0, -4, 11), (12, 14, 6), WHITE, blend=1.5, round=1.5),
        win("左窓", (-6.2, -4, 11), (1, 6, 3), round=0.5),
        win("右窓", (6.2, -4, 11), (1, 6, 3), round=0.5),
        plain("Cylinder", "マスト", (0, 4, 32), (0.9, 48, 1), color=CHROME, blend=0.5, roughness=0.3),
        plain("Cylinder", "ブーム", (0, 15, 11), (0.7, 22, 1), rot=(90, 0, 0), color=CHROME, blend=0.5,
              roughness=0.3),
        paint("Box", "メインセール", (0, 15, 30), (1.2, 22, 38), (0.98, 0.98, 0.95), blend=0.5,
              roughness=0.9, coat=0.0, round=2),
        paint("Box", "ジブセール", (0, -14, 24), (1.0, 24, 30), (0.98, 0.98, 0.95), blend=0.5,
              roughness=0.9, coat=0.0, round=2),
        shape("Box", "青いライン", pos=(0, 0, 9), size=(30, 60, 1.6), op="Paint",
              color=(0.1, 0.35, 0.85)),
    ]
    return s


def make_rocket():
    # Revolve profiles are (radius, height) centred on pos: heights 0..88 -> z -44..44, pos z = 44.
    body = [[0, 0], [8, 0], [10, 6], [10, 62], [9, 66], [0, 66]]
    nose = [[0, 64], [9, 64], [7, 76], [3, 84], [0, 88]]
    centred = lambda pts: [[x, h - 44] for x, h in pts]
    s = [
        shape("Revolve", "機体", pos=(0, 0, 44), outline=centred(body), params=(360, 0, 0, 0),
              color=WHITE, roughness=0.25, coat=1.0, blend=0),
        shape("Revolve", "先端", pos=(0, 0, 44), outline=centred(nose), params=(360, 0, 0, 0),
              color=(0.9, 0.12, 0.1), roughness=0.25, coat=1.0, blend=1.0),
        plain("Cylinder", "ノズル", (0, 0, 3), (5, 6, 1), color=DARK, blend=1.0),
        win("窓1", (0, -9.6, 46), (3.2, 1, 1), blend=0.5),
        win("窓2", (0, -9.6, 34), (2.4, 1, 1), blend=0.5),
    ]
    import math
    for i, a in enumerate((0, 120, 240)):
        r = math.radians(a)
        s.append(paint("Box", "フィン%d" % (i + 1), (11.5 * math.cos(r), 11.5 * math.sin(r), 11),
                       (5, 1.5, 22), (0.9, 0.12, 0.1), blend=1.0, rot=(0, 0, a), round=0.5))
    return s


def make_train():
    green = (0.14, 0.52, 0.34)
    s = [
        paint("Box", "車体", (0, 0, 25), (30, 96, 26), green, blend=3, round=6),
        plain("Box", "先頭の下の板", (0, -48, 15), (26, 4, 6), color=DARK, blend=1),
        win("フロント窓", (0, -48.4, 31), (24, 1, 16), round=2),
        win("左側窓", (-15.4, -4, 31), (1, 62, 10), round=1),
        win("右側窓", (15.4, -4, 31), (1, 62, 10), round=1),
        lamp("左前灯", (-9, -48.5, 19), size=2.5, glow=1.5),
        lamp("右前灯", (9, -48.5, 19), size=2.5, glow=1.5),
        plain("Box", "台車前", (0, -26, 11), (24, 16, 5), color=DARK, blend=1),
        plain("Box", "台車後", (0, 26, 11), (24, 16, 5), color=DARK, blend=1),
        plain("Box", "パンタグラフ台", (0, 10, 40), (10, 8, 4), color=DARK, blend=1),
        plain("Box", "パンタグラフ", (0, 10, 44.5), (16, 1.5, 1.5), color=DARK, blend=0.5),
        shape("Box", "白い帯左", pos=(-15.1, 0, 17), size=(1, 92, 2.5), op="Paint", color=WHITE),
        shape("Box", "白い帯右", pos=(15.1, 0, 17), size=(1, 92, 2.5), op="Paint", color=WHITE),
    ]
    for i, y in enumerate((-32, -20, 20, 32)):
        for side, x in (("左", -12), ("右", 12)):
            s.append(plain("Cylinder", "%s車輪%d" % (side, i + 1), (x, y, 7), (7, 4, 1), rot=(0, 90, 0),
                           color=CHROME, blend=0.5, roughness=0.3, metallic=0.8))
    return s


def make_bike():
    red = (0.85, 0.14, 0.12)
    R, t = 12.0, 2.2
    hub = dict(hub_r=1.8, hub_len=4.0)
    front, rear = -30.0, 28.0
    bb = (0, 0, 15)
    ht = (0, -22, 30)
    st = (0, 10, 32)
    s = []
    s += wheel("前", 0, front, R=R, t=t, **hub)
    s += wheel("後", 0, rear, R=R, t=t, **hub)

    def tube(name, pts, r=1.2):
        return shape("Curve", name, pos=(0, 0, 0), path=[[p[0], p[1], p[2], r] for p in pts],
                     params=(0, 0.7, 0, 1), color=red, roughness=0.3, coat=1.0, blend=0.3)

    s += [
        tube("ダウンチューブ", [ht, bb]),
        tube("トップチューブ", [ht, st]),
        tube("シートチューブ", [st, bb]),
        tube("チェーンステー", [bb, (0, rear, 14.2)], r=1.0),
        tube("シートステー", [st, (0, rear, 14.2)], r=1.0),
        tube("フォーク", [ht, (0, front, 14.2)], r=1.2),
        tube("ハンドルの軸", [ht, (0, -20, 36)], r=1.2),
        tube("サドルの軸", [st, (0, 9, 36)], r=1.0),
        shape("Curve", "ハンドル", pos=(0, 0, 0),
              path=[[-7, -20, 38.5, 0.9], [0, -21, 39.5, 0.9], [7, -20, 38.5, 0.9]],
              params=(0, 0.7, 0, 1), color=DARK, roughness=0.4, metallic=0.6, blend=0.3),
        shape("Box", "サドル", pos=(0, 8, 39), size=(4, 12, 2.5), color=DARK, roughness=0.8,
              blend=0.5, round=1.2),
        shape("Cylinder", "クランク", pos=bb, size=(4, 1.5, 1), rot=(0, 90, 0), color=CHROME,
              roughness=0.3, metallic=0.8, blend=0.5),
    ]
    return s


def make_items():
    specs = [
        ("くるま", ["car", "自動車", "クルマ", "vehicle"], make_car),
        ("スポーツカー", ["sports car", "race car", "スポーツ"], make_sports),
        ("バス", ["bus", "バス", "vehicle"], make_bus),
        ("トラック", ["truck", "lorry", "トラック"], make_truck),
        ("パトカー", ["police car", "patrol", "パトロール"], make_police),
        ("ひこうき", ["airplane", "plane", "飛行機", "aircraft"], make_plane),
        ("ヘリコプター", ["helicopter", "heli", "ヘリ"], make_heli),
        ("ふね", ["boat", "ship", "船"], make_boat),
        ("ヨット", ["yacht", "sailboat", "sail", "帆船"], make_yacht),
        ("ロケット", ["rocket", "spaceship", "ロケット"], make_rocket),
        ("でんしゃ", ["train", "electric train", "電車"], make_train),
        ("じてんしゃ", ["bicycle", "bike", "自転車"], make_bike),
    ]
    items = []
    for name, tags, make in specs:
        items.append(item(name, tags, group(name, make())))
    return items


items = make_items()

if __name__ == "__main__":
    write_category("vehicles.json", "のりもの", items)
    preview(items, outdir="/tmp/lib_preview_veh")
