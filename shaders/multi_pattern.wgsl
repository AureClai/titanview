// Multi-pattern scan compute shader.
// Scans input data for multiple patterns simultaneously (e.g., file signatures).
// Each invocation checks one byte offset against ALL patterns.

@group(0) @binding(0) var<storage, read> input_data: array<u32>;
@group(0) @binding(1) var<storage, read_write> results: array<atomic<u32>>;
@group(0) @binding(2) var<uniform> params: Params;
// All patterns concatenated (padded to 4-byte alignment)
@group(0) @binding(3) var<storage, read> patterns: array<u32>;
// Pattern metadata: [offset_in_patterns, length] pairs for each pattern
@group(0) @binding(4) var<storage, read> pattern_meta: array<u32>;

struct Params {
    total_bytes: u32,
    num_patterns: u32,
    max_results: u32,
    _pad: u32,
}

fn read_byte(offset: u32) -> u32 {
    let word_idx = offset / 4u;
    let byte_lane = offset % 4u;
    return (input_data[word_idx] >> (byte_lane * 8u)) & 0xFFu;
}

fn read_pattern_byte(patterns_offset: u32) -> u32 {
    let word_idx = patterns_offset / 4u;
    let byte_lane = patterns_offset % 4u;
    return (patterns[word_idx] >> (byte_lane * 8u)) & 0xFFu;
}

@compute @workgroup_size(256)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    let data_offset = id.x;

    // Early exit if we're past the data
    if data_offset >= params.total_bytes {
        return;
    }

    // Check this position against every pattern
    for (var pat_idx: u32 = 0u; pat_idx < params.num_patterns; pat_idx = pat_idx + 1u) {
        // Read pattern metadata: offset and length
        let meta_base = pat_idx * 2u;
        let pat_start = pattern_meta[meta_base];
        let pat_len = pattern_meta[meta_base + 1u];

        // Bounds check: need pat_len bytes starting at data_offset
        if data_offset + pat_len > params.total_bytes {
            continue;
        }

        // Check if pattern matches at this offset
        var matched = true;
        for (var i: u32 = 0u; i < pat_len; i = i + 1u) {
            if read_byte(data_offset + i) != read_pattern_byte(pat_start + i) {
                matched = false;
                break;
            }
        }

        if matched {
            // Atomically increment hit counter (stored at results[0])
            let result_idx = atomicAdd(&results[0], 1u);

            // Store (pattern_index, offset) pair
            // Each result uses 2 u32s: [pattern_idx, data_offset]
            if result_idx < params.max_results {
                let store_base = 1u + result_idx * 2u;
                atomicStore(&results[store_base], pat_idx);
                atomicStore(&results[store_base + 1u], data_offset);
            }
        }
    }
}
