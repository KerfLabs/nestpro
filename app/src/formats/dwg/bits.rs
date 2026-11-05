//! Bit-level reader for DWG data. Every read is bounds-checked: running off the end
//! of the data gives an error, never a panic (DW15).

/// DWG file versions, oldest first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Ver {
    R13,
    R14,
    R2000,
    R2004,
    R2007,
    R2010,
    R2013,
    R2018,
}

/// Ran past the end of the data (or found a value that cannot be right).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Eof;

pub type BR<T> = Result<T, Eof>;

/// A handle reference: its code (ownership / offset kind) and value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Href {
    pub code: u8,
    pub val: u64,
}

impl Href {
    /// The handle this reference points at. Codes 6/8/A/C are offsets from `base`
    /// (the handle of the object that holds the reference).
    pub fn abs(self, base: u64) -> u64 {
        match self.code {
            6 => base.wrapping_add(1),
            8 => base.wrapping_sub(1),
            0xA => base.wrapping_add(self.val),
            0xC => base.wrapping_sub(self.val),
            _ => self.val,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Bits<'a> {
    pub data: &'a [u8],
    /// Position in bits from the start of `data`.
    pub pos: usize,
    /// Reads stop at this bit.
    pub end: usize,
    pub ver: Ver,
}

impl<'a> Bits<'a> {
    pub fn new(data: &'a [u8], ver: Ver) -> Self {
        Bits { data, pos: 0, end: data.len() * 8, ver }
    }

    pub fn at(data: &'a [u8], byte: usize, ver: Ver) -> Self {
        Bits { data, pos: byte * 8, end: data.len() * 8, ver }
    }

    pub fn left(&self) -> usize {
        self.end.saturating_sub(self.pos)
    }

    fn need(&self, n: usize) -> BR<()> {
        if self.pos.checked_add(n).is_some_and(|e| e <= self.end) {
            Ok(())
        } else {
            Err(Eof)
        }
    }

    pub fn skip(&mut self, nbits: usize) -> BR<()> {
        self.need(nbits)?;
        self.pos += nbits;
        Ok(())
    }


    pub fn b(&mut self) -> BR<u8> {
        self.need(1)?;
        let byte = self.data[self.pos >> 3];
        let bit = (byte >> (7 - (self.pos & 7))) & 1;
        self.pos += 1;
        Ok(bit)
    }

    pub fn bits(&mut self, n: u32) -> BR<u32> {
        let mut v = 0u32;
        for _ in 0..n {
            v = (v << 1) | self.b()? as u32;
        }
        Ok(v)
    }

    pub fn bb(&mut self) -> BR<u8> {
        Ok(self.bits(2)? as u8)
    }

    pub fn rc(&mut self) -> BR<u8> {
        self.need(8)?;
        let i = self.pos >> 3;
        let sh = self.pos & 7;
        let v = if sh == 0 {
            self.data[i]
        } else {
            let hi = self.data[i] << sh;
            let lo = if i + 1 < self.data.len() { self.data[i + 1] >> (8 - sh) } else { 0 };
            hi | lo
        };
        self.pos += 8;
        Ok(v)
    }

    pub fn bytes(&mut self, n: usize) -> BR<Vec<u8>> {
        self.need(n.checked_mul(8).ok_or(Eof)?)?;
        (0..n).map(|_| self.rc()).collect()
    }

    pub fn rs(&mut self) -> BR<u16> {
        Ok(u16::from_le_bytes([self.rc()?, self.rc()?]))
    }

    pub fn rl(&mut self) -> BR<u32> {
        Ok(u32::from_le_bytes([self.rc()?, self.rc()?, self.rc()?, self.rc()?]))
    }

    pub fn rd(&mut self) -> BR<f64> {
        let mut b = [0u8; 8];
        for x in &mut b {
            *x = self.rc()?;
        }
        Ok(f64::from_le_bytes(b))
    }

    pub fn bs(&mut self) -> BR<u16> {
        Ok(match self.bb()? {
            0 => self.rs()?,
            1 => self.rc()? as u16,
            2 => 0,
            _ => 256,
        })
    }

    pub fn bl(&mut self) -> BR<u32> {
        match self.bb()? {
            0 => self.rl(),
            1 => Ok(self.rc()? as u32),
            2 => Ok(0),
            _ => Err(Eof),
        }
    }

    /// Bit long long: a 3-bit byte count, then that many bytes (low byte first).
    pub fn bll(&mut self) -> BR<u64> {
        let n = self.bits(3)?;
        let mut v = 0u64;
        for i in 0..n {
            v |= (self.rc()? as u64) << (8 * i);
        }
        Ok(v)
    }

    pub fn bd(&mut self) -> BR<f64> {
        match self.bb()? {
            0 => self.rd(),
            1 => Ok(1.0),
            2 => Ok(0.0),
            _ => Err(Eof),
        }
    }

    /// Double stored as a patch on a default value.
    pub fn dd(&mut self, def: f64) -> BR<f64> {
        let mut b = def.to_le_bytes();
        match self.bb()? {
            0 => {}
            1 => {
                for x in b.iter_mut().take(4) {
                    *x = self.rc()?;
                }
            }
            2 => {
                b[4] = self.rc()?;
                b[5] = self.rc()?;
                for x in b.iter_mut().take(4) {
                    *x = self.rc()?;
                }
            }
            _ => return self.rd(),
        }
        Ok(f64::from_le_bytes(b))
    }

    pub fn rd2(&mut self) -> BR<[f64; 2]> {
        Ok([self.rd()?, self.rd()?])
    }


    pub fn bd3(&mut self) -> BR<[f64; 3]> {
        Ok([self.bd()?, self.bd()?, self.bd()?])
    }

    /// Extrusion: R2000+ stores a single bit when it is 0,0,1.
    pub fn be(&mut self) -> BR<[f64; 3]> {
        if self.ver >= Ver::R2000 && self.b()? == 1 {
            return Ok([0.0, 0.0, 1.0]);
        }
        self.bd3()
    }

    /// Thickness: R2000+ stores a single bit when it is 0.
    pub fn bt(&mut self) -> BR<f64> {
        if self.ver >= Ver::R2000 && self.b()? == 1 {
            return Ok(0.0);
        }
        self.bd()
    }

    /// Modular char, signed (the 0x40 bit of the last byte is the sign).
    pub fn mc(&mut self) -> BR<i64> {
        let mut v: i64 = 0;
        let mut shift = 0;
        loop {
            let b = self.rc()?;
            if b & 0x80 == 0 {
                let neg = b & 0x40 != 0;
                v |= ((b & 0x3f) as i64) << shift;
                return Ok(if neg { -v } else { v });
            }
            v |= ((b & 0x7f) as i64) << shift;
            shift += 7;
            if shift > 56 {
                return Err(Eof);
            }
        }
    }

    /// Modular char, unsigned.
    pub fn mcu(&mut self) -> BR<u64> {
        let mut v: u64 = 0;
        let mut shift = 0;
        loop {
            let b = self.rc()?;
            v |= ((b & 0x7f) as u64) << shift;
            if b & 0x80 == 0 {
                return Ok(v);
            }
            shift += 7;
            if shift > 63 {
                return Err(Eof);
            }
        }
    }

    /// Modular short (object sizes).
    pub fn ms(&mut self) -> BR<u32> {
        let mut v: u32 = 0;
        let mut shift = 0;
        loop {
            let w = self.rs()?;
            v |= ((w & 0x7fff) as u32) << shift;
            if w & 0x8000 == 0 {
                return Ok(v);
            }
            shift += 15;
            if shift > 30 {
                return Err(Eof);
            }
        }
    }

    pub fn h(&mut self) -> BR<Href> {
        let b = self.rc()?;
        let code = b >> 4;
        let n = b & 0x0f;
        if n > 8 {
            return Err(Eof);
        }
        let mut val = 0u64;
        for _ in 0..n {
            val = (val << 8) | self.rc()? as u64;
        }
        Ok(Href { code, val })
    }

    /// Object type: a bit short until R2007, a packed form from R2010.
    pub fn ot(&mut self) -> BR<u16> {
        if self.ver < Ver::R2010 {
            return self.bs();
        }
        match self.bb()? {
            0 => Ok(self.rc()? as u16),
            1 => Ok(self.rc()? as u16 + 0x1f0),
            _ => self.rs(),
        }
    }

    /// 8-bit text (before R2007): bit-short length, then the bytes.
    pub fn t8(&mut self) -> BR<String> {
        let n = self.bs()? as usize;
        let b = self.bytes(n)?;
        Ok(cp1252(&b))
    }

    /// Unicode text (R2007+): bit-short length in characters, then UTF-16.
    pub fn tu(&mut self) -> BR<String> {
        let n = self.bs()? as usize;
        let mut u = Vec::with_capacity(n);
        for _ in 0..n {
            u.push(self.rs()?);
        }
        while u.last() == Some(&0) {
            u.pop();
        }
        Ok(String::from_utf16_lossy(&u))
    }

    /// Colour: R2004+ may carry an RGB value and transparency. Returns the flags byte.
    pub fn cmc(&mut self) -> BR<u16> {
        let idx = self.bs()?;
        if self.ver >= Ver::R2004 {
            let flags = idx >> 8;
            if flags & 0x80 != 0 {
                self.bl()?;
            }
            if flags & 0x20 != 0 {
                self.bl()?;
            }
            return Ok(flags);
        }
        Ok(0)
    }
}

/// Variable text: 8-bit before R2007; from R2007 it is read from the object's string
/// stream.
pub fn tv<'a>(main: &mut Bits<'a>, strs: &mut Option<Bits<'a>>) -> BR<String> {
    if main.ver >= Ver::R2007 {
        match strs {
            Some(s) => s.tu(),
            None => Ok(String::new()),
        }
    } else {
        main.t8()
    }
}

/// Windows-1252 bytes to text (the usual code page of shop drawings).
pub fn cp1252(b: &[u8]) -> String {
    const HI: [char; 32] = [
        '€', '\u{81}', '‚', 'ƒ', '„', '…', '†', '‡', 'ˆ', '‰', 'Š', '‹', 'Œ', '\u{8d}', 'Ž', '\u{8f}', '\u{90}', '‘',
        '’', '“', '”', '•', '–', '—', '˜', '™', 'š', '›', 'œ', '\u{9d}', 'ž', 'Ÿ',
    ];
    b.iter()
        .take_while(|&&c| c != 0)
        .map(|&c| if (0x80..0xa0).contains(&c) { HI[(c - 0x80) as usize] } else { c as char })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Pack a string of '0'/'1' into bytes (MSB first), padding with zeros.
    pub fn pack(s: &str) -> Vec<u8> {
        let bits: Vec<u8> = s.bytes().filter(|c| *c == b'0' || *c == b'1').map(|c| c - b'0').collect();
        bits.chunks(8).map(|ch| ch.iter().enumerate().fold(0u8, |a, (i, &b)| a | (b << (7 - i)))).collect()
    }

    #[test]
    fn bit_shorts_decode_all_four_forms() {
        // short 257, 0, 256, char 15, 0
        let d = pack("00 00000001 00000001 10 11 01 00001111 10");
        let mut r = Bits::new(&d, Ver::R2000);
        assert_eq!(r.bs(), Ok(257));
        assert_eq!(r.bs(), Ok(0));
        assert_eq!(r.bs(), Ok(256));
        assert_eq!(r.bs(), Ok(15));
        assert_eq!(r.bs(), Ok(0));
    }

    #[test]
    fn bit_longs_and_doubles() {
        let d = pack("00 00000001 00000001 00000000 00000000 10 01 00001111 01 10");
        let mut r = Bits::new(&d, Ver::R2000);
        assert_eq!(r.bl(), Ok(257));
        assert_eq!(r.bl(), Ok(0));
        assert_eq!(r.bl(), Ok(15));
        assert_eq!(r.bd(), Ok(1.0));
        assert_eq!(r.bd(), Ok(0.0));
        // A full double after a 2-bit prefix (unaligned).
        let mut s = String::from("00");
        for byte in 12.5f64.to_le_bytes() {
            s += &format!("{byte:08b}");
        }
        let d = pack(&s);
        assert_eq!(Bits::new(&d, Ver::R2000).bd(), Ok(12.5));
    }

    #[test]
    fn modular_chars_and_shorts() {
        let d = [0b1000_0010u8, 0b0010_0100];
        assert_eq!(Bits::new(&d, Ver::R14).mc(), Ok(0x1202));
        let d = [0xe9u8, 0x97, 0xe6, 0x35];
        assert_eq!(Bits::new(&d, Ver::R14).mc(), Ok(112823273));
        let d = [0b1000_0101u8, 0b0100_1011];
        assert_eq!(Bits::new(&d, Ver::R14).mc(), Ok(-1413));
        let d = [0x31u8, 0xf4, 0x8d, 0x00];
        assert_eq!(Bits::new(&d, Ver::R14).ms(), Ok(4650033));
    }

    #[test]
    fn default_doubles_patch_the_default() {
        // 01: four bytes replace the low half of the default.
        let def = 10.0f64;
        let mut want = def.to_le_bytes();
        want[..4].copy_from_slice(&[1, 2, 3, 4]);
        let d = pack(&format!("01{:08b}{:08b}{:08b}{:08b}", 1, 2, 3, 4));
        assert_eq!(Bits::new(&d, Ver::R2000).dd(def), Ok(f64::from_le_bytes(want)));
        assert_eq!(Bits::new(&pack("00"), Ver::R2000).dd(def), Ok(def));
    }

    #[test]
    fn handles_and_offsets() {
        // 5.2.05E7: hard pointer to handle 0x5E7
        let d = [0x52u8, 0x05, 0xe7];
        let h = Bits::new(&d, Ver::R14).h().unwrap();
        assert_eq!((h.code, h.val), (5, 0x5e7));
        assert_eq!(Href { code: 6, val: 0 }.abs(0x40), 0x41);
        assert_eq!(Href { code: 0xC, val: 3 }.abs(0x40), 0x3d);
    }

    #[test]
    fn reading_past_the_end_is_an_error_not_a_panic() {
        let d = [0xffu8];
        let mut r = Bits::new(&d, Ver::R2000);
        assert_eq!(r.rd(), Err(Eof));
        assert!(r.bytes(usize::MAX).is_err());
        let mut r = Bits::new(&[], Ver::R2018);
        assert!(r.ot().is_err() && r.h().is_err() && r.ms().is_err() && r.tu().is_err());
    }

    #[test]
    fn text_in_windows_code_page() {
        assert_eq!(cp1252(b"\xd8 50 \x80"), "Ø 50 €");
    }
}
