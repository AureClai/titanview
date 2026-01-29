//! Example: Open a file with MappedFile and explore its contents.
//!
//! Usage:
//!   cargo run -p tv-core --example explore_file -- test-fixtures/ascii_log.txt
//!   cargo run -p tv-core --example explore_file -- test-fixtures/mixed_entropy.bin
//!   cargo run -p tv-core --example explore_file -- test-fixtures/embedded_magic.bin

use std::path::Path;
use tv_core::{FileRegion, MappedFile, ViewPort};

fn main() {
    let path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| {
            eprintln!("Usage: explore_file <path>");
            eprintln!("  Try: cargo run -p tv-core --example explore_file -- test-fixtures/ascii_log.txt");
            std::process::exit(1);
        });

    let file = MappedFile::open(Path::new(&path)).expect("failed to open file");

    println!("=== File: {} ===", path);
    println!("Size: {} bytes ({:.2} KB)", file.len(), file.len() as f64 / 1024.0);
    println!();

    // --- FileRegion: read specific byte ranges ---
    println!("--- FileRegion: first 64 bytes ---");
    let region = FileRegion::new(0, 64.min(file.len()));
    let data = file.slice(region);
    print_hex_ascii(0, data);
    println!();

    // --- FileRegion: read from the middle ---
    if file.len() > 128 {
        let mid = file.len() / 2;
        println!("--- FileRegion: 32 bytes from offset 0x{:X} ---", mid);
        let region = FileRegion::new(mid, 32);
        let data = file.slice(region);
        print_hex_ascii(mid, data);
        println!();
    }

    // --- ViewPort: simulate what the UI would display ---
    println!("--- ViewPort simulation ---");
    let viewport = ViewPort::new(0, 256);
    let clamped = viewport.clamp(file.len());
    println!(
        "Requested: start=0x{:X} visible={} bytes",
        viewport.start, viewport.visible_bytes
    );
    println!(
        "Clamped:   start=0x{:X} visible={} bytes",
        clamped.start, clamped.visible_bytes
    );

    // Show what the viewport would render
    let vp_data = file.slice(clamped.as_region());
    let lines = vp_data.len() / 16 + if vp_data.len() % 16 != 0 { 1 } else { 0 };
    println!("Would render {} hex lines", lines);
    println!();

    // --- FileRegion overlap detection ---
    println!("--- Overlap detection ---");
    let r1 = FileRegion::new(0, 100);
    let r2 = FileRegion::new(50, 100);
    let r3 = FileRegion::new(200, 50);
    println!("R1=[0..100], R2=[50..150], R3=[200..250]");
    println!("R1 overlaps R2? {} (expected: true)", r1.overlaps(&r2));
    println!("R1 overlaps R3? {} (expected: false)", r1.overlaps(&r3));
    println!("R1 contains R2? {} (expected: false)", r1.contains(&r2));
    println!();

    // --- Byte statistics (preview of what GPU entropy will do) ---
    println!("--- Byte frequency (first 1024 bytes) ---");
    let sample = file.slice(FileRegion::new(0, 1024.min(file.len())));
    let mut freq = [0u32; 256];
    for &b in sample {
        freq[b as usize] += 1;
    }
    let non_zero = freq.iter().filter(|&&f| f > 0).count();
    let total = sample.len() as f64;
    let entropy: f64 = freq.iter()
        .filter(|&&f| f > 0)
        .map(|&f| {
            let p = f as f64 / total;
            -p * p.log2()
        })
        .sum();
    println!("Unique byte values: {}/256", non_zero);
    println!("Shannon entropy: {:.3} bits/byte (max=8.0)", entropy);
    // Show top 5 most frequent bytes
    let mut ranked: Vec<(u8, u32)> = freq.iter().enumerate()
        .map(|(i, &f)| (i as u8, f))
        .filter(|(_, f)| *f > 0)
        .collect();
    ranked.sort_by(|a, b| b.1.cmp(&a.1));
    println!("Top 5 bytes:");
    for (byte, count) in ranked.iter().take(5) {
        let pct = *count as f64 / total * 100.0;
        let ch = if byte.is_ascii_graphic() || *byte == b' ' {
            format!("'{}'", *byte as char)
        } else {
            format!("   ")
        };
        println!("  0x{:02X} {} : {} ({:.1}%)", byte, ch, count, pct);
    }
}

/// Print bytes in classic hex dump format: offset | hex | ASCII
fn print_hex_ascii(base_offset: u64, data: &[u8]) {
    for (i, chunk) in data.chunks(16).enumerate() {
        let offset = base_offset + (i * 16) as u64;
        // Offset
        print!("{:08X}  ", offset);
        // Hex bytes
        for (j, &b) in chunk.iter().enumerate() {
            print!("{:02X} ", b);
            if j == 7 {
                print!(" ");
            }
        }
        // Pad if less than 16 bytes
        for j in chunk.len()..16 {
            print!("   ");
            if j == 7 {
                print!(" ");
            }
        }
        // ASCII
        print!(" |");
        for &b in chunk {
            if b.is_ascii_graphic() || b == b' ' {
                print!("{}", b as char);
            } else {
                print!(".");
            }
        }
        println!("|");
    }
}
