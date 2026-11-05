//! R2007 (AC1021) file layout (DW2, DW4): pages protected by an error-correcting code
//! (we read the data bytes and drop the check bytes) and a second kind of compression.

use super::container::cut_short;
use std::collections::HashMap;

fn u64_at(b: &[u8], at: usize) -> Result<u64, String> {
    b.get(at..at + 8).map(|s| u64::from_le_bytes(s.try_into().unwrap_or([0; 8]))).ok_or_else(cut_short)
}

/// Undo the interleaving of `blocks` code words of 255 bytes, keeping `k` data bytes of each.
fn deinterleave(src: &[u8], blocks: usize, k: usize) -> Vec<u8> {
    let mut out = Vec::with_capacity(blocks * k);
    for b in 0..blocks {
        for j in 0..k {
            out.push(src.get(j * blocks + b).copied().unwrap_or(0));
        }
    }
    out
}

/// Copy `n` literal bytes: the compressor stores them in a shuffled order.
fn copy_literal(dst: &mut Vec<u8>, src: &[u8], n: usize) {
    // Byte order of a literal run of 1..=31 bytes: (size, offset) pieces in output order.
    const T: [&[(u8, u8)]; 32] = [
        &[],
        &[(1, 0)],
        &[(1, 1), (1, 0)],
        &[(1, 2), (1, 1), (1, 0)],
        &[(1, 0), (1, 1), (1, 2), (1, 3)],
        &[(1, 4), (4, 0)],
        &[(1, 5), (4, 1), (1, 0)],
        &[(2, 5), (4, 1), (1, 0)],
        &[(4, 0), (4, 4)],
        &[(1, 8), (8, 0)],
        &[(1, 9), (8, 1), (1, 0)],
        &[(2, 9), (8, 1), (1, 0)],
        &[(4, 8), (8, 0)],
        &[(1, 12), (4, 8), (8, 0)],
        &[(1, 13), (4, 9), (8, 1), (1, 0)],
        &[(2, 13), (4, 9), (8, 1), (1, 0)],
        &[(8, 8), (8, 0)],
        &[(8, 9), (1, 8), (8, 0)],
        &[(1, 17), (16, 1), (1, 0)],
        &[(3, 16), (16, 0)],
        &[(4, 16), (16, 0)],
        &[(1, 20), (4, 16), (16, 0)],
        &[(2, 20), (4, 16), (16, 0)],
        &[(3, 20), (4, 16), (16, 0)],
        &[(8, 16), (16, 0)],
        &[(8, 17), (1, 16), (16, 0)],
        &[(1, 25), (8, 17), (1, 16), (16, 0)],
        &[(2, 25), (8, 17), (1, 16), (16, 0)],
        &[(4, 24), (8, 16), (16, 0)],
        &[(1, 28), (4, 24), (8, 16), (16, 0)],
        &[(2, 28), (4, 24), (8, 16), (16, 0)],
        &[(1, 30), (4, 26), (8, 18), (16, 2), (2, 0)],
    ];
    fn piece(dst: &mut Vec<u8>, s: &[u8], size: usize) {
        match size {
            1 => dst.push(s[0]),
            2 => dst.extend_from_slice(&[s[1], s[0]]),
            3 => dst.extend_from_slice(&[s[2], s[1], s[0]]),
            16 => {
                dst.extend_from_slice(&s[8..16]);
                dst.extend_from_slice(&s[0..8]);
            }
            _ => dst.extend_from_slice(&s[..size]),
        }
    }
    let mut at = 0;
    let mut n = n;
    while n >= 32 {
        piece(dst, &src[at + 16..], 16);
        piece(dst, &src[at..], 16);
        at += 32;
        n -= 32;
    }
    for &(size, off) in T[n] {
        piece(dst, &src[at + off as usize..], size as usize);
    }
}

