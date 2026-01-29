// Pattern scan compute shader.
// Each invocation checks one byte offset for a match against a pattern.

@group(0) @binding(0) var<storage, read> input_data: array<u32>;
@group(0) @binding(1) var<storage, read_write> results: array<atomic<u32>>;
@group(0) @binding(2) var<uniform> params: Params;
// Pattern stored as u32 array (up to 16 bytes = 4 u32s)
@group(0) @binding(3) var<storage, read> pattern: array<u32>;

struct Params {
    total_bytes: u32,
    pattern_len: u32,
    // results[0] is used as an atomic counter for number of hits
    max_results: u32,
    _pad: u32,
}

fn read_byte(offset: u32) -> u32 {
    let word_idx = offset / 4u;
    let byte_lane = offset % 4u;
    return (input_data[word_idx] >> (byte_lane * 8u)) & 0xFFu;
}

fn read_pattern_byte(offset: u32) -> u32 {
    let word_idx = offset / 4u;
    let byte_lane = offset % 4u;
    return (pattern[word_idx] >> (byte_lane * 8u)) & 0xFFu;
}

@compute @workgroup_size(256)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    let offset = id.x;
    let pat_len = params.pattern_len;

    // Bounds check: need pat_len bytes starting at offset
    if offset + pat_len > params.total_bytes {
        return;
    }

    // Check if pattern matches at this offset
    var matched = true;
    for (var i: u32 = 0u; i < pat_len; i = i + 1u) {
        if read_byte(offset + i) != read_pattern_byte(i) {
            matched = false;
            break;
        }
    }

    if matched {
        // Atomically increment hit counter (stored at results[0])
        let idx = atomicAdd(&results[0], 1u);
        // Store the offset (results[1..] holds the offsets)
        if idx < params.max_results {
            atomicStore(&results[idx + 1u], offset);
        }
    }
}
