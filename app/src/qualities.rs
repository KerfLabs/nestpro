//! How the machine's 12 cut qualities show on screen and to agents: each
//! one's name from the machine's technology file (CQ4) and its colour from
//! the original's colour settings (CQ5).

use std::collections::HashMap;
use std::path::Path;

use crate::nest::CutQuality;

/// The name that hides a quality from the pickers (CQ4).
const HIDE: &str = "<HIDE>";

/// The colours the original ships for qualities 1–12 (CQ5).
const SHIPPED_COLOURS: [[u8; 3]; CutQuality::COUNT] = [
    [255, 128, 64],
    [0, 128, 192],
    [255, 0, 255],
    [0, 128, 0],
    [128, 0, 128],
    [128, 64, 64],
    [0, 128, 255],
    [0, 128, 128],
    [0, 64, 128],
    [128, 128, 255],
    [128, 255, 255],
    [255, 128, 255],
];

/// The machine's cut qualities as they show: names and colours.
#[derive(Clone, Debug, PartialEq)]
pub struct CutQualities {
    /// Quality 1's name first; empty = the file gives none.
    names: [String; CutQuality::COUNT],
    /// Quality 1's colour first, RGB.
    colours: [[u8; 3]; CutQuality::COUNT],
}

impl Default for CutQualities {
    /// No names, the shipped colours.
    fn default() -> Self {
        CutQualities { names: Default::default(), colours: SHIPPED_COLOURS }
    }
}

impl CutQualities {
    /// From the machine file's own values (CQ4): A922 names quality 1 …
    /// A933 quality 12, in quotes.
    pub fn from_codes(m: &HashMap<String, String>) -> CutQualities {
        let mut q = CutQualities::default();
        for (k, name) in q.names.iter_mut().enumerate() {
            if let Some(v) = m.get(&format!("A{}", 922 + k)) {
                *name = v.trim().trim_matches('"').trim().to_string();
            }
        }
        q
    }

    /// Take the colours of the original's colour settings (CQ5): its
    /// `LANTEKIN.CNI` lines `Cnnn R G B style width`, C041 for quality 1 …
    /// C052 for quality 12. A quality without a line keeps its colour.
    pub fn with_colour_settings(mut self, text: &str) -> CutQualities {
        for line in text.lines() {
            let mut w = line.split(';').next().unwrap_or("").split_whitespace();
            let Some(n) = w.next().and_then(|c| c.strip_prefix('C')).filter(|n| n.len() == 3).and_then(|n| n.parse::<usize>().ok()) else { continue };
            let rgb: Vec<u8> = w.take(3).filter_map(|v| v.parse::<i64>().ok()).map(|v| v.clamp(0, 255) as u8).collect();
            if (41..41 + CutQuality::COUNT).contains(&n) && rgb.len() == 3 {
                self.colours[n - 41] = [rgb[0], rgb[1], rgb[2]];
            }
        }
        self
    }

    /// The colours of the colour settings in a Lantek folder, when it has them.
    pub fn with_colours_in(self, lantek_dir: &Path) -> CutQualities {
        match std::fs::read(lantek_dir.join("LANTEKIN.CNI")) {
            Ok(b) => self.with_colour_settings(&b.iter().map(|&c| c as char).collect::<String>()),
            Err(_) => self,
        }
    }

    /// The machine's name for quality `q`, when it gives one.
    pub fn name(&self, q: CutQuality) -> Option<&str> {
        let n = self.names[q.number() - 1].as_str();
        (!n.is_empty() && n != HIDE).then_some(n)
    }

    /// Whether the pickers offer quality `q` (CQ4): not when the machine file
    /// names it `<HIDE>`. The machine still has it: size bands and agents may
    /// still give it.
    pub fn offered(&self, q: CutQuality) -> bool {
        self.names[q.number() - 1] != HIDE
    }

    /// Quality `q` as screens and agents show it (CQ4): "Slow (1)" when the
    /// machine names it, else "Quality 1".
    pub fn label(&self, q: CutQuality) -> String {
        match self.name(q) {
            Some(n) => format!("{n} ({})", q.number()),
            None => q.label(),
        }
    }

    /// Quality `q`'s colour, RGB (CQ5).
    pub fn colour(&self, q: CutQuality) -> [u8; 3] {
        self.colours[q.number() - 1]
    }

    /// Quality `q`'s colour as `#rrggbb`.
    pub fn colour_hex(&self, q: CutQuality) -> String {
        let [r, g, b] = self.colour(q);
        format!("#{r:02x}{g:02x}{b:02x}")
    }
}
