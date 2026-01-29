// Hilbert Curve Visualization Shader
// Maps file data to a 2D texture using Hilbert space-filling curve
// Preserves spatial locality: nearby bytes appear nearby in 2D

struct HilbertParams {
    // Texture size (must be power of 2, e.g., 256, 512, 1024)
    texture_size: u32,
    // Total bytes in file
    file_size: u32,
    // Bytes per pixel (downsampling factor)
    bytes_per_pixel: u32,
    // Visualization mode: 0=entropy, 1=classification, 2=byte value
    mode: u32,
}

@group(0) @binding(0) var<storage, read> input_data: array<u32>;
@group(0) @binding(1) var<storage, read> entropy_data: array<f32>;
@group(0) @binding(2) var<storage, read> class_data: array<u32>;
@group(0) @binding(3) var<uniform> params: HilbertParams;
@group(0) @binding(4) var<storage, read_write> output_texture: array<u32>;

// Convert (x, y) coordinates to Hilbert curve index (distance along curve)
// n = size of grid (power of 2)
fn xy2d(n: u32, x: u32, y: u32) -> u32 {
    var rx: u32;
    var ry: u32;
    var d: u32 = 0u;
    var s: u32 = n / 2u;
    var px = x;
    var py = y;

    while (s > 0u) {
        rx = select(0u, 1u, (px & s) > 0u);
        ry = select(0u, 1u, (py & s) > 0u);
        d += s * s * ((3u * rx) ^ ry);

        // Rotate quadrant
        if (ry == 0u) {
            if (rx == 1u) {
                px = s - 1u - px;
                py = s - 1u - py;
            }
            let temp = px;
            px = py;
            py = temp;
        }

        s = s / 2u;
    }

    return d;
}

// Convert Hilbert curve index to (x, y) coordinates
fn d2xy(n: u32, d: u32) -> vec2<u32> {
    var rx: u32;
    var ry: u32;
    var t: u32 = d;
    var x: u32 = 0u;
    var y: u32 = 0u;
    var s: u32 = 1u;

    while (s < n) {
        rx = 1u & (t / 2u);
        ry = 1u & (t ^ rx);

        // Rotate
        if (ry == 0u) {
            if (rx == 1u) {
                x = s - 1u - x;
                y = s - 1u - y;
            }
            let temp = x;
            x = y;
            y = temp;
        }

        x += s * rx;
        y += s * ry;
        t = t / 4u;
        s = s * 2u;
    }

    return vec2<u32>(x, y);
}

// Map entropy value (0-8) to color
fn entropy_to_color(entropy: f32) -> vec4<f32> {
    let t = clamp(entropy / 8.0, 0.0, 1.0);

    // Blue (low) -> Green (medium) -> Yellow -> Red (high)
    if (t < 0.25) {
        let s = t * 4.0;
        return vec4<f32>(0.0, s * 0.5, 0.3 + s * 0.2, 1.0);
    } else if (t < 0.5) {
        let s = (t - 0.25) * 4.0;
        return vec4<f32>(s * 0.3, 0.5 + s * 0.3, 0.5 - s * 0.2, 1.0);
    } else if (t < 0.75) {
        let s = (t - 0.5) * 4.0;
        return vec4<f32>(0.3 + s * 0.5, 0.8 - s * 0.2, 0.3 - s * 0.2, 1.0);
    } else {
        let s = (t - 0.75) * 4.0;
        return vec4<f32>(0.8 + s * 0.2, 0.6 - s * 0.4, 0.1, 1.0);
    }
}

// Map classification to color
fn class_to_color(class_id: u32) -> vec4<f32> {
    switch (class_id) {
        case 0u: { return vec4<f32>(0.2, 0.25, 0.35, 1.0); }  // Zeros - dark blue-gray
        case 1u: { return vec4<f32>(0.3, 0.7, 0.3, 1.0); }   // ASCII - green
        case 2u: { return vec4<f32>(0.3, 0.5, 0.8, 1.0); }   // UTF-8 - blue
        case 3u: { return vec4<f32>(0.8, 0.6, 0.3, 1.0); }   // Binary - amber
        case 4u: { return vec4<f32>(0.8, 0.2, 0.2, 1.0); }   // HighEntropy - red
        default: { return vec4<f32>(0.3, 0.3, 0.3, 1.0); }   // Unknown - gray
    }
}

