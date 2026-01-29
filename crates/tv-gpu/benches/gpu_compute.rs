use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use tv_core::entropy::compute_entropy_cpu;
use tv_core::pattern::scan_pattern_cpu;
use tv_gpu::GpuContext;

const BLOCK_SIZE: usize = 256;

fn generate_data(size: usize) -> Vec<u8> {
    let mut data = Vec::with_capacity(size);
    let mut state: u64 = 0xDEAD_BEEF_CAFE_BABE;
    for _ in 0..size {
        state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
        data.push((state >> 33) as u8);
    }
    data
}

fn bench_entropy(c: &mut Criterion) {
    let ctx = pollster::block_on(GpuContext::new()).expect("failed to init GPU");

    let sizes: &[(usize, &str)] = &[
        (1 << 20, "1MB"),
        (10 << 20, "10MB"),
        (100 << 20, "100MB"),
    ];

    let mut group = c.benchmark_group("entropy");

    for &(size, label) in sizes {
        let data = generate_data(size);

        group.throughput(Throughput::Bytes(size as u64));

        // CPU
        group.bench_with_input(
            BenchmarkId::new("cpu", label),
            &data,
            |b, data| {
                b.iter(|| {
                    let result = compute_entropy_cpu(data, BLOCK_SIZE);
                    std::hint::black_box(result);
                });
            },
        );

        // GPU
        group.bench_with_input(
            BenchmarkId::new("gpu", label),
            &data,
            |b, data| {
                b.iter(|| {
                    let result = ctx.compute_entropy(data, BLOCK_SIZE as u32).unwrap();
                    std::hint::black_box(result);
                });
            },
        );
    }

    group.finish();
}

fn bench_pattern_scan(c: &mut Criterion) {
    let ctx = pollster::block_on(GpuContext::new()).expect("failed to init GPU");

    // 4-byte pattern (e.g. ELF magic)
    let pattern: &[u8] = &[0x7F, 0x45, 0x4C, 0x46];

    let sizes: &[(usize, &str)] = &[
        (1 << 20, "1MB"),
        (10 << 20, "10MB"),
        (100 << 20, "100MB"),
    ];

    let mut group = c.benchmark_group("pattern_scan");

    for &(size, label) in sizes {
        let mut data = generate_data(size);
        // Plant pattern at a few known offsets
        for offset in [0, size / 4, size / 2, 3 * size / 4] {
            if offset + pattern.len() <= data.len() {
                data[offset..offset + pattern.len()].copy_from_slice(pattern);
            }
        }

        group.throughput(Throughput::Bytes(size as u64));

        // CPU
        group.bench_with_input(
            BenchmarkId::new("cpu", label),
            &data,
            |b, data| {
                b.iter(|| {
                    let result = scan_pattern_cpu(data, pattern);
                    std::hint::black_box(result);
                });
            },
        );

        // GPU
        group.bench_with_input(
            BenchmarkId::new("gpu", label),
            &data,
            |b, data| {
                b.iter(|| {
                    let result = ctx.scan_pattern(data, pattern).unwrap();
                    std::hint::black_box(result);
                });
            },
        );
    }

    group.finish();
}

criterion_group!(benches, bench_entropy, bench_pattern_scan);
criterion_main!(benches);
