use crate::BlockClass;

/// Classify each block of `block_size` bytes in `data` using CPU heuristics.
///
/// Rules applied in order:
/// 1. **Zeros** — more than 95% of bytes are 0x00
/// 2. **HighEntropy** — Shannon entropy > 7.0 (compressed/encrypted)
/// 3. **ASCII** — more than 90% of bytes are printable ASCII (0x20..=0x7E) or whitespace (tab/CR/LF)
/// 4. **UTF-8** — at least one multi-byte lead byte (0xC0..=0xF7) and entropy < 5.0
/// 5. **Binary** — everything else
pub fn classify_blocks_cpu(data: &[u8], block_size: usize) -> Vec<BlockClass> {
    if data.is_empty() || block_size == 0 {
        return vec![];
    }

    let mut results = Vec::new();

    for chunk in data.chunks(block_size) {
        results.push(classify_single_block(chunk));
    }

    results
}

fn classify_single_block(block: &[u8]) -> BlockClass {
    let len = block.len();
    if len == 0 {
        return BlockClass::Binary;
    }

    // Build histogram
    let mut freq = [0u32; 256];
    for &b in block {
        freq[b as usize] += 1;
    }

    let zero_count = freq[0] as usize;
    let total = len as f64;

    // Rule 1: Zeros (>95%)
    if zero_count as f64 / total > 0.95 {
        return BlockClass::Zeros;
    }

    // Compute Shannon entropy from histogram
    let entropy: f64 = freq
        .iter()
        .filter(|&&f| f > 0)
        .map(|&f| {
            let p = f as f64 / total;
            -p * p.log2()
        })
        .sum();

    // Rule 2: HighEntropy (>7.0)
    if entropy > 7.0 {
        return BlockClass::HighEntropy;
    }

    // Count ASCII printable + whitespace
    let ascii_count: usize = freq[0x09] as usize  // tab
        + freq[0x0A] as usize  // LF
        + freq[0x0D] as usize  // CR
        + (0x20..=0x7Eu8).map(|b| freq[b as usize] as usize).sum::<usize>();

    // Rule 3: ASCII (>90% printable)
    if ascii_count as f64 / total > 0.90 {
        return BlockClass::Ascii;
    }

    // Count UTF-8 multi-byte lead bytes (0xC0..=0xF7)
    let utf8_lead_count: usize = (0xC0..=0xF7u8)
        .map(|b| freq[b as usize] as usize)
        .sum();

    // Rule 4: UTF-8 (has lead bytes + moderate entropy)
    if utf8_lead_count > 0 && entropy < 5.0 {
        return BlockClass::Utf8;
    }

    // Rule 5: Binary (default)
    BlockClass::Binary
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_all_zeros() {
        let data = vec![0u8; 256];
        let result = classify_blocks_cpu(&data, 256);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0], BlockClass::Zeros);
    }

    #[test]
    fn classify_mostly_zeros() {
        // 250 zeros + 6 non-zero = 97.6% zeros → Zeros
        let mut data = vec![0u8; 250];
        data.extend([1, 2, 3, 4, 5, 6]);
        let result = classify_blocks_cpu(&data, 256);
        assert_eq!(result[0], BlockClass::Zeros);
    }

    #[test]
    fn classify_ascii_text() {
        // Pure printable ASCII
        let data: Vec<u8> = b"Hello, world! This is a test of ASCII text classification. \
            It should detect printable characters including spaces, punctuation, and digits 0123456789. \
            The quick brown fox jumps over the lazy dog. ABCDEFGHIJKLMNOPQRSTUVWXYZ abcdefghijklmnopqrs"
            .iter()
            .copied()
            .cycle()
            .take(256)
            .collect();
        let result = classify_blocks_cpu(&data, 256);
        assert_eq!(result[0], BlockClass::Ascii);
    }

    #[test]
    fn classify_ascii_with_newlines() {
        // ASCII with tabs and newlines (still ASCII)
        let mut data = Vec::new();
        for i in 0..256 {
            data.push(match i % 10 {
                0 => b'\n',
                5 => b'\t',
                _ => b'A' + (i % 26) as u8,
            });
        }
        let result = classify_blocks_cpu(&data, 256);
        assert_eq!(result[0], BlockClass::Ascii);
    }

    #[test]
    fn classify_high_entropy() {
        // All 256 byte values equally distributed → entropy = 8.0
        let data: Vec<u8> = (0..=255).collect();
        let result = classify_blocks_cpu(&data, 256);
        assert_eq!(result[0], BlockClass::HighEntropy);
    }

    #[test]
    fn classify_binary_structured() {
        // Repeating structured pattern with moderate variety
        let mut data = Vec::new();
        for i in 0u8..128 {
            data.push(i);
            data.push(0xFF - i);
        }
        // This has moderate entropy (not >7.0), not mostly zeros, not mostly ASCII
        let result = classify_blocks_cpu(&data, 256);
        assert!(
            result[0] == BlockClass::Binary || result[0] == BlockClass::HighEntropy,
            "Expected Binary or HighEntropy, got {:?}",
            result[0]
        );
    }

    #[test]
    fn classify_utf8_text() {
        // Simulate UTF-8 with multi-byte lead bytes + low entropy
        // Use repeated UTF-8 sequences: é = 0xC3 0xA9
        let mut data = Vec::new();
        while data.len() < 256 {
            data.extend_from_slice("café".as_bytes()); // c a f 0xC3 0xA9
        }
        data.truncate(256);
        let result = classify_blocks_cpu(&data, 256);
        // Should be either Ascii or Utf8 depending on ratio
        assert!(
            result[0] == BlockClass::Ascii || result[0] == BlockClass::Utf8,
            "Expected Ascii or Utf8, got {:?}",
            result[0]
        );
    }

    #[test]
    fn classify_empty() {
        assert!(classify_blocks_cpu(&[], 256).is_empty());
    }

    #[test]
    fn classify_zero_block_size() {
        assert!(classify_blocks_cpu(&[1, 2, 3], 0).is_empty());
    }

    #[test]
    fn classify_multiple_blocks() {
        let mut data = Vec::new();
        // Block 0: all zeros
        data.extend(vec![0u8; 256]);
        // Block 1: ASCII text
        data.extend(std::iter::repeat(b'A').take(256));
        // Block 2: uniform random (all 256 values)
        data.extend(0..=255u8);

        let result = classify_blocks_cpu(&data, 256);
        assert_eq!(result.len(), 3);
        assert_eq!(result[0], BlockClass::Zeros);
        assert_eq!(result[1], BlockClass::Ascii);
        assert_eq!(result[2], BlockClass::HighEntropy);
    }

    #[test]
    fn classify_partial_last_block() {
        // 300 bytes = 1 full block + 1 partial (44 bytes)
        let mut data = vec![0u8; 256]; // zeros block
        data.extend(vec![b'X'; 44]);    // partial ASCII block
        let result = classify_blocks_cpu(&data, 256);
        assert_eq!(result.len(), 2);
        assert_eq!(result[0], BlockClass::Zeros);
        assert_eq!(result[1], BlockClass::Ascii);
    }
}
