//! Generates test fixture files in `test-fixtures/`.
//! Run with: `cargo run -p tv-core --example gen_fixtures`

use std::fs;
use std::path::Path;

fn main() {
    let dir = Path::new("test-fixtures");
    fs::create_dir_all(dir).expect("failed to create test-fixtures/");

    gen_ascii_log(dir);
    gen_mixed_entropy(dir);
    gen_embedded_magic(dir);
    gen_uniform_random(dir);
    gen_all_zeros(dir);

    println!("All fixtures generated in {}", dir.display());
}

/// ~2 KB simulated server log with timestamps, levels, IPs.
fn gen_ascii_log(dir: &Path) {
    let lines = [
        "2025-01-15T08:00:01Z INFO  [server] Listening on 0.0.0.0:8080",
        "2025-01-15T08:00:02Z DEBUG [auth]   Connection from 192.168.1.42",
        "2025-01-15T08:00:02Z INFO  [auth]   User admin authenticated",
        "2025-01-15T08:00:03Z WARN  [db]     Slow query: SELECT * FROM users (230ms)",
        "2025-01-15T08:00:03Z INFO  [api]    GET /api/v1/users -> 200 (12ms)",
        "2025-01-15T08:00:04Z ERROR [api]    POST /api/v1/upload -> 500 Internal Server Error",
        "2025-01-15T08:00:04Z DEBUG [server] Connection from 10.0.0.5",
        "2025-01-15T08:00:05Z INFO  [auth]   User guest authenticated",
        "2025-01-15T08:00:05Z INFO  [api]    GET /api/v1/status -> 200 (2ms)",
        "2025-01-15T08:00:06Z WARN  [server] High memory usage: 87%",
        "2025-01-15T08:00:06Z ERROR [db]     Connection pool exhausted, retrying...",
        "2025-01-15T08:00:07Z INFO  [db]     Connection restored",
        "2025-01-15T08:00:07Z DEBUG [api]    Request headers: Accept: application/json",
        "2025-01-15T08:00:08Z INFO  [api]    GET /api/v1/metrics -> 200 (5ms)",
        "2025-01-15T08:00:08Z WARN  [auth]   Failed login attempt from 203.0.113.99",
        "2025-01-15T08:00:09Z ERROR [auth]   Brute force detected from 203.0.113.99, blocking IP",
        "2025-01-15T08:00:09Z INFO  [server] Active connections: 42",
        "2025-01-15T08:00:10Z INFO  [api]    DELETE /api/v1/sessions/expired -> 204 (8ms)",
        "2025-01-15T08:00:10Z DEBUG [gc]     Garbage collection completed in 15ms",
        "2025-01-15T08:00:11Z INFO  [server] Heartbeat OK, uptime: 3d 14h 22m",
        "2025-01-15T08:00:11Z WARN  [disk]   Partition /data at 92% capacity",
        "2025-01-15T08:00:12Z ERROR [api]    GET /api/v1/export -> 503 Service Unavailable",
        "2025-01-15T08:00:12Z INFO  [api]    Retry queued for /api/v1/export",
        "2025-01-15T08:00:13Z DEBUG [server] TLS handshake completed with 10.0.0.5",
        "2025-01-15T08:00:13Z INFO  [api]    POST /api/v1/data -> 201 Created (45ms)",
    ];
    let content = lines.join("\n") + "\n";
    fs::write(dir.join("ascii_log.txt"), content).expect("failed to write ascii_log.txt");
    println!("  ascii_log.txt     ({} bytes)", lines.join("\n").len() + 1);
}

/// 4 KB with 4 distinct entropy zones (1 KB each):
/// - Block 0: all zeros (entropy ~0)
/// - Block 1: low entropy (repeating short pattern)
/// - Block 2: high entropy (pseudo-random via simple LCG)
/// - Block 3: repeating ASCII pattern "ABCD"
fn gen_mixed_entropy(dir: &Path) {
    let mut data = Vec::with_capacity(4096);

    // Block 0: zeros
    data.extend_from_slice(&[0u8; 1024]);

    // Block 1: low entropy — cycling through 4 values
    for i in 0..1024u16 {
        data.push((i % 4) as u8);
    }

    // Block 2: high entropy — LCG pseudo-random
    let mut state: u32 = 0xDEAD_BEEF;
    for _ in 0..1024 {
        state = state.wrapping_mul(1664525).wrapping_add(1013904223);
        data.push((state >> 16) as u8);
    }

    // Block 3: repeating ASCII "ABCD"
    for _ in 0..256 {
        data.extend_from_slice(b"ABCD");
    }

    assert_eq!(data.len(), 4096);
    fs::write(dir.join("mixed_entropy.bin"), &data).expect("failed to write mixed_entropy.bin");
    println!("  mixed_entropy.bin (4096 bytes, 4 zones)");
}

/// 1 KB binary with known magic bytes at specific offsets:
/// - Offset 0x000: ELF header magic  (7F 45 4C 46)
/// - Offset 0x100: JPEG SOI marker   (FF D8 FF E0)
/// - Offset 0x200: PNG signature      (89 50 4E 47)
/// - Rest: 0xCC fill (easily distinguishable)
fn gen_embedded_magic(dir: &Path) {
    let mut data = vec![0xCCu8; 1024];

    // ELF magic at offset 0
    data[0x000] = 0x7F;
    data[0x001] = 0x45; // E
    data[0x002] = 0x4C; // L
    data[0x003] = 0x46; // F

    // JPEG SOI at offset 256
    data[0x100] = 0xFF;
    data[0x101] = 0xD8;
    data[0x102] = 0xFF;
    data[0x103] = 0xE0;

    // PNG signature at offset 512
    data[0x200] = 0x89;
    data[0x201] = 0x50; // P
    data[0x202] = 0x4E; // N
    data[0x203] = 0x47; // G

    fs::write(dir.join("embedded_magic.bin"), &data).expect("failed to write embedded_magic.bin");
    println!("  embedded_magic.bin (1024 bytes, 3 magic signatures)");
}

/// 4 KB of pseudo-random bytes (max entropy ~8.0).
fn gen_uniform_random(dir: &Path) {
    let mut data = Vec::with_capacity(4096);
    // Use a different LCG seed for variety
    let mut state: u64 = 0xCAFE_BABE_1234_5678;
    for _ in 0..4096 {
        state = state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        data.push((state >> 33) as u8);
    }
    fs::write(dir.join("uniform_random.bin"), &data).expect("failed to write uniform_random.bin");
    println!("  uniform_random.bin (4096 bytes)");
}

/// 4 KB of zero bytes (min entropy = 0.0).
fn gen_all_zeros(dir: &Path) {
    let data = vec![0u8; 4096];
    fs::write(dir.join("all_zeros.bin"), &data).expect("failed to write all_zeros.bin");
    println!("  all_zeros.bin     (4096 bytes)");
}