/// Unpack R2007-style compressed data.
pub fn decompress_r21(src: &[u8], out_size: usize) -> Result<Vec<u8>, String> {
    let bad = || "A compressed part of the DWG file is damaged.".to_string();
    let get = |i: usize| -> Result<usize, String> { src.get(i).map(|&b| b as usize).ok_or_else(bad) };
    let mut out: Vec<u8> = Vec::with_capacity(out_size);
    let end = src.len();
    let mut si = 0usize;
    let mut op = get(si)?;
    si += 1;
    let mut length = 0usize;
    if op & 0xf0 == 0x20 {
        si += 2;
        length = get(si)? & 7;
        si += 1;
    }
    let instr = |si: &mut usize, op: &mut usize| -> Result<(usize, usize), String> {
        let (off, len);
        match *op >> 4 {
            0 => {
                let l = (*op & 0xf) + 0x13;
                let o = get(*si)?;
                *op = get(*si + 1)?;
                *si += 2;
                len = l + ((*op >> 3) & 0x10);
                off = ((*op & 0x78) << 5) + 1 + o;
            }
            1 => {
                let l = (*op & 0xf) + 3;
                let o = get(*si)?;
                *op = get(*si + 1)?;
                *si += 2;
                len = l;
                off = ((*op & 0xf8) << 5) + 1 + o;
            }
            2 => {
                let mut o = get(*si)? | (get(*si + 1)? << 8);
                *si += 2;
                let mut l = *op & 7;
                if *op & 8 == 0 {
                    *op = get(*si)?;
                    *si += 1;
                    l += *op & 0xf8;
                } else {
                    o += 1;
                    l += get(*si)? << 3;
                    *op = get(*si + 1)?;
                    *si += 2;
                    l += ((*op & 0xf8) << 8) + 0x100;
                }
                len = l;
                off = o;
            }
            _ => {
                let l = *op >> 4;
                let o = *op & 15;
                *op = get(*si)?;
                *si += 1;
                len = l;
                off = ((*op & 0xf8) << 1) + o + 1;
            }
        }
        Ok((off, len))
    };
    while si < end {
        if length == 0 {
            length = op + 8;
            if length == 0x17 {
                let n = get(si)?;
                si += 1;
                length += n;
                if n == 0xff {
                    loop {
                        let n = get(si)? | (get(si + 1)? << 8);
                        si += 2;
                        length += n;
                        if n != 0xffff {
                            break;
                        }
                    }
                }
            }
        }
        if si + length > end || out.len() + length > out_size {
            return Err(bad());
        }
        copy_literal(&mut out, &src[si..si + length], length);
        si += length;
        if si >= end {
            break;
        }
        op = get(si)?;
        si += 1;
        let (mut off, mut len) = instr(&mut si, &mut op)?;
        loop {
            let from = out.len().checked_sub(off).ok_or_else(bad)?;
            if out.len() + len > out_size {
                return Err(bad());
            }
            for k in 0..len {
                let b = out[from + k];
                out.push(b);
            }
            length = op & 7;
            if length != 0 || si >= end {
                break;
            }
            op = get(si)?;
            si += 1;
            if op >> 4 == 0 {
                break;
            }
            if op >> 4 == 0x0f {
                op &= 0x0f;
            }
            (off, len) = instr(&mut si, &mut op)?;
        }
    }
    out.resize(out_size, 0);
    Ok(out)
}

/// A system page (page map, section map): repeated copies, coded in blocks of 239.
fn sys_page(bytes: &[u8], at: usize, comp: usize, uncomp: usize, factor: usize) -> Result<Vec<u8>, String> {
    if comp > 1 << 26 || uncomp > 1 << 26 || factor == 0 || factor > 64 {
        return Err(cut_short());
    }
    let pesize = ((comp + 7) & !7) * factor;
    let blocks = pesize.div_ceil(239);
    let size = (blocks * 255 + 7) & !7;
    // DW15: the whole coded page must be in the file — a damaged size can't make
    // the reader fill gigabytes with zeros.
    let raw = bytes.get(at..at.checked_add(size).ok_or_else(cut_short)?).ok_or_else(cut_short)?;
    let data = deinterleave(raw, blocks, 239);
    let body = data.get(..comp).ok_or_else(cut_short)?;
    if comp < uncomp {
        decompress_r21(body, uncomp)
    } else {
        Ok(body[..uncomp.min(body.len())].to_vec())
    }
}

