/// Compute per-block Shannon entropy on the CPU.
/// Returns one f32 per block. Entropy ranges from 0.0 (uniform) to 8.0 (max).
pub fn compute_entropy_cpu(data: &[u8], block_size: usize) -> Vec<f32> {
    if data.is_empty() || block_size == 0 {
        return vec![];
    }

    let mut results = Vec::new();

    for chunk in data.chunks(block_size) {
        let mut freq = [0u32; 256];
        for &b in chunk {
            freq[b as usize] += 1;
        }

        let total = chunk.len() as f64;
        let entropy: f64 = freq
            .iter()
            .filter(|&&f| f > 0)
            .map(|&f| {
                let p = f as f64 / total;
                -p * p.log2()
            })
            .sum();

        results.push(entropy as f32);
    }

    results
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cpu_entropy_all_zeros() {
        let data = vec![0u8; 256];
        let result = compute_entropy_cpu(&data, 256);
        assert_eq!(result.len(), 1);
        assert!(result[0].abs() < 0.001);
    }

    #[test]
    fn cpu_entropy_uniform() {
        let data: Vec<u8> = (0..=255).collect();
        let result = compute_entropy_cpu(&data, 256);
        assert_eq!(result.len(), 1);
        assert!((result[0] - 8.0).abs() < 0.001);
    }

    #[test]
    fn cpu_entropy_two_values() {
        // 128 zeros + 128 ones → entropy = 1.0
        let mut data = vec![0u8; 128];
        data.extend(vec![1u8; 128]);
        let result = compute_entropy_cpu(&data, 256);
        assert!((result[0] - 1.0).abs() < 0.001);
    }

    #[test]
    fn cpu_entropy_empty() {
        assert!(compute_entropy_cpu(&[], 256).is_empty());
    }

    #[test]
    fn cpu_entropy_multiple_blocks() {
        let mut data = vec![0u8; 256]; // block 0: all zeros
        data.extend(0..=255u8);        // block 1: uniform
        let result = compute_entropy_cpu(&data, 256);
        assert_eq!(result.len(), 2);
        assert!(result[0].abs() < 0.001);
        assert!((result[1] - 8.0).abs() < 0.001);
    }
}
