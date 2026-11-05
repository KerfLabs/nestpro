//! How a DWG file is laid out around its objects: where the object data, the handle
//! map and the class list are, and unpacking them (DW2–DW4).
//!
//! - R13–R2000: the file is the object data; a locator table at the start says where
//!   the classes and the handle map are.
//! - R2004, R2010, R2013, R2018: the file is a set of compressed pages; a page map and a
//!   section map (both compressed) say which pages make up each named section.
//! - R2007: the same idea with a different page coding (see `r2007`).

use super::bits::{cp1252, Bits, Ver};
use std::collections::HashMap;

/// A custom class (object types 500 and up).
#[derive(Debug, Clone, Default)]
pub struct Class {
    pub num: u16,
    pub dxf_name: String,
    pub is_entity: bool,
}

/// The parts of a file the object reader needs.
pub struct Layout {
    /// Bytes the object map offsets point into.
    pub objects: Vec<u8>,
    /// (handle, byte offset in `objects`), in file order.
    pub map: Vec<(u64, usize)>,
    pub classes: Vec<Class>,
}

pub fn version(bytes: &[u8]) -> Result<(Ver, &'static str), String> {
    let id = bytes.get(..6).ok_or("This file is too short to be a DWG drawing.")?;
    Ok(match id {
        b"AC1012" => (Ver::R13, "R13"),
        b"AC1014" => (Ver::R14, "R14"),
        b"AC1015" => (Ver::R2000, "2000"),
        b"AC1018" => (Ver::R2004, "2004"),
        b"AC1021" => (Ver::R2007, "2007"),
        b"AC1024" => (Ver::R2010, "2010"),
        b"AC1027" => (Ver::R2013, "2013"),
        b"AC1032" => (Ver::R2018, "2018"),
        _ => {
            let s = String::from_utf8_lossy(id).to_string();
            if s.starts_with("AC") || s.starts_with("MC0.0") {
                return Err(format!(
                    "This DWG ({s}) is from AutoCAD R12 or older, which cannot be read. Open it in a CAD program and save it as DXF."
                ));
            }
            return Err("This is not a DWG drawing (it does not start with an AutoCAD version mark).".into());
        }
    })
}

fn u32_at(b: &[u8], at: usize) -> Result<u32, String> {
    b.get(at..at + 4).map(|s| u32::from_le_bytes([s[0], s[1], s[2], s[3]])).ok_or_else(|| cut_short())
}

fn u64_at(b: &[u8], at: usize) -> Result<u64, String> {
    b.get(at..at + 8).map(|s| u64::from_le_bytes(s.try_into().unwrap_or([0; 8]))).ok_or_else(|| cut_short())
}

pub fn cut_short() -> String {
    "The DWG file is damaged or cut short (a part of it points outside the file).".into()
}

// ---------------------------------------------------------------- R13–R2000

const CLASS_SENTINEL: [u8; 16] = [0x8D, 0xA1, 0xC4, 0xB8, 0xC4, 0xA9, 0xF8, 0xC5, 0xC0, 0xDC, 0xF4, 0x5F, 0xE7, 0xCF, 0xB6, 0x8A];

pub fn layout_r13(bytes: &[u8], ver: Ver) -> Result<Layout, String> {
    let n = u32_at(bytes, 0x15)? as usize;
    if n > 16 {
        return Err(cut_short());
    }
    let mut rec: HashMap<u8, (usize, usize)> = HashMap::new();
    for i in 0..n {
        let at = 0x19 + i * 9;
        let id = *bytes.get(at).ok_or_else(cut_short)?;
        rec.insert(id, (u32_at(bytes, at + 1)? as usize, u32_at(bytes, at + 5)? as usize));
    }
    let (cls_at, cls_len) = *rec.get(&1).ok_or("The DWG file has no class list.")?;
    let (map_at, map_len) = *rec.get(&2).ok_or("The DWG file has no object map.")?;
    let classes = classes_r13(bytes, cls_at, cls_len, ver).unwrap_or_default();
    let map = object_map(bytes.get(map_at..map_at.saturating_add(map_len).min(bytes.len())).ok_or_else(cut_short)?, 0);
    Ok(Layout { objects: bytes.to_vec(), map, classes })
}

