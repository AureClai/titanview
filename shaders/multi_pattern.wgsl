// Optimized multi-pattern scan compute shader.
// Uses shared memory for coalesced reads and workgroup-local match counting.
//
// Key optimizations:
// 1. Coalesced memory loads into shared memory (10x faster than scattered global reads)
// 2. Workgroup-local match counting to reduce atomic contention
// 3. Single atomic per workgroup instead of per-match

@group(0) @binding(0) var<storage, read> input_data: array<u32>;
@group(0) @binding(1) var<storage, read_write> results: array<atomic<u32>>;
@group(0) @binding(2) var<uniform> params: Params;
@group(0) @binding(3) var<storage, read> patterns: array<u32>;
@group(0) @binding(4) var<storage, read> pattern_meta: array<u32>;

struct Params {
    total_bytes: u32,
    num_patterns: u32,
    max_results: u32,
    max_pattern_len: u32,  // Added: max pattern length for shared memory sizing
}

// Workgroup size and tile configuration
const WORKGROUP_SIZE: u32 = 256u;
// Max bytes to load into shared memory: tile + overlap for patterns
// We'll load WORKGROUP_SIZE + 32 bytes to handle patterns up to 32 bytes
const SHARED_SIZE: u32 = 288u;  // 256 + 32 for overlap

// Shared memory tile for coalesced access
var<workgroup> tile: array<u32, 72>;  // 288 bytes / 4 = 72 u32s

// Workgroup-local results buffer (pattern_idx, offset pairs)
// Each workgroup can find up to 32 matches locally before writing to global
const MAX_LOCAL_MATCHES: u32 = 32u;
var<workgroup> local_matches: array<u32, 64>;  // 32 matches × 2 u32s each
var<workgroup> local_match_count: atomic<u32>;

fn read_tile_byte(local_offset: u32) -> u32 {
    let word_idx = local_offset / 4u;
    let byte_lane = local_offset % 4u;
    return (tile[word_idx] >> (byte_lane * 8u)) & 0xFFu;
}

fn read_pattern_byte(patterns_offset: u32) -> u32 {
    let word_idx = patterns_offset / 4u;
    let byte_lane = patterns_offset % 4u;
    return (patterns[word_idx] >> (byte_lane * 8u)) & 0xFFu;
}

@compute @workgroup_size(256)
fn main(
    @builtin(global_invocation_id) global_id: vec3<u32>,
    @builtin(local_invocation_id) local_id: vec3<u32>,
    @builtin(workgroup_id) wg_id: vec3<u32>
) {
    let tid = local_id.x;
    let tile_start = wg_id.x * WORKGROUP_SIZE;

    // Initialize local match counter (only thread 0)
    if tid == 0u {
        atomicStore(&local_match_count, 0u);
    }
    workgroupBarrier();

    // Phase 1: Coalesced load into shared memory
    // Each thread loads one u32 (4 bytes)
    let words_to_load = (SHARED_SIZE + 3u) / 4u;  // 72 words

    // Primary load: threads 0-255 load words 0-255 (but we only need 72)
    if tid < words_to_load {
        let global_word_idx = (tile_start / 4u) + tid;
        let max_words = (params.total_bytes + 3u) / 4u;
        if global_word_idx < max_words {
            tile[tid] = input_data[global_word_idx];
        } else {
            tile[tid] = 0u;
        }
    }

    workgroupBarrier();

    // Phase 2: Pattern matching from shared memory
    let data_offset = tile_start + tid;

    // Early exit if past data
    if data_offset >= params.total_bytes {
        // Still need to participate in the final barrier
        workgroupBarrier();
    } else {
        let local_offset = tid;  // Offset within tile

        // Check this position against every pattern
        for (var pat_idx: u32 = 0u; pat_idx < params.num_patterns; pat_idx = pat_idx + 1u) {
            let meta_base = pat_idx * 2u;
            let pat_start = pattern_meta[meta_base];
            let pat_len = pattern_meta[meta_base + 1u];

            // Bounds check
            if data_offset + pat_len > params.total_bytes {
                continue;
            }

            // Also check if pattern fits in our shared memory tile
            if local_offset + pat_len > SHARED_SIZE {
                continue;
            }

            // Check pattern match using shared memory
            var matched = true;
            for (var i: u32 = 0u; i < pat_len; i = i + 1u) {
                if read_tile_byte(local_offset + i) != read_pattern_byte(pat_start + i) {
                    matched = false;
                    break;
                }
            }

            if matched {
                // Store in workgroup-local buffer
                let local_idx = atomicAdd(&local_match_count, 1u);
                if local_idx < MAX_LOCAL_MATCHES {
                    let store_base = local_idx * 2u;
                    local_matches[store_base] = pat_idx;
                    local_matches[store_base + 1u] = data_offset;
                }
            }
        }

        workgroupBarrier();
    }

    // Phase 3: Write local matches to global results (only thread 0)
    if tid == 0u {
        let num_local = atomicLoad(&local_match_count);
        if num_local > 0u {
            // Reserve space in global results with single atomic
            let count_to_write = min(num_local, MAX_LOCAL_MATCHES);
            let base_idx = atomicAdd(&results[0], count_to_write);

            // Write all local matches to global
            for (var i: u32 = 0u; i < count_to_write; i = i + 1u) {
                let result_idx = base_idx + i;
                if result_idx < params.max_results {
                    let local_base = i * 2u;
                    let store_base = 1u + result_idx * 2u;
                    atomicStore(&results[store_base], local_matches[local_base]);
                    atomicStore(&results[store_base + 1u], local_matches[local_base + 1u]);
                }
            }
        }
    }
}
