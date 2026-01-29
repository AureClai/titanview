/// Known file format signatures (magic bytes) and carving utilities.

/// A known signature definition.
pub struct Signature {
    pub name: &'static str,
    pub magic: &'static [u8],
    /// Some signatures have a fixed offset (e.g., offset 0 for file headers).
    /// None means the signature can appear anywhere.
    pub fixed_offset: Option<u64>,
    /// File extension for carving.
    pub extension: &'static str,
}

/// A match found in the file.
#[derive(Debug, Clone)]
pub struct SignatureMatch {
    pub offset: u64,
    pub name: &'static str,
    pub magic_len: usize,
}

/// Result of carving analysis.
#[derive(Debug, Clone)]
pub struct CarveInfo {
    /// Detected or estimated file size (None if unknown).
    pub size: Option<u64>,
    /// Suggested file extension.
    pub extension: &'static str,
    /// Whether the size is exact or estimated.
    pub size_exact: bool,
}

/// Built-in signature database.
pub static SIGNATURES: &[Signature] = &[
    // Executables
    Signature { name: "ELF", magic: b"\x7fELF", fixed_offset: Some(0), extension: "elf" },
    Signature { name: "PE/COFF (MZ)", magic: b"MZ", fixed_offset: Some(0), extension: "exe" },
    Signature { name: "Mach-O (64-bit)", magic: &[0xCF, 0xFA, 0xED, 0xFE], fixed_offset: Some(0), extension: "macho" },
    Signature { name: "Mach-O (32-bit)", magic: &[0xCE, 0xFA, 0xED, 0xFE], fixed_offset: Some(0), extension: "macho" },
    Signature { name: "Java class", magic: &[0xCA, 0xFE, 0xBA, 0xBE], fixed_offset: Some(0), extension: "class" },
    Signature { name: "DEX (Dalvik)", magic: b"dex\n", fixed_offset: Some(0), extension: "dex" },

    // Archives & compressed
    Signature { name: "ZIP/JAR/APK/DOCX", magic: b"PK\x03\x04", fixed_offset: None, extension: "zip" },
    Signature { name: "RAR", magic: b"Rar!\x1a\x07", fixed_offset: None, extension: "rar" },
    Signature { name: "7z", magic: &[0x37, 0x7A, 0xBC, 0xAF, 0x27, 0x1C], fixed_offset: None, extension: "7z" },
    Signature { name: "gzip", magic: &[0x1F, 0x8B], fixed_offset: None, extension: "gz" },
    Signature { name: "bzip2", magic: b"BZh", fixed_offset: None, extension: "bz2" },
    Signature { name: "XZ", magic: &[0xFD, 0x37, 0x7A, 0x58, 0x5A, 0x00], fixed_offset: None, extension: "xz" },
    Signature { name: "Zstandard", magic: &[0x28, 0xB5, 0x2F, 0xFD], fixed_offset: None, extension: "zst" },
    Signature { name: "LZ4 frame", magic: &[0x04, 0x22, 0x4D, 0x18], fixed_offset: None, extension: "lz4" },

    // Images
    Signature { name: "PNG", magic: &[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A], fixed_offset: None, extension: "png" },
    Signature { name: "JPEG", magic: &[0xFF, 0xD8, 0xFF], fixed_offset: None, extension: "jpg" },
    Signature { name: "GIF87a", magic: b"GIF87a", fixed_offset: None, extension: "gif" },
    Signature { name: "GIF89a", magic: b"GIF89a", fixed_offset: None, extension: "gif" },
    Signature { name: "BMP", magic: b"BM", fixed_offset: Some(0), extension: "bmp" },
    Signature { name: "TIFF (LE)", magic: &[0x49, 0x49, 0x2A, 0x00], fixed_offset: None, extension: "tiff" },
    Signature { name: "TIFF (BE)", magic: &[0x4D, 0x4D, 0x00, 0x2A], fixed_offset: None, extension: "tiff" },
    Signature { name: "WebP", magic: b"RIFF", fixed_offset: None, extension: "webp" },

    // Documents
    Signature { name: "PDF", magic: b"%PDF", fixed_offset: None, extension: "pdf" },
    Signature { name: "OLE2 (DOC/XLS/PPT)", magic: &[0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1], fixed_offset: None, extension: "doc" },

    // Databases
    Signature { name: "SQLite", magic: b"SQLite format 3\x00", fixed_offset: Some(0), extension: "sqlite" },

    // Crypto / keys
    Signature { name: "PGP public key", magic: &[0x99, 0x01], fixed_offset: None, extension: "pgp" },
    Signature { name: "SSH private key", magic: b"-----BEGIN OPENSSH", fixed_offset: None, extension: "key" },
    Signature { name: "PEM certificate", magic: b"-----BEGIN CERTIFICATE", fixed_offset: None, extension: "pem" },

    // Disk / filesystem
    Signature { name: "ISO 9660", magic: b"CD001", fixed_offset: Some(0x8001), extension: "iso" },
    Signature { name: "LUKS", magic: b"LUKS\xba\xbe", fixed_offset: Some(0), extension: "luks" },

    // Multimedia
    Signature { name: "OGG", magic: b"OggS", fixed_offset: None, extension: "ogg" },
    Signature { name: "FLAC", magic: b"fLaC", fixed_offset: None, extension: "flac" },
    Signature { name: "MP3 (ID3v2)", magic: b"ID3", fixed_offset: Some(0), extension: "mp3" },
    Signature { name: "WAV", magic: b"RIFF", fixed_offset: Some(0), extension: "wav" },
    Signature { name: "AVI", magic: b"RIFF", fixed_offset: Some(0), extension: "avi" },

    // Misc
    Signature { name: "WASM", magic: &[0x00, 0x61, 0x73, 0x6D], fixed_offset: Some(0), extension: "wasm" },
    Signature { name: "tar (ustar)", magic: b"ustar", fixed_offset: Some(257), extension: "tar" },
];

