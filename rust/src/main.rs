// ASCII-зум: глаз -> зрачок -> красная сфера -> снова глаз.
// Rust-порт python-версии (../ascii-zoom/main.py): окно minifb, вся сетка
// символов перерисовывается каждый кадр без всяких бюджетов — CPU держит 60 fps
// с запасом. Глифы Consolas запечены в assets/font.bin (tools/gen_font.py).
//
// Глаз живой: моргает (иногда дважды), щурится, саккады (прыжки взгляда),
// зрачок дышит и расширяется перед влётом. Сфера пульсирует и вспыхивает.
// В нечётных циклах камеры наоборот вылетает из зрачка к полному глазу.

use minifb::{Key, KeyRepeat, Window, WindowOptions};
use std::collections::HashMap;
use std::time::Instant;

const W: usize = 960;
const H: usize = 620;
const FPS: f64 = 60.0;
const BG_RGB: [u8; 3] = [0x0b, 0x07, 0x09];
const BG: u32 = 0xFF0B_0709;

const PALETTE_RGB: [[[u8; 3]; 6]; 2] = [
    // amber
    [
        [0x3c, 0x16, 0x0a],
        [0x7a, 0x2d, 0x10],
        [0xbe, 0x5a, 0x19],
        [0xf5, 0x96, 0x2d],
        [0xff, 0xcd, 0x5a],
        [0xff, 0xf4, 0xdc],
    ],
    // red
    [
        [0x2d, 0x0c, 0x18],
        [0x6e, 0x14, 0x2d],
        [0xc8, 0x28, 0x46],
        [0xff, 0x50, 0x6e],
        [0xff, 0x96, 0xaf],
        [0xff, 0xe6, 0xee],
    ],
];

// порядок символов должен совпадать с tools/gen_font.py
const CHARS: &[u8] = b" .,'`:;-~^il/\\()[]{}<>=+*|xzuvwmnrfXZWM#%8&0QN@";
const BUCKETS: [&str; 7] = [
    " ",
    ".,'`\"",
    ":;-~^il",
    "/\\()[]{}<>=+*",
    "/\\|xzuvwmnrf",
    "XZWM#%8&0QN",
    "@#%&WM08Q",
];

#[derive(Clone, Copy, PartialEq)]
enum Scene {
    Noise,
    Eye,
    Orb,
}

struct Seg {
    scene: Scene,
    dur: f64,
    pal: usize,
    half: f64,
}

const SEGMENTS: [Seg; 5] = [
    Seg { scene: Scene::Noise, dur: 1.0, pal: 0, half: 2.7 },
    Seg { scene: Scene::Eye, dur: 8.0, pal: 0, half: 2.7 },
    Seg { scene: Scene::Noise, dur: 0.7, pal: 1, half: 2.5 },
    Seg { scene: Scene::Orb, dur: 8.2, pal: 1, half: 2.5 },
    Seg { scene: Scene::Noise, dur: 0.7, pal: 0, half: 2.7 },
];
const PERIOD: f64 = 18.6;

const FONT_BIN: &[u8] = include_bytes!("../assets/font.bin");

fn sstep(a: f64, b: f64, x: f64) -> f64 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn cell_hash(cx: usize, cy: usize) -> f64 {
    let s = (cx as f64 * 127.1 + cy as f64 * 311.7).sin() * 43758.5453;
    s - s.floor()
}

fn hash01(x: f64) -> f64 {
    let s = (x * 127.1).sin() * 43758.5453;
    s - s.floor()
}

// Саккады: цель меняется каждые ~1.6 с, глаз прыгает к новой цели ~120 мс.
fn gaze_target(k: i64) -> (f64, f64) {
    (
        (hash01(k as f64 * 12.9898 + 4.1) - 0.5) * 0.84,
        (hash01(k as f64 * 78.233 + 9.2) - 0.5) * 0.14,
    )
}

