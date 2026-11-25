# -*- coding: utf-8 -*-
"""ASCII-превью сцен main.py в терминале — для быстрой проверки математики."""

import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import main as m  # noqa: E402


def frame(name, z, t, cols=110, rows=42):
    A = (m.W / m.H)  # экран по горизонтали в единицах сцены
    out = []
    for r in range(rows):
        line = []
        v = ((r + 0.5) / rows * 2.0 - 1.0) / z
        for c in range(cols):
            u = ((c + 0.5) / cols * 2.0 * A - A) / z
            if name == "eye":
                b = m.eye_b(u, v)
            else:
                b = m.orb_b(u, v, t)
                hsh = m.cell_hash(c, r)
                if hsh > 0.986:
                    b = max(b, 0.5)
            bi = int(b * 6.999)
            line.append(" " if bi == 0 else m.BUCKETS[bi][0])
        out.append("".join(line))
    return "\n".join(out)


print("=== EYE z=1 ===")
print(frame("eye", 1.0, 0.0))
print("=== EYE z=3 (влёт в радужку) ===")
print(frame("eye", 3.0, 0.0))
print("=== ORB z=1 ===")
print(frame("orb", 1.0, 1.0))
print("=== ORB z=4 (ядро растёт) ===")
print(frame("orb", 4.0, 1.0))