/// Get the extension for a signature name.
pub fn get_extension(name: &str) -> &'static str {
    SIGNATURES.iter()
        .find(|s| s.name == name)
        .map(|s| s.extension)
        .unwrap_or("bin")
}

/// Analyze embedded file to determine its size for carving.
/// `data` should start at the signature offset.
/// `max_size` limits how far to search for end markers.
pub fn analyze_carve_size(name: &str, data: &[u8], max_size: u64) -> CarveInfo {
    let extension = get_extension(name);
    let max_len = data.len().min(max_size as usize);

    match name {
        "PNG" => {
            // PNG ends with IEND chunk: 00 00 00 00 49 45 4E 44 AE 42 60 82
            let iend = b"\x49\x45\x4E\x44\xAE\x42\x60\x82";
            if let Some(pos) = find_pattern(&data[..max_len], iend) {
                CarveInfo { size: Some((pos + 8) as u64), extension, size_exact: true }
            } else {
                CarveInfo { size: None, extension, size_exact: false }
            }
        }
        "JPEG" => {
            // JPEG ends with FFD9
            if let Some(pos) = find_pattern(&data[..max_len], &[0xFF, 0xD9]) {
                CarveInfo { size: Some((pos + 2) as u64), extension, size_exact: true }
            } else {
                CarveInfo { size: None, extension, size_exact: false }
            }
        }
        "GIF87a" | "GIF89a" => {
            // GIF ends with trailer byte 0x3B
            if let Some(pos) = data[..max_len].iter().rposition(|&b| b == 0x3B) {
                CarveInfo { size: Some((pos + 1) as u64), extension, size_exact: true }
            } else {
                CarveInfo { size: None, extension, size_exact: false }
            }
        }
        "PDF" => {
            // PDF ends with %%EOF (possibly followed by whitespace)
            if let Some(pos) = find_pattern(&data[..max_len], b"%%EOF") {
                // Include a bit after %%EOF for trailing newlines
                let end = (pos + 10).min(max_len);
                CarveInfo { size: Some(end as u64), extension, size_exact: true }
            } else {
                CarveInfo { size: None, extension, size_exact: false }
            }
        }
        "ZIP/JAR/APK/DOCX" => {
            // ZIP: search for End of Central Directory signature
            let eocd = b"PK\x05\x06";
            if let Some(pos) = find_pattern_reverse(&data[..max_len], eocd) {
                // EOCD is at least 22 bytes, comment length at offset 20
                if pos + 22 <= max_len {
                    let comment_len = u16::from_le_bytes([data[pos + 20], data[pos + 21]]) as usize;
                    let total = pos + 22 + comment_len;
                    CarveInfo { size: Some(total as u64), extension, size_exact: true }
                } else {
                    CarveInfo { size: Some((pos + 22) as u64), extension, size_exact: false }
                }
            } else {
                CarveInfo { size: None, extension, size_exact: false }
            }
        }
        "BMP" => {
            // BMP: file size at offset 2 (4 bytes LE)
            if data.len() >= 6 {
                let size = u32::from_le_bytes([data[2], data[3], data[4], data[5]]) as u64;
                CarveInfo { size: Some(size), extension, size_exact: true }
            } else {
                CarveInfo { size: None, extension, size_exact: false }
            }
        }
        "PE/COFF (MZ)" => {
            // PE: need to parse headers to get image size
            if let Some(size) = parse_pe_size(data) {
                CarveInfo { size: Some(size), extension, size_exact: true }
            } else {
                CarveInfo { size: None, extension, size_exact: false }
            }
        }
        "ELF" => {
            // ELF: section headers tell us the extent
            if let Some(size) = parse_elf_size(data) {
                CarveInfo { size: Some(size), extension, size_exact: true }
            } else {
                CarveInfo { size: None, extension, size_exact: false }
            }
        }
        "WAV" | "AVI" | "WebP" => {
            // RIFF format: size at offset 4 (4 bytes LE) + 8 for header
            if data.len() >= 8 {
                let size = u32::from_le_bytes([data[4], data[5], data[6], data[7]]) as u64 + 8;
                CarveInfo { size: Some(size), extension, size_exact: true }
            } else {
                CarveInfo { size: None, extension, size_exact: false }
            }
        }
        "gzip" => {
            // gzip: need to decompress or search for next gzip/EOF
            // Use conservative estimate: search for next signature or use max
            CarveInfo { size: Some(max_size.min(1024 * 1024)), extension, size_exact: false }
        }
        "SQLite" => {
            // SQLite: page size at offset 16, page count can be computed
            if data.len() >= 100 {
                let page_size = u16::from_be_bytes([data[16], data[17]]) as u64;
                let page_count = u32::from_be_bytes([data[28], data[29], data[30], data[31]]) as u64;
                if page_size > 0 && page_count > 0 {
                    CarveInfo { size: Some(page_size * page_count), extension, size_exact: true }
                } else {
                    CarveInfo { size: None, extension, size_exact: false }
                }
            } else {
                CarveInfo { size: None, extension, size_exact: false }
            }
        }
        _ => {
            // Unknown format: user will need to specify size manually
            CarveInfo { size: None, extension, size_exact: false }
        }
    }
}