fn gaze(t: f64) -> (f64, f64) {
    let k = (t / 1.6).floor() as i64;
    let cur = gaze_target(k);
    let nxt = gaze_target(k + 1);
    let f = sstep(0.0, 0.12, t / 1.6 - k as f64);
    (
        cur.0 + (nxt.0 - cur.0) * f,
        cur.1 + (nxt.1 - cur.1) * f,
    )
}

fn blink_pulse(dt: f64) -> f64 {
    // 0.09 закрытие, 0.06 держим, 0.13 открытие
    if !(0.0..=0.28).contains(&dt) {
        return 0.0;
    }
    if dt < 0.09 {
        return dt / 0.09;
    }
    if dt < 0.15 {
        return 1.0;
    }
    1.0 - (dt - 0.15) / 0.13
}

fn squint_pulse(dt: f64) -> f64 {
    // прищур до половины
    if !(0.0..=0.75).contains(&dt) {
        return 0.0;
    }
    if dt < 0.15 {
        return 0.5 * (dt / 0.15);
    }
    if dt < 0.55 {
        return 0.5;
    }
    0.5 * (1.0 - (dt - 0.55) / 0.2)
}

// 0 — открыто, 1 — закрыто. Обычное моргание, двойное и прищур.
fn blink_c(t: f64) -> f64 {
    let mut c = 0.0f64;
    let k0 = (((t - 2.5) / 2.3).floor() as i64).max(0);
    for k in k0..=(k0 + 2) {
        let start = 1.0 + k as f64 * 2.3 + 1.3 * hash01(k as f64 * 3.7 + 11.3);
        let r = hash01(k as f64 * 9.1 + 5.5);
        let v = if r < 0.18 {
            blink_pulse(t - start).max(blink_pulse(t - start - 0.45))
        } else if r < 0.36 {
            squint_pulse(t - start)
        } else {
            blink_pulse(t - start)
        };
        c = c.max(v);
    }
    c.min(1.0)
}

#[derive(Clone, Copy)]
struct EyeP {
    gx: f64,
    gy: f64,
    blink: f64,
    pupil: f64,
}

// Параметры глаза на кадр: взгляд, веки, зрачок (дыхание + расширение перед влётом).
fn eye_params(t: f64, scene: Scene, dur: f64, t_in: f64) -> EyeP {
    let (gx, gy) = gaze(t);
    let dilate = if scene == Scene::Eye {
        0.10 * sstep(dur - 2.2, dur - 0.6, t_in)
    } else {
        0.0
    };
    EyeP {
        gx,
        gy,
        blink: blink_c(t),
        pupil: 0.24 + 0.035 * (t * 0.7).sin() + dilate,
    }
}

fn eye_b(u: f64, v: f64, p: &EyeP) -> f64 {
    let ax = u.abs() / 1.35;
    if ax >= 1.0 {
        return 0.03;
    }
    let lid_org = 0.78 * (1.0 - ax * ax).powf(0.6);
    let av = v.abs();
    if av > lid_org {
        return 0.03;
    }

    let lid = lid_org * (1.0 - p.blink);
    if av > lid {
        // веко прикрыло глаз: кожа со складкой
        let k = (av - lid) / (lid_org - lid).max(1e-4);
        let b = 0.30 - 0.13 * k + 0.05 * (av * 42.0).sin();
        return (b * (1.0 - p.blink * 0.4)).max(0.06);
    }

    let u2 = u - p.gx;
    let v2 = v - p.gy;
    let d = (u2 * u2 + v2 * v2).sqrt();
    if d < p.pupil {
        // зрачок
        return 0.04;
    }
    if d < 0.62 {
        // радужка: радиальные волокна + тёмное лимбальное кольцо
        let ang = v2.atan2(u2);
        let mut b = 0.52 + 0.22 * (ang * 34.0 + 2.0 * (ang * 6.0).sin()).sin();
        b += 0.14 * (d * 48.0).sin();
        b += 0.28 * (-((d - 0.56) / 0.07).powi(2)).exp();
        b -= 0.55 * (-((d - 0.615) / 0.022).powi(2)).exp();
        b *= 0.5 + 0.5 * sstep(0.24, 0.36, d);
        return b.clamp(0.0, 1.0);
    }
    let mut b = 0.74 - 0.16 * ax * ax; // склера
    let vein = (u * 21.0 + 3.0 * (v * 27.0).sin()).sin() * (v * 15.0 + 1.7).sin();
    if vein > 0.82 {
        b *= 0.42;
    }
    b *= 0.72 + 0.28 * (1.0 - av / lid_org); // тень у краёв век
    let hx = -0.30 + p.gx * 0.4; // блик едет вместе со взглядом
    let hy = -0.26 + p.gy * 0.4;
    let hd = ((u - hx).powi(2) + (v - hy).powi(2)).sqrt();
    if hd < 0.07 {
        return 1.0;
    }
    if hd < 0.13 {
        b = b.max(0.88);
    }
    b.clamp(0.0, 1.0)
}

