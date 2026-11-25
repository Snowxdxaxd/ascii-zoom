// Математика сцен ASCII-зума: глаз (моргание, саккады, зрачок) и сфера.
// Одна и та же логика продублирована в Rust-версии (rust/src/main.rs).

export const BG = "#0b0709";

export const PALETTES = {
  amber: ["#3c160a", "#7a2d10", "#be5a19", "#f5962d", "#ffcd5a", "#fff4dc"],
  red: ["#2d0c18", "#6e142d", "#c82846", "#ff506e", "#ff96af", "#ffe6ee"],
};

export const BUCKETS = [
  " ",
  ".,'`\"",
  ":;-~^il",
  "/\\()[]{}<>=+*",
  "/\\|xzuvwmnrf",
  "XZWM#%8&0QN",
  "@#%&WM08Q",
];

// (сцена, длительность с, палитра, полупериод зума: z = 2**(t/период))
export const SEGMENTS = [
  { scene: "noise", dur: 1.0, pal: "amber", half: 2.7 },
  { scene: "eye", dur: 8.0, pal: "amber", half: 2.7 },
  { scene: "noise", dur: 0.7, pal: "red", half: 2.5 },
  { scene: "orb", dur: 8.2, pal: "red", half: 2.5 },
  { scene: "noise", dur: 0.7, pal: "amber", half: 2.7 },
];
export const PERIOD = SEGMENTS.reduce((s, x) => s + x.dur, 0);

const fract = (x) => x - Math.floor(x);
export const hash01 = (x) => fract(Math.sin(x * 127.1) * 43758.5453);

export function sstep(a, b, x) {
  const t = Math.min(1, Math.max(0, (x - a) / (b - a)));
  return t * t * (3 - 2 * t);
}

export function clamp01(x) {
  return x < 0 ? 0 : x > 1 ? 1 : x;
}

export function cellHash(cx, cy) {
  const s = Math.sin(cx * 127.1 + cy * 311.7) * 43758.5453;
  return fract(s);
}

// Сцена в цикле; variant меняет направление зума у глаза (чётный — влёт,
// нечётный — наоборот, вылет из зрачка обратно к полному глазу).
export function segmentAt(t) {
  const loop = Math.floor(t / PERIOD);
  let tl = t - loop * PERIOD;
  let acc = 0;
  let seg = SEGMENTS[SEGMENTS.length - 1];
  for (const s of SEGMENTS) {
    if (tl < acc + s.dur) {
      seg = s;
      break;
    }
    acc += s.dur;
  }
  return { loop, seg, tIn: tl - acc };
}

export function zoomFactor(seg, tIn, loop) {
  if (seg.scene === "eye" && loop % 2 === 1) {
    return Math.pow(2, (seg.dur - tIn) / seg.half); // вылет обратно
  }
  return Math.pow(2, tIn / seg.half);
}

// Саккады: цель меняется каждые ~1.6 с, глаз прыгает к новой цели ~120 мс.
function gazeTarget(k) {
  return [
    (hash01(k * 12.9898 + 4.1) - 0.5) * 0.84,
    (hash01(k * 78.233 + 9.2) - 0.5) * 0.14,
  ];
}

export function gaze(t) {
  const k = Math.floor(t / 1.6);
  const cur = gazeTarget(k);
  const nxt = gazeTarget(k + 1);
  const f = sstep(0, 0.12, t / 1.6 - k);
  return [cur[0] + (nxt[0] - cur[0]) * f, cur[1] + (nxt[1] - cur[1]) * f];
}

function blinkPulse(dt) {
  // 0.09 закрытие, 0.06 держим, 0.13 открытие
  if (dt < 0 || dt > 0.28) return 0;
  if (dt < 0.09) return dt / 0.09;
  if (dt < 0.15) return 1;
  return 1 - (dt - 0.15) / 0.13;
}

function squintPulse(dt) {
  // прищур до половины
  if (dt < 0 || dt > 0.75) return 0;
  if (dt < 0.15) return 0.5 * (dt / 0.15);
  if (dt < 0.55) return 0.5;
  return 0.5 * (1 - (dt - 0.55) / 0.2);
}

// 0 — открыто, 1 — закрыто. Обычное моргание, двойное и прищур.
export function blinkC(t) {
  let c = 0;
  const k0 = Math.max(0, Math.floor((t - 2.5) / 2.3));
  for (let k = k0; k <= k0 + 2; k++) {
    const start = 1.0 + k * 2.3 + 1.3 * hash01(k * 3.7 + 11.3);
    const r = hash01(k * 9.1 + 5.5);
    if (r < 0.18) {
      c = Math.max(c, blinkPulse(t - start), blinkPulse(t - start - 0.45));
    } else if (r < 0.36) {
      c = Math.max(c, squintPulse(t - start));
    } else {
      c = Math.max(c, blinkPulse(t - start));
    }
  }
  return Math.min(1, c);
}

