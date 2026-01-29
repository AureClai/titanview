// Binary Diff Shader
// Compares two buffers byte-by-byte and outputs difference flags

struct DiffParams {
    // Number of bytes to compare
    byte_count: u32,
    // Padding for alignment
    _pad0: u32,
    _pad1: u32,
    _pad2: u32,
}

@group(0) @binding(0) var<storage, read> buffer_a: array<u32>;
@group(0) @binding(1) var<storage, read> buffer_b: array<u32>;
@group(0) @binding(2) var<uniform> params: DiffParams;
@group(0) @binding(3) var<storage, read_write> diff_flags: array<u32>;

// Each thread compares 4 bytes (one u32)
@compute @workgroup_size(256, 1, 1)
fn main(@builtin(global_invocation_id) global_id: vec3<u32>) {
    let idx = global_id.x;
    let byte_offset = idx * 4u;

    // Bounds check
    if (byte_offset >= params.byte_count) {
        return;
    }

    let word_a = buffer_a[idx];
    let word_b = buffer_b[idx];

    // Compare word and extract per-byte differences
    // Each bit in the output represents a different byte
    var diff_bits: u32 = 0u;

    // Check each byte in the word
    let bytes_to_check = min(4u, params.byte_count - byte_offset);

    for (var i: u32 = 0u; i < bytes_to_check; i++) {
        let byte_a = (word_a >> (i * 8u)) & 0xFFu;
        let byte_b = (word_b >> (i * 8u)) & 0xFFu;

        if (byte_a != byte_b) {
            diff_bits |= (1u << i);
        }
    }

    diff_flags[idx] = diff_bits;
}
