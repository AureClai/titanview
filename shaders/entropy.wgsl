// Entropy compute shader.
// Each workgroup processes one block of BLOCK_SIZE bytes.
// Computes a byte frequency histogram in shared memory,
// then derives Shannon entropy: H = -sum(p * log2(p)).

// Input data as u32 (4 bytes packed per element).
@group(0) @binding(0) var<storage, read> input_data: array<u32>;
// Output: one f32 entropy value per block.
@group(0) @binding(1) var<storage, read_write> output_entropy: array<f32>;
// Uniforms: block_size and total byte count.
@group(0) @binding(2) var<uniform> params: Params;

struct Params {
    block_size: u32,
    total_bytes: u32,
}

// Histogram bins in workgroup shared memory (256 bins for each byte value).
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

    // Each thread processes multiple bytes within the block.
    // block_size bytes total, WORKGROUP_SIZE threads.
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
            // Read the byte: input_data is array<u32>, so 4 bytes per element.
            let word_idx = byte_offset / 4u;
            let byte_lane = byte_offset % 4u;
            let word = input_data[word_idx];
            let byte_val = (word >> (byte_lane * 8u)) & 0xFFu;
            atomicAdd(&histogram[byte_val], 1u);
        }
    }

    workgroupBarrier();

    // Each thread computes partial entropy for one histogram bin.
    let count = atomicLoad(&histogram[tid]);
    var partial_entropy: f32 = 0.0;

    // Actual number of bytes in this block (last block may be smaller).
    let block_end_byte = min(block_start_byte + block_size, params.total_bytes);
    let actual_block_bytes = block_end_byte - block_start_byte;

    if count > 0u && actual_block_bytes > 0u {
        let p = f32(count) / f32(actual_block_bytes);
        partial_entropy = -p * log2(p);
    }

    // Parallel reduction to sum all 256 partial entropies.
    // Store in shared histogram (reuse the memory, reinterpreted).
    // We use a simple approach: write partial to shared, then reduce.
    // Reuse histogram as f32 storage via bitcast.
    atomicStore(&histogram[tid], bitcast<u32>(partial_entropy));
    workgroupBarrier();

    // Tree reduction across 256 threads.
    for (var stride: u32 = 128u; stride > 0u; stride = stride >> 1u) {
        if tid < stride {
            let a = bitcast<f32>(atomicLoad(&histogram[tid]));
            let b = bitcast<f32>(atomicLoad(&histogram[tid + stride]));
            atomicStore(&histogram[tid], bitcast<u32>(a + b));
        }
        workgroupBarrier();
    }

    // Thread 0 writes the final entropy for this block.
    if tid == 0u {
        output_entropy[block_idx] = bitcast<f32>(atomicLoad(&histogram[0u]));
    }
}
