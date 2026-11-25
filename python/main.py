# -*- coding: utf-8 -*-
"""ASCII-зум: глаз -> зрачок -> красная сфера -> снова глаз.

По мотивам видео @ch20f3rst (TikTok). Окно Tkinter целиком заполнено сеткой
символов; яркость каждой ячейки считается процедурно и превращается в пару
(символ, цвет). Зум экспоненциальный, сцены сменяются через шум.

itemconfig у Tk медленный, поэтому за тик обновляется не вся сетка, а бюджет
ячеек (курсор бежит по кругу) — как в оригинальном коде из видео.

Только стандартная библиотека. Запуск: python main.py (Esc — выход).
"""

import math
import random
import sys
import time
import tkinter as tk

W, H = 960, 620
FONT = ("Consolas", 9)
BG = "#0b0709"
TARGET_MS = 33.0  # целевой бюджет кадра, мс (~30 fps)

PALETTES = {
    "amber": ("#3c160a", "#7a2d10", "#be5a19", "#f5962d", "#ffcd5a", "#fff4dc"),
    "red": ("#2d0c18", "#6e142d", "#c82846", "#ff506e", "#ff96af", "#ffe6ee"),
}

# бакеты символов по плотности: чем ярче ячейка, тем «жирнее» символ
BUCKETS = (
    " ",
    ".,'`\"",
    ":;-~^il",
    "/\\()[]{}<>=+*",
    "/\\|xzuvwmnrf",
    "XZWM#%8&0QN",
    "@#%&WM08Q",
)

# (сцена, длительность с, палитра, полупериод зума: z = 2**(t/период))
SEGMENTS = (
    ("noise", 1.0, "amber", 2.7),
    ("eye", 8.0, "amber", 2.7),
    ("noise", 0.7, "red", 2.5),
    ("orb", 8.2, "red", 2.5),
    ("noise", 0.7, "amber", 2.7),
)
PERIOD = sum(seg[1] for seg in SEGMENTS)


def sstep(a, b, x):
    t = max(0.0, min(1.0, (x - a) / (b - a)))
    return t * t * (3.0 - 2.0 * t)


def clamp01(x):
    return 0.0 if x < 0.0 else (1.0 if x > 1.0 else x)


def cell_hash(cx, cy):
    s = math.sin(cx * 127.1 + cy * 311.7) * 43758.5453
    return s - math.floor(s)


def eye_b(u, v):
    """Яркость глаза в координатах сцены (полуширина 1.35, зрачок 0.24)."""
    ax = abs(u) / 1.35
    if ax >= 1.0:
        return 0.03
    lid = 0.78 * (1.0 - ax * ax) ** 0.6
    if abs(v) > lid:
        return 0.03
    d = math.hypot(u, v)
    if d < 0.24:  # зрачок
        return 0.04
    if d < 0.62:  # радужка: радиальные волокна + тёмное лимбальное кольцо
        ang = math.atan2(v, u)
        b = 0.52 + 0.22 * math.sin(ang * 34.0 + 2.0 * math.sin(ang * 6.0))
        b += 0.14 * math.sin(d * 48.0)
        b += 0.28 * math.exp(-(((d - 0.56) / 0.07) ** 2))
        b -= 0.55 * math.exp(-(((d - 0.615) / 0.022) ** 2))
        b *= 0.5 + 0.5 * sstep(0.24, 0.36, d)
        return clamp01(b)
    b = 0.74 - 0.16 * ax * ax  # склера
    vein = math.sin(u * 21.0 + 3.0 * math.sin(v * 27.0)) * math.sin(v * 15.0 + 1.7)
    if vein > 0.82:
        b *= 0.42
    b *= 0.72 + 0.28 * (1.0 - abs(v) / lid)  # тень у краёв век
    hd = math.hypot(u + 0.30, v + 0.26)  # блик
    if hd < 0.07:
        return 1.0
    if hd < 0.13:
        b = max(b, 0.88)
    return clamp01(b)


