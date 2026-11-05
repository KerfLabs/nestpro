//! Standard block ciphers used by encrypted Lantek files (MF4, MF5): Blowfish
//! and AES-128, both in CBC mode. Written out here so the app needs no extra
//! crates. No keys live in this file: they come from a `KeySource` (MF7).
//!
//! Blowfish's starting tables are the hexadecimal digits of pi (Schneier, 1993);
//! they are computed once at run time instead of being written out. The AES
//! S-box is likewise computed from its definition (FIPS-197).

use std::sync::OnceLock;

// ---------------------------------------------------------------- pi digits

/// The first `words` 32-bit words of pi's fractional part (0x243F6A88, …).
fn pi_fraction_words(words: usize) -> Vec<u32> {
    // Fixed point: limb 0 is the integer part, limbs 1.. the fraction, base 2^32.
    let n = words + 4; // guard limbs
    fn div_small(a: &mut [u32], d: u32) {
        let mut rem: u64 = 0;
        for limb in a.iter_mut() {
            let cur = (rem << 32) | *limb as u64;
            *limb = (cur / d as u64) as u32;
            rem = cur % d as u64;
        }
    }
    fn add(a: &mut [u32], b: &[u32]) {
        let mut carry = 0u64;
        for i in (0..a.len()).rev() {
            let s = a[i] as u64 + b[i] as u64 + carry;
            a[i] = s as u32;
            carry = s >> 32;
        }
    }
    fn sub(a: &mut [u32], b: &[u32]) {
        let mut borrow = 0i64;
        for i in (0..a.len()).rev() {
            let s = a[i] as i64 - b[i] as i64 - borrow;
            if s < 0 {
                a[i] = (s + (1i64 << 32)) as u32;
                borrow = 1;
            } else {
                a[i] = s as u32;
                borrow = 0;
            }
        }
    }
    fn is_zero(a: &[u32]) -> bool {
        a.iter().all(|&x| x == 0)
    }
    // mult * atan(1/x) = mult * sum (-1)^k / ((2k+1) x^(2k+1))
    let atan_inv = |x: u32, mult: u32| -> Vec<u32> {
        let mut sum = vec![0u32; n];
        let mut power = vec![0u32; n];
        power[0] = mult;
        div_small(&mut power, x);
        let x2 = x * x;
        let mut k: u32 = 0;
        let mut term = vec![0u32; n];
        loop {
            term.copy_from_slice(&power);
            div_small(&mut term, 2 * k + 1);
            if is_zero(&term) {
                break;
            }
            if k % 2 == 0 {
                add(&mut sum, &term);
            } else {
                sub(&mut sum, &term);
            }
            div_small(&mut power, x2);
            k += 1;
        }
        sum
    };
    // Machin: pi = 16 atan(1/5) - 4 atan(1/239)
    let mut pi = atan_inv(5, 16);
    let b = atan_inv(239, 4);
    sub(&mut pi, &b);
    pi[1..=words].to_vec()
}

// ---------------------------------------------------------------- Blowfish

/// Blowfish's starting P-array (18 words) and S-boxes (4 × 256 words).
fn blowfish_init() -> &'static (Vec<u32>, Vec<u32>) {
    static INIT: OnceLock<(Vec<u32>, Vec<u32>)> = OnceLock::new();
    INIT.get_or_init(|| {
        let w = pi_fraction_words(18 + 1024);
        (w[..18].to_vec(), w[18..].to_vec())
    })
}

pub struct Blowfish {
    p: [u32; 18],
    s: Vec<u32>, // 4 × 256
}

impl Blowfish {
    /// Key schedule for a key of 1–56 bytes.
    pub fn new(key: &[u8]) -> Option<Blowfish> {
        if key.is_empty() || key.len() > 56 {
            return None;
        }
        let (p0, s0) = blowfish_init();
        let mut bf = Blowfish { p: [0; 18], s: s0.clone() };
        bf.p.copy_from_slice(p0);
        let mut j = 0;
        for i in 0..18 {
            let mut d = 0u32;
            for _ in 0..4 {
                d = (d << 8) | key[j] as u32;
                j = (j + 1) % key.len();
            }
            bf.p[i] ^= d;
        }
        let (mut l, mut r) = (0u32, 0u32);
        for i in (0..18).step_by(2) {
            (l, r) = bf.encrypt_block(l, r);
            bf.p[i] = l;
            bf.p[i + 1] = r;
        }
        for i in (0..1024).step_by(2) {
            (l, r) = bf.encrypt_block(l, r);
            bf.s[i] = l;
            bf.s[i + 1] = r;
        }
        Some(bf)
    }

