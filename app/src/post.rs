//! The machine's post-processor settings, as the original keeps them: a post
//! library file (.CFF in a machine's seed, .CF1 in MchFiles) whose `Annn` lines
//! say how a CNC program is written — block numbers, units, axis signs,
//! extension, circles in one move. Our G-code writer follows them, so loading
//! the LC5's own file makes our programs take its form. See MECHANICS.md
//! "How Expert makes a machine's programs".

use std::path::Path;

/// Expert's own post library template (LANTEKPR.CFF in the install).
pub const EXPERT_CFF: &str = "LANTEKPR.CFF";

#[derive(Clone, Debug, PartialEq)]
pub struct PostConfig {
    /// Where these settings came from.
    pub source: String,
    /// A001 machine type (0 cut, 1 cut-punch, 2 punch).
    pub machine_type: u8,
    /// A005 machine tolerance.
    pub tolerance: f64,
    /// A006 output units: 1 = mm, 25.4 = inch.
    pub units: f64,
    /// A007 characters in a block number.
    pub block_digits: u8,
    /// A008 zero suppression in block numbers (0 = keep leading zeros).
    pub zero_suppress: u8,
    /// A009 text before the block number.
    pub prefix: String,
    /// A011 first block number, A012 increment.
    pub first: u32,
    pub step: u32,
    /// A013 CNC folder, A014 extension.
    pub cnc_dir: String,
    pub extension: String,
    /// A016 / A017 sign of X and Y.
    pub sign_x: f64,
    pub sign_y: f64,
    /// A024 circles in one move.
    pub circles_one_move: bool,
}

impl Default for PostConfig {
    /// The values of Expert's LANTEKPR.CFF on this install.
    fn default() -> Self {
        PostConfig {
            source: "built-in (LANTEKPR.CFF values)".into(),
            machine_type: 1,
            tolerance: 0.010,
            units: 1.0,
            block_digits: 5,
            zero_suppress: 1,
            prefix: "N".into(),
            first: 10,
            step: 10,
            cnc_dir: r"C:\LANTEK\EXPERT\CNC\".into(),
            extension: ".CNC".into(),
            sign_x: 1.0,
            sign_y: 1.0,
            circles_one_move: false,
        }
    }
}

fn quoted(rest: &str) -> Option<String> {
    let a = rest.find('"')?;
    let b = rest[a + 1..].find('"')? + a + 1;
    Some(rest[a + 1..b].to_string())
}

impl PostConfig {
    /// Read the `Annn` lines of a post library (.CFF / .CF1). Lines it doesn't
    /// have keep the defaults.
    pub fn parse(text: &str, source: &str) -> PostConfig {
        let mut p = PostConfig { source: source.to_string(), ..Default::default() };
        for line in text.lines() {
            let mut it = line.splitn(2, char::is_whitespace);
            let key = it.next().unwrap_or("");
            let rest = it.next().unwrap_or("").split(';').next().unwrap_or("").trim();
            let num = || rest.split_whitespace().next().and_then(|v| v.parse::<f64>().ok());
            let on = || rest.split_whitespace().next().is_some_and(|v| v.eq_ignore_ascii_case("ON"));
            match key {
                "A001" => p.machine_type = num().map_or(p.machine_type, |v| v as u8),
                "A005" => p.tolerance = num().unwrap_or(p.tolerance),
                "A006" => p.units = num().filter(|v| *v > 0.0).unwrap_or(p.units),
                "A007" => p.block_digits = num().map_or(p.block_digits, |v| v as u8),
                "A008" => p.zero_suppress = num().map_or(p.zero_suppress, |v| v as u8),
                "A009" => p.prefix = quoted(rest).unwrap_or(p.prefix.clone()),
                "A011" => p.first = num().map_or(p.first, |v| v as u32),
                "A012" => p.step = num().map_or(p.step, |v| (v as u32).max(1)),
                "A013" => p.cnc_dir = quoted(rest).unwrap_or(p.cnc_dir.clone()),
                "A014" => p.extension = quoted(rest).unwrap_or(p.extension.clone()),
                "A016" => p.sign_x = num().map_or(p.sign_x, |v| if v < 0.0 { -1.0 } else { 1.0 }),
                "A017" => p.sign_y = num().map_or(p.sign_y, |v| if v < 0.0 { -1.0 } else { 1.0 }),
                "A024" => p.circles_one_move = on(),
                _ => {}
            }
        }
        p
    }

    /// Read a post library file (Latin-1 text).
    pub fn load(path: &Path) -> Option<PostConfig> {
        let bytes = std::fs::read(path).ok()?;
        let text: String = bytes.iter().map(|&c| c as char).collect();
        Some(PostConfig::parse(&text, &path.display().to_string()))
    }

