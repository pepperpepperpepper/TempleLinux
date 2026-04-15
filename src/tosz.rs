//! TempleOS `.Z` (CArcCompress) compression support.
//!
//! TempleOS stores many files with a `.Z` suffix. These files are not encrypted; they are wrapped
//! in a small `CArcCompress` header and either stored uncompressed (`CT_NONE`) or compressed with
//! TempleOS's dictionary coder (`CT_7_BIT` / `CT_8_BIT`).

use std::fmt;

pub const CT_NONE: u8 = 1;
pub const CT_7_BIT: u8 = 2;
pub const CT_8_BIT: u8 = 3;

pub const ARC_MAX_BITS: usize = 12;
const ARC_DICT_SIZE: usize = 1 << ARC_MAX_BITS;

/// Packed `CArcCompress` header size:
/// - `I64 compressed_size`
/// - `I64 expanded_size`
/// - `U8 compression_type`
pub const ARC_HEADER_LEN: usize = 8 + 8 + 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ArcCompressHeader {
    pub compressed_size: u64,
    pub expanded_size: u64,
    pub compression_type: u8,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ArcError {
    TooShort,
    InvalidHeader,
    UnsupportedCompressionType(u8),
    ExpandedTooLarge(u64),
    InvalidCtNoneBodyLen { expected: usize, actual: usize },
    BitstreamTruncated,
    StackOverflow,
    OutputTooShort { expected: usize, actual: usize },
}

impl fmt::Display for ArcError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooShort => write!(f, "CArcCompress: buffer too short"),
            Self::InvalidHeader => write!(f, "CArcCompress: invalid header"),
            Self::UnsupportedCompressionType(ct) => {
                write!(f, "CArcCompress: unsupported compression type {ct}")
            }
            Self::ExpandedTooLarge(n) => write!(f, "CArcCompress: expanded_size too large ({n})"),
            Self::InvalidCtNoneBodyLen { expected, actual } => write!(
                f,
                "CArcCompress: CT_NONE body length mismatch (expected {expected}, got {actual})"
            ),
            Self::BitstreamTruncated => write!(f, "CArcCompress: compressed bitstream truncated"),
            Self::StackOverflow => write!(f, "CArcCompress: decode stack overflow"),
            Self::OutputTooShort { expected, actual } => {
                write!(
                    f,
                    "CArcCompress: output too short (expected {expected}, got {actual})"
                )
            }
        }
    }
}

impl std::error::Error for ArcError {}

pub fn parse_arc_compress_header(bytes: &[u8]) -> Option<ArcCompressHeader> {
    if bytes.len() < ARC_HEADER_LEN {
        return None;
    }
    let compressed_size = i64::from_le_bytes(bytes[0..8].try_into().ok()?)
        .try_into()
        .ok()?;
    let expanded_size = i64::from_le_bytes(bytes[8..16].try_into().ok()?)
        .try_into()
        .ok()?;
    let compression_type = bytes[16];
    if compressed_size != bytes.len() as u64 {
        return None;
    }
    if !matches!(compression_type, CT_NONE | CT_7_BIT | CT_8_BIT) {
        return None;
    }
    Some(ArcCompressHeader {
        compressed_size,
        expanded_size,
        compression_type,
    })
}

pub fn maybe_expand_arc_compress(bytes: &[u8]) -> Result<Option<Vec<u8>>, ArcError> {
    if parse_arc_compress_header(bytes).is_none() {
        return Ok(None);
    }
    Ok(Some(expand_arc_compress(bytes)?))
}