    fn f(&self, x: u32) -> u32 {
        let s = &self.s;
        let a = s[(x >> 24) as usize];
        let b = s[256 + ((x >> 16) & 0xff) as usize];
        let c = s[512 + ((x >> 8) & 0xff) as usize];
        let d = s[768 + (x & 0xff) as usize];
        (a.wrapping_add(b) ^ c).wrapping_add(d)
    }

    pub fn encrypt_block(&self, mut l: u32, mut r: u32) -> (u32, u32) {
        for i in 0..16 {
            l ^= self.p[i];
            r ^= self.f(l);
            std::mem::swap(&mut l, &mut r);
        }
        std::mem::swap(&mut l, &mut r);
        r ^= self.p[16];
        l ^= self.p[17];
        (l, r)
    }

    pub fn decrypt_block(&self, mut l: u32, mut r: u32) -> (u32, u32) {
        for i in (2..18).rev() {
            l ^= self.p[i];
            r ^= self.f(l);
            std::mem::swap(&mut l, &mut r);
        }
        std::mem::swap(&mut l, &mut r);
        r ^= self.p[1];
        l ^= self.p[0];
        (l, r)
    }

    fn words(b: &[u8]) -> (u32, u32) {
        (u32::from_be_bytes([b[0], b[1], b[2], b[3]]), u32::from_be_bytes([b[4], b[5], b[6], b[7]]))
    }

    /// CBC decryption (big-endian words), length a multiple of 8.
    pub fn decrypt_cbc(&self, iv: [u8; 8], data: &[u8]) -> Vec<u8> {
        let mut out = Vec::with_capacity(data.len());
        let mut chain = iv;
        for blk in data.chunks_exact(8) {
            let (l, r) = Self::words(blk);
            let (l, r) = self.decrypt_block(l, r);
            let mut p = [0u8; 8];
            p[..4].copy_from_slice(&l.to_be_bytes());
            p[4..].copy_from_slice(&r.to_be_bytes());
            for k in 0..8 {
                out.push(p[k] ^ chain[k]);
            }
            chain.copy_from_slice(blk);
        }
        out
    }

    /// CBC encryption (big-endian words), length a multiple of 8.
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn encrypt_cbc(&self, iv: [u8; 8], data: &[u8]) -> Vec<u8> {
        let mut out = Vec::with_capacity(data.len());
        let mut chain = iv;
        for blk in data.chunks_exact(8) {
            let mut x = [0u8; 8];
            for k in 0..8 {
                x[k] = blk[k] ^ chain[k];
            }
            let (l, r) = Self::words(&x);
            let (l, r) = self.encrypt_block(l, r);
            chain[..4].copy_from_slice(&l.to_be_bytes());
            chain[4..].copy_from_slice(&r.to_be_bytes());
            out.extend_from_slice(&chain);
        }
        out
    }
}

// ---------------------------------------------------------------- AES-128

struct AesTables {
    sbox: [u8; 256],
    inv: [u8; 256],
}

fn gmul(mut a: u8, mut b: u8) -> u8 {
    let mut p = 0u8;
    while b != 0 {
        if b & 1 != 0 {
            p ^= a;
        }
        let hi = a & 0x80;
        a <<= 1;
        if hi != 0 {
            a ^= 0x1b;
        }
        b >>= 1;
    }
    p
}

fn aes_tables() -> &'static AesTables {
    static T: OnceLock<AesTables> = OnceLock::new();
    T.get_or_init(|| {
        let mut sbox = [0u8; 256];
        let mut inv = [0u8; 256];
        for x in 0..256usize {
            // Multiplicative inverse in GF(2^8) (x^254), then the affine map.
            let mut y: u8 = if x == 0 { 0 } else { 1 };
            if x != 0 {
                for _ in 0..254 {
                    y = gmul(y, x as u8);
                }
            }
            let s = y ^ y.rotate_left(1) ^ y.rotate_left(2) ^ y.rotate_left(3) ^ y.rotate_left(4) ^ 0x63;
            sbox[x] = s;
            inv[s as usize] = x as u8;
        }
        AesTables { sbox, inv }
    })
}

