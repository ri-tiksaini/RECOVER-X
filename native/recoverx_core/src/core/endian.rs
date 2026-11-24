use crate::error::{RxError, RxResult};

#[inline]
pub fn read_le_u16(buf: &[u8], off: usize) -> RxResult<u16> {
    let end = off.checked_add(2).ok_or(RxError::ShortBuffer { offset: off, needed: 2, buf_len: buf.len() })?;
    let slice = buf.get(off..end).ok_or(RxError::ShortBuffer { offset: off, needed: 2, buf_len: buf.len() })?;
    Ok(u16::from_le_bytes([slice[0], slice[1]]))
}

#[inline]
pub fn read_le_u32(buf: &[u8], off: usize) -> RxResult<u32> {
    let end = off.checked_add(4).ok_or(RxError::ShortBuffer { offset: off, needed: 4, buf_len: buf.len() })?;
    let slice = buf.get(off..end).ok_or(RxError::ShortBuffer { offset: off, needed: 4, buf_len: buf.len() })?;
    Ok(u32::from_le_bytes([slice[0], slice[1], slice[2], slice[3]]))
}

#[inline]
pub fn read_le_u64(buf: &[u8], off: usize) -> RxResult<u64> {
    let end = off.checked_add(8).ok_or(RxError::ShortBuffer { offset: off, needed: 8, buf_len: buf.len() })?;
    let slice = buf.get(off..end).ok_or(RxError::ShortBuffer { offset: off, needed: 8, buf_len: buf.len() })?;
    Ok(u64::from_le_bytes([
        slice[0], slice[1], slice[2], slice[3], slice[4], slice[5], slice[6], slice[7],
    ]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_buffers_return_error() {
        let b = [0x01u8, 0x02, 0x03];
        assert_eq!(read_le_u16(&b, 0).unwrap(), 0x0201);
        assert!(read_le_u32(&b, 0).is_err());
        assert!(read_le_u64(&b, 0).is_err());
    }
}
