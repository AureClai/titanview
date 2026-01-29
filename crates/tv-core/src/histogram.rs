//! Byte histogram computation for frequency analysis.
//!
//! Computes the distribution of byte values (0-255) in a data block,
//! useful for identifying encrypted/compressed data vs. structured data.

/// Histogram of byte values (0-255).
#[derive(Debug, Clone)]
pub struct ByteHistogram {
    /// Count of each byte value (index = byte value).
    pub counts: [u64; 256],
    /// Total number of bytes analyzed.
    pub total: u64,
}

impl Default for ByteHistogram {
    fn default() -> Self {
        Self::new()
    }
}

impl ByteHistogram {
    /// Create an empty histogram.
    pub fn new() -> Self {
        Self {
            counts: [0; 256],
            total: 0,
        }
    }

    /// Compute histogram from data.
    pub fn from_data(data: &[u8]) -> Self {
        let mut hist = Self::new();
        for &byte in data {
            hist.counts[byte as usize] += 1;
        }
        hist.total = data.len() as u64;
        hist
    }

    /// Merge another histogram into this one.
    pub fn merge(&mut self, other: &ByteHistogram) {
        for i in 0..256 {
            self.counts[i] += other.counts[i];
        }
        self.total += other.total;
    }

    /// Get the frequency (probability) of a byte value.
    pub fn frequency(&self, byte: u8) -> f64 {
        if self.total == 0 {
            0.0
        } else {
            self.counts[byte as usize] as f64 / self.total as f64
        }
    }

    /// Get all frequencies as a slice.
    pub fn frequencies(&self) -> [f64; 256] {
        let mut freqs = [0.0; 256];
        if self.total > 0 {
            let total = self.total as f64;
            for i in 0..256 {
                freqs[i] = self.counts[i] as f64 / total;
            }
        }
        freqs
    }

    /// Get the maximum count.
    pub fn max_count(&self) -> u64 {
        *self.counts.iter().max().unwrap_or(&0)
    }

    /// Calculate Shannon entropy from histogram.
    pub fn entropy(&self) -> f64 {
        if self.total == 0 {
            return 0.0;
        }

        let total = self.total as f64;
        let mut entropy = 0.0;

        for &count in &self.counts {
            if count > 0 {
                let p = count as f64 / total;
                entropy -= p * p.log2();
            }
        }

        entropy
    }

    /// Get statistics about the distribution.
    pub fn stats(&self) -> HistogramStats {
        let max_count = self.max_count();
        let min_count = *self.counts.iter().min().unwrap_or(&0);

        // Count how many byte values appear
        let unique_values = self.counts.iter().filter(|&&c| c > 0).count() as u32;

        // Find most common byte
        let most_common = self.counts.iter()
            .enumerate()
            .max_by_key(|(_, &c)| c)
            .map(|(i, _)| i as u8)
            .unwrap_or(0);

        // Calculate flatness (how uniform the distribution is)
        // 1.0 = perfectly flat (all values equally likely)
        // 0.0 = all data is one byte value
        let expected = self.total as f64 / 256.0;
        let flatness = if self.total > 0 && expected > 0.0 {
            let variance: f64 = self.counts.iter()
                .map(|&c| (c as f64 - expected).powi(2))
                .sum::<f64>() / 256.0;
            let std_dev = variance.sqrt();
            let max_std_dev = expected; // Maximum possible std dev
            1.0 - (std_dev / max_std_dev).min(1.0)
        } else {
            0.0
        };

        HistogramStats {
            total: self.total,
            unique_values,
            most_common,
            most_common_count: max_count,
            least_common_count: min_count,
            entropy: self.entropy(),
            flatness,
        }
    }

    /// Check if distribution suggests encrypted/compressed data.
    /// Encrypted data typically has very flat distribution (high entropy).
    pub fn looks_encrypted(&self) -> bool {
        let stats = self.stats();
        stats.entropy > 7.5 && stats.flatness > 0.8
    }

    /// Check if distribution suggests ASCII text.
    pub fn looks_ascii(&self) -> bool {
        if self.total == 0 {
            return false;
        }

        // Count bytes in printable ASCII range (0x20-0x7E) plus common whitespace
        let printable: u64 = self.counts[0x20..=0x7E].iter().sum::<u64>()
            + self.counts[0x09]  // Tab
            + self.counts[0x0A]  // LF
            + self.counts[0x0D]; // CR

        printable as f64 / self.total as f64 > 0.85
    }
}

/// Statistics derived from a histogram.
#[derive(Debug, Clone, Copy)]
pub struct HistogramStats {
    /// Total bytes analyzed.
    pub total: u64,
    /// Number of unique byte values present.
    pub unique_values: u32,
    /// Most common byte value.
    pub most_common: u8,
    /// Count of most common byte.
    pub most_common_count: u64,
    /// Count of least common byte (that appears at least once).
    pub least_common_count: u64,
    /// Shannon entropy.
    pub entropy: f64,
    /// Distribution flatness (0.0 = peaked, 1.0 = uniform).
    pub flatness: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_histogram() {
        let hist = ByteHistogram::new();
        assert_eq!(hist.total, 0);
        assert_eq!(hist.entropy(), 0.0);
    }

    #[test]
    fn test_single_value() {
        let data = vec![0x42; 100];
        let hist = ByteHistogram::from_data(&data);
        assert_eq!(hist.total, 100);
        assert_eq!(hist.counts[0x42], 100);
        assert_eq!(hist.entropy(), 0.0); // Only one value = 0 entropy
    }

    #[test]
    fn test_uniform_distribution() {
        // Create data with all 256 values equally represented
        let data: Vec<u8> = (0..=255u8).cycle().take(256 * 100).collect();
        let hist = ByteHistogram::from_data(&data);

        assert_eq!(hist.total, 256 * 100);
        for i in 0..256 {
            assert_eq!(hist.counts[i], 100);
        }

        // Uniform distribution has maximum entropy (8.0 for 256 values)
        let entropy = hist.entropy();
        assert!((entropy - 8.0).abs() < 0.01, "Expected ~8.0, got {}", entropy);
    }

    #[test]
    fn test_ascii_detection() {
        let text = b"Hello, World! This is a test of ASCII text detection.";
        let hist = ByteHistogram::from_data(text);
        assert!(hist.looks_ascii());

        let binary = vec![0x00, 0x01, 0x02, 0xFF, 0xFE, 0xFD];
        let hist2 = ByteHistogram::from_data(&binary);
        assert!(!hist2.looks_ascii());
    }

    #[test]
    fn test_frequency() {
        let data = vec![0x00, 0x00, 0x00, 0x01];
        let hist = ByteHistogram::from_data(&data);

        assert_eq!(hist.frequency(0x00), 0.75);
        assert_eq!(hist.frequency(0x01), 0.25);
        assert_eq!(hist.frequency(0x02), 0.0);
    }

    #[test]
    fn test_merge() {
        let data1 = vec![0x00; 50];
        let data2 = vec![0x01; 50];

        let mut hist = ByteHistogram::from_data(&data1);
        let hist2 = ByteHistogram::from_data(&data2);
        hist.merge(&hist2);

        assert_eq!(hist.total, 100);
        assert_eq!(hist.counts[0x00], 50);
        assert_eq!(hist.counts[0x01], 50);
    }
}
