//! Ogg files: pages cut into packets, which hold Vorbis audio (see `vorbis/`).

use super::vorbis;
use super::Pcm;
use alloc::vec::Vec;

/// The packets of the first logical stream in an Ogg file, and the number of
/// samples the last page says the stream ends at (if it says).
fn packets(bytes: &[u8]) -> Option<(Vec<Vec<u8>>, Option<u64>)> {
    let mut packets: Vec<Vec<u8>> = Vec::new();
    let mut cur: Vec<u8> = Vec::new();
    let mut serial: Option<u32> = None;
    let mut granule: Option<u64> = None;
    let mut at = 0usize;
    while at + 27 <= bytes.len() {
        if &bytes[at..at + 4] != b"OggS" || bytes[at + 4] != 0 {
            return None;
        }
        let kind = bytes[at + 5];
        let gp = i64::from_le_bytes([
            bytes[at + 6],
            bytes[at + 7],
            bytes[at + 8],
            bytes[at + 9],
            bytes[at + 10],
            bytes[at + 11],
            bytes[at + 12],
            bytes[at + 13],
        ]);
        let ser = u32::from_le_bytes([bytes[at + 14], bytes[at + 15], bytes[at + 16], bytes[at + 17]]);
        let nsegs = bytes[at + 26] as usize;
        let table = at + 27;
        let Some(lacing) = bytes.get(table..table + nsegs) else { break };
        let body: usize = lacing.iter().map(|&l| l as usize).sum();
        let start = table + nsegs;
        let Some(data) = bytes.get(start..start + body) else { break }; // cut short
        at = start + body;
        match serial {
            None => serial = Some(ser),
            Some(s) if s != ser => continue, // another stream in the same file
            _ => {}
        }
        if kind & 1 == 0 {
            cur.clear(); // a packet that should have continued from a lost page
        }
        let mut p = 0usize;
        let mut finished = false;
        for &l in lacing {
            let l = l as usize;
            cur.extend_from_slice(&data[p..p + l]);
            p += l;
            if l < 255 {
                packets.push(core::mem::take(&mut cur));
                finished = true;
            }
        }
        if finished && gp >= 0 {
            granule = Some(gp as u64);
        }
        if kind & 4 != 0 {
            break;
        }
    }
    if packets.is_empty() {
        return None;
    }
    Some((packets, granule))
}

/// Decode an Ogg Vorbis file. `None` for anything else, or if damaged.
pub(super) fn ogg(bytes: &[u8]) -> Option<Pcm> {
    let (packets, granule) = packets(bytes)?;
    vorbis::decode(&packets, granule)
}

#[cfg(test)]
mod tests {
    use super::super::{decode, RATE};

    const FILES: [(&str, &[u8], &[u8]); 4] = [
        ("mono 22050", include_bytes!("testdata/mono22.ogg"), include_bytes!("testdata/mono22_ref.wav")),
        ("stereo 22050", include_bytes!("testdata/stereo22.ogg"), include_bytes!("testdata/stereo22_ref.wav")),
        ("mono 44100", include_bytes!("testdata/mono44.ogg"), include_bytes!("testdata/mono44_ref.wav")),
        ("stereo 44100", include_bytes!("testdata/stereo44.ogg"), include_bytes!("testdata/stereo44_ref.wav")),
    ];

    fn rms(a: &[i16]) -> f64 {
        (a.iter().map(|&v| (v as f64) * (v as f64)).sum::<f64>() / a.len().max(1) as f64).sqrt()
    }

    #[test]
    fn decodes_what_ffmpeg_decodes() {
        for (name, ogg, wav) in FILES {
            let got = decode(ogg).unwrap_or_else(|| panic!("{name}: not read"));
            let want = decode(wav).unwrap();
            // The Ogg file says where the sound ends; ffmpeg plays on a little past it.
            assert!(got.len() <= want.len() && want.len() - got.len() < 600, "{name}: {} samples against {}", got.len(), want.len());
            assert!(rms(&got) > 500.0, "{name}: silent");
            let n = got.len();
            let err = rms(&got.iter().zip(&want[..n]).map(|(&a, &b)| (a as i32 - b as i32).clamp(-32768, 32767) as i16).collect::<alloc::vec::Vec<i16>>());
            assert!(err < 40.0, "{name}: error {err}");
        }
        assert_eq!(RATE, 22050);
    }

    #[test]
    fn cut_short_files_do_not_panic() {
        for (name, ogg, _) in FILES {
            for cut in [0, 1, 4, 10, 26, 27, 28, 60, 100, 500, 2000, 4000, ogg.len() / 2, ogg.len() - 100, ogg.len() - 1] {
                let _ = decode(&ogg[..cut.min(ogg.len())]);
            }
            assert!(decode(&ogg[..20]).is_none(), "{name}");
            assert!(decode(&ogg[..200]).is_none(), "{name}");
        }
    }

    #[test]
    fn damaged_files_do_not_panic() {
        for (_, ogg, _) in FILES {
            // Every bit of the first stretch (the headers), then every so often further on.
            let mut spots: alloc::vec::Vec<usize> = (0..ogg.len().min(120) * 8).collect();
            spots.extend((0..ogg.len() * 8).step_by(1009));
            for bit in spots {
                let mut copy = ogg.to_vec();
                copy[bit / 8] ^= 1 << (bit % 8);
                let _ = decode(&copy);
            }
            // A run of garbage in the middle of the audio.
            let mut copy = ogg.to_vec();
            for i in 0..64 {
                copy[ogg.len() / 2 + i] = (i * 37) as u8;
            }
            let _ = decode(&copy);
        }
    }

    #[test]
    fn things_that_are_not_ogg_vorbis_are_refused() {
        assert!(decode(b"OggS").is_none());
        assert!(decode(&[b'O', b'g', b'g', b'S', 0, 2, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 5, 1, 2, 3, 4, 5]).is_none());
        // Some other codec's header in a well-formed page.
        let mut page = alloc::vec::Vec::from(&b"OggS\0\x02\0\0\0\0\0\0\0\0\x01\0\0\0\0\0\0\0\0\0\0\0\x01\x13"[..]);
        page.extend_from_slice(b"OpusHead\x01\x01\0\0\x80\xbb\0\0\0\0\0");
        assert!(decode(&page).is_none());
    }
}