fn classes_r13(bytes: &[u8], at: usize, _len: usize, ver: Ver) -> Option<Vec<Class>> {
    if bytes.get(at..at + 16)? != CLASS_SENTINEL {
        return None;
    }
    let size = u32_at(bytes, at + 16).ok()? as usize;
    let start = at + 20;
    let end = start.checked_add(size)?;
    if end > bytes.len() {
        return None;
    }
    let mut r = Bits::at(bytes, start, ver);
    r.end = end * 8;
    let mut out = vec![];
    while r.left() > 8 * 8 {
        let num = r.bs().ok()?;
        r.bs().ok()?;
        r.t8().ok()?;
        r.t8().ok()?;
        let dxf_name = r.t8().ok()?;
        r.b().ok()?;
        let item = r.bs().ok()?;
        out.push(Class { num, dxf_name, is_entity: item == 0x1f2 });
    }
    Some(out)
}

/// The handle → offset map: blocks of (handle step, offset step) pairs (DW3).
pub fn object_map(data: &[u8], _base: usize) -> Vec<(u64, usize)> {
    let mut out = vec![];
    let mut pos = 0usize;
    while pos + 2 <= data.len() {
        let size = u16::from_be_bytes([data[pos], data[pos + 1]]) as usize;
        if size <= 2 {
            break;
        }
        let end = (pos + size).min(data.len());
        // Each block starts again from handle 0 and offset 0.
        let mut handle: u64 = 0;
        let mut loc: i64 = 0;
        let mut r = Bits::at(data, pos + 2, Ver::R2000);
        r.end = end * 8;
        while r.left() >= 8 {
            let (Ok(dh), Ok(dl)) = (r.mcu(), r.mc()) else { break };
            handle = handle.wrapping_add(dh);
            loc += dl;
            if loc >= 0 {
                out.push((handle, loc as usize));
            }
        }
        pos = end + 2; // skip the block's CRC
    }
    out
}

// ---------------------------------------------------------------- R2004+

/// Fixed byte sequence the R2004 file header is masked with.
fn magic_seq(n: usize) -> Vec<u8> {
    let mut seed: u32 = 1;
    (0..n)
        .map(|_| {
            seed = seed.wrapping_mul(0x343fd).wrapping_add(0x269ec3);
            (seed >> 16) as u8
        })
        .collect()
}