// Map byte value to grayscale with hints
fn byte_to_color(value: u32) -> vec4<f32> {
    if (value == 0u) {
        return vec4<f32>(0.05, 0.05, 0.1, 1.0); // Near-black for zeros
    } else if (value == 255u) {
        return vec4<f32>(1.0, 1.0, 0.9, 1.0); // Near-white for 0xFF
    } else if (value >= 32u && value <= 126u) {
        // Printable ASCII - slight green tint
        let v = f32(value) / 255.0;
        return vec4<f32>(v * 0.8, v, v * 0.8, 1.0);
    } else {
        // Other bytes - grayscale
        let v = f32(value) / 255.0;
        return vec4<f32>(v, v, v, 1.0);
    }
}

// Map bit value to color (for bit density mode)
fn bit_to_color(bit: u32) -> vec4<f32> {
    if (bit != 0u) {
        // 1 bit - bright cyan/green
        return vec4<f32>(0.0, 1.0, 0.5, 1.0);
    } else {
        // 0 bit - dark
        return vec4<f32>(0.04, 0.04, 0.08, 1.0);
    }
}

// Pack color to u32 (RGBA8)
fn pack_color(color: vec4<f32>) -> u32 {
    let r = u32(clamp(color.r * 255.0, 0.0, 255.0));
    let g = u32(clamp(color.g * 255.0, 0.0, 255.0));
    let b = u32(clamp(color.b * 255.0, 0.0, 255.0));
    let a = u32(clamp(color.a * 255.0, 0.0, 255.0));
    return (a << 24u) | (b << 16u) | (g << 8u) | r;
}

@compute @workgroup_size(16, 16, 1)
fn main(@builtin(global_invocation_id) global_id: vec3<u32>) {
    let x = global_id.x;
    let y = global_id.y;
    let size = params.texture_size;

    // Bounds check
    if (x >= size || y >= size) {
        return;
    }

    // Convert (x, y) to Hilbert index
    let hilbert_index = xy2d(size, x, y);

    // Convert Hilbert index to file offset (u32 arithmetic, files up to 4GB)
    let file_offset = hilbert_index * params.bytes_per_pixel;

    // Output pixel index
    let pixel_idx = y * size + x;

    // Check if this pixel maps to valid file data
    if (hilbert_index >= params.file_size / params.bytes_per_pixel) {
        // Beyond file - dark gray
        output_texture[pixel_idx] = pack_color(vec4<f32>(0.1, 0.1, 0.1, 1.0));
        return;
    }

    var color: vec4<f32>;

    switch (params.mode) {
        case 0u: {
            // Entropy mode - use precomputed entropy data
            let block_idx = file_offset / 256u;
            if (block_idx < arrayLength(&entropy_data)) {
                let entropy = entropy_data[block_idx];
                color = entropy_to_color(entropy);
            } else {
                color = vec4<f32>(0.1, 0.1, 0.1, 1.0);
            }
        }
        case 1u: {
            // Classification mode
            let block_idx = file_offset / 256u;
            if (block_idx < arrayLength(&class_data)) {
                let class_id = class_data[block_idx];
                color = class_to_color(class_id);
            } else {
                color = vec4<f32>(0.1, 0.1, 0.1, 1.0);
            }
        }
        case 2u: {
            // Byte value mode - use pre-sampled bytes (indexed by pixel_idx)
            let word_idx = pixel_idx / 4u;
            let byte_offset = pixel_idx % 4u;

            if (word_idx < arrayLength(&input_data)) {
                let word = input_data[word_idx];
                let byte_val = (word >> (byte_offset * 8u)) & 0xFFu;
                color = byte_to_color(byte_val);
            } else {
                color = vec4<f32>(0.1, 0.1, 0.1, 1.0);
            }
        }
        case 3u: {
            // Bit density mode - each pixel is a single bit
            // pixel_idx = bit index in the file
            // 8 pixels per byte
            let byte_idx = pixel_idx / 8u;
            let bit_offset = pixel_idx % 8u;
            let word_idx = byte_idx / 4u;
            let byte_in_word = byte_idx % 4u;

            if (word_idx < arrayLength(&input_data)) {
                let word = input_data[word_idx];
                let byte_val = (word >> (byte_in_word * 8u)) & 0xFFu;
                // Extract bit (MSB first for natural reading)
                let bit = (byte_val >> (7u - bit_offset)) & 1u;
                color = bit_to_color(bit);
            } else {
                color = vec4<f32>(0.05, 0.05, 0.05, 1.0);
            }
        }
        default: {
            color = vec4<f32>(0.5, 0.5, 0.5, 1.0);
        }
    }

    output_texture[pixel_idx] = pack_color(color);
}