/// Find pattern in data, return position of start.
fn find_pattern(data: &[u8], pattern: &[u8]) -> Option<usize> {
    data.windows(pattern.len()).position(|w| w == pattern)
}

/// Find pattern in data searching from the end.
fn find_pattern_reverse(data: &[u8], pattern: &[u8]) -> Option<usize> {
    data.windows(pattern.len()).rposition(|w| w == pattern)
}

/// Parse PE file to get total size.
fn parse_pe_size(data: &[u8]) -> Option<u64> {
    if data.len() < 0x40 {
        return None;
    }

    // Get PE header offset
    let pe_offset = u32::from_le_bytes([data[0x3C], data[0x3D], data[0x3E], data[0x3F]]) as usize;

    if pe_offset + 0x18 > data.len() {
        return None;
    }

    // Verify PE signature
    if &data[pe_offset..pe_offset + 4] != b"PE\x00\x00" {
        return None;
    }

    // Number of sections at PE+6
    let num_sections = u16::from_le_bytes([data[pe_offset + 6], data[pe_offset + 7]]) as usize;

    // Optional header size at PE+20
    let opt_header_size = u16::from_le_bytes([data[pe_offset + 20], data[pe_offset + 21]]) as usize;

    // Section headers start after optional header
    let section_table_offset = pe_offset + 24 + opt_header_size;

    // Each section header is 40 bytes
    // Find the section with highest (PointerToRawData + SizeOfRawData)
    let mut max_end: u64 = 0;
    for i in 0..num_sections {
        let sec_offset = section_table_offset + i * 40;
        if sec_offset + 40 > data.len() {
            break;
        }

        let size_of_raw = u32::from_le_bytes([
            data[sec_offset + 16], data[sec_offset + 17],
            data[sec_offset + 18], data[sec_offset + 19],
        ]) as u64;

        let ptr_to_raw = u32::from_le_bytes([
            data[sec_offset + 20], data[sec_offset + 21],
            data[sec_offset + 22], data[sec_offset + 23],
        ]) as u64;

        let end = ptr_to_raw + size_of_raw;
        if end > max_end {
            max_end = end;
        }
    }

    if max_end > 0 { Some(max_end) } else { None }
}