    /// Block number `n`: prefix, then the number — padded with zeros to A007
    /// characters when A008 keeps the zeros.
    pub fn block(&self, n: u32) -> String {
        if self.zero_suppress == 0 {
            format!("{}{:0w$}", self.prefix, n, w = self.block_digits as usize)
        } else {
            format!("{}{}", self.prefix, n)
        }
    }

    /// Decimals for coordinates (mm 2, inch 4).
    fn decimals(&self) -> usize {
        if self.units > 1.0 { 4 } else { 2 }
    }

    /// Rewrite a program written in mm with N10, N20… blocks into this post's
    /// form: block numbers, units and axis signs. Comments are left alone.
    pub fn apply(&self, program: &str) -> String {
        let mut out = String::with_capacity(program.len());
        let mut n = self.first;
        // One axis turned round mirrors the path, so the scrap side of every
        // cut swaps: G41 ↔ G42 (KC2, our rule for the axis signs).
        let mirrored = self.sign_x * self.sign_y < 0.0;
        for line in program.lines() {
            let (code, comment) = match line.find('(') {
                Some(k) => (&line[..k], &line[k..]),
                None => (line, ""),
            };
            let mut toks: Vec<String> = vec![];
            let mut numbered = false;
            for (i, t) in code.split_whitespace().enumerate() {
                if i == 0 && t.starts_with('N') && t[1..].chars().all(|c| c.is_ascii_digit()) && t.len() > 1 {
                    numbered = true;
                    continue;
                }
                let axis = t.chars().next().unwrap_or(' ');
                let val = t.get(1..).and_then(|v| v.parse::<f64>().ok());
                match (axis, val) {
                    ('X' | 'I', Some(v)) => toks.push(format!("{axis}{:.*}", self.decimals(), v / self.units * self.sign_x)),
                    ('Y' | 'J', Some(v)) => toks.push(format!("{axis}{:.*}", self.decimals(), v / self.units * self.sign_y)),
                    ('F', Some(v)) if self.units > 1.0 => toks.push(format!("F{:.1}", v / self.units)),
                    ('G', Some(v)) if mirrored && (v == 41.0 || v == 42.0) => toks.push(if v == 41.0 { "G42".into() } else { "G41".into() }),
                    ('G', Some(v)) if self.units > 1.0 && (v - 21.0).abs() < 1e-9 => toks.push("G20".into()),
                    _ => toks.push(t.to_string()),
                }
            }
            if numbered {
                out.push_str(&self.block(n));
                out.push(' ');
                n += self.step;
            }
            out.push_str(&toks.join(" "));
            if !comment.is_empty() {
                if !toks.is_empty() {
                    out.push(' ');
                }
                out.push_str(comment);
            }
            out.push('\n');
        }
        out
    }

    /// File name of a sheet's program: Expert names it after the job (CFT A007
    /// "CNC name = job name") plus the sheet, with the post's extension.
    pub fn file_name(&self, job: &str, sheet: usize) -> String {
        let j: String = job.chars().filter(|c| c.is_ascii_alphanumeric()).collect::<String>().to_uppercase();
        format!("{}X{}{}", if j.is_empty() { "JOB".into() } else { j }, sheet, self.extension)
    }
}

#[cfg(test)]
mod post_tests {
    use super::*;

    #[test]
    fn reads_a_post_library_and_writes_its_form() {
        let cff = "A001 1 ; t\nA006 25.4 ; in\nA007 5\nA008 0 ; keep zeros\nA009 \"N\"\nA011 3\nA012 3\nA014 \".TXT\"\nA017 -1\nA024 ON ; one move\n";
        let p = PostConfig::parse(cff, "test");
        assert_eq!((p.first, p.step, p.block_digits, p.zero_suppress), (3, 3, 5, 0));
        assert!(p.circles_one_move && p.sign_y < 0.0 && p.extension == ".TXT");
        let out = p.apply("( HEAD )\nN10 G90 G21 G17\nN20 G0 X25.40 Y50.80\nN30 G1 X0 Y0 F2540 ( CUT )\nN40 G1 G42 D2 X1 Y1\n");
        let lines: Vec<&str> = out.lines().collect();
        assert_eq!(lines[0], "( HEAD )");
        assert_eq!(lines[1], "N00003 G90 G20 G17");
        assert_eq!(lines[2], "N00006 G0 X1.0000 Y-2.0000");
        assert_eq!(lines[3], "N00009 G1 X0.0000 Y-0.0000 F100.0 ( CUT )");
        // Y turned round mirrors the path: the scrap side swaps.
        assert_eq!(lines[4], "N00012 G1 G41 D2 X0.0394 Y-0.0394");
        // Expert's own template: N10, N20… in mm, .CNC files.
        let d = PostConfig::default();
        assert_eq!(d.block(10), "N10");
        assert_eq!(d.file_name("Sample job 1", 2), "SAMPLEJOB1X2.CNC");
    }
}
