//! Reading zip archives (the way mods often arrive): the entries of an
//! archive held in memory, and one entry's bytes. Stored and deflated
//! entries are read; anything else (encryption, other methods) is refused.

use alloc::string::String;
use alloc::vec::Vec;

/// Largest entry read, in bytes.
const MAX_ENTRY: usize = 16 << 20;

pub struct Entry {
    pub name: String,
    method: u16,
    csize: usize,
    usize: usize,
    offset: usize,
}

fn u16_at(d: &[u8], p: usize) -> Option<usize> {
    Some(u16::from_le_bytes([*d.get(p)?, *d.get(p + 1)?]) as usize)
}

fn u32_at(d: &[u8], p: usize) -> Option<usize> {
    Some(u32::from_le_bytes([*d.get(p)?, *d.get(p + 1)?, *d.get(p + 2)?, *d.get(p + 3)?]) as usize)
}

/// The entries listed in the archive's central directory.
pub fn entries(d: &[u8]) -> Option<Vec<Entry>> {
    // The end-of-directory record is the last one, within the last 64 KB.
    let floor = d.len().saturating_sub(22 + 65535);
    let eocd = (floor..=d.len().checked_sub(22)?).rev().find(|&p| u32_at(d, p) == Some(0x0605_4b50))?;
    let count = u16_at(d, eocd + 10)?;
    let mut p = u32_at(d, eocd + 16)?;
    let mut out = Vec::new();
    for _ in 0..count {
        if u32_at(d, p)? != 0x0201_4b50 {
            return None;
        }
        let method = u16_at(d, p + 10)? as u16;
        let csize = u32_at(d, p + 20)?;
        let usize = u32_at(d, p + 24)?;
        let (nlen, xlen, clen) = (u16_at(d, p + 28)?, u16_at(d, p + 30)?, u16_at(d, p + 32)?);
        let offset = u32_at(d, p + 42)?;
        let name = String::from_utf8_lossy(d.get(p + 46..p + 46 + nlen)?).into_owned();
        out.push(Entry { name, method, csize, usize, offset });
        p += 46 + nlen + xlen + clen;
    }
    Some(out)
}

/// The bytes of the entry called `name`, or None if there is no such entry
/// or it cannot be read.
pub fn read(d: &[u8], name: &str) -> Option<Vec<u8>> {
    let e = entries(d)?.into_iter().find(|e| e.name == name)?;
    if e.usize > MAX_ENTRY {
        return None;
    }
    // The local header repeats the name and has its own extra field.
    if u32_at(d, e.offset)? != 0x0403_4b50 {
        return None;
    }
    let start = e.offset + 30 + u16_at(d, e.offset + 26)? + u16_at(d, e.offset + 28)?;
    let data = d.get(start..start.checked_add(e.csize)?)?;
    let out = match e.method {
        0 => data.to_vec(),
        8 => crate::png::inflate_raw(data, MAX_ENTRY)?,
        _ => return None,
    };
    if out.len() != e.usize {
        return None;
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A zip with one stored entry and one deflated entry (a stored deflate
    /// block, which is valid raw deflate).
    fn archive() -> Vec<u8> {
        let mut z = Vec::new();
        let mut central = Vec::new();
        let add = |z: &mut Vec<u8>, central: &mut Vec<u8>, name: &str, method: u16, body: &[u8], raw: &[u8], size: usize| {
            let offset = z.len() as u32;
            z.extend_from_slice(&0x0403_4b50u32.to_le_bytes());
            z.extend_from_slice(&[20, 0, 0, 0]);
            z.extend_from_slice(&method.to_le_bytes());
            z.extend_from_slice(&[0; 8]);
            z.extend_from_slice(&(raw.len() as u32).to_le_bytes());
            z.extend_from_slice(&(size as u32).to_le_bytes());
            z.extend_from_slice(&(name.len() as u16).to_le_bytes());
            z.extend_from_slice(&[0, 0]);
            z.extend_from_slice(name.as_bytes());
            z.extend_from_slice(raw);
            central.extend_from_slice(&0x0201_4b50u32.to_le_bytes());
            central.extend_from_slice(&[20, 0, 20, 0, 0, 0]);
            central.extend_from_slice(&method.to_le_bytes());
            central.extend_from_slice(&[0; 8]);
            central.extend_from_slice(&(raw.len() as u32).to_le_bytes());
            central.extend_from_slice(&(size as u32).to_le_bytes());
            central.extend_from_slice(&(name.len() as u16).to_le_bytes());
            central.extend_from_slice(&[0; 12]);
            central.extend_from_slice(&offset.to_le_bytes());
            central.extend_from_slice(name.as_bytes());
            let _ = body;
        };
        add(&mut z, &mut central, "mods/a/pack.txt", 0, b"hello", b"hello", 5);
        // Deflate: one final stored block holding "deflated".
        let mut raw = alloc::vec![0x01, 8, 0, !8u8, 0xFF];
        raw.extend_from_slice(b"deflated");
        add(&mut z, &mut central, "b.txt", 8, b"deflated", &raw, 8);
        let cd = z.len() as u32;
        z.extend_from_slice(&central);
        z.extend_from_slice(&0x0605_4b50u32.to_le_bytes());
        z.extend_from_slice(&[0, 0, 0, 0]);
        z.extend_from_slice(&2u16.to_le_bytes());
        z.extend_from_slice(&2u16.to_le_bytes());
        z.extend_from_slice(&(central.len() as u32).to_le_bytes());
        z.extend_from_slice(&cd.to_le_bytes());
        z.extend_from_slice(&[0, 0]);
        z
    }

    #[test]
    fn the_entries_of_an_archive_are_listed() {
        let names: Vec<String> = entries(&archive()).unwrap().into_iter().map(|e| e.name).collect();
        assert_eq!(names, alloc::vec![String::from("mods/a/pack.txt"), String::from("b.txt")]);
    }

    #[test]
    fn stored_and_deflated_entries_are_read() {
        let z = archive();
        assert_eq!(read(&z, "mods/a/pack.txt").as_deref(), Some(&b"hello"[..]));
        assert_eq!(read(&z, "b.txt").as_deref(), Some(&b"deflated"[..]));
        assert_eq!(read(&z, "missing.txt"), None);
    }

    #[test]
    fn a_damaged_archive_gives_nothing() {
        let mut z = archive();
        z.truncate(z.len() / 2);
        assert!(entries(&z).is_none());
        assert!(entries(b"not a zip").is_none());
    }
}
