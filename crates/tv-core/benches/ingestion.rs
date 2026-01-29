use std::io::{BufRead, BufReader, Read, Write};
use std::path::Path;

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use tv_core::{FileRegion, MappedFile};

const SIZES: &[(u64, &str)] = &[
    (1 << 20, "1MB"),
    (10 << 20, "10MB"),
    (100 << 20, "100MB"),
];

/// Create a temporary file of the given size filled with a repeating pattern.
fn create_temp_file(size: u64) -> tempfile::NamedTempFile {
    let mut f = tempfile::NamedTempFile::new().expect("failed to create temp file");
    let pattern: Vec<u8> = (0..=255u8).collect();
    let mut remaining = size as usize;
    while remaining > 0 {
        let chunk = remaining.min(pattern.len());
        f.write_all(&pattern[..chunk]).unwrap();
        remaining -= chunk;
    }
    f.flush().unwrap();
    f
}

/// Checksum used by all scan benchmarks to prevent elision.
fn checksum_bytes(data: &[u8]) -> u64 {
    let mut sum: u64 = 0;
    for &b in data {
        sum = sum.wrapping_add(b as u64);
    }
    sum
}

// ============================================================================
// Baseline: std::fs::read (allocate + copy entire file into a Vec<u8>)
// ============================================================================

fn bench_stdfs_read(c: &mut Criterion) {
    let mut group = c.benchmark_group("read_full_file");

    for &(size, label) in SIZES {
        let tmp = create_temp_file(size);

        group.throughput(Throughput::Bytes(size));

        // --- std::fs::read ---
        group.bench_with_input(
            BenchmarkId::new("std_fs_read", label),
            tmp.path(),
            |b, path| {
                b.iter(|| {
                    let data = std::fs::read(path).unwrap();
                    std::hint::black_box(checksum_bytes(&data));
                });
            },
        );

        // --- BufReader (8 KB default) ---
        group.bench_with_input(
            BenchmarkId::new("bufreader_8k", label),
            tmp.path(),
            |b, path| {
                b.iter(|| {
                    let f = std::fs::File::open(path).unwrap();
                    let mut reader = BufReader::new(f);
                    let mut sum: u64 = 0;
                    loop {
                        let buf = reader.fill_buf().unwrap();
                        if buf.is_empty() {
                            break;
                        }
                        sum = sum.wrapping_add(checksum_bytes(buf));
                        let len = buf.len();
                        reader.consume(len);
                    }
                    std::hint::black_box(sum);
                });
            },
        );

        // --- BufReader (64 KB) ---
        group.bench_with_input(
            BenchmarkId::new("bufreader_64k", label),
            tmp.path(),
            |b, path| {
                b.iter(|| {
                    let f = std::fs::File::open(path).unwrap();
                    let mut reader = BufReader::with_capacity(64 * 1024, f);
                    let mut sum: u64 = 0;
                    loop {
                        let buf = reader.fill_buf().unwrap();
                        if buf.is_empty() {
                            break;
                        }
                        sum = sum.wrapping_add(checksum_bytes(buf));
                        let len = buf.len();
                        reader.consume(len);
                    }
                    std::hint::black_box(sum);
                });
            },
        );

        // --- Read::read_to_end (heap alloc) ---
        group.bench_with_input(
            BenchmarkId::new("read_to_end", label),
            tmp.path(),
            |b, path| {
                b.iter(|| {
                    let mut f = std::fs::File::open(path).unwrap();
                    let mut buf = Vec::new();
                    f.read_to_end(&mut buf).unwrap();
                    std::hint::black_box(checksum_bytes(&buf));
                });
            },
        );

        // --- MappedFile (our mmap approach) ---
        group.bench_with_input(
            BenchmarkId::new("mmap_zero_copy", label),
            tmp.path(),
            |b, path| {
                b.iter(|| {
                    let mf = MappedFile::open(path).unwrap();
                    let data = mf.slice(FileRegion::new(0, mf.len()));
                    std::hint::black_box(checksum_bytes(data));
                });
            },
        );

        // --- MappedFile pre-opened (amortized, no open cost) ---
        let mf = MappedFile::open(tmp.path()).unwrap();
        group.bench_with_input(
            BenchmarkId::new("mmap_preopened", label),
            &mf,
            |b, mf| {
                b.iter(|| {
                    let data = mf.slice(FileRegion::new(0, mf.len()));
                    std::hint::black_box(checksum_bytes(data));
                });
            },
        );
    }

    group.finish();
}

// ============================================================================
// Sequential scan comparison: chunked reads vs mmap slices
// ============================================================================