/// Unpack R2004-style compressed data (DW4).
pub fn decompress_r18(src: &[u8], out_size: usize) -> Result<Vec<u8>, String> {
    let bad = || "A compressed part of the DWG file is damaged.".to_string();
    let mut out: Vec<u8> = Vec::with_capacity(out_size);
    let mut i = 0usize;
    let byte = |i: &mut usize| -> Result<u8, String> {
        let b = *src.get(*i).ok_or_else(bad)?;
        *i += 1;
        Ok(b)
    };
    let lit_len = |i: &mut usize| -> Result<usize, String> {
        let b = *src.get(*i).ok_or_else(bad)?;
        if b & 0xf0 != 0 {
            return Ok(0);
        }
        *i += 1;
        if b != 0 {
            return Ok(b as usize + 3);
        }
        let mut total = 0x0f;
        loop {
            let b = *src.get(*i).ok_or_else(bad)?;
            *i += 1;
            if b == 0 {
                total += 0xff;
            } else {
                return Ok(total + b as usize + 3);
            }
        }
    };
    let long_len = |i: &mut usize| -> Result<usize, String> {
        let b = *src.get(*i).ok_or_else(bad)?;
        *i += 1;
        if b != 0 {
            return Ok(b as usize);
        }
        let mut total = 0xff;
        loop {
            let b = *src.get(*i).ok_or_else(bad)?;
            *i += 1;
            if b == 0 {
                total += 0xff;
            } else {
                return Ok(total + b as usize);
            }
        }
    };
    let two_byte = |i: &mut usize| -> Result<(usize, usize), String> {
        let a = *src.get(*i).ok_or_else(bad)? as usize;
        let b = *src.get(*i + 1).ok_or_else(bad)? as usize;
        *i += 2;
        Ok(((a >> 2) | (b << 6), a & 3))
    };
    let copy_lit = |out: &mut Vec<u8>, i: &mut usize, n: usize| -> Result<(), String> {
        let s = src.get(*i..*i + n).ok_or_else(bad)?;
        out.extend_from_slice(s);
        *i += n;
        Ok(())
    };
    let n = lit_len(&mut i)?;
    if n > out_size {
        return Err(bad());
    }
    copy_lit(&mut out, &mut i, n)?;
    loop {
        if out.len() > out_size {
            return Err(bad());
        }
        let op = match src.get(i) {
            Some(&b) => b,
            None => break,
        };
        i += 1;
        let (count, off, mut lit) = match op {
            0x10 => {
                let c = long_len(&mut i)? + 9;
                let (o, l) = two_byte(&mut i)?;
                (c, o + 0x3fff, l)
            }
            0x11 => break,
            0x12..=0x1f => {
                let (o, l) = two_byte(&mut i)?;
                ((op & 0x0f) as usize + 2, o + 0x3fff, l)
            }
            0x20 => {
                let c = long_len(&mut i)? + 0x21;
                let (o, l) = two_byte(&mut i)?;
                (c, o, l)
            }
            0x21..=0x3f => {
                let (o, l) = two_byte(&mut i)?;
                (op as usize - 0x1e, o, l)
            }
            0x40..=0xff => {
                let op2 = byte(&mut i)? as usize;
                ((op as usize >> 4) - 1, (op2 << 2) | ((op as usize & 0x0c) >> 2), (op & 3) as usize)
            }
            _ => return Err(bad()),
        };
        if lit == 0 {
            lit = lit_len(&mut i)?;
        }
        // DW15: the copy and the literal must fit in the page before anything is
        // copied, so a damaged length can't grow the output first.
        if out.len().saturating_add(count).saturating_add(lit) > out_size {
            return Err(bad());
        }
        let from = out.len().checked_sub(off + 1).ok_or_else(bad)?;
        for k in 0..count {
            let b = out[from + k];
            out.push(b);
        }
        copy_lit(&mut out, &mut i, lit)?;
    }
    out.resize(out_size, 0);
    Ok(out)
}

struct SectionInfo {
    size: u64,
    max_page: usize,
    compressed: bool,
    encrypted: u32,
    name: String,
    /// (page number, start offset)
    pages: Vec<(i32, u64)>,
}

