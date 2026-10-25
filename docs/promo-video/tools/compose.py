#!/usr/bin/env python3
"""Melt 3D intro: white, colourful, fast cuts. Real screen recordings in clips/<id>.mp4 (or .gif / png folder).
Voice: audio/<id>.wav (VOICEVOX Zundamon). Without voice, a cut lasts len(text)/7 s (fast speech).
Output: out/melt3d_intro.mp4 + out/melt3d_intro.srt
"""
import json, os, subprocess, glob, math, random
import numpy as np
import cv2
from PIL import Image, ImageDraw, ImageFont

W, H, FPS = 1280, 720, 30
HERE = os.path.dirname(os.path.abspath(__file__))
FONT = "/usr/share/fonts/opentype/noto/NotoSansCJK-Black.ttc"
if not os.path.exists(FONT):
    FONT = "/usr/share/fonts/opentype/noto/NotoSansCJK-Bold.ttc"
MONO = "/usr/share/fonts/truetype/dejavu/DejaVuSansMono-Bold.ttf"
def F(sz, idx=0): return ImageFont.truetype(FONT, max(1, int(sz)), index=idx)
F_TAG, F_TITLE, F_CAP, F_SM, F_BIG, F_MID = F(30), F(68), F(40), F(24), F(190), F(64)
F_MONO = ImageFont.truetype(MONO, 32) if os.path.exists(MONO) else F(30)

COL = {"pink": (255, 92, 138), "yellow": (255, 196, 40), "blue": (50, 160, 255), "green": (40, 205, 140), "purple": (140, 96, 246)}
INK = (36, 34, 54)
cues = json.load(open(os.path.join(HERE, "narration.json"), encoding="utf-8"))


def ease(t): t = max(0.0, min(1.0, t)); return 1 - (1 - t) ** 3
def pop(t):  # overshoot
    t = max(0.0, min(1.0, t)); c = 1.70158 * 1.4
    return 1 + (c + 1) * (t - 1) ** 3 + c * (t - 1) ** 2


def text_c(d, xy, s, font, fill, stroke=0, sc=None, anchor="mm"):
    d.text(xy, s, font=font, fill=fill, anchor=anchor, stroke_width=stroke, stroke_fill=sc)


def background(t, col):
    img = Image.new("RGB", (W, H), (255, 252, 246))
    d = ImageDraw.Draw(img, "RGBA")
    d.rectangle([0, 0, W, 18], fill=col + (255,))
    return img


def open_clip(cid, ss=0.0, maxsec=9.0):
    for ext in (".mp4", ".gif"):
        p = os.path.join(HERE, "clips", cid + ext)
        if os.path.exists(p):
            cap = cv2.VideoCapture(p)
            if ext == '.mp4' and ss: cap.set(cv2.CAP_PROP_POS_FRAMES, int(ss * 30))
            fr = []
            while len(fr) < int(maxsec * 30):
                ok, f = cap.read()
                if not ok: break
                fr.append(cv2.cvtColor(cv2.resize(f,(960,int(960*f.shape[0]/f.shape[1])),interpolation=cv2.INTER_AREA), cv2.COLOR_BGR2RGB))
            cap.release()
            if fr:
                if ext == '.gif': fr = [fr[min(len(fr) - 1, int(i * 12 / 30))] for i in range(int(len(fr) * 30 / 12) * 3)]
                return fr
    pngs = sorted(glob.glob(os.path.join(HERE, "clips", cid, "*.png")))
    return [np.array(Image.open(p).convert("RGB")) for p in pngs]


def screen_frame(frames, t, w, h, crop=None, crop2=None, dur=1.0):
    if not frames: return None
    f = frames[int(t * FPS) % len(frames)] if len(frames) > 1 else frames[0]
    fh, fw = f.shape[:2]; k = fw / 1280.0
    if crop:
        c = list(crop)
        if crop2:
            u = ease((t - 0.3) / max(dur * 0.7, 0.5)); c = [a + (b - a) * u for a, b in zip(crop, crop2)]
        x0, y0, x1, y1 = [int(v * k) for v in c]
        f = f[max(0, y0):y1, max(0, x0):x1]
    im = Image.fromarray(f)
    s = max(w / im.width, h / im.height)
    im = im.resize((int(im.width * s), int(im.height * s)), Image.LANCZOS)
    x0, y0 = (im.width - w) // 2, (im.height - h) // 2
    return im.crop((x0, y0, x0 + w, y0 + h))