pub struct Aes128 {
    rk: [[u8; 16]; 11],
}

impl Aes128 {
    pub fn new(key: &[u8; 16]) -> Aes128 {
        let t = aes_tables();
        let mut w = [[0u8; 4]; 44];
        for i in 0..4 {
            w[i].copy_from_slice(&key[4 * i..4 * i + 4]);
        }
        let mut rcon = 1u8;
        for i in 4..44 {
            let mut tmp = w[i - 1];
            if i % 4 == 0 {
                tmp = [t.sbox[tmp[1] as usize] ^ rcon, t.sbox[tmp[2] as usize], t.sbox[tmp[3] as usize], t.sbox[tmp[0] as usize]];
                rcon = gmul(rcon, 2);
            }
            for k in 0..4 {
                w[i][k] = w[i - 4][k] ^ tmp[k];
            }
        }
        let mut rk = [[0u8; 16]; 11];
        for r in 0..11 {
            for c in 0..4 {
                rk[r][4 * c..4 * c + 4].copy_from_slice(&w[4 * r + c]);
            }
        }
        Aes128 { rk }
    }

    fn add(s: &mut [u8; 16], k: &[u8; 16]) {
        for i in 0..16 {
            s[i] ^= k[i];
        }
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn encrypt_block(&self, b: &[u8; 16]) -> [u8; 16] {
        let t = aes_tables();
        let mut s = *b;
        Self::add(&mut s, &self.rk[0]);
        for r in 1..=10 {
            for x in s.iter_mut() {
                *x = t.sbox[*x as usize];
            }
            // ShiftRows: byte i is row i%4, column i/4.
            let o = s;
            for c in 0..4 {
                for row in 0..4 {
                    s[4 * c + row] = o[4 * ((c + row) % 4) + row];
                }
            }
            if r != 10 {
                for c in 0..4 {
                    let a = [s[4 * c], s[4 * c + 1], s[4 * c + 2], s[4 * c + 3]];
                    s[4 * c] = gmul(a[0], 2) ^ gmul(a[1], 3) ^ a[2] ^ a[3];
                    s[4 * c + 1] = a[0] ^ gmul(a[1], 2) ^ gmul(a[2], 3) ^ a[3];
                    s[4 * c + 2] = a[0] ^ a[1] ^ gmul(a[2], 2) ^ gmul(a[3], 3);
                    s[4 * c + 3] = gmul(a[0], 3) ^ a[1] ^ a[2] ^ gmul(a[3], 2);
                }
            }
            Self::add(&mut s, &self.rk[r]);
        }
        s
    }

    pub fn decrypt_block(&self, b: &[u8; 16]) -> [u8; 16] {
        let t = aes_tables();
        let mut s = *b;
        Self::add(&mut s, &self.rk[10]);
        for r in (0..10).rev() {
            let o = s;
            for c in 0..4 {
                for row in 0..4 {
                    s[4 * ((c + row) % 4) + row] = o[4 * c + row];
                }
            }
            for x in s.iter_mut() {
                *x = t.inv[*x as usize];
            }
            Self::add(&mut s, &self.rk[r]);
            if r != 0 {
                for c in 0..4 {
                    let a = [s[4 * c], s[4 * c + 1], s[4 * c + 2], s[4 * c + 3]];
                    s[4 * c] = gmul(a[0], 14) ^ gmul(a[1], 11) ^ gmul(a[2], 13) ^ gmul(a[3], 9);
                    s[4 * c + 1] = gmul(a[0], 9) ^ gmul(a[1], 14) ^ gmul(a[2], 11) ^ gmul(a[3], 13);
                    s[4 * c + 2] = gmul(a[0], 13) ^ gmul(a[1], 9) ^ gmul(a[2], 14) ^ gmul(a[3], 11);
                    s[4 * c + 3] = gmul(a[0], 11) ^ gmul(a[1], 13) ^ gmul(a[2], 9) ^ gmul(a[3], 14);
                }
            }
        }
        s
    }

    /// CBC decryption without removing padding; length a multiple of 16.
    pub fn decrypt_cbc(&self, iv: &[u8; 16], data: &[u8]) -> Vec<u8> {
        let mut out = Vec::with_capacity(data.len());
        let mut chain = *iv;
        for blk in data.chunks_exact(16) {
            let c: [u8; 16] = blk.try_into().unwrap();
            let p = self.decrypt_block(&c);
            for k in 0..16 {
                out.push(p[k] ^ chain[k]);
            }
            chain = c;
        }
        out
    }

    /// CBC encryption with PKCS#7 padding.
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn encrypt_cbc_padded(&self, iv: &[u8; 16], data: &[u8]) -> Vec<u8> {
        let pad = 16 - data.len() % 16;
        let mut buf = data.to_vec();
        buf.extend(std::iter::repeat_n(pad as u8, pad));
        let mut out = Vec::with_capacity(buf.len());
        let mut chain = *iv;
        for blk in buf.chunks_exact(16) {
            let mut x = [0u8; 16];
            for k in 0..16 {
                x[k] = blk[k] ^ chain[k];
            }
            chain = self.encrypt_block(&x);
            out.extend_from_slice(&chain);
        }
        out
    }
}

/// Removes PKCS#7 padding; None when the padding is not valid.
pub fn unpad(mut v: Vec<u8>) -> Option<Vec<u8>> {
    let n = *v.last()? as usize;
    if n == 0 || n > 16 || n > v.len() || !v[v.len() - n..].iter().all(|&b| b as usize == n) {
        return None;
    }
    v.truncate(v.len() - n);
    Some(v)
}

// ---------------------------------------------------------------- hex / base64

pub fn from_hex(s: &str) -> Option<Vec<u8>> {
    let s = s.trim();
    if s.len() % 2 != 0 {
        return None;
    }
    (0..s.len()).step_by(2).map(|i| u8::from_str_radix(s.get(i..i + 2)?, 16).ok()).collect()
}

pub fn from_base64(s: &[u8]) -> Option<Vec<u8>> {
    let val = |c: u8| -> Option<u32> {
        Some(match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            _ => return None,
        } as u32)
    };
    let chars: Vec<u8> = s.iter().copied().filter(|c| !c.is_ascii_whitespace()).collect();
    let body: Vec<u8> = chars.iter().copied().take_while(|&c| c != b'=').collect();
    if chars[body.len()..].iter().any(|&c| c != b'=') {
        return None;
    }
    let mut out = Vec::with_capacity(body.len() * 3 / 4);
    let mut acc = 0u32;
    let mut bits = 0;
    for c in body {
        acc = (acc << 6) | val(c)?;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
            acc &= (1 << bits) - 1;
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pi_words_start_like_blowfish_tables() {
        let w = pi_fraction_words(4);
        assert_eq!(w, vec![0x243F6A88, 0x85A308D3, 0x13198A2E, 0x03707344]);
        let (p, s) = blowfish_init();
        assert_eq!(p[17], 0x8979FB1B);
        assert_eq!(s[0], 0xD1310BA6);
        assert_eq!(s[1023], 0x3AC372E6);
    }

    #[test]
    fn blowfish_known_answers() {
        // Published Blowfish test vectors (Eric Young's set).
        let bf = Blowfish::new(&[0u8; 8]).unwrap();
        assert_eq!(bf.encrypt_block(0, 0), (0x4EF99745, 0x6198DD78));
        let bf = Blowfish::new(&[0xFF; 8]).unwrap();
        assert_eq!(bf.encrypt_block(0xFFFFFFFF, 0xFFFFFFFF), (0x51866FD5, 0xB85ECB8A));
        assert_eq!(bf.decrypt_block(0x51866FD5, 0xB85ECB8A), (0xFFFFFFFF, 0xFFFFFFFF));
    }

    #[test]
    fn aes_known_answer() {
        // FIPS-197 appendix C.1.
        let key: [u8; 16] = std::array::from_fn(|i| i as u8);
        let pt: [u8; 16] = std::array::from_fn(|i| (i as u8) * 0x11);
        let a = Aes128::new(&key);
        let ct = a.encrypt_block(&pt);
        assert_eq!(ct.to_vec(), from_hex("69c4e0d86a7b0430d8cdb78070b4c55a").unwrap());
        assert_eq!(a.decrypt_block(&ct), pt);
    }

    #[test]
    fn base64_and_hex() {
        assert_eq!(from_base64(b"aGVs\nbG8=").unwrap(), b"hello");
        assert_eq!(from_hex("00ff10").unwrap(), vec![0, 255, 16]);
        assert!(from_hex("0g").is_none());
    }
}