/// Read the named sections of an R2004/2010/2013/2018 file (DW2).
pub fn sections_r2004(bytes: &[u8]) -> Result<HashMap<String, Vec<u8>>, String> {
    let enc = bytes.get(0x80..0x80 + 0x6c).ok_or_else(cut_short)?;
    let hdr: Vec<u8> = enc.iter().zip(magic_seq(0x6c)).map(|(a, b)| a ^ b).collect();
    if &hdr[..11] != b"AcFssFcAJMB" {
        return Err(cut_short());
    }
    let security = u32_at(bytes, 0x18)?;
    if security & 1 != 0 {
        return Err("This DWG is password-protected (encrypted). Open it in AutoCAD and save it without a password, or save it as DXF.".into());
    }
    let page_map_id = u32_at(&hdr, 0x50)?;
    let page_map_addr = u64_at(&hdr, 0x54)? as usize + 0x100;
    let section_map_id = u32_at(&hdr, 0x5c)? as i32;
    let _ = page_map_id;

    // Page map: (page number, size) pairs; pages follow each other from 0x100.
    let pm = system_page(bytes, page_map_addr, 0x41630e3b)?;
    let mut page_addr: HashMap<i32, usize> = HashMap::new();
    let mut addr = 0x100usize;
    let mut k = 0;
    while k + 8 <= pm.len() {
        let num = u32_at(&pm, k)? as i32;
        let size = u32_at(&pm, k + 4)? as usize;
        k += 8;
        if num < 0 {
            k += 16;
        } else {
            page_addr.insert(num, addr);
        }
        addr = addr.checked_add(size).ok_or_else(cut_short)?;
    }

    // Section map.
    let sm_at = *page_addr.get(&section_map_id).ok_or_else(cut_short)?;
    let sm = system_page(bytes, sm_at, 0x4163003b)?;
    let count = u32_at(&sm, 0)? as usize;
    let mut k = 0x14;
    let mut infos = vec![];
    for _ in 0..count.min(1000) {
        let size = u64_at(&sm, k)?;
        let npages = u32_at(&sm, k + 8)? as usize;
        let max_page = u32_at(&sm, k + 0x0c)? as usize;
        let compressed = u32_at(&sm, k + 0x14)? == 2;
        let encrypted = u32_at(&sm, k + 0x1c)?;
        let name_raw = sm.get(k + 0x20..k + 0x60).ok_or_else(cut_short)?;
        let name = cp1252(name_raw);
        k += 0x60;
        let mut pages = vec![];
        for _ in 0..npages.min(100_000) {
            let num = u32_at(&sm, k)? as i32;
            let start = u64_at(&sm, k + 8)?;
            pages.push((num, start));
            k += 16;
        }
        infos.push(SectionInfo { size, max_page, compressed, encrypted, name, pages });
    }

    let mut out = HashMap::new();
    for s in infos {
        if !matches!(s.name.as_str(), "AcDb:AcDbObjects" | "AcDb:Handles" | "AcDb:Classes" | "AcDb:Header") {
            continue;
        }
        if s.encrypted == 1 {
            return Err("This DWG's data is encrypted. Save it without a password, or save it as DXF.".into());
        }
        let total = usize::try_from(s.size).map_err(|_| cut_short())?;
        if total > 1 << 30 || s.max_page == 0 || s.max_page > 1 << 24 {
            return Err(cut_short());
        }
        // The pages are unpacked first, and the section is no bigger than what they
        // really fill without gaps (a damaged size or offset can't claim memory on
        // its own; unpacking itself is bounded by the packed bytes).
        let mut unpacked: Vec<(usize, Vec<u8>)> = vec![];
        for &(num, start) in &s.pages {
            let at = *page_addr.get(&num).ok_or_else(cut_short)?;
            let raw = bytes.get(at..at + 32).ok_or_else(cut_short)?;
            let mask = 0x4164536bu32 ^ (at as u32);
            let h: Vec<u32> = (0..8).map(|j| u32::from_le_bytes(raw[j * 4..j * 4 + 4].try_into().unwrap_or([0; 4])) ^ mask).collect();
            let comp_size = h[2] as usize;
            let body = bytes.get(at + 32..(at + 32).saturating_add(comp_size)).ok_or_else(cut_short)?;
            let page = if s.compressed { decompress_r18(body, s.max_page)? } else { body.to_vec() };
            unpacked.push((usize::try_from(start).map_err(|_| cut_short())?, page));
        }
        let filled: usize = unpacked.iter().map(|p| p.1.len()).sum();
        let end = unpacked.iter().map(|p| p.0.saturating_add(p.1.len())).max().unwrap_or(0);
        if end > filled {
            return Err(cut_short());
        }
        let total = total.min(end);
        let mut data = vec![0u8; total];
        for (start, page) in unpacked {
            if start >= total {
                continue;
            }
            let n = page.len().min(total - start);
            data[start..start + n].copy_from_slice(&page[..n]);
        }
        out.insert(s.name, data);
    }
    Ok(out)
}

