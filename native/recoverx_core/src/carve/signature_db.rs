#[derive(Debug, Clone)]
pub struct FileSignature {
    pub kind: &'static str,
    pub ext: &'static str,
    pub header: &'static [u8],
    pub header_offset: usize,
    pub footer: Option<&'static [u8]>,
    pub max_size: u64,
}

pub fn default_signatures() -> Vec<FileSignature> {
    vec![
        // Photos & Graphics
        FileSignature { kind: "jpeg", ext: "jpg", header: &[0xFF, 0xD8, 0xFF], header_offset: 0, footer: Some(&[0xFF, 0xD9]), max_size: 50 * 1024 * 1024 },
        FileSignature { kind: "png", ext: "png", header: &[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A], header_offset: 0, footer: Some(&[0x49, 0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82]), max_size: 50 * 1024 * 1024 },
        FileSignature { kind: "gif", ext: "gif", header: b"GIF8", header_offset: 0, footer: Some(&[0x00, 0x3B]), max_size: 32 * 1024 * 1024 },
        FileSignature { kind: "bmp", ext: "bmp", header: b"BM", header_offset: 0, footer: None, max_size: 50 * 1024 * 1024 },
        // RIFF Media Containers (WAV / AVI / WEBP — exact header size + FourCC check)
        FileSignature { kind: "riff", ext: "wav", header: b"RIFF", header_offset: 0, footer: None, max_size: 4 * 1024 * 1024 * 1024 },
        // ISO BMFF Video/Photo/Audio (MP4 / MOV / HEIC / M4A — box-walking exact size)
        FileSignature { kind: "isobmff", ext: "mp4", header: b"ftyp", header_offset: 4, footer: None, max_size: 4 * 1024 * 1024 * 1024 },
        // Matroska / WebM Video
        FileSignature { kind: "mkv", ext: "mkv", header: &[0x1A, 0x45, 0xDF, 0xA3], header_offset: 0, footer: None, max_size: 4 * 1024 * 1024 * 1024 },
        // Audio (MP3 ID3, FLAC, OGG)
        FileSignature { kind: "mp3", ext: "mp3", header: b"ID3", header_offset: 0, footer: None, max_size: 200 * 1024 * 1024 },
        FileSignature { kind: "flac", ext: "flac", header: b"fLaC", header_offset: 0, footer: None, max_size: 500 * 1024 * 1024 },
        FileSignature { kind: "ogg", ext: "ogg", header: b"OggS", header_offset: 0, footer: None, max_size: 500 * 1024 * 1024 },
        // Documents & Archives
        FileSignature { kind: "pdf", ext: "pdf", header: b"%PDF-", header_offset: 0, footer: Some(b"%%EOF"), max_size: 128 * 1024 * 1024 },
        FileSignature { kind: "zip_ooxml", ext: "zip", header: &[0x50, 0x4B, 0x03, 0x04], header_offset: 0, footer: Some(&[0x50, 0x4B, 0x05, 0x06]), max_size: 256 * 1024 * 1024 },
        FileSignature { kind: "sqlite", ext: "sqlite", header: b"SQLite format 3\0", header_offset: 0, footer: None, max_size: 256 * 1024 * 1024 },
        FileSignature { kind: "gzip", ext: "gz", header: &[0x1F, 0x8B, 0x08], header_offset: 0, footer: None, max_size: 256 * 1024 * 1024 },
        FileSignature { kind: "sevenz", ext: "7z", header: &[0x37, 0x7A, 0xBC, 0xAF, 0x27, 0x1C], header_offset: 0, footer: None, max_size: 256 * 1024 * 1024 },
        FileSignature { kind: "rar", ext: "rar", header: &[0x52, 0x61, 0x72, 0x21, 0x1A, 0x07], header_offset: 0, footer: None, max_size: 256 * 1024 * 1024 },
    ]
}