/// Parse ELF file to get total size.
fn parse_elf_size(data: &[u8]) -> Option<u64> {
    if data.len() < 52 {
        return None;
    }

    // Check ELF magic
    if &data[0..4] != b"\x7FELF" {
        return None;
    }

    let is_64bit = data[4] == 2;

    if is_64bit {
        if data.len() < 64 {
            return None;
        }

        // e_shoff (section header offset) at 0x28 (8 bytes)
        let sh_off = u64::from_le_bytes([
            data[0x28], data[0x29], data[0x2A], data[0x2B],
            data[0x2C], data[0x2D], data[0x2E], data[0x2F],
        ]);

        // e_shentsize at 0x3A (2 bytes)
        let sh_ent_size = u16::from_le_bytes([data[0x3A], data[0x3B]]) as u64;

        // e_shnum at 0x3C (2 bytes)
        let sh_num = u16::from_le_bytes([data[0x3C], data[0x3D]]) as u64;

        Some(sh_off + sh_ent_size * sh_num)
    } else {
        // 32-bit ELF
        // e_shoff at 0x20 (4 bytes)
        let sh_off = u32::from_le_bytes([data[0x20], data[0x21], data[0x22], data[0x23]]) as u64;

        // e_shentsize at 0x2E (2 bytes)
        let sh_ent_size = u16::from_le_bytes([data[0x2E], data[0x2F]]) as u64;

        // e_shnum at 0x30 (2 bytes)
        let sh_num = u16::from_le_bytes([data[0x30], data[0x31]]) as u64;

        Some(sh_off + sh_ent_size * sh_num)
    }
}

/// Scan the first `scan_len` bytes of `data` for known signatures.
/// Returns matches sorted by offset.
pub fn detect_signatures(data: &[u8], scan_len: usize) -> Vec<SignatureMatch> {
    let scan_end = data.len().min(scan_len);
    let mut matches = Vec::new();

    for sig in SIGNATURES {
        if sig.magic.len() > scan_end {
            continue;
        }

        if let Some(fixed) = sig.fixed_offset {
            // Only check at the fixed offset
            let off = fixed as usize;
            if off + sig.magic.len() <= data.len()
                && &data[off..off + sig.magic.len()] == sig.magic
            {
                matches.push(SignatureMatch {
                    offset: fixed,
                    name: sig.name,
                    magic_len: sig.magic.len(),
                });
            }
        } else {
            // Scan through the data
            let end = scan_end.saturating_sub(sig.magic.len() - 1);
            for i in 0..end {
                if &data[i..i + sig.magic.len()] == sig.magic {
                    matches.push(SignatureMatch {
                        offset: i as u64,
                        name: sig.name,
                        magic_len: sig.magic.len(),
                    });
                    // Don't report the same signature at every byte — skip ahead
                    // (but keep scanning for other signatures)
                }
            }
        }
    }

    matches.sort_by_key(|m| m.offset);
    // Dedup: same offset + same name
    matches.dedup_by(|a, b| a.offset == b.offset && a.name == b.name);
    matches
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_elf() {
        let mut data = vec![0u8; 256];
        data[0] = 0x7F;
        data[1] = b'E';
        data[2] = b'L';
        data[3] = b'F';
        let hits = detect_signatures(&data, 256);
        assert!(hits.iter().any(|h| h.name == "ELF" && h.offset == 0));
    }

    #[test]
    fn detect_pdf_embedded() {
        let mut data = vec![0u8; 1024];
        // PDF at offset 100
        data[100..105].copy_from_slice(b"%PDF-");
        let hits = detect_signatures(&data, 1024);
        assert!(hits.iter().any(|h| h.name == "PDF" && h.offset == 100));
    }

    #[test]
    fn detect_png_and_jpeg() {
        let mut data = vec![0u8; 512];
        // PNG at offset 0
        data[0..8].copy_from_slice(&[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]);
        // JPEG at offset 256
        data[256..259].copy_from_slice(&[0xFF, 0xD8, 0xFF]);
        let hits = detect_signatures(&data, 512);
        assert!(hits.iter().any(|h| h.name == "PNG" && h.offset == 0));
        assert!(hits.iter().any(|h| h.name == "JPEG" && h.offset == 256));
    }

    #[test]
    fn detect_sqlite() {
        let mut data = vec![0u8; 64];
        data[0..16].copy_from_slice(b"SQLite format 3\x00");
        let hits = detect_signatures(&data, 64);
        assert!(hits.iter().any(|h| h.name == "SQLite"));
    }

    #[test]
    fn detect_nothing() {
        let data = vec![0xAA; 256];
        let hits = detect_signatures(&data, 256);
        assert!(hits.is_empty());
    }

    #[test]
    fn detect_zip_embedded() {
        let mut data = vec![0u8; 2048];
        data[1000..1004].copy_from_slice(b"PK\x03\x04");
        let hits = detect_signatures(&data, 2048);
        assert!(hits.iter().any(|h| h.name == "ZIP/JAR/APK/DOCX" && h.offset == 1000));
    }
}