fn system_page(bytes: &[u8], at: usize, kind: u32) -> Result<Vec<u8>, String> {
    if u32_at(bytes, at)? != kind {
        return Err(cut_short());
    }
    let size = u32_at(bytes, at + 4)? as usize;
    let comp = u32_at(bytes, at + 8)? as usize;
    // A page map or section map is small; its unpacked size can't be far beyond
    // what its packed bytes can make (DW15).
    if size > 1 << 28 || size > comp.saturating_mul(512).saturating_add(4096) {
        return Err(cut_short());
    }
    let body = bytes.get(at + 0x14..(at + 0x14).saturating_add(comp)).ok_or_else(cut_short)?;
    if u32_at(bytes, at + 0x0c)? == 2 {
        decompress_r18(body, size)
    } else {
        Ok(body.to_vec())
    }
}

pub fn layout_r2004(bytes: &[u8], ver: Ver, sections: HashMap<String, Vec<u8>>) -> Result<Layout, String> {
    let mut sections = sections;
    let objects = sections.remove("AcDb:AcDbObjects").ok_or("The DWG file has no objects section.")?;
    let handles = sections.remove("AcDb:Handles").ok_or("The DWG file has no object map.")?;
    let maint = bytes.get(0x0b).copied().unwrap_or(0);
    let classes = sections.get("AcDb:Classes").and_then(|c| classes_r2004(c, ver, maint)).unwrap_or_default();
    let map = object_map(&handles, 0);
    Ok(Layout { objects, map, classes })
}

/// Class list of R2004+ (strings in a separate stream from R2007). The few header
/// fields before the list differ between versions, so each known layout is tried and
/// the one that reads cleanly is kept.
pub fn classes_r2004(c: &[u8], ver: Ver, maint: u8) -> Option<Vec<Class>> {
    if c.get(..16)? != CLASS_SENTINEL {
        return None;
    }
    let size = u32_at(c, 16).ok()? as usize;
    let mut best: Vec<Class> = vec![];
    let starts: &[usize] = if ver >= Ver::R2010 && maint > 3 { &[24, 20] } else { &[20, 24] };
    for &start in starts {
        for two_bytes in [true, false] {
            for stream_from_rl in [true, false] {
                let list = classes_try(c, ver, start, size, two_bytes, stream_from_rl);
                if list.len() > best.len() {
                    best = list;
                }
            }
        }
    }
    Some(best)
}