pub fn expand_arc_compress(bytes: &[u8]) -> Result<Vec<u8>, ArcError> {
    let Some(hdr) = parse_arc_compress_header(bytes) else {
        return Err(if bytes.len() < ARC_HEADER_LEN {
            ArcError::TooShort
        } else {
            ArcError::InvalidHeader
        });
    };

    // Keep the same conservative cap as the upstream Linux `TOSZ` utility.
    if hdr.expanded_size >= 0x2000_0000 {
        return Err(ArcError::ExpandedTooLarge(hdr.expanded_size));
    }
    let expanded_size: usize = hdr
        .expanded_size
        .try_into()
        .map_err(|_| ArcError::ExpandedTooLarge(hdr.expanded_size))?;

    match hdr.compression_type {
        CT_NONE => {
            let body = &bytes[ARC_HEADER_LEN..];
            if body.len() != expanded_size {
                return Err(ArcError::InvalidCtNoneBodyLen {
                    expected: expanded_size,
                    actual: body.len(),
                });
            }
            Ok(body.to_vec())
        }
        CT_7_BIT | CT_8_BIT => expand_arc_lzw(bytes, hdr.compression_type, expanded_size),
        other => Err(ArcError::UnsupportedCompressionType(other)),
    }
}

pub fn wrap_arc_compress_none(src: &[u8]) -> Vec<u8> {
    let compressed_size = (ARC_HEADER_LEN + src.len()) as i64;
    let expanded_size = src.len() as i64;
    let mut out = Vec::with_capacity(ARC_HEADER_LEN + src.len());
    out.extend_from_slice(&compressed_size.to_le_bytes());
    out.extend_from_slice(&expanded_size.to_le_bytes());
    out.push(CT_NONE);
    out.extend_from_slice(src);
    out
}

pub fn compress_arc_compress_buf(src: &[u8]) -> Vec<u8> {
    if src.is_empty() {
        return wrap_arc_compress_none(src);
    }

    let compression_type = arc_determine_compression_type(src);
    let dst_len = ARC_HEADER_LEN + src.len();
    let dst_size_bits = dst_len * 8;
    let mut dst = vec![0u8; dst_len];

    let mut dict = ArcDict::new(compression_type);

    let mut src_pos: usize = 0;
    let mut dst_pos_bits: usize = ARC_HEADER_LEN * 8;

    let mut basecode: usize = src[src_pos] as usize;
    src_pos += 1;

    while src_pos < src.len() && dst_pos_bits + dict.cur_bits_in_use <= dst_size_bits {
        dict.arc_entry_get();

        loop {
            if src_pos >= src.len() {
                break;
            }
            let ch = src[src_pos];
            src_pos += 1;

            if let Some(found) = dict.find_entry(basecode, ch) {
                basecode = found;
                continue;
            }

            bitfield_or_u32(&mut dst, dst_pos_bits, basecode as u32);
            dst_pos_bits += dict.cur_bits_in_use;

            dict.entry_used = true;
            let entry_idx = dict.cur_entry;
            dict.compress[entry_idx].basecode = basecode as u16;
            dict.compress[entry_idx].ch = ch;
            dict.compress[entry_idx].next = dict.hash[basecode];
            dict.hash[basecode] = Some(entry_idx);

            basecode = ch as usize;
            break;
        }
    }

    let saved_basecode: u32 = basecode as u32;
    let finish_ok = {
        if dst_pos_bits + dict.cur_bits_in_use <= dst_size_bits {
            bitfield_or_u32(&mut dst, dst_pos_bits, saved_basecode);
            dst_pos_bits += dict.next_bits_in_use;
            true
        } else {
            false
        }
    };

    if finish_ok && src_pos == src.len() {
        let out_len = (dst_pos_bits + 7) / 8;
        dst.truncate(out_len);
        let compressed_size = dst.len() as i64;
        let expanded_size = src.len() as i64;
        dst[0..8].copy_from_slice(&compressed_size.to_le_bytes());
        dst[8..16].copy_from_slice(&expanded_size.to_le_bytes());
        dst[16] = compression_type;
        dst
    } else {
        wrap_arc_compress_none(src)
    }
}

fn arc_determine_compression_type(src: &[u8]) -> u8 {
    if src.iter().any(|b| (b & 0x80) != 0) {
        CT_8_BIT
    } else {
        CT_7_BIT
    }
}

#[derive(Clone, Copy, Debug)]
struct ArcEntry {
    next: Option<usize>,
    basecode: u16,
    ch: u8,
}

impl Default for ArcEntry {
    fn default() -> Self {
        Self {
            next: None,
            basecode: 0,
            ch: 0,
        }
    }
}

