/// Checks whether the first bytes of a file match its claimed extension.
/// Returns None if extension has no strict magic (e.g., plain text/source code).
pub fn check_header_agreement(filename: &str, head: &[u8]) -> Option<bool> {
    let ext = filename.rsplit('.').next()?.to_ascii_lowercase();
    if head.len() < 4 {
        return None;
    }
    match ext.as_str() {
        "jpg" | "jpeg" => Some(head.starts_with(&[0xFF, 0xD8, 0xFF])),
        "png" => Some(head.starts_with(&[0x89, 0x50, 0x4E, 0x47])),
        "gif" => Some(head.starts_with(b"GIF8")),
        "bmp" => Some(head.starts_with(b"BM")),
        "webp" => Some(head.starts_with(b"RIFF") && head.len() >= 12 && &head[8..12] == b"WEBP"),
        "wav" => Some(head.starts_with(b"RIFF") && head.len() >= 12 && &head[8..12] == b"WAVE"),
        "avi" => Some(head.starts_with(b"RIFF") && head.len() >= 12 && &head[8..12] == b"AVI "),
        "mp4" | "mov" | "m4a" | "m4v" | "heic" | "heif" | "3gp" => {
            Some(head.len() >= 8 && &head[4..8] == b"ftyp")
        }
        "mkv" | "webm" => Some(head.starts_with(&[0x1A, 0x45, 0xDF, 0xA3])),
        "mp3" => Some(
            head.starts_with(b"ID3")
                || (head[0] == 0xFF && (head[1] == 0xFB || head[1] == 0xF3 || head[1] == 0xF2)),
        ),
        "flac" => Some(head.starts_with(b"fLaC")),
        "ogg" | "oga" | "ogv" => Some(head.starts_with(b"OggS")),
        "pdf" => Some(head.starts_with(b"%PDF")),
        "zip" | "docx" | "xlsx" | "pptx" => Some(head.starts_with(&[0x50, 0x4B, 0x03, 0x04])),
        "gz" => Some(head.starts_with(&[0x1F, 0x8B])),
        "7z" => Some(head.starts_with(&[0x37, 0x7A, 0xBC, 0xAF])),
        "rar" => Some(head.starts_with(b"Rar!")),
        "sqlite" | "db" => Some(head.starts_with(b"SQLite format 3")),
        _ => None,
    }
}