fn classes_try(c: &[u8], ver: Ver, start: usize, size: usize, two_bytes: bool, stream_from_rl: bool) -> Vec<Class> {
    let mut out = vec![];
    let Some(data_end) = start.checked_add(size) else { return out };
    let mut r = Bits::at(c, start, ver);
    r.end = data_end.min(c.len()) * 8;
    let mut strs = None;
    if ver >= Ver::R2007 {
        let Ok(bitsize) = r.rl() else { return out };
        let endbit = start * 8 + bitsize as usize + if stream_from_rl { 32 } else { 0 };
        strs = super::objects::string_stream(c, endbit.saturating_sub(1), ver);
        if strs.is_none() {
            return out;
        }
        r.end = r.end.min(endbit);
    } else if !stream_from_rl {
        return out;
    }
    let Ok(max) = r.bs() else { return out };
    if two_bytes && (r.rc().is_err() || r.rc().is_err()) {
        return out;
    }
    if r.b().is_err() {
        return out;
    }
    while out.len() < 5000 {
        let Ok(num) = r.bs() else { break };
        if num > max || num < 500 {
            break;
        }
        let ok = (|| {
            r.bs().ok()?;
            super::bits::tv(&mut r, &mut strs).ok()?;
            super::bits::tv(&mut r, &mut strs).ok()?;
            let dxf_name = super::bits::tv(&mut r, &mut strs).ok()?;
            r.b().ok()?;
            let item = r.bs().ok()?;
            r.bl().ok()?;
            r.bs().ok()?;
            r.bs().ok()?;
            r.bl().ok()?;
            r.bl().ok()?;
            Some((dxf_name, item))
        })();
        let Some((dxf_name, item)) = ok else { break };
        if dxf_name.is_empty() || !dxf_name.chars().all(|ch| ch.is_ascii_graphic()) {
            break;
        }
        out.push(Class { num, dxf_name, is_entity: item == 0x1f2 });
        if num == max {
            break;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_long_copy_beyond_the_page_is_refused_before_copying() {
        // 4 literal bytes, then a back-reference whose length runs on for
        // 100 000 extension bytes, into a 4-byte page: refused at once.
        let mut src = vec![0x01, 1, 2, 3, 4, 0x20];
        src.extend(std::iter::repeat_n(0u8, 100_000));
        src.extend([0x01, 0x00, 0x00, 0x11]);
        assert!(decompress_r18(&src, 4).is_err());
    }

    #[test]
    fn r18_unpacks_literals_and_back_references() {
        // Literal length 1+3 = "abcd"; op 0x51 copies 4 bytes from 1 back and then
        // 1 literal byte; 0x11 ends the stream.
        let src = [0x01, b'a', b'b', b'c', b'd', 0x51, 0x00, b'x', 0x11];
        let out = decompress_r18(&src, 9).unwrap();
        assert_eq!(&out, b"abcdddddx");
    }

    #[test]
    fn r18_long_back_reference() {
        // literal "abc", then op 0x21+1=0x22 → count 4, two-byte offset 2 (copies from 3 back).
        let src = [0x00, 0x00, 0x01, b'a', b'b', b'c'];
        // 0x00 0x00 0x01: running total 0x0f + 0xff + 1 + 3 = 0x112 literal bytes — too many;
        // so use the short form instead.
        let _ = src;
        let src = [0x00, 0x01, b'a', b'b', b'c', b'd', b'e', b'f', b'g', b'h', b'i', b'j', b'k', b'l', b'm', b'n', b'o', b'p', b'q', b'r', b's', 0x22, 0x08, 0x00, 0x11];
        // literal length 0x0f+1+3 = 19; op 0x22 count 4, offset (0x08>>2)=2 → from 3 back.
        let out = decompress_r18(&src, 23).unwrap();
        assert_eq!(&out[..19], b"abcdefghijklmnopqrs");
        assert_eq!(&out[19..], b"qrsq");
    }

    #[test]
    fn damaged_compressed_data_is_an_error() {
        assert!(decompress_r18(&[0x01, b'a'], 10).is_err());
        assert!(decompress_r18(&[0x01, b'a', b'b', b'c', b'd', 0x40, 0xff], 10).is_err());
    }

    #[test]
    fn object_map_blocks() {
        // One block: handle +1 at offset 100, handle +2 at offset 100-20.
        let body = [0x01u8, 0xe4, 0x00, 0x02, 0x54];
        let mut data = vec![0u8, (body.len() + 2) as u8];
        data.extend_from_slice(&body);
        data.extend_from_slice(&[0, 0, 0, 2]); // CRC, then the empty last block
        assert_eq!(object_map(&data, 0), vec![(1, 100), (3, 80)]);
    }

    #[test]
    fn version_marks() {
        assert_eq!(version(b"AC1014xx").unwrap().0, Ver::R14);
        assert_eq!(version(b"AC1032").unwrap().1, "2018");
        assert!(version(b"AC1009").unwrap_err().contains("R12"));
        assert!(version(b"hello!").unwrap_err().contains("not a DWG"));
        assert!(version(b"AC").is_err());
    }
}
