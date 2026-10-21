//! Fitur Water: pelacak minum, pengingat, slide Water.

use super::*;

pub(crate) const BX: f32 = RX - 112.0;

pub(crate) struct Water {
    pub(crate) day: String,
    pub(crate) ml: u32,
    pub(crate) goal: u32,
    pub(crate) remind_min: u32,
    pub(crate) next: Instant,
}

impl Water {
    pub(crate) fn load() -> Water {
        let mut w = Water { day: today(), ml: 0, goal: 2000, remind_min: 60, next: Instant::now() };
        if let Ok(s) = std::fs::read_to_string(data_path("water.txt")) {
            let p: Vec<&str> = s.trim().split('|').collect();
            if p.len() >= 4 {
                w.goal = p[2].parse().unwrap_or(2000);
                w.remind_min = p[3].parse().unwrap_or(60);
                if p[0] == w.day {
                    w.ml = p[1].parse().unwrap_or(0);
                }
            }
        }
        w.reset_timer();
        w
    }
    pub(crate) fn save(&self) {
        write_data("water.txt", format!("{}|{}|{}|{}\n", self.day, self.ml, self.goal, self.remind_min));
    }
    pub(crate) fn reset_timer(&mut self) {
        self.next = Instant::now() + Duration::from_secs(self.remind_min.max(1) as u64 * 60);
    }
    pub(crate) fn roll(&mut self) -> bool {
        let d = today();
        if d != self.day {
            self.day = d;
            self.ml = 0;
            self.save();
            return true;
        }
        false
    }
    fn add(&mut self, d: i32) {
        self.ml = (self.ml as i32 + d).max(0) as u32;
        self.reset_timer();
        self.save();
    }
}

pub(crate) fn check_reminder(a: &mut App) -> bool {
    if a.water.remind_min == 0 {
        return false;
    }
    let now = Instant::now();
    if now < a.water.next {
        return false;
    }
    a.water.reset_timer();
    let st = now_tm();
    if st.wHour < 8 || st.wHour >= 22 || a.water.ml >= a.water.goal {
        return false;
    }
    a.show(2);
    a.reminding = true;
    a.peek_until = Some(now + Duration::from_secs(7));
    true
}

impl App {
    pub(crate) fn water_add(&mut self, d: i32) {
        self.water.add(d);
        self.reminding = false;
        self.peek_until = None;
    }

    pub(crate) fn slide_water(&self, g: &mut Gfx, ox: f32, al: f32) {
        let blue = paint(80, 170, 255, al);
        if let Some(path) = PathBuilder::from_circle(LX + 6.0 + ox, 23.0, 7.0) {
            g.fill(&path, &blue);
        }
        let (label, lc) = if self.reminding { ("Time to drink water", [120, 190, 255]) } else { ("Water", [170, 170, 175]) };
        g.txtb(label, LX + 20.0 + ox, 14.0, 13.0, lc, al, 240.0);
        let wt = &self.water;
        g.txtb(&format!("{:.2} L", wt.ml as f32 / 1000.0), LX + ox, 40.0, 38.0, [255, 255, 255], al, 230.0);
        let pct = if wt.goal > 0 { (wt.ml * 100 / wt.goal).min(999) } else { 0 };
        let sub = if wt.ml >= wt.goal {
            format!("Goal of {:.1} L reached", wt.goal as f32 / 1000.0)
        } else {
            format!("of {:.1} L  \u{b7}  {}%", wt.goal as f32 / 1000.0, pct)
        };
        g.txt(&sub, LX + ox, 92.0, 13.0, [170, 170, 175], al, 230.0);
        for (label, by) in [("+250 ml", 42.0f32), ("+500 ml", 84.0f32)] {
            g.fill(&rrect(BX + ox, by, 112.0, 34.0, 17.0), &paint(80, 170, 255, 0.24 * al));
            g.txt_c(label, BX + ox + 56.0, by + ty(34.0, 15.0), 15.0, [205, 230, 255], al, 100.0, true);
        }
        g.fill(&rrect(LX + ox, 130.0, CW, 8.0, 4.0), &paint(255, 255, 255, 0.14 * al));
        if wt.goal > 0 && wt.ml > 0 {
            let fw = (CW * (wt.ml as f32 / wt.goal as f32).min(1.0)).max(8.0);
            g.fill(&rrect(LX + ox, 130.0, fw, 8.0, 4.0), &blue);
        }
    }
}