#[derive(Debug)]
struct ArcDict {
    min_table_entry: usize,
    cur_entry: usize,
    next_entry: usize,
    cur_bits_in_use: usize,
    next_bits_in_use: usize,
    free_idx: usize,
    free_limit: usize,
    entry_used: bool,
    compress: Vec<ArcEntry>,
    hash: Vec<Option<usize>>,
}

impl ArcDict {
    fn new(compression_type: u8) -> Self {
        let min_bits = if compression_type == CT_7_BIT { 7 } else { 8 };
        let min_table_entry = 1usize << min_bits;
        let mut dict = Self {
            min_table_entry,
            cur_entry: 0,
            next_entry: 0,
            cur_bits_in_use: 0,
            next_bits_in_use: min_bits + 1,
            free_idx: min_table_entry,
            free_limit: 1usize << (min_bits + 1),
            entry_used: true,
            compress: vec![ArcEntry::default(); ARC_DICT_SIZE],
            hash: vec![None; ARC_DICT_SIZE],
        };
        dict.arc_entry_get();
        dict.entry_used = true;
        dict
    }

    fn find_entry(&self, basecode: usize, ch: u8) -> Option<usize> {
        let mut idx = self.hash.get(basecode).copied().unwrap_or(None);
        while let Some(i) = idx {
            let ent = &self.compress[i];
            if ent.ch == ch {
                return Some(i);
            }
            idx = ent.next;
        }
        None
    }

    fn arc_entry_get(&mut self) {
        if !self.entry_used {
            return;
        }

        let mut i = self.free_idx;

        self.entry_used = false;
        self.cur_entry = self.next_entry;
        self.cur_bits_in_use = self.next_bits_in_use;

        if self.next_bits_in_use < ARC_MAX_BITS {
            self.next_entry = i;
            i += 1;
            if i == self.free_limit {
                self.next_bits_in_use += 1;
                self.free_limit = 1usize << self.next_bits_in_use;
            }
        } else {
            loop {
                i += 1;
                if i == self.free_limit {
                    i = self.min_table_entry;
                }
                if self.hash[i].is_none() {
                    break;
                }
            }

            let tmp_idx = i;
            self.next_entry = tmp_idx;

            let basecode = self.compress[tmp_idx].basecode as usize;
            let mut cur = self.hash[basecode];
            let mut prev: Option<usize> = None;
            while let Some(ci) = cur {
                if ci == tmp_idx {
                    break;
                }
                prev = Some(ci);
                cur = self.compress[ci].next;
            }
            if let Some(ci) = cur {
                let next = self.compress[ci].next;
                if let Some(prev) = prev {
                    self.compress[prev].next = next;
                } else {
                    self.hash[basecode] = next;
                }
            }
        }

        self.free_idx = i;
    }
}

fn bitfield_or_u32(dst: &mut [u8], bit_pos: usize, pattern: u32) {
    for i in 0..32usize {
        if (pattern & (1u32 << i)) == 0 {
            continue;
        }
        let bit = bit_pos + i;
        let byte = bit >> 3;
        let b = bit & 7;
        if let Some(cell) = dst.get_mut(byte) {
            *cell |= 1u8 << b;
        }
    }
}

fn bitfield_ext_u32(src: &[u8], bit_pos: usize, bits: usize) -> Option<u32> {
    if bits > 32 {
        return None;
    }
    let end = bit_pos.checked_add(bits)?;
    if end > src.len() * 8 {
        return None;
    }
    let mut res: u32 = 0;
    for i in 0..bits {
        let bit = bit_pos + i;
        let byte = src[bit >> 3];
        let b = (bit & 7) as u8;
        if (byte & (1u8 << b)) != 0 {
            res |= 1u32 << i;
        }
    }
    Some(res)
}

