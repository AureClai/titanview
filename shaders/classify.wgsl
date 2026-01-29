// Block classification compute shader.
// Each workgroup processes one block of BLOCK_SIZE bytes.
// Builds a byte histogram in shared memory, computes entropy and counters,
// then classifies the block:
//   0 = Zeros (>95% null bytes)
//   1 = ASCII (>90% printable + whitespace)
//   2 = UTF-8 (multi-byte lead bytes present, entropy < 5.0)
//   3 = Binary (default)
//   4 = HighEntropy (Shannon entropy > 7.0)

@group(0) @binding(0) var<storage, read> input_data: array<u32>;
@group(0) @binding(1) var<storage, read_write> output_class: array<u32>;
@group(0) @binding(2) var<uniform> params: Params;

struct Params {
    block_size: u32,
    total_bytes: u32,
}

var<workgroup> histogram: array<atomic<u32>, 256>;

const WORKGROUP_SIZE: u32 = 256u;

@compute @workgroup_size(256)
fn main(
    @builtin(local_invocation_id) local_id: vec3<u32>,
    @builtin(workgroup_id) group_id: vec3<u32>,
) {
    let tid = local_id.x;
    let block_idx = group_id.x;
    let block_size = params.block_size;
    let block_start_byte = block_idx * block_size;

    // Clear histogram bin for this thread.
    atomicStore(&histogram[tid], 0u);
    workgroupBarrier();

    // Each thread processes its share of bytes in the block.
    let bytes_per_thread = block_size / WORKGROUP_SIZE;
    let remainder = block_size % WORKGROUP_SIZE;

    let my_start = tid * bytes_per_thread + min(tid, remainder);
    var my_count = bytes_per_thread;
    if tid < remainder {
        my_count = my_count + 1u;
    }

    for (var i: u32 = 0u; i < my_count; i = i + 1u) {
        let byte_offset = block_start_byte + my_start + i;
        if byte_offset < params.total_bytes {
            let word_idx = byte_offset / 4u;
            let byte_lane = byte_offset % 4u;
            let word = input_data[word_idx];
            let byte_val = (word >> (byte_lane * 8u)) & 0xFFu;
            atomicAdd(&histogram[byte_val], 1u);
        }
    }

    workgroupBarrier();

    // --- Phase 2: Each thread computes partial entropy for its bin ---
    let count = atomicLoad(&histogram[tid]);
    let block_end_byte = min(block_start_byte + block_size, params.total_bytes);
    let actual_block_bytes = block_end_byte - block_start_byte;

    var partial_entropy: f32 = 0.0;
    if count > 0u && actual_block_bytes > 0u {
        let p = f32(count) / f32(actual_block_bytes);
        partial_entropy = -p * log2(p);
    }

    // Store partial entropy in histogram (reuse via bitcast)
    atomicStore(&histogram[tid], bitcast<u32>(partial_entropy));
    workgroupBarrier();

    // Tree reduction for entropy sum
    for (var stride: u32 = 128u; stride > 0u; stride = stride >> 1u) {
        if tid < stride {
            let a = bitcast<f32>(atomicLoad(&histogram[tid]));
            let b = bitcast<f32>(atomicLoad(&histogram[tid + stride]));
            atomicStore(&histogram[tid], bitcast<u32>(a + b));
        }
        workgroupBarrier();
    }

    // Thread 0: read entropy, reload counters, classify
    if tid == 0u {
        let entropy = bitcast<f32>(atomicLoad(&histogram[0u]));
        let total = f32(actual_block_bytes);

        // We need to re-read specific histogram values.
        // But histogram was overwritten by entropy reduction.
        // So we re-count directly from input_data for the needed stats.
        var zero_count: u32 = 0u;
        var ascii_count: u32 = 0u;
        var utf8_lead_count: u32 = 0u;

        for (var j: u32 = 0u; j < actual_block_bytes; j = j + 1u) {
            let byte_offset = block_start_byte + j;
            let word_idx = byte_offset / 4u;
            let byte_lane = byte_offset % 4u;
            let word = input_data[word_idx];
            let byte_val = (word >> (byte_lane * 8u)) & 0xFFu;

            if byte_val == 0u {
                zero_count = zero_count + 1u;
            }
            // Printable ASCII: 0x20..=0x7E, tab(0x09), LF(0x0A), CR(0x0D)
            if (byte_val >= 0x20u && byte_val <= 0x7Eu) || byte_val == 0x09u || byte_val == 0x0Au || byte_val == 0x0Du {
                ascii_count = ascii_count + 1u;
            }
            // UTF-8 multi-byte lead: 0xC0..=0xF7
            if byte_val >= 0xC0u && byte_val <= 0xF7u {
                utf8_lead_count = utf8_lead_count + 1u;
            }
        }

        // Classification rules (same order as CPU)
        var class_id: u32 = 3u; // Binary default

        if f32(zero_count) / total > 0.95 {
            class_id = 0u; // Zeros
        } else if entropy > 7.0 {
            class_id = 4u; // HighEntropy
        } else if f32(ascii_count) / total > 0.90 {
            class_id = 1u; // ASCII
        } else if utf8_lead_count > 0u && entropy < 5.0 {
            class_id = 2u; // UTF-8
        }

        output_class[block_idx] = class_id;
    }
}
