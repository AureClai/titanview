use memchr::memmem;
use rayon::prelude::*;

/// CPU-based pattern scan (naive byte-by-byte).
/// Returns all offsets where `pattern` occurs in `data`.
pub fn scan_pattern_cpu(data: &[u8], pattern: &[u8]) -> Vec<u64> {
    if pattern.is_empty() || data.len() < pattern.len() {
        return vec![];
    }
    let mut results = Vec::new();
    let limit = data.len() - pattern.len() + 1;
    for i in 0..limit {
        if &data[i..i + pattern.len()] == pattern {
            results.push(i as u64);
        }
    }
    results
}

/// High-performance parallel pattern scan using SIMD (memchr) + multi-threading (rayon).
/// Splits data across CPU cores, each using SIMD-accelerated search.
/// Returns sorted offsets where `pattern` occurs in `data`.
///
/// Performance: ~3-10 GB/s on modern CPUs (vs ~500 MB/s for naive or GPU with PCIe overhead).
pub fn scan_pattern_parallel(data: &[u8], pattern: &[u8]) -> Vec<u64> {
    if pattern.is_empty() || data.len() < pattern.len() {
        return vec![];
    }

    // For small data, just use single-threaded memchr (still SIMD)
    let min_parallel_size = 1024 * 1024; // 1 MB threshold
    if data.len() < min_parallel_size {
        return scan_pattern_simd(data, pattern);
    }

    // Determine chunk count based on available parallelism
    let num_threads = rayon::current_num_threads().max(1);
    let chunk_size = (data.len() / num_threads).max(min_parallel_size);
    let overlap = pattern.len().saturating_sub(1);

    // Build chunk ranges with overlap to catch boundary matches
    let mut chunks: Vec<(usize, usize)> = Vec::new();
    let mut start = 0;
    while start < data.len() {
        let end = (start + chunk_size).min(data.len());
        chunks.push((start, end));
        if end >= data.len() {
            break;
        }
        // Next chunk starts with overlap to catch patterns spanning boundaries
        start = end.saturating_sub(overlap);
    }

    // Parallel search with rayon
    let finder = memmem::Finder::new(pattern);
    let mut all_results: Vec<Vec<u64>> = chunks
        .par_iter()
        .map(|&(chunk_start, chunk_end)| {
            let chunk = &data[chunk_start..chunk_end];
            finder
                .find_iter(chunk)
                .map(|pos| (chunk_start + pos) as u64)
                .collect()
        })
        .collect();

    // Merge and deduplicate (overlapping regions may find same match twice)
    let mut results: Vec<u64> = all_results.drain(..).flatten().collect();
    results.sort_unstable();
    results.dedup();
    results
}

/// Single-threaded SIMD-accelerated search using memchr.
/// Used for small data or as building block for parallel search.
fn scan_pattern_simd(data: &[u8], pattern: &[u8]) -> Vec<u64> {
    if pattern.is_empty() || data.len() < pattern.len() {
        return vec![];
    }
    memmem::find_iter(data, pattern)
        .map(|pos| pos as u64)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cpu_scan_found() {
        let data = b"hello world hello";
        let hits = scan_pattern_cpu(data, b"hello");
        assert_eq!(hits, vec![0, 12]);
    }

    #[test]
    fn cpu_scan_not_found() {
        let data = b"hello world";
        let hits = scan_pattern_cpu(data, b"xyz");
        assert!(hits.is_empty());
    }

    #[test]
    fn cpu_scan_single_byte() {
        let data = vec![0xAA, 0xBB, 0xAA, 0xCC, 0xAA];
        let hits = scan_pattern_cpu(&data, &[0xAA]);
        assert_eq!(hits, vec![0, 2, 4]);
    }

    #[test]
    fn cpu_scan_empty_pattern() {
        let hits = scan_pattern_cpu(b"data", b"");
        assert!(hits.is_empty());
    }

    #[test]
    fn cpu_scan_pattern_longer_than_data() {
        let hits = scan_pattern_cpu(b"ab", b"abcdef");
        assert!(hits.is_empty());
    }

    // --- Parallel scanner tests ---

    #[test]
    fn parallel_scan_found() {
        let data = b"hello world hello";
        let hits = scan_pattern_parallel(data, b"hello");
        assert_eq!(hits, vec![0, 12]);
    }

    #[test]
    fn parallel_scan_not_found() {
        let data = b"hello world";
        let hits = scan_pattern_parallel(data, b"xyz");
        assert!(hits.is_empty());
    }

    #[test]
    fn parallel_scan_single_byte() {
        let data = vec![0xAA, 0xBB, 0xAA, 0xCC, 0xAA];
        let hits = scan_pattern_parallel(&data, &[0xAA]);
        assert_eq!(hits, vec![0, 2, 4]);
    }

    #[test]
    fn parallel_scan_empty_pattern() {
        let hits = scan_pattern_parallel(b"data", b"");
        assert!(hits.is_empty());
    }

    #[test]
    fn parallel_scan_pattern_longer_than_data() {
        let hits = scan_pattern_parallel(b"ab", b"abcdef");
        assert!(hits.is_empty());
    }

    #[test]
    fn parallel_scan_large_data() {
        // 4 MB of data with pattern every 1024 bytes
        let mut data = vec![0u8; 4 * 1024 * 1024];
        let pattern = b"MARKER";
        let mut expected: Vec<u64> = Vec::new();
        for i in (0..data.len()).step_by(1024) {
            if i + pattern.len() <= data.len() {
                data[i..i + pattern.len()].copy_from_slice(pattern);
                expected.push(i as u64);
            }
        }
        let hits = scan_pattern_parallel(&data, pattern);
        assert_eq!(hits, expected);
    }

    #[test]
    fn parallel_scan_boundary_crossing() {
        // Create data where pattern crosses chunk boundary
        // With 1MB threshold, this tests the overlap logic
        let mut data = vec![0u8; 2 * 1024 * 1024 + 100];
        let pattern = b"BOUNDARY";
        // Place pattern right at the 1MB mark (potential chunk boundary)
        let boundary_pos = 1024 * 1024 - 3; // Pattern straddles boundary
        data[boundary_pos..boundary_pos + pattern.len()].copy_from_slice(pattern);

        let hits = scan_pattern_parallel(&data, pattern);
        assert_eq!(hits, vec![boundary_pos as u64]);
    }

    #[test]
    fn parallel_matches_naive() {
        // Verify parallel results match naive implementation
        let data: Vec<u8> = (0..100_000).map(|i| (i % 256) as u8).collect();
        let pattern = &[0u8, 1, 2, 3];

        let naive_hits = scan_pattern_cpu(&data, pattern);
        let parallel_hits = scan_pattern_parallel(&data, pattern);

        assert_eq!(naive_hits, parallel_hits);
    }

    #[test]
    fn simd_scan_found() {
        let data = b"hello world hello";
        let hits = scan_pattern_simd(data, b"hello");
        assert_eq!(hits, vec![0, 12]);
    }
}
