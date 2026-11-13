//! IMA and Microsoft ADPCM blocks, as WAV files hold them.

use super::u16le;
use alloc::vec::Vec;

pub(super) const IMA_STEPS: [i32; 89] = [
    7, 8, 9, 10, 11, 12, 13, 14, 16, 17, 19, 21, 23, 25, 28, 31, 34, 37, 41, 45, 50, 55, 60, 66, 73, 80, 88, 97, 107, 118, 130, 143, 157, 173, 190, 209, 230, 253, 279, 307, 337, 371,
    408, 449, 494, 544, 598, 658, 724, 796, 876, 963, 1060, 1166, 1282, 1411, 1552, 1707, 1878, 2066, 2272, 2499, 2749, 3024, 3327, 3660, 4026, 4428, 4871, 5358, 5894, 6484, 7132,
    7845, 8630, 9493, 10442, 11487, 12635, 13899, 15289, 16818, 18500, 20350, 22385, 24623, 27086, 29794, 32767,
];
pub(super) const IMA_INDEX: [i32; 16] = [-1, -1, -1, -1, 2, 4, 6, 8, -1, -1, -1, -1, 2, 4, 6, 8];

pub(super) fn ima_adpcm(data: &[u8], ch: usize, align: usize, out: &mut Vec<f32>) -> Option<()> {
    if align < 4 * ch || align > 65_536 {
        return None;
    }
    for block in data.chunks(align) {
        if block.len() < 4 * ch {
            break;
        }
        let mut pred = [0i32; 8];
        let mut idx = [0i32; 8];
        let mut chans: Vec<Vec<f32>> = alloc::vec![Vec::new(); ch];
        for c in 0..ch {
            pred[c] = i16::from_le_bytes([block[c * 4], block[c * 4 + 1]]) as i32;
            idx[c] = (block[c * 4 + 2] as i32).min(88);
            chans[c].push(pred[c] as f32 / 32768.0);
        }
        // After the headers: groups of four bytes (eight samples) per channel, in turn.
        let body = &block[4 * ch..];
        for group in body.chunks(4 * ch) {
            for c in 0..ch {
                let Some(bytes) = group.get(c * 4..c * 4 + 4) else { continue };
                for &byte in bytes {
                    for nib in [byte & 15, byte >> 4] {
                        let step = IMA_STEPS[idx[c] as usize];
                        let mut diff = step >> 3;
                        if nib & 1 != 0 { diff += step >> 2; }
                        if nib & 2 != 0 { diff += step >> 1; }
                        if nib & 4 != 0 { diff += step; }
                        pred[c] = (pred[c] + if nib & 8 != 0 { -diff } else { diff }).clamp(-32768, 32767);
                        idx[c] = (idx[c] + IMA_INDEX[nib as usize]).clamp(0, 88);
                        chans[c].push(pred[c] as f32 / 32768.0);
                    }
                }
            }
        }
        let frames = chans.iter().map(|c| c.len()).min().unwrap_or(0);
        for f in 0..frames {
            for c in 0..ch {
                out.push(chans[c][f]);
            }
        }
    }
    Some(())
}

pub(super) fn ms_adpcm(data: &[u8], ch: usize, align: usize, extra: &[u8], out: &mut Vec<f32>) -> Option<()> {
    // extra: samples per block (u16), number of coefficient pairs (u16), then the pairs.
    let ncoef = u16le(extra, 2)? as usize;
    if ncoef == 0 || ncoef > 16 || extra.len() < 4 + ncoef * 4 || align < 7 * ch || align > 65_536 {
        return None;
    }
    let coef: Vec<(i32, i32)> = (0..ncoef).map(|k| (u16le(extra, 4 + k * 4).unwrap() as i16 as i32, u16le(extra, 6 + k * 4).unwrap() as i16 as i32)).collect();
    const ADAPT: [i32; 16] = [230, 230, 230, 230, 307, 409, 512, 614, 768, 614, 512, 409, 307, 230, 230, 230];
    for block in data.chunks(align) {
        if block.len() < 7 * ch {
            break;
        }
        let mut pred = [0usize; 8];
        let mut delta = [0i32; 8];
        let mut s1 = [0i32; 8];
        let mut s2 = [0i32; 8];
        let mut chans: Vec<Vec<f32>> = alloc::vec![Vec::new(); ch];
        for c in 0..ch {
            pred[c] = (block[c] as usize).min(ncoef - 1);
        }
        for c in 0..ch {
            delta[c] = i16::from_le_bytes([block[ch + c * 2], block[ch + c * 2 + 1]]) as i32;
        }
        for c in 0..ch {
            s1[c] = i16::from_le_bytes([block[3 * ch + c * 2], block[3 * ch + c * 2 + 1]]) as i32;
        }
        for c in 0..ch {
            s2[c] = i16::from_le_bytes([block[5 * ch + c * 2], block[5 * ch + c * 2 + 1]]) as i32;
        }
        for c in 0..ch {
            chans[c].push(s2[c] as f32 / 32768.0);
            chans[c].push(s1[c] as f32 / 32768.0);
        }
        // Nibbles alternate between the channels.
        let mut turn = 0usize;
        for &byte in &block[7 * ch..] {
            for nib in [(byte >> 4) as i32, (byte & 15) as i32] {
                let c = turn % ch;
                turn += 1;
                let signed = if nib >= 8 { nib - 16 } else { nib };
                let p = ((s1[c] * coef[pred[c]].0 + s2[c] * coef[pred[c]].1) >> 8) + signed * delta[c];
                let p = p.clamp(-32768, 32767);
                s2[c] = s1[c];
                s1[c] = p;
                delta[c] = ((ADAPT[nib as usize] * delta[c]) >> 8).max(16);
                chans[c].push(p as f32 / 32768.0);
            }
        }
        let frames = chans.iter().map(|c| c.len()).min().unwrap_or(0);
        for f in 0..frames {
            for c in 0..ch {
                out.push(chans[c][f]);
            }
        }
    }
    Some(())
}
