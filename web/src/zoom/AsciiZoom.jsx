import { useEffect, useRef } from "react";
import {
  BG,
  BUCKETS,
  PALETTES,
  segmentAt,
  zoomFactor,
  eyeParams,
  eyeB,
  orbB,
  orbRadius,
  orbFlare,
  cellHash,
} from "./scenes.js";

// Страница-«знакоместа»: canvas на всё окно, каждая ячейка — символ с цветом
// по яркости. FPS ограничен, при ресайзе сетка пересчитывается.
const spark = { pts: null, loop: -1 };

export default function AsciiZoom({ fps = 30 }) {
  const ref = useRef(null);

  useEffect(() => {
    const cv = ref.current;
    const ctx = cv.getContext("2d");
    const FONT_SIZE = 13;
    const FONT = `${FONT_SIZE}px Consolas, "Courier New", monospace`;
    let raf = 0;
    let grid = null;

    function fit() {
      const dpr = window.devicePixelRatio || 1;
      const w = window.innerWidth;
      const h = window.innerHeight;
      cv.width = Math.max(1, Math.floor(w * dpr));
      cv.height = Math.max(1, Math.floor(h * dpr));
      cv.style.width = `${w}px`;
      cv.style.height = `${h}px`;
      ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
      ctx.font = FONT;
      ctx.textAlign = "center";
      ctx.textBaseline = "middle";
      const cw = Math.max(4, ctx.measureText("M").width);
      const ch = Math.max(8, FONT_SIZE * 1.3);
      grid = { cw, ch, cols: Math.ceil(w / cw), rows: Math.ceil(h / ch), w, h };
      ctx.fillStyle = BG;
      ctx.fillRect(0, 0, w, h);
    }

    fit();
    window.addEventListener("resize", fit);

    const rng = Math.random;
    const t0 = performance.now();
    let last = 0;

    function frame(now) {
      raf = requestAnimationFrame(frame);
      if (now - last < 1000 / fps) return;
      last = now;
      if (!grid) return;
      const t = (now - t0) / 1000;
      const { cw, ch, cols, rows, w, h } = grid;
      const { loop, seg, tIn } = segmentAt(t);
      const z = zoomFactor(seg, tIn, loop);
      const iz = 1 / z;
      const pal = PALETTES[seg.pal];
      const hx = h / 2;
      const cx = w / 2;
      const cy = h / 2;

      const p = seg.scene === "eye" ? eyeParams(t, seg, tIn) : null;
      const R = seg.scene === "orb" ? orbRadius(t) : 0;
      const flare = seg.scene === "orb" ? orbFlare(t) : 0;

      // блики-«кресты» в сцене со сферой
      const overlay = new Map();
      if (seg.scene === "orb") {
        if (spark.loop !== loop || !spark.pts) {
          const pts = [];
          for (let i = 0; i < 26; i++) {
            pts.push([rng() * 3.6 - 1.8, rng() * 2.1 - 1.05, rng() * 6.283]);
          }
          spark.pts = pts;
          spark.loop = loop;
        }
        for (const [su, sv, ph] of spark.pts) {
          const a = 0.5 + 0.5 * Math.sin(t * 2.6 + ph);
          if (a <= 0.25) continue;
          const px = cx + su * z * hx;
          const py = cy + sv * z * hx;
          const c0 = Math.floor(px / cw);
          const r0 = Math.floor(py / ch);
          for (let rr = r0 - 1; rr <= r0 + 1; rr++) {
            for (let cc = c0 - 2; cc <= c0 + 2; cc++) {
              if (rr < 0 || cc < 0 || rr >= rows || cc >= cols) continue;
              const dc = Math.max(
                Math.abs((cc + 0.5) * cw - px) / cw,
                Math.abs((rr + 0.5) * ch - py) / ch,
              );
              if (dc < 1.6) {
                const qv = ((1.6 - dc) / 1.6) * a;
                const k = rr * cols + cc;
                if (qv > (overlay.get(k) ?? 0)) overlay.set(k, qv);
              }
            }
          }
        }
      }

      ctx.fillStyle = BG;
      ctx.fillRect(0, 0, w, h);

      for (let r = 0; r < rows; r++) {
        const v = ((r + 0.5) * ch - cy) / hx / z;
        const vy = ((r + 0.5) * ch - cy) / cy;
        for (let c = 0; c < cols; c++) {
          const u = ((c + 0.5) * cw - cx) / hx / z;
          let b;
          if (seg.scene === "noise") {
            b = rng() * rng() * 0.45;
            if (rng() < 0.015) b = 0.9;
          } else if (seg.scene === "eye") {
            b = eyeB(u, v, p);
          } else {
            b = orbB(u, v, t, R, flare);
            const hs = cellHash(c, r);
            if (hs > 0.986) {
              b = Math.max(b, 0.35 + 0.65 * (0.5 + 0.5 * Math.sin(t * 2.1 + hs * 90)));
            }
          }
          const ov = overlay.get(r * cols + c);
          if (ov !== undefined && ov > b) b = ov;
          const vx = ((c + 0.5) * cw - cx) / cx;
          b *= 1 - 0.22 * (vx * vx + vy * vy); // виньетка

          let chTxt;
          let ci;
          if (b < 0.05) {
            chTxt = rng() < 0.05 ? "." : " ";
            ci = 0;
          } else {
            ci = Math.min(5, Math.floor(b * 5.999));
            const bucket = BUCKETS[Math.min(6, Math.floor(b * 6.999))];
            chTxt = bucket[Math.floor(rng() * bucket.length)];
          }
          if (chTxt === " ") continue;
          ctx.fillStyle = pal[ci];
          ctx.fillText(chTxt, (c + 0.5) * cw, (r + 0.5) * ch);
        }
      }
    }

    raf = requestAnimationFrame(frame);
    return () => {
      cancelAnimationFrame(raf);
      window.removeEventListener("resize", fit);
    };
  }, [fps]);

  return (
    <canvas
      ref={ref}
      style={{ display: "block", position: "fixed", inset: 0, background: BG }}
    />
  );
}