def orb_b(u, v, t):
    """Красная полосатая сфера: ореол, тёмный центр, горячее розовое ядро."""
    d = math.hypot(u, v)
    R = 0.55
    b = 0.5 * math.exp(-(((d - R) / (R * 0.55)) ** 2))
    if d < R:
        stripes = 0.55 + 0.45 * math.sin(v * 36.0 + 2.5 * math.sin(u * 9.0) + t * 1.7)
        core = math.exp(-((d / (R * 0.42)) ** 2))
        b = max(b, (0.22 + 0.78 * core) * (0.40 + 0.60 * stripes))
        rim = math.exp(-(((d - R * 0.90) / (R * 0.14)) ** 2))
        b = max(b, 0.85 * rim)
        if d < 0.17:  # тёмный центр, который потом покроет весь экран
            b *= (d / 0.17) ** 1.4
        b = max(b, 1.15 * math.exp(-((d / 0.05) ** 2)))
    return min(1.0, b)


def main(selftest=False):
    root = tk.Tk()
    root.title("/")
    root.resizable(False, False)
    if selftest:
        root.withdraw()
    cv = tk.Canvas(root, width=W, height=H, bg=BG, highlightthickness=0)
    cv.pack()

    probe = cv.create_text(0, 0, text="M", font=FONT, anchor="nw")
    x0, y0, x1, y1 = cv.bbox(probe)
    cv.delete(probe)
    cols = max(1, int(W / max(4, x1 - x0)))
    rows = max(1, int(H / max(6, y1 - y0)))
    cu = W / cols  # размер ячейки, px
    rv = H / rows

    hx = H / 2.0  # нормирующий полуразмер: сцена живёт в [-1..1] по вертикали
    cxp, cyp = W / 2.0, H / 2.0
    n = rows * cols

    # предрасчёт координат сетки (u/v считаются делением на z каждый кадр)
    NX = [(c + 0.5) * cu for c in range(cols)]
    U0 = [(x - cxp) / hx for x in NX]
    VX = [(x - cxp) / cxp for x in NX]
    V0 = [((r + 0.5) * rv - cyp) / hx for r in range(rows)]
    VY = [((r + 0.5) * rv - cyp) / cyp for r in range(rows)]

    items = [cv.create_text(0, 0, text=" ", font=FONT,
                            fill=PALETTES["amber"][0], anchor="center")
             for _ in range(n)]
    for i in range(n):
        r, c = divmod(i, cols)
        cv.coords(items[i], NX[c], (r + 0.5) * rv)

    prev_ch = [" "] * n
    prev_ci = [0] * n
    cursor = {"i": 0}
    st = {"cost": 25.0, "cell": 0.02, "budget": 1500}
    spark = {"pts": (), "loop": -1}

    def segment(tl):
        acc = 0.0
        for name, dur, pal, half in SEGMENTS:
            if tl < acc + dur:
                return name, tl - acc, pal, half
            acc += dur
        return SEGMENTS[-1]

    def render(t):
        loop_idx = int(t // PERIOD)
        name, t_in, pal_name, half = segment(t - loop_idx * PERIOD)
        z = 2.0 ** (t_in / half)
        iz = 1.0 / z
        pal = PALETTES[pal_name]

        # блики-«кресты» в сцене со сферой: заранее считаем их вклад в ячейки
        overlay = {}
        if name == "orb":
            if spark["loop"] != loop_idx:
                rnd = random.Random(100 + loop_idx)
                spark["pts"] = tuple(
                    (rnd.uniform(-1.8, 1.8), rnd.uniform(-1.05, 1.05), rnd.uniform(0.0, 6.28))
                    for _ in range(26))
                spark["loop"] = loop_idx
            for su, sv, ph in spark["pts"]:
                a = 0.5 + 0.5 * math.sin(t * 2.6 + ph)
                if a <= 0.25:
                    continue
                px = cxp + su * z * hx
                py = cyp + sv * z * hx
                r0, c0 = int(py / rv), int(px / cu)
                for rr in range(r0 - 1, r0 + 2):
                    for cc in range(c0 - 2, c0 + 3):
                        if 0 <= rr < rows and 0 <= cc < cols:
                            dc = max(abs(NX[cc] - px) / cu, abs((rr + 0.5) * rv - py) / rv)
                            if dc < 1.6:
                                qv = (1.6 - dc) / 1.6 * a
                                k = rr * cols + cc
                                if qv > overlay.get(k, 0.0):
                                    overlay[k] = qv

        rng = random.random
        choice = random.choice
        itemconfig = cv.itemconfig
        i = cursor["i"]
        budget = st["budget"]
        spent = 0
        scanned = 0
        noise = name == "noise"
        while scanned < n:
            r, c = divmod(i, cols)
            if noise:
                b = rng() * rng() * 0.45
                if rng() < 0.015:
                    b = 0.9
            else:
                u = U0[c] * iz
                v = V0[r] * iz
                if name == "eye":
                    b = eye_b(u, v)
                else:
                    b = orb_b(u, v, t)
                    hsh = cell_hash(c, r)
                    if hsh > 0.986:  # мерцающие точки-звёзды
                        b = max(b, 0.35 + 0.65 * (0.5 + 0.5 * math.sin(t * 2.1 + hsh * 90.0)))
                ov = overlay.get(i)
                if ov is not None and ov > b:
                    b = ov
                vx = VX[c]
                vy = VY[r]
                b *= 1.0 - 0.22 * (vx * vx + vy * vy)  # виньетка

            if b < 0.05:
                ch = "." if rng() < 0.05 else " "
                ci = 0
            else:
                ci = int(b * 5.999)
                ch = choice(BUCKETS[int(b * 6.999)])

            if noise or prev_ch[i] != ch or prev_ci[i] != ci:
                itemconfig(items[i], text=ch, fill=pal[ci])
                prev_ch[i] = ch
                prev_ci[i] = ci
                spent += 1
                if spent >= budget:
                    scanned += 1
                    i = i + 1 if i + 1 < n else 0
                    break
            scanned += 1
            i = i + 1 if i + 1 < n else 0
        cursor["i"] = i
        return spent

    t_start = time.perf_counter()

    def tick():
        t0 = time.perf_counter()
        spent = render(t0 - t_start)
        par = (time.perf_counter() - t0) * 1000.0
        st["cost"] = 0.7 * st["cost"] + 0.3 * par
        if spent:
            st["cell"] = 0.7 * st["cell"] + 0.3 * (par / spent)  # мс на ячейку
        # сколько ячеек можем обновить за TARGET_MS (как в оригинале)
        st["budget"] = int(min(2000, max(400, TARGET_MS / max(st["cell"], 1e-9))))
        fps = min(40.0, max(8.0, 1000.0 / max(st["cost"], 8.0)))
        root.after(max(1, int(1000.0 / fps - par)), tick)

    root.bind("<Escape>", lambda e: root.destroy())
    root.protocol("WM_DELETE_WINDOW", root.destroy)

    if selftest:
        import statistics
        times = []
        for k in range(80):  # 28 секунд покрывают все сцены цикла
            t0 = time.perf_counter()
            spent = render(k * 0.35)
            root.update()
            par = (time.perf_counter() - t0) * 1000.0
            # та же адаптация, что в tick()
            st["cost"] = 0.7 * st["cost"] + 0.3 * par
            if spent:
                st["cell"] = 0.7 * st["cell"] + 0.3 * (par / spent)
            st["budget"] = int(min(2000, max(400, TARGET_MS / max(st["cell"], 1e-9))))
            times.append(par)
        print(f"cells={n} ({cols}x{rows})  avg={statistics.mean(times):.1f}ms  "
              f"min={min(times):.1f}ms  max={max(times):.1f}ms  "
              f"budget={st['budget']}  cell={st['cell']:.3f}ms")
        root.destroy()
        return

    root.after(30, tick)
    root.mainloop()


if __name__ == "__main__":
    main(selftest="--selftest" in sys.argv)