// Параметры глаза на кадр: взгляд, веки, зрачок (дыхание + расширение перед влётом).
export function eyeParams(t, seg, tIn) {
  const [gx, gy] = gaze(t);
  const dilate = seg.scene === "eye" ? 0.10 * sstep(seg.dur - 2.2, seg.dur - 0.6, tIn) : 0;
  return {
    gx,
    gy,
    blink: blinkC(t),
    pupil: 0.24 + 0.035 * Math.sin(t * 0.7) + dilate,
  };
}

// Яркость глаза. p — параметры из eyeParams.
export function eyeB(u, v, p) {
  const ax = Math.abs(u) / 1.35;
  if (ax >= 1) return 0.03;
  const lidOrg = 0.78 * Math.pow(1 - ax * ax, 0.6);
  const av = Math.abs(v);
  if (av > lidOrg) return 0.03;

  const lid = lidOrg * (1 - p.blink);
  if (av > lid) {
    // веко прикрыло глаз: кожа со складкой
    const k = (av - lid) / Math.max(lidOrg - lid, 1e-4);
    const b = 0.30 - 0.13 * k + 0.05 * Math.sin(av * 42);
    return Math.max(0.06, b * (1 - p.blink * 0.4));
  }

  const u2 = u - p.gx;
  const v2 = v - p.gy;
  const d = Math.hypot(u2, v2);
  if (d < p.pupil) return 0.04; // зрачок
  if (d < 0.62) {
    // радужка: радиальные волокна + тёмное лимбальное кольцо
    const ang = Math.atan2(v2, u2);
    let b =
      0.52 +
      0.22 * Math.sin(ang * 34 + 2.0 * Math.sin(ang * 6.0)) +
      0.14 * Math.sin(d * 48);
    b += 0.28 * Math.exp(-Math.pow((d - 0.56) / 0.07, 2));
    b -= 0.55 * Math.exp(-Math.pow((d - 0.615) / 0.022, 2));
    b *= 0.5 + 0.5 * sstep(0.24, 0.36, d);
    return clamp01(b);
  }
  let b = 0.74 - 0.16 * ax * ax; // склера
  const vein = Math.sin(u * 21 + 3 * Math.sin(v * 27)) * Math.sin(v * 15 + 1.7);
  if (vein > 0.82) b *= 0.42;
  b *= 0.72 + 0.28 * (1 - av / lidOrg); // тень у краёв век
  const hx = -0.30 + p.gx * 0.4; // блик едет вместе со взглядом
  const hy = -0.26 + p.gy * 0.4;
  const hd = Math.hypot(u - hx, v - hy);
  if (hd < 0.07) return 1;
  if (hd < 0.13) b = Math.max(b, 0.88);
  return clamp01(b);
}

// Красная сфера: пульс радиуса, дрейф полос, вспышки (flare 0..1).
export function orbB(u, v, t, R, flare) {
  const d = Math.hypot(u, v);
  let b = (0.5 + 0.12 * flare) * Math.exp(-Math.pow((d - R) / (R * 0.55), 2));
  if (d < R) {
    const stripes =
      0.55 + 0.45 * Math.sin(v * 36 + 2.5 * Math.sin(u * 9 + t * 0.5) + t * 1.7);
    const core = Math.exp(-Math.pow(d / (R * 0.42), 2));
    b = Math.max(b, (0.22 + 0.78 * core) * (0.40 + 0.60 * stripes));
    const rim = Math.exp(-Math.pow((d - R * 0.9) / (R * 0.14), 2));
    b = Math.max(b, (0.85 + 0.3 * flare) * rim);
    if (d < 0.17) b *= Math.pow(d / 0.17, 1.4); // тёмный центр
    b = Math.max(b, 1.15 * Math.exp(-Math.pow(d / 0.05, 2))); // горячее ядро
  }
  return Math.min(1, b * (1 + 0.5 * flare));
}

export function orbRadius(t) {
  return 0.55 * (1 + 0.07 * Math.sin(t * 1.2));
}

export function orbFlare(t) {
  // редкие вспышки раз в ~3.3 с
  const k = Math.floor((t - 1.7) / 3.3);
  let m = 0;
  for (let i = k; i <= k + 1; i++) {
    const s = 1.7 + i * 3.3 + 1.7 * hash01(i * 5.3 + 2.2);
    m = Math.max(m, Math.exp(-Math.pow(t - s, 2) / 0.012));
  }
  return m;
}
