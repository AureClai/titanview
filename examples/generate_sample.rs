//! Run with: rustc generate_sample.rs && ./generate_sample
//! Or: cargo script generate_sample.rs (if cargo-script is installed)

use std::fs::File;
use std::io::Write;

fn main() -> std::io::Result<()> {
    let mut file = File::create("sample_file.tvts")?;

    // Header (64 bytes)
    let mut header = vec![0u8; 64];

    // Magic: TVTS (bytes 0-3)
    header[0] = b'T';
    header[1] = b'V';
    header[2] = b'T';
    header[3] = b'S';

    // Version: 1.5 (bytes 4-5)
    header[4] = 1; // major
    header[5] = 5; // minor

    // Flags: COMPRESSED | HAS_CHECKSUM = 0x11 (bytes 6-7, little endian u16)
    header[6] = 0x11;
    header[7] = 0x00;

    // File type: DATA = 3 (byte 8)
    header[8] = 3;

    // Reserved (bytes 9-11)
    header[9] = 0;
    header[10] = 0;
    header[11] = 0;

    // Data offset: 64 (bytes 12-15, little endian u32)
    header[12] = 64;
    header[13] = 0;
    header[14] = 0;
    header[15] = 0;

    // Data size: 256 (bytes 16-19, little endian u32)
    header[16] = 0;
    header[17] = 1;
    header[18] = 0;
    header[19] = 0;

    // Entry count: 16 (bytes 20-21, little endian u16)
    header[20] = 16;
    header[21] = 0;

    // Checksum: 0xABCD (bytes 22-23, little endian u16)
    header[22] = 0xCD;
    header[23] = 0xAB;

    // Timestamp: 1706500000 (Jan 29, 2024) (bytes 24-31, little endian u64)
    let timestamp: u64 = 1706500000;
    header[24..32].copy_from_slice(&timestamp.to_le_bytes());

    // Name: "example_data_file" (bytes 32-63)
    let name = b"example_data_file";
    header[32..32 + name.len()].copy_from_slice(name);

    file.write_all(&header)?;

    // Data section (256 bytes of sample data)
    // 16 entries, each 16 bytes
    for i in 0..16u8 {
        let mut entry = [0u8; 16];
        // Entry ID
        entry[0] = i;
        // Some pattern data
        entry[1] = i * 2;
        entry[2] = i * 3;
        entry[3] = i * 4;
        // Fill rest with incrementing values
        for j in 4..16 {
            entry[j] = (i.wrapping_add(j as u8)) ^ 0x55;
        }
        file.write_all(&entry)?;
    }

    // Add some trailing data to make it more interesting
    // ASCII text section
    let text = b"This is sample text data in the TitanView test file format.\n\
                 It demonstrates various data types and patterns.\n\
                 You can use the Structure Inspector to parse the header.\n";
    file.write_all(text)?;

    // High entropy section (pseudo-random)
    let mut random_data = vec![0u8; 128];
    let mut seed: u32 = 0xDEADBEEF;
    for byte in random_data.iter_mut() {
        seed = seed.wrapping_mul(1103515245).wrapping_add(12345);
        *byte = (seed >> 16) as u8;
    }
    file.write_all(&random_data)?;

    // Zero section
    file.write_all(&[0u8; 64])?;

    println!("Created sample_file.tvts ({} bytes)", 64 + 256 + text.len() + 128 + 64);

    Ok(())
}