fn orb_b(u: f64, v: f64, t: f64, rr: f64, flare: f64) -> f64 {
    // Красная сфера: пульс радиуса, дрейф полос, вспышки (flare 0..1).
    let d = (u * u + v * v).sqrt();
    let mut b = (0.5 + 0.12 * flare) * (-(((d - rr) / (rr * 0.55)).powi(2))).exp();
    if d < rr {
        let stripes =
            0.55 + 0.45 * (v * 36.0 + 2.5 * (u * 9.0 + t * 0.5).sin() + t * 1.7).sin();
        let core = (-(d / (rr * 0.42)).powi(2)).exp();
        b = b.max((0.22 + 0.78 * core) * (0.40 + 0.60 * stripes));
        let rim = (-(((d - rr * 0.90) / (rr * 0.14)).powi(2))).exp();
        b = b.max((0.85 + 0.3 * flare) * rim);
        if d < 0.17 {
            // тёмный центр, который потом покроет весь экран
            b *= (d / 0.17).powf(1.4);
        }
        b = b.max(1.15 * (-(d / 0.05).powi(2)).exp()); // горячее ядро
    }
    (b * (1.0 + 0.5 * flare)).min(1.0)
}

fn orb_radius(t: f64) -> f64 {
    0.55 * (1.0 + 0.07 * (t * 1.2).sin())
}

fn orb_flare(t: f64) -> f64 {
    // редкие вспышки раз в ~3.3 с
    let k = ((t - 1.7) / 3.3).floor() as i64;
    let mut m = 0.0f64;
    for i in k..=(k + 1) {
        let s = 1.7 + i as f64 * 3.3 + 1.7 * hash01(i as f64 * 5.3 + 2.2);
        m = m.max(-((t - s).powi(2) / 0.012).exp());
    }
    m
}

struct Rng(u64);

impl Rng {
    fn f(&mut self) -> f64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 >> 11) as f64 / (1u64 << 53) as f64
    }
    fn below(&mut self, n: usize) -> usize {
        (self.f() * n as f64) as usize % n.max(1)
    }
}

struct Lcg(u64);

impl Lcg {
    fn f(&mut self) -> f64 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        (self.0 >> 11) as f64 / (1u64 << 53) as f64
    }
}

fn segment(tl: f64) -> (Scene, f64, usize, f64, f64) {
    let mut acc = 0.0;
    for seg in SEGMENTS.iter() {
        if tl < acc + seg.dur {
            return (seg.scene, tl - acc, seg.pal, seg.half, seg.dur);
        }
        acc += seg.dur;
    }
    let s = &SEGMENTS[SEGMENTS.len() - 1];
    (s.scene, s.dur, s.pal, s.half, s.dur)
}

fn blend(bg: u8, fg: u8, cov: u32) -> u32 {
    (bg as u32 + (fg as u32 - bg as u32) * cov / 255) as u32
}