fn expand_arc_lzw(
    bytes: &[u8],
    compression_type: u8,
    expanded_size: usize,
) -> Result<Vec<u8>, ArcError> {
    let mut dict = ArcDict::new(compression_type);
    let src_size_bits = bytes.len() * 8;
    let mut src_pos_bits: usize = ARC_HEADER_LEN * 8;

    let mut out = vec![0u8; expanded_size];
    let mut out_pos: usize = 0;

    let mut stack = [0u8; ARC_DICT_SIZE];
    let mut stk_len: usize = 0;

    let mut saved_basecode: u32 = u32::MAX;
    let mut last_ch: u32 = 0;
    let mut lastcode: u32;

    while out_pos < expanded_size {
        while out_pos < expanded_size && stk_len != 0 {
            stk_len -= 1;
            out[out_pos] = stack[stk_len];
            out_pos += 1;
        }

        if out_pos >= expanded_size {
            break;
        }

        if stk_len == 0 {
            if saved_basecode == u32::MAX {
                let Some(code) = bitfield_ext_u32(bytes, src_pos_bits, dict.next_bits_in_use)
                else {
                    return Err(ArcError::BitstreamTruncated);
                };
                src_pos_bits += dict.next_bits_in_use;
                out[out_pos] = code as u8;
                out_pos += 1;
                dict.arc_entry_get();
                last_ch = code;
                lastcode = code;
            } else {
                lastcode = saved_basecode;
            }

            while out_pos < expanded_size && src_pos_bits + dict.next_bits_in_use <= src_size_bits {
                let Some(basecode) = bitfield_ext_u32(bytes, src_pos_bits, dict.next_bits_in_use)
                else {
                    return Err(ArcError::BitstreamTruncated);
                };
                src_pos_bits += dict.next_bits_in_use;

                let mut code: u32 = if dict.cur_entry == basecode as usize {
                    if stk_len >= stack.len() {
                        return Err(ArcError::StackOverflow);
                    }
                    stack[stk_len] = last_ch as u8;
                    stk_len += 1;
                    lastcode
                } else {
                    basecode
                };

                while (code as usize) >= dict.min_table_entry {
                    if stk_len >= stack.len() {
                        return Err(ArcError::StackOverflow);
                    }
                    let ent = dict
                        .compress
                        .get(code as usize)
                        .ok_or(ArcError::InvalidHeader)?;
                    stack[stk_len] = ent.ch;
                    stk_len += 1;
                    code = ent.basecode as u32;
                }

                if stk_len >= stack.len() {
                    return Err(ArcError::StackOverflow);
                }
                stack[stk_len] = code as u8;
                stk_len += 1;
                last_ch = code;

                dict.entry_used = true;
                let entry_idx = dict.cur_entry;
                dict.compress[entry_idx].basecode = lastcode as u16;
                dict.compress[entry_idx].ch = last_ch as u8;
                dict.compress[entry_idx].next = dict.hash[lastcode as usize];
                dict.hash[lastcode as usize] = Some(entry_idx);
                dict.arc_entry_get();

                while out_pos < expanded_size && stk_len != 0 {
                    stk_len -= 1;
                    out[out_pos] = stack[stk_len];
                    out_pos += 1;
                }

                lastcode = basecode;
            }

            saved_basecode = lastcode;
        }
    }

    if out_pos != expanded_size {
        return Err(ArcError::OutputTooShort {
            expected: expanded_size,
            actual: out_pos,
        });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arccompress_roundtrip_ascii() {
        let src = b"Hello, TempleOS!\nThis is a test.\n".to_vec();
        let arc = compress_arc_compress_buf(&src);
        let out = expand_arc_compress(&arc).unwrap();
        assert_eq!(out, src);
    }

    #[test]
    fn arccompress_roundtrip_binary() {
        let mut src = Vec::new();
        for i in 0..4096usize {
            src.push((i & 0xFF) as u8);
        }
        let arc = compress_arc_compress_buf(&src);
        let out = expand_arc_compress(&arc).unwrap();
        assert_eq!(out, src);
    }

    #[test]
    fn maybe_expand_rejects_non_arccompress() {
        let src = b"not a Z file".to_vec();
        let res = maybe_expand_arc_compress(&src).unwrap();
        assert!(res.is_none());
    }
}