/// Read the sections the object reader needs from an R2007 file.
pub fn sections(bytes: &[u8]) -> Result<HashMap<String, Vec<u8>>, String> {
    let security = bytes.get(0x18..0x1c).map(|s| u32::from_le_bytes(s.try_into().unwrap_or([0; 4]))).ok_or_else(cut_short)?;
    if security & 1 != 0 {
        return Err("This DWG is password-protected (encrypted). Open it in AutoCAD and save it without a password, or save it as DXF.".into());
    }
    let raw = bytes.get(0x80..0x80 + 0x3d8).ok_or_else(cut_short)?;
    let dec = deinterleave(raw, 3, 239);
    let comp_len = i32::from_le_bytes(dec[0x18..0x1c].try_into().unwrap_or([0; 4]));
    let body = &dec[0x20..];
    let hdr = if comp_len < 0 {
        body.get(..(-(comp_len as i64)) as usize).ok_or_else(cut_short)?.to_vec()
    } else {
        decompress_r21(body.get(..comp_len as usize).ok_or_else(cut_short)?, 0x110)?
    };
    let f = |o: usize| u64_at(&hdr, o).map(|v| v as usize);
    let pm = sys_page(bytes, f(0x38)?.saturating_add(0x480), f(0x50)?, f(0x58)?, f(0x18)?)?;
    let mut pages: HashMap<i64, (usize, usize)> = HashMap::new();
    let mut off = 0usize;
    let mut k = 0;
    while k + 16 <= pm.len() {
        let size = u64_at(&pm, k)? as usize;
        let id = u64_at(&pm, k + 8)? as i64;
        k += 16;
        if size == 0 && id == 0 {
            break;
        }
        pages.insert(id.abs(), (off + 0x480, size));
        off = off.checked_add(size).ok_or_else(cut_short)?;
    }
    let (sm_at, _) = *pages.get(&(f(0xc0)? as i64)).ok_or_else(cut_short)?;
    let sm = sys_page(bytes, sm_at, f(0xb0)?, f(0xc8)?, f(0xd8)?)?;

    let mut out = HashMap::new();
    let mut k = 0;
    while k + 64 <= sm.len() {
        let size = u64_at(&sm, k)? as usize;
        let max = u64_at(&sm, k + 8)? as usize;
        let encrypted = u64_at(&sm, k + 0x10)?;
        let name_len = u64_at(&sm, k + 0x20)? as usize;
        let encoding = u64_at(&sm, k + 0x30)?;
        let npages = u64_at(&sm, k + 0x38)? as usize;
        k += 0x40;
        if name_len > 512 || npages > 100_000 {
            return Err(cut_short());
        }
        let name_bytes = sm.get(k..k + name_len).ok_or_else(cut_short)?;
        let u: Vec<u16> = name_bytes.chunks(2).map(|c| u16::from_le_bytes([c[0], *c.get(1).unwrap_or(&0)])).take_while(|&c| c != 0).collect();
        let name = String::from_utf16_lossy(&u);
        k += name_len;
        let mut plist = vec![];
        for _ in 0..npages {
            let data_off = u64_at(&sm, k)? as usize;
            let id = u64_at(&sm, k + 0x10)? as i64;
            let usize_ = u64_at(&sm, k + 0x18)? as usize;
            let csize = u64_at(&sm, k + 0x20)? as usize;
            plist.push((data_off, id, usize_, csize));
            k += 0x38;
        }
        let wanted = matches!(name.as_str(), "AcDb:AcDbObjects" | "AcDb:Handles" | "AcDb:Classes" | "AcDb:Header");
        if !wanted {
            continue;
        }
        if encrypted == 1 {
            return Err("This DWG's data is encrypted. Save it without a password, or save it as DXF.".into());
        }
        if size > 1 << 30 || max > 1 << 24 {
            return Err(cut_short());
        }
        // Every page checked before anything is allocated: it must exist, be of a
        // sane size and lie wholly in the file; and the pages must fill the
        // section without gaps (a damaged offset can't claim memory on its own).
        let mut filled = 0usize;
        let mut end = 0usize;
        for &(doff, id, usz, csz) in &plist {
            let (at, _) = *pages.get(&id).ok_or_else(cut_short)?;
            if usz > 1 << 24 || csz > 1 << 24 || at.checked_add(((csz + 7) & !7).div_ceil(251) * 255).is_none_or(|e| e > bytes.len()) {
                return Err(cut_short());
            }
            filled = filled.saturating_add(usz);
            end = end.max(doff.saturating_add(usz));
        }
        if end > filled {
            return Err(cut_short());
        }
        let size = size.min(end);
        let mut data = vec![0u8; size];
        for (doff, id, usz, csz) in plist {
            let (at, _) = *pages.get(&id).ok_or_else(cut_short)?;
            if usz > 1 << 24 || csz > 1 << 24 {
                return Err(cut_short());
            }
            let blocks = ((csz + 7) & !7).div_ceil(251);
            let raw = bytes.get(at..at.checked_add(blocks * 255).ok_or_else(cut_short)?).ok_or_else(cut_short)?;
            let dec = if encoding == 4 {
                deinterleave(raw, blocks, 251)
            } else {
                raw.chunks(255).flat_map(|c| c.iter().take(251).copied()).collect()
            };
            let body = dec.get(..csz).ok_or_else(cut_short)?;
            let page = if csz < usz { decompress_r21(body, usz)? } else { body.to_vec() };
            if doff >= size {
                continue;
            }
            let n = page.len().min(size - doff);
            data[doff..doff + n].copy_from_slice(&page[..n]);
        }
        out.insert(name, data);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn literal_runs_are_unshuffled() {
        let src: Vec<u8> = (0..40).collect();
        let mut d = vec![];
        copy_literal(&mut d, &src, 2);
        assert_eq!(d, [1, 0]);
        let mut d = vec![];
        copy_literal(&mut d, &src, 8);
        assert_eq!(d, [0, 1, 2, 3, 4, 5, 6, 7]);
        let mut d = vec![];
        copy_literal(&mut d, &src, 16);
        assert_eq!(d, [8, 9, 10, 11, 12, 13, 14, 15, 0, 1, 2, 3, 4, 5, 6, 7]);
        // Every length 1..=40 uses each source byte exactly once.
        for n in 1..=40 {
            let mut d = vec![];
            copy_literal(&mut d, &src, n);
            let mut s = d.clone();
            s.sort();
            assert_eq!(s, (0..n as u8).collect::<Vec<_>>(), "length {n}");
        }
    }

    #[test]
    fn r21_literal_then_back_reference() {
        // First opcode 0x00: a literal of 8 bytes; then opcode 0x40 (default case:
        // length 4) with offset byte 0x00 → offset 1, and opcode&7 = 0 with the end of
        // input ends the stream.
        let src = [0x00, 0, 1, 2, 3, 4, 5, 6, 7, 0x40, 0x00];
        let out = decompress_r21(&src, 12).unwrap();
        assert_eq!(out, [0, 1, 2, 3, 4, 5, 6, 7, 7, 7, 7, 7]);
    }

    #[test]
    fn damaged_r21_data_is_an_error() {
        assert!(decompress_r21(&[0x05, 1, 2], 100).is_err());
        assert!(decompress_r21(&[], 10).is_err());
        assert!(sections(&[0u8; 0x100]).is_err());
    }
}