struct Glyphs {
    cw: usize,
    ch: usize,
    cells: Vec<u32>, // [(char*2 + pal)*6 + color] * (cw*ch)
}

impl Glyphs {
    fn load() -> Glyphs {
        assert!(
            FONT_BIN.len() >= 3,
            "assets/font.bin не найден — запусти tools/gen_font.py"
        );
        let cw = FONT_BIN[0] as usize;
        let ch = FONT_BIN[1] as usize;
        let nchars = FONT_BIN[2] as usize;
        let cell_len = cw * ch;
        assert_eq!(
            FONT_BIN.len(),
            3 + nchars * cell_len,
            "font.bin не подходит под CHARS — перезапусти tools/gen_font.py"
        );
        let mut cells = vec![0u32; nchars * 2 * 6 * cell_len];
        for ci in 0..nchars {
            for pal in 0..2 {
                for color in 0..6 {
                    let fg = PALETTE_RGB[pal][color];
                    let off = ((ci * 2 + pal) * 6 + color) * cell_len;
                    for px in 0..cell_len {
                        let cov = FONT_BIN[3 + ci * cell_len + px] as u32;
                        if cov == 0 {
                            continue;
                        }
                        cells[off + px] = 0xFF00_0000
                            | (blend(BG_RGB[0], fg[0], cov) << 16)
                            | (blend(BG_RGB[1], fg[1], cov) << 8)
                            | blend(BG_RGB[2], fg[2], cov);
                    }
                }
            }
        }
        Glyphs { cw, ch, cells }
    }
}

