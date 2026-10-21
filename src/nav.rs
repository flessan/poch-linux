//! Carousel: tombol panah, dots, perpindahan slide.

use super::*;

pub(crate) fn cyc(d: f32) -> f32 {
    let n = N_SLIDES as f32;
    d - n * (d / n).round()
}

pub(crate) fn dot_x(i: usize) -> f32 {
    CX + (i as f32 - (N_SLIDES as f32 - 1.0) / 2.0) * 14.0
}

pub(crate) const NAV_L: (f32, f32) = (CL + 22.0, NAVY);
pub(crate) const NAV_R: (f32, f32) = (CL + EXP_W - 22.0, NAVY);

impl App {
    pub(crate) fn cur(&self) -> usize {
        self.slide.rem_euclid(N_SLIDES as i32) as usize
    }

    pub(crate) fn step(&mut self, dir: i32) {
        self.slide += dir;
    }

    pub(crate) fn show(&mut self, idx: usize) {
        let n = N_SLIDES as i32;
        let mut d = (idx as i32 - self.cur() as i32).rem_euclid(n);
        if d > n / 2 {
            d -= n;
        }
        self.slide += d;
    }

    pub(crate) fn draw_nav(&self, g: &mut Gfx, p: f32) {
        let bg = paint(255, 255, 255, 0.12 * p);
        let wh = paint(255, 255, 255, 0.95 * p);
        let (lx, ly) = NAV_L;
        g.circle(lx, ly, 13.0, &bg);
        g.line(lx + 2.5, ly - 5.5, lx - 2.5, ly, 2.2, &wh);
        g.line(lx - 2.5, ly, lx + 2.5, ly + 5.5, 2.2, &wh);
        let (rx, ry) = NAV_R;
        g.circle(rx, ry, 13.0, &bg);
        g.line(rx - 2.5, ry - 5.5, rx + 2.5, ry, 2.2, &wh);
        g.line(rx + 2.5, ry, rx - 2.5, ry + 5.5, 2.2, &wh);
    }

    pub(crate) fn draw_dots(&self, g: &mut Gfx, pos: f32, p: f32) {
        for i in 0..N_SLIDES {
            let near = (1.0 - cyc(i as f32 - pos).abs()).clamp(0.0, 1.0);
            let r = 2.3 + 1.2 * near;
            let a = (0.28 + 0.72 * near) * p;
            if near > 0.5 {
                g.circle(dot_x(i), EXP_H - 11.0, r, &acc(a));
            } else {
                g.circle(dot_x(i), EXP_H - 11.0, r, &paint(255, 255, 255, a));
            }
        }
    }
}
