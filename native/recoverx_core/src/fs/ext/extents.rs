use crate::core::endian::{read_le_u16, read_le_u32};
use crate::core::types::ByteOffset;
use crate::fs::common::extent::{Extent, ExtentKind};
use crate::platform::raw_reader::{read_logical_slice, RawReader};

/// Parses ext4 extent tree (magic 0xF30A) or legacy ext2/3 direct block pointers.
pub fn parse_inode_extents(
    reader: &dyn RawReader,
    i_block: &[u8],
    uses_extents: bool,
    block_size: u64,
    logical_size: u64,
) -> Vec<Extent> {
    if i_block.len() < 60 {
        return Vec::new();
    }
    if uses_extents && read_le_u16(i_block, 0).unwrap_or(0) == 0xF30A {
        let mut out = Vec::new();
        walk_ext4_node(reader, i_block, block_size, &mut out, 0);
        return out;
    }

    // Legacy ext2/3 direct blocks (0..12)
    let mut out = Vec::new();
    let mut rem = logical_size;
    for i in 0..12usize {
        if rem == 0 {
            break;
        }
        let blk = read_le_u32(i_block, i * 4).unwrap_or(0) as u64;
        if blk == 0 {
            continue;
        }
        let take = block_size.min(rem);
        out.push(Extent {
            logical_off: (i as u64) * block_size,
            physical_off: blk * block_size,
            len: block_size,
            kind: ExtentKind::Data,
        });
        rem = rem.saturating_sub(take);
    }
    out
}

fn walk_ext4_node(
    reader: &dyn RawReader,
    node: &[u8],
    block_size: u64,
    out: &mut Vec<Extent>,
    recursion: usize,
) {
    if recursion > 5 || node.len() < 12 || read_le_u16(node, 0).unwrap_or(0) != 0xF30A {
        return;
    }
    let entries = read_le_u16(node, 2).unwrap_or(0) as usize;
    let depth = read_le_u16(node, 6).unwrap_or(0);
    for i in 0..entries {
        let base = 12 + i * 12;
        if base + 12 > node.len() {
            break;
        }
        if depth == 0 {
            let ee_block = read_le_u32(node, base).unwrap_or(0) as u64;
            let ee_len_raw = read_le_u16(node, base + 4).unwrap_or(0);
            let ee_len = (ee_len_raw & 0x7FFF) as u64;
            let start_hi = read_le_u16(node, base + 6).unwrap_or(0) as u64;
            let start_lo = read_le_u32(node, base + 8).unwrap_or(0) as u64;
            let phys_blk = (start_hi << 32) | start_lo;
            if ee_len > 0 && phys_blk > 0 {
                out.push(Extent {
                    logical_off: ee_block * block_size,
                    physical_off: phys_blk * block_size,
                    len: ee_len * block_size,
                    kind: ExtentKind::Data,
                });
            }
        } else {
            let leaf_lo = read_le_u32(node, base + 4).unwrap_or(0) as u64;
            let leaf_hi = read_le_u16(node, base + 8).unwrap_or(0) as u64;
            let child_blk = (leaf_hi << 32) | leaf_lo;
            if child_blk > 0 {
                if let Ok(buf) = read_logical_slice(reader, ByteOffset(child_blk * block_size), block_size as usize) {
                    walk_ext4_node(reader, &buf, block_size, out, recursion + 1);
                }
            }
        }
    }
}