fn main() {
    let glyphs = Glyphs::load();
    let chars = std::str::from_utf8(CHARS).unwrap();
    let map: HashMap<char, usize> = chars.chars().enumerate().map(|(i, c)| (c, i)).collect();
    let buckets: Vec<Vec<usize>> = BUCKETS
        .iter()
        .map(|s| s.chars().filter_map(|c| map.get(&c).copied()).collect())
        .collect();

    let (cw, chh) = (glyphs.cw, glyphs.ch);
    let cols = W / cw;
    let rows = H / chh;
    let n = cols * rows;
    let ox = (W - cols * cw) / 2;
    let oy = (H - rows * chh) / 2;
    let hx = H as f64 / 2.0;
    let cxp = W as f64 / 2.0;
    let cyp = H as f64 / 2.0;

    let u0: Vec<f64> = (0..cols)
        .map(|c| ((c as f64 + 0.5) * cw as f64 - cxp) / hx)
        .collect();
    let vxx: Vec<f64> = (0..cols)
        .map(|c| ((c as f64 + 0.5) * cw as f64 - cxp) / cxp)
        .collect();
    let v0: Vec<f64> = (0..rows)
        .map(|r| ((r as f64 + 0.5) * chh as f64 - cyp) / hx)
        .collect();
    let vyy: Vec<f64> = (0..rows)
        .map(|r| ((r as f64 + 0.5) * chh as f64 - cyp) / cyp)
        .collect();

    let mut window = Window::new("/", W, H, WindowOptions::default()).unwrap();
    window.limit_update_rate(Some(std::time::Duration::from_secs_f64(1.0 / FPS)));

    let mut buf = vec![BG; W * H];
    let mut overlay = vec![0.0f64; n];
    let mut rng = Rng(0x1234_5678_9abc_def0);
    let mut spark_pts: Vec<(f64, f64, f64)> = Vec::new();
    let mut spark_loop: i64 = -1;

    let t0 = Instant::now();
    while window.is_open() && !window.is_key_pressed(Key::Escape, KeyRepeat::No) {
        let t = t0.elapsed().as_secs_f64();
        let loop_idx = (t / PERIOD) as i64;
        let (scene, t_in, pal_idx, half, dur) = segment(t - loop_idx as f64 * PERIOD);
        // нечётный цикл: камера вылетает из зрачка обратно к полному глазу
        let z = if scene == Scene::Eye && loop_idx % 2 == 1 {
            2f64.powf((dur - t_in) / half)
        } else {
            2f64.powf(t_in / half)
        };
        let iz = 1.0 / z;

        let ep = if scene == Scene::Eye {
            Some(eye_params(t, scene, dur, t_in))
        } else {
            None
        };
        let (orb_r, flare) = if scene == Scene::Orb {
            (orb_radius(t), orb_flare(t))
        } else {
            (0.55, 0.0)
        };

        overlay.fill(0.0);
        if scene == Scene::Orb {
            if spark_loop != loop_idx {
                let mut l = Lcg((100 + loop_idx) as u64);
                spark_pts = (0..26)
                    .map(|_| (l.f() * 3.6 - 1.8, l.f() * 2.1 - 1.05, l.f() * 6.283))
                    .collect();
                spark_loop = loop_idx;
            }
            for &(su, sv, ph) in &spark_pts {
                let a = 0.5 + 0.5 * (t * 2.6 + ph).sin();
                if a <= 0.25 {
                    continue;
                }
                let px = cxp + su * z * hx;
                let py = cyp + sv * z * hx;
                let c0 = (px / cw as f64) as isize;
                let r0 = (py / chh as f64) as isize;
                for rr in (r0 - 1)..=(r0 + 1) {
                    for cc in (c0 - 2)..=(c0 + 2) {
                        if rr < 0 || cc < 0 || rr as usize >= rows || cc as usize >= cols {
                            continue;
                        }
                        let dc = f64::max(
                            ((cc as f64 + 0.5) * cw as f64 - px).abs() / cw as f64,
                            ((rr as f64 + 0.5) * chh as f64 - py).abs() / chh as f64,
                        );
                        if dc < 1.6 {
                            let qv = (1.6 - dc) / 1.6 * a;
                            let k = rr as usize * cols + cc as usize;
                            if qv > overlay[k] {
                                overlay[k] = qv;
                            }
                        }
                    }
                }
            }
        }

        buf.fill(BG);
        for r in 0..rows {
            let v = v0[r] * iz;
            let vyv = vyy[r];
            let ybase = oy + r * chh;
            for c in 0..cols {
                let i = r * cols + c;
                let b;
                if scene == Scene::Noise {
                    let mut bb = rng.f() * rng.f() * 0.45;
                    if rng.f() < 0.015 {
                        bb = 0.9;
                    }
                    b = bb;
                } else {
                    let u = u0[c] * iz;
                    let v = v;
                    let mut bb = if scene == Scene::Eye {
                        eye_b(u, v, ep.as_ref().unwrap())
                    } else {
                        let mut ob = orb_b(u, v, t, orb_r, flare);
                        let hsh = cell_hash(c, r);
                        if hsh > 0.986 {
                            ob = ob.max(0.35 + 0.65 * (0.5 + 0.5 * (t * 2.1 + hsh * 90.0).sin()));
                        }
                        ob
                    };
                    let ov = overlay[i];
                    if ov > bb {
                        bb = ov;
                    }
                    bb *= 1.0 - 0.22 * (vxx[c] * vxx[c] + vyv * vyv); // виньетка
                    b = bb;
                }

                let (ch_idx, color) = if b < 0.05 {
                    (
                        if rng.f() < 0.05 { map[&'.'] } else { map[&' '] },
                        0usize,
                    )
                } else {
                    let color = ((b * 5.999) as usize).min(5);
                    let bucket = &buckets[((b * 6.999) as usize).min(6)];
                    (bucket[rng.below(bucket.len())], color)
                };

                let cell_off = ((ch_idx * 2 + pal_idx) * 6 + color) * cw * chh;
                let xbase = ox + c * cw;
                for gy in 0..chh {
                    let src = cell_off + gy * cw;
                    let dst = (ybase + gy) * W + xbase;
                    buf[dst..dst + cw].copy_from_slice(&glyphs.cells[src..src + cw]);
                }
            }
        }
        window.update_with_buffer(&buf, W, H).unwrap();
    }
}