def draw_screen(img, d, frames, t, col, dur, cue=None):
    cue = cue or {}
    p = pop(t / 0.45)
    BW, BH, BX, BY = (660, 520, 590, 130) if cue.get('split') else (896, 560, 192, 120)
    w, h = int(BW * p), int(BH * p)
    if w < 10: return
    x, y = BX + (BW - w) // 2, BY + (BH - h) // 2
    zoom = 1.0 if cue.get('crop') else 1.0 + 0.05 * (t / max(dur, 0.1))  # slow push-in
    d.rounded_rectangle([x + 14, y + 14, x + w + 14, y + h + 14], 28, fill=col)
    d.rounded_rectangle([x - 8, y - 8, x + w + 8, y + h + 8], 26, fill=(255, 255, 255), outline=INK, width=5)
    sf = screen_frame(frames, t, w, h, cue.get('crop'), cue.get('crop2'), dur)
    if sf is not None:
        if zoom > 1.001:
            sf = sf.resize((int(w * zoom), int(h * zoom)), Image.BILINEAR)
            a, b = (sf.width - w) // 2, (sf.height - h) // 2
            sf = sf.crop((a, b, a + w, b + h))
        m = Image.new("L", (w, h), 0)
        ImageDraw.Draw(m).rounded_rectangle([0, 0, w, h], 20, fill=255)
        img.paste(sf, (x, y), m)
    else:
        d.text((W // 2, y + h // 2), "(ここに実際の操作画面)", font=F_CAP, fill=(150, 150, 170), anchor="mm")


def ai_panel(img, d, t, dur, cue, col):
    lines = cue["split"]
    d.rounded_rectangle([40 + 8, 130 + 8, 570 + 8, 650 + 8], 28, fill=col)
    d.rounded_rectangle([40, 130, 570, 650], 28, fill=(30, 28, 46), outline=INK, width=5)
    total = sum(len(l) for l in lines); n = int(total * min(1.0, t / (dur * 0.8)))
    y = 160
    for l in lines:
        shown = l[:max(0, n)]; n -= len(l)
        if shown:
            fnt = F_MONO if all(ord(c) < 128 for c in l) else F(26)
            c = (255, 214, 102) if l.startswith("$") else ((130, 230, 190) if l.startswith("{") else (255, 255, 255))
            d.text((64, y), shown, font=F(24) if fnt is F_MONO else fnt, fill=c)
        y += 44
        if n <= 0: break


def card_hook(img, d, t, dur, col):
    n = int(772 * ease(t / (dur * 0.55)))
    s = f"{n}"
    sc = 1 + 0.06 * math.sin(t * 9) if t > dur * 0.55 else 1
    big = F(int(230 * sc))
    d.text((W // 2 - 60, 290), s, font=big, fill=col, anchor="mm", stroke_width=10, stroke_fill=INK)
    d.text((W // 2 + d.textlength(s, font=big) // 2 - 20, 360), "回", font=F_MID, fill=INK, anchor="lm")
    d.text((W // 2, 450), "10/15 → 10/25 の 更新回数", font=F_CAP, fill=INK, anchor="mm")


def blob(k, shift, w=620, h=170):
    y, x = np.mgrid[0:h, 0:w].astype(np.float32)
    r = 52
    a = np.hypot(x - (w / 2 - shift), y - h / 2) - r
    b = np.hypot(x - (w / 2 + shift), y - h / 2) - r
    hh = np.clip(0.5 + 0.5 * (b - a) / max(k, 1e-3), 0, 1)
    dd = b + (a - b) * hh - k * hh * (1 - hh)
    ins = np.clip(0.5 - dd, 0, 1)[..., None]
    c1, c2 = np.array(COL["green"], np.float32), np.array(COL["pink"], np.float32)
    col = c1 * hh[..., None] + c2 * (1 - hh[..., None])
    return Image.fromarray((col * ins + 255 * (1 - ins)).astype(np.uint8))


CODE = ["let h = (0.5 + 0.5 * (b - a) / k)", "        .clamp(0.0, 1.0);", "let d = b + (a - b) * h", "      - k * h * (1.0 - h);"]


def card_code(img, d, t, dur, col):
    d.rounded_rectangle([60, 118, 700, 508], 26, fill=(34, 32, 58), outline=INK, width=5)
    d.text((90, 135), "sdf.rs   smin()", font=F_SM, fill=(160, 160, 200))
    n = int(sum(map(len, CODE)) * ease(t / (dur * 0.5))); y = 185
    for i, ln in enumerate(CODE):
        d.text((90, y), ln[:max(0, n)], font=F_MONO, fill=[COL["green"], COL["yellow"], COL["blue"], COL["pink"]][i])
        n -= len(ln); y += 56
    k = 6 + 46 * (0.5 + 0.5 * math.sin(t * 1.6)); sh = 95 + 45 * math.sin(t * 0.9)
    img.paste(blob(k, sh, 520, 380), (720, 128))
    d.text((980, 530), "k = とけぐあい", font=F_CAP, fill=INK, anchor="mm")
    d.text((380, 535), "d = とけた距離 / h = 色の混ぜ具合", font=F_SM, fill=INK, anchor="mm")


def card_stats(img, d, t, dur, col):
    items = [(591504, "行の Rust", "pink"), (7264, "本のテスト", "green"), (3557, "行の GPU シェーダ", "blue")]
    for n, (v, lab, c) in enumerate(items):
        p = pop((t - n * 0.35) / 0.45)
        if p <= 0: continue
        x = 60 + n * 400; w = int(360 * p); h = int(300 * p)
        d.rounded_rectangle([x + 10, 140 + 10, x + w + 10, 140 + h + 10], 28, fill=COL[c])
        d.rounded_rectangle([x, 140, x + w, 140 + h], 28, fill=(255, 255, 255), outline=INK, width=5)
        val = int(v * ease((t - n * 0.35) / 1.6))
        d.text((x + w // 2, 140 + h // 2 - 25), f"{val:,}", font=F(int(58 * p)), fill=INK, anchor="mm")
        d.text((x + w // 2, 140 + h // 2 + 55), lab, font=F(int(30 * p)), fill=INK, anchor="mm")
    x = 70
    for i, c in enumerate(["Rust", "wgpu", "egui", "WGSL", "CI"]):
        p = pop((t - 1.2 - i * 0.12) / 0.35)
        if p <= 0: continue
        w = d.textlength(c, font=F_CAP) + 50
        d.rounded_rectangle([x, 470, x + w, 540], 35, fill=list(COL.values())[i], outline=INK, width=4)
        d.text((x + w / 2, 505), c, font=F_CAP, fill=INK, anchor="mm"); x += w + 18


FIXES = [("#248", "やり直し(Ctrl+Shift+Z) が 別の操作に", "pink"), ("#232", "作業中に閉じると 消えた", "yellow"),
         ("#249", "結合を戻すのに 何回も必要だった", "green"), ("#255", "小さい模型の座標が ズレた", "blue"),
         ("#156", "裏返った面が 見つからなかった", "purple")]


def card_fixes(img, d, t, dur, col):
    per = (dur - 0.8) / len(FIXES)
    for n, (no, bug, c) in enumerate(FIXES):
        p = pop((t - 0.3 - n * per) / 0.4)
        if p <= 0: continue
        y = 118 + n * 90; off = int((1 - min(p, 1)) * 400)
        d.rounded_rectangle([70 + off + 8, y + 8, W - 70 + off + 8, y + 78], 26, fill=COL[c])
        d.rounded_rectangle([70 + off, y, W - 70 + off, y + 70], 26, fill=(255, 255, 255), outline=INK, width=4)
        d.text((100 + off, y + 35), no, font=F_CAP, fill=COL[c], anchor="lm", stroke_width=2, stroke_fill=INK)
        d.text((220 + off, y + 35), bug, font=F_CAP, fill=INK, anchor="lm")
        if p > 0.95:
            d.ellipse([W - 150 + off, y + 10, W - 98 + off, y + 58], fill=COL["green"], outline=INK, width=4)
            d.line([(W - 138 + off, y + 35), (W - 128 + off, y + 46), (W - 110 + off, y + 22)], fill=(255, 255, 255), width=6)

CAN = [("とける合成", "pink"), ("足す・削る・ぬる", "yellow"), ("スケッチ→押し出し", "blue"),
       ("回転・ロフト・スイープ", "green"), ("SolidCraft 寸法", "purple"), ("粘土アニメ", "pink"),
       ("GIF 書き出し", "yellow"), ("STL・3MF・OBJ", "blue"), ("AI 連携 (MCP)", "green")]


def card_can(img, d, t, dur, col):
    for n, (name, c) in enumerate(CAN):
        p = pop((t - 0.25 - n * 0.28) / 0.35)
        if p <= 0: continue
        cx, cy = 110 + (n % 3) * 380, 130 + (n // 3) * 180
        sc = min(p, 1.0)
        w, h = int(350 * sc), int(150 * sc)
        d.rounded_rectangle([cx + 8, cy + 8, cx + w + 8, cy + h + 8], 30, fill=COL[c])
        d.rounded_rectangle([cx, cy, cx + w, cy + h], 30, fill=(255, 255, 255), outline=INK, width=5)
        if sc > 0.9:
            fs = 34
            while fs > 18 and d.textlength(name, font=F(fs)) > 320: fs -= 2
            d.text((cx + w // 2, cy + h // 2), name, font=F(fs), fill=INK, anchor="mm")


def card_end(img, d, t, dur, col):
    p = pop(t / 0.5)
    d.rounded_rectangle([140, 118, W - 140, 520], 40, fill=(255, 255, 255), outline=INK, width=6)
    d.text((W // 2, 210), "無料ダウンロード", font=F(int(80 * p)), fill=INK, anchor="mm")
    d.text((W // 2, 310), "github.com/07egta/melt3D", font=F_MID, fill=COL["blue"], anchor="mm")
    for i, os_ in enumerate(["Windows", "Mac", "Linux"]):
        q = pop((t - 0.5 - i * 0.15) / 0.35)
        if q <= 0: continue
        x = 330 + i * 230
        d.rounded_rectangle([x - 100, 390, x + 100, 460], 35, fill=list(COL.values())[i + 1], outline=INK, width=4)
        d.text((x, 425), os_, font=F_CAP, fill=INK, anchor="mm")
    d.text((W // 2, 545), "VOICEVOX:ずんだもん   制作：江口太一朗 + AI(Claude)", font=F_SM, fill=INK, anchor="mm")


CARDS = {"hook": card_hook, "code": card_code, "stats": card_stats, "fixes": card_fixes, "can": card_can, "end": card_end}


def wrap(text, width_px, font, d):
    lines, cur = [], ""
    for ch in text:
        if d.textlength(cur + ch, font=font) > width_px and cur and ch not in "、。！？」）)・":
            lines.append(cur); cur = ch
        else: cur += ch
    return lines + ([cur] if cur else [])


def frame(cue, clip, i, total):
    t, dur = i / FPS, total / FPS
    col = COL[cue["color"]]
    img = background(t, col); d = ImageDraw.Draw(img)
    kind = cue.get("kind", "clip")
    p = pop(t / 0.35)
    if kind not in ("hook", "end"):
        tw = d.textlength(cue["title"], font=F(52))
        d.rounded_rectangle([40 + 6, 40 + 6, 40 + tw * min(p, 1) + 56, 100 + 6], 30, fill=col)
        d.rounded_rectangle([40, 40, 40 + tw * min(p, 1) + 50, 100], 30, fill=(255, 255, 255), outline=INK, width=4)
        if p > 0.7: d.text((65, 70), cue["title"], font=F(52), fill=INK, anchor="lm")
    if kind in CARDS:
        CARDS[kind](img, d, t, dur, col)
    else:
        draw_screen(img, d, clip, t, col, dur, cue)
        if cue.get('split'): ai_panel(img, d, t, dur, cue, col)
        for (n0, n1, txt) in cue.get('notes', []):
            if n0 <= t < n1:
                q = pop((t - n0) / 0.25); fw_ = d.textlength(txt, font=F(40)) + 60
                d.rounded_rectangle([W // 2 - fw_ / 2 * min(q, 1) + 5, 665 + 5, W // 2 + fw_ / 2 * min(q, 1) + 5, 715 + 5], 25, fill=col)
                d.rounded_rectangle([W // 2 - fw_ / 2 * min(q, 1), 665, W // 2 + fw_ / 2 * min(q, 1), 715], 25, fill=(255, 255, 255), outline=INK, width=4)
                if q > 0.8: d.text((W // 2, 690), txt, font=F(34), fill=INK, anchor="mm")
    # quick colour wipe in
    if t < 0.28:
        x = int(W * (t / 0.28)); d.rectangle([0, 0, x, H], fill=col); d.rectangle([x - 12, 0, x, H], fill=INK)
    elif t < 0.5:
        x = int(W * ((t - 0.28) / 0.22)); d.rectangle([x, 0, W, H], fill=col) if False else None
    return img


def srt_time(t):
    return f"{int(t // 3600):02}:{int(t % 3600 // 60):02}:{int(t % 60):02},{int((t % 1) * 1000):03}"


def main():
    os.makedirs(os.path.join(HERE, "out"), exist_ok=True); os.makedirs(os.path.join(HERE, "work"), exist_ok=True)
    parts, srt, t0 = [], [], 0.0
    for n, cue in enumerate(cues, 1):
        cid = cue["id"]; wav = os.path.join(HERE, "audio", cid + ".wav")
        dur = max(2.6, len(cue["text"]) / 7.0)
        if os.path.exists(wav):
            pr = subprocess.run(["ffprobe", "-v", "error", "-show_entries", "format=duration", "-of", "csv=p=0", wav], capture_output=True, text=True).stdout.strip()
            dur = float(pr) + 0.25
        clip = (open_clip(cue.get("src", cid), cue.get("ss", 0)) if cue.get("kind", "clip") == "clip" else [])
        total = int(dur * FPS); out = os.path.join(HERE, "work", cid + ".mp4")
        cmd = ["ffmpeg", "-y", "-loglevel", "error", "-f", "rawvideo", "-pix_fmt", "rgb24", "-s", f"{W}x{H}", "-r", str(FPS), "-i", "-"]
        cmd += ["-i", wav] if os.path.exists(wav) else ["-f", "lavfi", "-i", "anullsrc=r=48000:cl=stereo"]
        cmd += ["-t", f"{dur:.3f}", "-c:v", "libx264", "-preset", "veryfast", "-crf", "19", "-pix_fmt", "yuv420p", "-c:a", "aac", "-ar", "48000", "-ac", "2", "-shortest", out]
        pr = subprocess.Popen(cmd, stdin=subprocess.PIPE)
        for i in range(total): pr.stdin.write(np.asarray(frame(cue, clip, i, total)).tobytes())
        pr.stdin.close(); pr.wait(); parts.append(out)
        srt.append(f"{n}\n{srt_time(t0)} --> {srt_time(t0 + dur)}\n{cue['text']}\n"); t0 += dur
        print(f"{cid}: {dur:.1f}s clip_frames={len(clip)} voice={'yes' if os.path.exists(wav) else 'no'}", flush=True)
    lst = os.path.join(HERE, "work", "list.txt"); open(lst, "w").write("".join(f"file '{p}'\n" for p in parts))
    final = os.path.join(HERE, "out", "melt3d_intro.mp4")
    subprocess.run(["ffmpeg", "-y", "-loglevel", "error", "-f", "concat", "-safe", "0", "-i", lst, "-c", "copy", final], check=True)
    open(os.path.join(HERE, "out", "melt3d_intro.srt"), "w", encoding="utf-8").write("\n".join(srt))
    print("total", f"{t0:.1f}s ->", final)


if __name__ == "__main__":
    main()