fn bench_sequential_scan(c: &mut Criterion) {
    let mut group = c.benchmark_group("sequential_scan_64k_chunks");

    for &(size, label) in SIZES {
        let tmp = create_temp_file(size);

        group.throughput(Throughput::Bytes(size));

        // --- BufReader chunked scan ---
        group.bench_with_input(
            BenchmarkId::new("bufreader", label),
            tmp.path(),
            |b, path| {
                b.iter(|| {
                    let f = std::fs::File::open(path).unwrap();
                    let mut reader = BufReader::with_capacity(64 * 1024, f);
                    let mut sum: u64 = 0;
                    let mut buf = [0u8; 64 * 1024];
                    loop {
                        let n = reader.read(&mut buf).unwrap();
                        if n == 0 {
                            break;
                        }
                        sum = sum.wrapping_add(checksum_bytes(&buf[..n]));
                    }
                    std::hint::black_box(sum);
                });
            },
        );

        // --- mmap chunked scan (pre-opened) ---
        let mf = MappedFile::open(tmp.path()).unwrap();
        group.bench_with_input(
            BenchmarkId::new("mmap", label),
            &mf,
            |b, mf| {
                b.iter(|| {
                    let chunk_size: u64 = 64 * 1024;
                    let mut sum: u64 = 0;
                    let mut offset: u64 = 0;
                    while offset < mf.len() {
                        let data = mf.slice(FileRegion::new(offset, chunk_size));
                        sum = sum.wrapping_add(checksum_bytes(data));
                        offset += chunk_size;
                    }
                    std::hint::black_box(sum);
                });
            },
        );
    }

    group.finish();
}

// ============================================================================
// Random access: seek+read vs mmap slice
// ============================================================================

fn bench_random_access(c: &mut Criterion) {
    use std::io::{Seek, SeekFrom};

    let mut group = c.benchmark_group("random_access_1000x4KB");

    let tmp = create_temp_file(100 << 20);
    let file_len = 100u64 << 20;

    // Pre-generate deterministic offsets
    let mut offsets = Vec::with_capacity(1000);
    let mut state: u64 = 0xDEAD_BEEF;
    for _ in 0..1000 {
        state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
        offsets.push(state % (file_len - 4096));
    }

    group.throughput(Throughput::Bytes(1000 * 4096));

    // --- File seek+read ---
    group.bench_function("file_seek_read", |b| {
        let mut f = std::fs::File::open(tmp.path()).unwrap();
        let mut buf = [0u8; 4096];
        b.iter(|| {
            let mut sum: u64 = 0;
            for &offset in &offsets {
                f.seek(SeekFrom::Start(offset)).unwrap();
                f.read_exact(&mut buf).unwrap();
                sum = sum.wrapping_add(checksum_bytes(&buf));
            }
            std::hint::black_box(sum);
        });
    });

    // --- mmap slice ---
    let mf = MappedFile::open(tmp.path()).unwrap();
    group.bench_function("mmap_slice", |b| {
        b.iter(|| {
            let mut sum: u64 = 0;
            for &offset in &offsets {
                let data = mf.slice(FileRegion::new(offset, 4096));
                sum = sum.wrapping_add(checksum_bytes(data));
            }
            std::hint::black_box(sum);
        });
    });

    group.finish();
}

// ============================================================================
// Fixture files scan (small files, mmap vs fs::read)
// ============================================================================

fn bench_fixture_files(c: &mut Criterion) {
    let mut group = c.benchmark_group("fixture_files");

    let workspace_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent().unwrap()
        .parent().unwrap();

    let fixtures = [
        "test-fixtures/ascii_log.txt",
        "test-fixtures/mixed_entropy.bin",
        "test-fixtures/embedded_magic.bin",
    ];

    for fixture in &fixtures {
        let path = workspace_root.join(fixture);
        if !path.exists() {
            eprintln!("Skipping {}: not found (run gen_fixtures first)", fixture);
            continue;
        }

        let name = path.file_stem().unwrap().to_str().unwrap();
        let file_len = std::fs::metadata(&path).unwrap().len();

        group.throughput(Throughput::Bytes(file_len));

        // --- std::fs::read ---
        group.bench_function(format!("{}/std_fs_read", name), |b| {
            b.iter(|| {
                let data = std::fs::read(&path).unwrap();
                std::hint::black_box(checksum_bytes(&data));
            });
        });

        // --- mmap ---
        let mf = MappedFile::open(&path).unwrap();
        group.bench_function(format!("{}/mmap", name), |b| {
            b.iter(|| {
                let data = mf.slice(FileRegion::new(0, mf.len()));
                std::hint::black_box(checksum_bytes(data));
            });
        });
    }

    group.finish();
}

criterion_group!(
    benches,
    bench_stdfs_read,
    bench_sequential_scan,
    bench_random_access,
    bench_fixture_files,
);
criterion_main!(benches);
