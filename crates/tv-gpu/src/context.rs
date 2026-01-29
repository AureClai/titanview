use anyhow::{Context, Result};
use wgpu::util::DeviceExt;

/// Holds the GPU device and queue. Entry point for all GPU compute operations.
pub struct GpuContext {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
}

impl GpuContext {
    /// Initialize a GPU context, requesting a high-performance adapter.
    pub async fn new() -> Result<Self> {
        let instance = wgpu::Instance::default();

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: None,
                force_fallback_adapter: false,
            })
            .await
            .context("no suitable GPU adapter found")?;

        log::info!("GPU adapter: {}", adapter.get_info().name);

        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("tv-gpu"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::default(),
                memory_hints: wgpu::MemoryHints::Performance,
            }, None)
            .await
            .context("failed to create GPU device")?;

        Ok(Self { device, queue })
    }

    /// Run the passthrough compute shader: output[i] = input[i] + 1.
    pub fn run_passthrough(&self, data: &[u32]) -> Result<Vec<u32>> {
        let input_bytes = bytemuck::cast_slice(data);
        let size = input_bytes.len() as u64;

        // Input buffer (storage, read-only from shader)
        let input_buf = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("passthrough_input"),
            contents: input_bytes,
            usage: wgpu::BufferUsages::STORAGE,
        });

        // Output buffer (storage, read-write from shader, copy source for readback)
        let output_buf = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("passthrough_output"),
            size,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });

        // Staging buffer for CPU readback
        let staging_buf = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("passthrough_staging"),
            size,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // Shader module
        let shader = self.device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("passthrough_shader"),
            source: wgpu::ShaderSource::Wgsl(
                include_str!("../../../shaders/passthrough.wgsl").into(),
            ),
        });

        // Compute pipeline
        let pipeline = self.device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("passthrough_pipeline"),
            layout: None,
            module: &shader,
            entry_point: Some("main"),
            compilation_options: Default::default(),
            cache: None,
        });

        // Bind group
        let bind_group_layout = pipeline.get_bind_group_layout(0);
        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("passthrough_bind_group"),
            layout: &bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: input_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: output_buf.as_entire_binding(),
                },
            ],
        });

        // Encode and dispatch
        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("passthrough_encoder"),
        });

        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("passthrough_pass"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&pipeline);
            pass.set_bind_group(0, &bind_group, &[]);

            let workgroup_count = (data.len() as u32).div_ceil(64);
            pass.dispatch_workgroups(workgroup_count, 1, 1);
        }

        // Copy output to staging
        encoder.copy_buffer_to_buffer(&output_buf, 0, &staging_buf, 0, size);

        self.queue.submit(Some(encoder.finish()));

        // Read back
        let staging_slice = staging_buf.slice(..);
        let (tx, rx) = std::sync::mpsc::channel();
        staging_slice.map_async(wgpu::MapMode::Read, move |result| {
            tx.send(result).unwrap();
        });
        self.device.poll(wgpu::Maintain::Wait);
        rx.recv()
            .context("GPU readback channel closed")?
            .context("GPU readback failed")?;

        let mapped = staging_slice.get_mapped_range();
        let result: Vec<u32> = bytemuck::cast_slice(&mapped).to_vec();
        drop(mapped);
        staging_buf.unmap();

        Ok(result)
    }

    /// Compute per-block Shannon entropy on the GPU.
    /// `data` is raw bytes, `block_size` must be a multiple of 256 (workgroup size).
    /// Returns one f32 entropy value per block (0.0 = uniform, 8.0 = max entropy).
    pub fn compute_entropy(&self, data: &[u8], block_size: u32) -> Result<Vec<f32>> {
        assert!(
            block_size >= 256 && block_size % 256 == 0,
            "block_size must be a multiple of 256, got {}",
            block_size
        );

        if data.is_empty() {
            return Ok(vec![]);
        }

        // Process in chunks to stay within the 65535 workgroup dispatch limit.
        const MAX_WORKGROUPS: u32 = 65535;
        let chunk_bytes = MAX_WORKGROUPS as usize * block_size as usize;

        let mut all_results = Vec::new();

        for chunk_start in (0..data.len()).step_by(chunk_bytes) {
            let chunk_end = (chunk_start + chunk_bytes).min(data.len());
            let chunk = &data[chunk_start..chunk_end];
            let chunk_results = self.compute_entropy_chunk(chunk, block_size)?;
            all_results.extend(chunk_results);
        }

        Ok(all_results)
    }

    /// Internal: compute entropy for a single chunk that fits within dispatch limits.
    fn compute_entropy_chunk(&self, data: &[u8], block_size: u32) -> Result<Vec<f32>> {
        let num_blocks = (data.len() as u32).div_ceil(block_size);

        // Pad data to 4-byte alignment for u32 storage buffer
        let padded_len = (data.len() + 3) & !3;
        let mut padded = data.to_vec();
        padded.resize(padded_len, 0);

        let input_buf = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("entropy_input"),
            contents: &padded,
            usage: wgpu::BufferUsages::STORAGE,
        });

        let output_size = (num_blocks as u64) * 4;
        let output_buf = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("entropy_output"),
            size: output_size,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });

        #[repr(C)]
        #[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
        struct Params {
            block_size: u32,
            total_bytes: u32,
        }

        let params = Params {
            block_size,
            total_bytes: data.len() as u32,
        };

        let params_buf = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("entropy_params"),
            contents: bytemuck::bytes_of(&params),
            usage: wgpu::BufferUsages::UNIFORM,
        });

        let staging_buf = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("entropy_staging"),
            size: output_size,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let shader = self.device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("entropy_shader"),
            source: wgpu::ShaderSource::Wgsl(
                include_str!("../../../shaders/entropy.wgsl").into(),
            ),
        });

        let pipeline = self.device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("entropy_pipeline"),
            layout: None,
            module: &shader,
            entry_point: Some("main"),
            compilation_options: Default::default(),
            cache: None,
        });

        let bind_group_layout = pipeline.get_bind_group_layout(0);
        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("entropy_bind_group"),
            layout: &bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: input_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: output_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: params_buf.as_entire_binding(),
                },
            ],
        });

        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("entropy_encoder"),
        });

        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("entropy_pass"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&pipeline);
            pass.set_bind_group(0, &bind_group, &[]);
            pass.dispatch_workgroups(num_blocks, 1, 1);
        }

        encoder.copy_buffer_to_buffer(&output_buf, 0, &staging_buf, 0, output_size);
        self.queue.submit(Some(encoder.finish()));

        let staging_slice = staging_buf.slice(..);
        let (tx, rx) = std::sync::mpsc::channel();
        staging_slice.map_async(wgpu::MapMode::Read, move |result| {
            tx.send(result).unwrap();
        });
        self.device.poll(wgpu::Maintain::Wait);
        rx.recv()
            .context("entropy readback channel closed")?
            .context("entropy readback failed")?;

        let mapped = staging_slice.get_mapped_range();
        let result: Vec<f32> = bytemuck::cast_slice(&mapped).to_vec();
        drop(mapped);
        staging_buf.unmap();

        Ok(result)
    }

    /// Classify each block of `block_size` bytes by content type on the GPU.
    /// Returns one u8 per block matching `BlockClass` variants (0..=4).
    pub fn compute_classification(&self, data: &[u8], block_size: u32) -> Result<Vec<u8>> {
        assert!(
            block_size >= 256 && block_size % 256 == 0,
            "block_size must be a multiple of 256, got {}",
            block_size
        );

        if data.is_empty() {
            return Ok(vec![]);
        }

        const MAX_WORKGROUPS: u32 = 65535;
        let chunk_bytes = MAX_WORKGROUPS as usize * block_size as usize;

        let mut all_results = Vec::new();

        for chunk_start in (0..data.len()).step_by(chunk_bytes) {
            let chunk_end = (chunk_start + chunk_bytes).min(data.len());
            let chunk = &data[chunk_start..chunk_end];
            let chunk_results = self.compute_classify_chunk(chunk, block_size)?;
            all_results.extend(chunk_results);
        }

        Ok(all_results)
    }

    /// Internal: classify a single chunk that fits within dispatch limits.
    fn compute_classify_chunk(&self, data: &[u8], block_size: u32) -> Result<Vec<u8>> {
        let num_blocks = (data.len() as u32).div_ceil(block_size);

        // Pad data to 4-byte alignment
        let padded_len = (data.len() + 3) & !3;
        let mut padded = data.to_vec();
        padded.resize(padded_len, 0);

        let input_buf = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("classify_input"),
            contents: &padded,
            usage: wgpu::BufferUsages::STORAGE,
        });

        // Output: one u32 per block (u32 for alignment, we only use low byte)
        let output_size = (num_blocks as u64) * 4;
        let output_buf = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("classify_output"),
            size: output_size,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });

        #[repr(C)]
        #[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
        struct Params {
            block_size: u32,
            total_bytes: u32,
        }

        let params = Params {
            block_size,
            total_bytes: data.len() as u32,
        };

        let params_buf = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("classify_params"),
            contents: bytemuck::bytes_of(&params),
            usage: wgpu::BufferUsages::UNIFORM,
        });

        let staging_buf = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("classify_staging"),
            size: output_size,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let shader = self.device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("classify_shader"),
            source: wgpu::ShaderSource::Wgsl(
                include_str!("../../../shaders/classify.wgsl").into(),
            ),
        });

        let pipeline = self.device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("classify_pipeline"),
            layout: None,
            module: &shader,
            entry_point: Some("main"),
            compilation_options: Default::default(),
            cache: None,
        });

        let bind_group_layout = pipeline.get_bind_group_layout(0);
        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("classify_bind_group"),
            layout: &bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: input_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: output_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: params_buf.as_entire_binding(),
                },
            ],
        });

        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("classify_encoder"),
        });

        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("classify_pass"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&pipeline);
            pass.set_bind_group(0, &bind_group, &[]);
            pass.dispatch_workgroups(num_blocks, 1, 1);
        }

        encoder.copy_buffer_to_buffer(&output_buf, 0, &staging_buf, 0, output_size);
        self.queue.submit(Some(encoder.finish()));

        let staging_slice = staging_buf.slice(..);
        let (tx, rx) = std::sync::mpsc::channel();
        staging_slice.map_async(wgpu::MapMode::Read, move |result| {
            tx.send(result).unwrap();
        });
        self.device.poll(wgpu::Maintain::Wait);
        rx.recv()
            .context("classify readback channel closed")?
            .context("classify readback failed")?;

        let mapped = staging_slice.get_mapped_range();
        let raw: &[u32] = bytemuck::cast_slice(&mapped);
        let result: Vec<u8> = raw.iter().map(|&v| v as u8).collect();
        drop(mapped);
        staging_buf.unmap();

        Ok(result)
    }

    /// Scan data for occurrences of a byte pattern (up to 16 bytes).
    /// Returns a sorted list of byte offsets where the pattern was found.
    /// Scan `data` for all occurrences of `pattern` (1–16 bytes).
    /// Processes data in GPU-friendly chunks. Returns sorted match offsets.
    pub fn scan_pattern(&self, data: &[u8], pattern: &[u8]) -> Result<Vec<u64>> {
        anyhow::ensure!(
            !pattern.is_empty() && pattern.len() <= 16,
            "pattern must be 1-16 bytes, got {}",
            pattern.len()
        );

        if data.len() < pattern.len() {
            return Ok(vec![]);
        }

        // --- Create shader & pipeline ONCE ---
        let shader = self.device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("scan_shader"),
            source: wgpu::ShaderSource::Wgsl(
                include_str!("../../../shaders/pattern_scan.wgsl").into(),
            ),
        });

        let pipeline = self.device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("scan_pipeline"),
            layout: None,
            module: &shader,
            entry_point: Some("main"),
            compilation_options: Default::default(),
            cache: None,
        });

        // --- Pad pattern once ---
        let pat_padded_len = (pattern.len() + 3) & !3;
        let mut padded_pattern = pattern.to_vec();
        padded_pattern.resize(pat_padded_len, 0);

        let pattern_buf = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("scan_pattern"),
            contents: &padded_pattern,
            usage: wgpu::BufferUsages::STORAGE,
        });

        // --- Persistent results + staging buffers (reused across chunks) ---
        let max_results: u32 = 65536;
        let results_size = ((1 + max_results) as u64) * 4;

        let staging_buf = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("scan_staging"),
            size: results_size,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // --- Chunk through data ---
        const MAX_WORKGROUPS: u32 = 65534;
        const WORKGROUP_SIZE: u32 = 256;
        let chunk_size = (MAX_WORKGROUPS * WORKGROUP_SIZE) as usize;
        let mut all_offsets = Vec::new();

        for chunk_start in (0..data.len()).step_by(chunk_size) {
            let actual_start = if chunk_start > 0 {
                chunk_start.saturating_sub(pattern.len() - 1)
            } else {
                0
            };
            let chunk_end = (chunk_start + chunk_size).min(data.len());
            let chunk = &data[actual_start..chunk_end];

            let offsets = self.scan_pattern_chunk_with(
                chunk, pattern, &pipeline, &pattern_buf, &staging_buf,
                max_results, results_size,
            )?;

            for offset in offsets {
                let global_offset = actual_start as u64 + offset;
                if all_offsets.last().map_or(true, |&last| last != global_offset) {
                    all_offsets.push(global_offset);
                }
            }
        }

        all_offsets.sort();
        all_offsets.dedup();
        Ok(all_offsets)
    }

    /// Run a single scan chunk, reusing the pre-created pipeline and buffers.
    fn scan_pattern_chunk_with(
        &self,
        data: &[u8],
        pattern: &[u8],
        pipeline: &wgpu::ComputePipeline,
        pattern_buf: &wgpu::Buffer,
        staging_buf: &wgpu::Buffer,
        max_results: u32,
        results_size: u64,
    ) -> Result<Vec<u64>> {
        // Pad data to 4-byte alignment
        let padded_len = (data.len() + 3) & !3;
        let mut padded_data = data.to_vec();
        padded_data.resize(padded_len, 0);

        let input_buf = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("scan_input"),
            contents: &padded_data,
            usage: wgpu::BufferUsages::STORAGE,
        });

        // Fresh results buffer each chunk (must be zeroed for atomics)
        let results_buf = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("scan_results"),
            size: results_size,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });

        #[repr(C)]
        #[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
        struct Params {
            total_bytes: u32,
            pattern_len: u32,
            max_results: u32,
            _pad: u32,
        }

        let params = Params {
            total_bytes: data.len() as u32,
            pattern_len: pattern.len() as u32,
            max_results,
            _pad: 0,
        };

        let params_buf = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("scan_params"),
            contents: bytemuck::bytes_of(&params),
            usage: wgpu::BufferUsages::UNIFORM,
        });

        let bind_group_layout = pipeline.get_bind_group_layout(0);
        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("scan_bind_group"),
            layout: &bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: input_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: results_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: params_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: pattern_buf.as_entire_binding(),
                },
            ],
        });

        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("scan_encoder"),
        });

        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("scan_pass"),
                timestamp_writes: None,
            });
            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, &bind_group, &[]);
            let workgroups = (data.len() as u32).div_ceil(256).min(65535);
            pass.dispatch_workgroups(workgroups, 1, 1);
        }

        encoder.copy_buffer_to_buffer(&results_buf, 0, staging_buf, 0, results_size);
        self.queue.submit(Some(encoder.finish()));

        // Wait for GPU completion before reading back
        let staging_slice = staging_buf.slice(..);
        let (tx, rx) = std::sync::mpsc::channel();
        staging_slice.map_async(wgpu::MapMode::Read, move |result| {
            let _ = tx.send(result);
        });
        self.device.poll(wgpu::Maintain::Wait);
        rx.recv()
            .context("scan readback channel closed")?
            .context("scan readback failed")?;

        let mapped = staging_slice.get_mapped_range();
        let raw: &[u32] = bytemuck::cast_slice(&mapped);

        let hit_count = raw[0].min(max_results) as usize;
        let mut offsets: Vec<u64> = raw[1..=hit_count]
            .iter()
            .map(|&o| o as u64)
            .collect();

        drop(mapped);
        staging_buf.unmap();

        // Explicitly drop per-chunk buffers and poll to release GPU memory
        drop(input_buf);
        drop(results_buf);
        drop(params_buf);
        self.device.poll(wgpu::Maintain::Wait);

        offsets.sort();
        Ok(offsets)
    }
}

/// A match result from multi-pattern scan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MultiPatternMatch {
    /// Index of the pattern that matched.
    pub pattern_idx: u32,
    /// Byte offset in the data where the match occurred.
    pub offset: u64,
}

impl GpuContext {
    /// Scan data for multiple patterns simultaneously (e.g., file signatures).
    /// All patterns are checked at every position in a single GPU pass.
    /// Returns matches sorted by offset.
    ///
    /// This amortizes the PCIe transfer cost across all patterns, making it
    /// much faster than running N separate scans for N patterns.
    pub fn scan_multi_pattern(&self, data: &[u8], patterns: &[&[u8]]) -> Result<Vec<MultiPatternMatch>> {
        if patterns.is_empty() || data.is_empty() {
            return Ok(vec![]);
        }

        // Find max pattern length and validate
        let max_pattern_len = patterns.iter().map(|p| p.len()).max().unwrap_or(0);
        if max_pattern_len == 0 || data.len() < max_pattern_len {
            return Ok(vec![]);
        }

        // Process in chunks to stay within GPU dispatch limits
        const MAX_WORKGROUPS: u32 = 65534;
        const WORKGROUP_SIZE: u32 = 256;
        let chunk_size = (MAX_WORKGROUPS * WORKGROUP_SIZE) as usize;

        let mut all_matches = Vec::new();

        for chunk_start in (0..data.len()).step_by(chunk_size) {
            // Overlap with previous chunk to catch patterns at boundaries
            let actual_start = if chunk_start > 0 {
                chunk_start.saturating_sub(max_pattern_len - 1)
            } else {
                0
            };
            let chunk_end = (chunk_start + chunk_size).min(data.len());
            let chunk = &data[actual_start..chunk_end];

            let mut chunk_matches = self.scan_multi_pattern_chunk(chunk, patterns)?;

            // Adjust offsets to global positions and deduplicate
            for m in &mut chunk_matches {
                m.offset += actual_start as u64;
            }

            all_matches.extend(chunk_matches);
        }

        // Sort by offset and deduplicate
        all_matches.sort_by_key(|m| (m.offset, m.pattern_idx));
        all_matches.dedup();

        Ok(all_matches)
    }

    /// Internal: scan a single chunk for multiple patterns.
    fn scan_multi_pattern_chunk(&self, data: &[u8], patterns: &[&[u8]]) -> Result<Vec<MultiPatternMatch>> {
        // Build concatenated pattern buffer with metadata
        let mut pattern_bytes = Vec::new();
        let mut pattern_meta: Vec<u32> = Vec::new(); // [offset, len] pairs

        for pattern in patterns {
            let offset = pattern_bytes.len() as u32;
            let len = pattern.len() as u32;
            pattern_meta.push(offset);
            pattern_meta.push(len);
            pattern_bytes.extend_from_slice(pattern);
            // Pad each pattern to 4-byte alignment for cleaner reads
            while pattern_bytes.len() % 4 != 0 {
                pattern_bytes.push(0);
            }
        }

        // Pad pattern buffer total to 4-byte alignment
        while pattern_bytes.len() % 4 != 0 {
            pattern_bytes.push(0);
        }

        // Pad data to 4-byte alignment
        let padded_len = (data.len() + 3) & !3;
        let mut padded_data = data.to_vec();
        padded_data.resize(padded_len, 0);

        // Create buffers
        let input_buf = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("multi_scan_input"),
            contents: &padded_data,
            usage: wgpu::BufferUsages::STORAGE,
        });

        let patterns_buf = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("multi_scan_patterns"),
            contents: &pattern_bytes,
            usage: wgpu::BufferUsages::STORAGE,
        });

        let meta_buf = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("multi_scan_meta"),
            contents: bytemuck::cast_slice(&pattern_meta),
            usage: wgpu::BufferUsages::STORAGE,
        });

        // Results: [count, (pattern_idx, offset), (pattern_idx, offset), ...]
        let max_results: u32 = 65536;
        let results_size = ((1 + max_results * 2) as u64) * 4;

        let results_buf = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("multi_scan_results"),
            size: results_size,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });

        let staging_buf = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("multi_scan_staging"),
            size: results_size,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        #[repr(C)]
        #[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
        struct Params {
            total_bytes: u32,
            num_patterns: u32,
            max_results: u32,
            _pad: u32,
        }

        let params = Params {
            total_bytes: data.len() as u32,
            num_patterns: patterns.len() as u32,
            max_results,
            _pad: 0,
        };

        let params_buf = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("multi_scan_params"),
            contents: bytemuck::bytes_of(&params),
            usage: wgpu::BufferUsages::UNIFORM,
        });

        // Shader and pipeline
        let shader = self.device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("multi_scan_shader"),
            source: wgpu::ShaderSource::Wgsl(
                include_str!("../../../shaders/multi_pattern.wgsl").into(),
            ),
        });

        let pipeline = self.device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("multi_scan_pipeline"),
            layout: None,
            module: &shader,
            entry_point: Some("main"),
            compilation_options: Default::default(),
            cache: None,
        });

        let bind_group_layout = pipeline.get_bind_group_layout(0);
        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("multi_scan_bind_group"),
            layout: &bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: input_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: results_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: params_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: patterns_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: meta_buf.as_entire_binding(),
                },
            ],
        });

        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("multi_scan_encoder"),
        });

        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("multi_scan_pass"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&pipeline);
            pass.set_bind_group(0, &bind_group, &[]);
            let workgroups = (data.len() as u32).div_ceil(256).min(65535);
            pass.dispatch_workgroups(workgroups, 1, 1);
        }

        encoder.copy_buffer_to_buffer(&results_buf, 0, &staging_buf, 0, results_size);
        self.queue.submit(Some(encoder.finish()));

        // Read back results
        let staging_slice = staging_buf.slice(..);
        let (tx, rx) = std::sync::mpsc::channel();
        staging_slice.map_async(wgpu::MapMode::Read, move |result| {
            let _ = tx.send(result);
        });
        self.device.poll(wgpu::Maintain::Wait);
        rx.recv()
            .context("multi_scan readback channel closed")?
            .context("multi_scan readback failed")?;

        let mapped = staging_slice.get_mapped_range();
        let raw: &[u32] = bytemuck::cast_slice(&mapped);

        let hit_count = raw[0].min(max_results) as usize;
        let mut matches = Vec::with_capacity(hit_count);

        for i in 0..hit_count {
            let base = 1 + i * 2;
            matches.push(MultiPatternMatch {
                pattern_idx: raw[base],
                offset: raw[base + 1] as u64,
            });
        }

        drop(mapped);
        staging_buf.unmap();

        // Clean up
        drop(input_buf);
        drop(patterns_buf);
        drop(meta_buf);
        drop(results_buf);
        drop(params_buf);
        self.device.poll(wgpu::Maintain::Wait);

        matches.sort_by_key(|m| (m.offset, m.pattern_idx));
        Ok(matches)
    }

    /// Generate a Hilbert curve visualization of file data.
    ///
    /// Maps file data to a 2D texture using the Hilbert space-filling curve,
    /// which preserves spatial locality (nearby bytes appear nearby in 2D).
    ///
    /// # Arguments
    /// * `file_size` - Total file size in bytes
    /// * `entropy` - Optional per-block entropy data (block_size=256)
    /// * `classification` - Optional per-block classification data
    /// * `raw_data` - Optional raw file data (for byte value mode)
    /// * `texture_size` - Output texture size (must be power of 2: 256, 512, 1024)
    /// * `mode` - 0=entropy, 1=classification, 2=byte value
    ///
    /// # Returns
    /// RGBA pixel data as Vec<u32> (texture_size × texture_size pixels)
    pub fn compute_hilbert_texture(
        &self,
        file_size: u64,
        entropy: Option<&[f32]>,
        classification: Option<&[u8]>,
        sampled_bytes: Option<&[u8]>,  // Pre-sampled bytes (one per pixel, Hilbert-ordered)
        texture_size: u32,
        mode: u32,
    ) -> Result<Vec<u32>> {
        // Validate texture size is power of 2
        assert!(
            texture_size.is_power_of_two() && texture_size >= 64 && texture_size <= 2048,
            "texture_size must be power of 2 between 64 and 2048"
        );

        let total_pixels = (texture_size * texture_size) as u64;
        let bytes_per_pixel = ((file_size as u64) / total_pixels).max(1) as u32;

        // Prepare input buffers
        let empty_data: Vec<u8> = vec![];
        let empty_entropy: Vec<f32> = vec![];
        let empty_class: Vec<u8> = vec![];

        let data_slice = sampled_bytes.unwrap_or(&empty_data);
        let entropy_slice = entropy.unwrap_or(&empty_entropy);
        let class_slice = classification.unwrap_or(&empty_class);

        // Pad data to 4-byte alignment (sampled data is already small - one byte per pixel)
        let padded_data_len = ((data_slice.len() + 3) / 4) * 4;
        let mut padded_data = data_slice.to_vec();
        padded_data.resize(padded_data_len.max(4), 0);

        // Create buffers
        let input_buf = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("hilbert_input"),
            contents: &padded_data,
            usage: wgpu::BufferUsages::STORAGE,
        });

        // Entropy buffer (already f32)
        let entropy_data: Vec<f32> = if entropy_slice.is_empty() {
            vec![0.0f32; 4]
        } else {
            entropy_slice.to_vec()
        };
        let entropy_buf = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("hilbert_entropy"),
            contents: bytemuck::cast_slice(&entropy_data),
            usage: wgpu::BufferUsages::STORAGE,
        });

        // Classification buffer (pack u8 to u32)
        let class_data: Vec<u32> = if class_slice.is_empty() {
            vec![0u32; 4]
        } else {
            class_slice.iter().map(|&c| c as u32).collect()
        };
        let class_buf = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("hilbert_class"),
            contents: bytemuck::cast_slice(&class_data),
            usage: wgpu::BufferUsages::STORAGE,
        });

        // Params uniform buffer
        #[repr(C)]
        #[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
        struct HilbertParams {
            texture_size: u32,
            file_size: u32,
            bytes_per_pixel: u32,
            mode: u32,
        }

        let params = HilbertParams {
            texture_size,
            file_size: file_size as u32,
            bytes_per_pixel: bytes_per_pixel as u32,
            mode,
        };

        let params_buf = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("hilbert_params"),
            contents: bytemuck::bytes_of(&params),
            usage: wgpu::BufferUsages::UNIFORM,
        });

        // Output texture buffer
        let output_size = (texture_size * texture_size * 4) as u64; // RGBA8
        let output_buf = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("hilbert_output"),
            size: output_size,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });

        let staging_buf = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("hilbert_staging"),
            size: output_size,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // Load shader
        let shader = self.device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("hilbert_shader"),
            source: wgpu::ShaderSource::Wgsl(
                include_str!("../../../shaders/hilbert.wgsl").into(),
            ),
        });

        // Create pipeline
        let pipeline = self.device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("hilbert_pipeline"),
            layout: None,
            module: &shader,
            entry_point: Some("main"),
            compilation_options: Default::default(),
            cache: None,
        });

        // Create bind group
        let bind_group_layout = pipeline.get_bind_group_layout(0);
        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("hilbert_bind_group"),
            layout: &bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: input_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: entropy_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: class_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: params_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: output_buf.as_entire_binding(),
                },
            ],
        });

        // Dispatch compute
        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("hilbert_encoder"),
        });

        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("hilbert_pass"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&pipeline);
            pass.set_bind_group(0, &bind_group, &[]);

            // Workgroups: texture_size/16 x texture_size/16 (shader uses 16x16 workgroups)
            let workgroups = texture_size.div_ceil(16);
            pass.dispatch_workgroups(workgroups, workgroups, 1);
        }

        encoder.copy_buffer_to_buffer(&output_buf, 0, &staging_buf, 0, output_size);
        self.queue.submit(Some(encoder.finish()));

        // Read back results
        let staging_slice = staging_buf.slice(..);
        let (tx, rx) = std::sync::mpsc::channel();
        staging_slice.map_async(wgpu::MapMode::Read, move |result| {
            let _ = tx.send(result);
        });
        self.device.poll(wgpu::Maintain::Wait);
        rx.recv()
            .context("hilbert readback channel closed")?
            .context("hilbert readback failed")?;

        let mapped = staging_slice.get_mapped_range();
        let pixels: Vec<u32> = bytemuck::cast_slice(&mapped).to_vec();

        drop(mapped);
        staging_buf.unmap();

        // Cleanup
        drop(input_buf);
        drop(entropy_buf);
        drop(class_buf);
        drop(params_buf);
        drop(output_buf);
        self.device.poll(wgpu::Maintain::Wait);

        Ok(pixels)
    }

    /// Compare two byte buffers and return offsets where they differ.
    /// Uses GPU acceleration for large comparisons.
    ///
    /// # Arguments
    /// * `data_a` - First buffer
    /// * `data_b` - Second buffer
    /// * `max_diffs` - Maximum number of differences to return (for performance)
    ///
    /// # Returns
    /// Vector of byte offsets where the buffers differ
    pub fn compute_diff(
        &self,
        data_a: &[u8],
        data_b: &[u8],
        max_diffs: usize,
    ) -> Result<Vec<u64>> {
        let compare_len = data_a.len().min(data_b.len());
        if compare_len == 0 {
            return Ok(vec![]);
        }

        // Pad to 4-byte alignment
        let padded_len = ((compare_len + 3) / 4) * 4;
        let word_count = padded_len / 4;

        let mut padded_a = data_a[..compare_len].to_vec();
        padded_a.resize(padded_len, 0);

        let mut padded_b = data_b[..compare_len].to_vec();
        padded_b.resize(padded_len, 0);

        // Create buffers
        let buffer_a = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("diff_buffer_a"),
            contents: &padded_a,
            usage: wgpu::BufferUsages::STORAGE,
        });

        let buffer_b = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("diff_buffer_b"),
            contents: &padded_b,
            usage: wgpu::BufferUsages::STORAGE,
        });

        // Params uniform
        #[repr(C)]
        #[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
        struct DiffParams {
            byte_count: u32,
            _pad0: u32,
            _pad1: u32,
            _pad2: u32,
        }

        let params = DiffParams {
            byte_count: compare_len as u32,
            _pad0: 0,
            _pad1: 0,
            _pad2: 0,
        };

        let params_buf = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("diff_params"),
            contents: bytemuck::bytes_of(&params),
            usage: wgpu::BufferUsages::UNIFORM,
        });

        // Output buffer (one u32 per 4 bytes, contains diff flags)
        let output_size = (word_count * 4) as u64;
        let output_buf = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("diff_output"),
            size: output_size,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });

        let staging_buf = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("diff_staging"),
            size: output_size,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // Load shader
        let shader = self.device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("diff_shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("../../../shaders/diff.wgsl").into()),
        });

        // Create pipeline
        let pipeline = self.device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("diff_pipeline"),
            layout: None,
            module: &shader,
            entry_point: Some("main"),
            compilation_options: Default::default(),
            cache: None,
        });

        // Create bind group
        let bind_group_layout = pipeline.get_bind_group_layout(0);
        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("diff_bind_group"),
            layout: &bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: buffer_a.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: buffer_b.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: params_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: output_buf.as_entire_binding(),
                },
            ],
        });

        // Dispatch
        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("diff_encoder"),
        });

        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("diff_pass"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&pipeline);
            pass.set_bind_group(0, &bind_group, &[]);

            // 256 threads per workgroup, each handles one u32 (4 bytes)
            let workgroups = word_count.div_ceil(256);
            pass.dispatch_workgroups(workgroups as u32, 1, 1);
        }

        encoder.copy_buffer_to_buffer(&output_buf, 0, &staging_buf, 0, output_size);
        self.queue.submit(Some(encoder.finish()));

        // Read back results
        let staging_slice = staging_buf.slice(..);
        let (tx, rx) = std::sync::mpsc::channel();
        staging_slice.map_async(wgpu::MapMode::Read, move |result| {
            let _ = tx.send(result);
        });
        self.device.poll(wgpu::Maintain::Wait);
        rx.recv()
            .context("diff readback channel closed")?
            .context("diff readback failed")?;

        let mapped = staging_slice.get_mapped_range();
        let diff_flags: &[u32] = bytemuck::cast_slice(&mapped);

        // Extract diff offsets from flags
        let mut diff_offsets = Vec::new();
        for (word_idx, &flags) in diff_flags.iter().enumerate() {
            if flags != 0 {
                let base_offset = (word_idx * 4) as u64;
                for bit in 0..4 {
                    if (flags & (1 << bit)) != 0 {
                        let offset = base_offset + bit as u64;
                        if offset < compare_len as u64 {
                            diff_offsets.push(offset);
                            if diff_offsets.len() >= max_diffs {
                                break;
                            }
                        }
                    }
                }
            }
            if diff_offsets.len() >= max_diffs {
                break;
            }
        }

        drop(mapped);
        staging_buf.unmap();

        // Cleanup
        drop(buffer_a);
        drop(buffer_b);
        drop(params_buf);
        drop(output_buf);
        self.device.poll(wgpu::Maintain::Wait);

        Ok(diff_offsets)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_context() -> GpuContext {
        pollster::block_on(GpuContext::new()).expect("failed to init GPU context")
    }

    #[test]
    fn test_gpu_init() {
        let ctx = create_context();
        // If we got here without panic, device and queue are valid
        drop(ctx);
    }

    #[test]
    fn test_passthrough_shader() {
        let ctx = create_context();
        let input: Vec<u32> = vec![0, 1, 2, 3];
        let output = ctx.run_passthrough(&input).unwrap();
        assert_eq!(output, vec![1, 2, 3, 4]);
    }

    #[test]
    fn test_passthrough_large() {
        let ctx = create_context();
        let n = 65_536;
        let input: Vec<u32> = (0..n).collect();
        let output = ctx.run_passthrough(&input).unwrap();

        for i in 0..n as usize {
            assert_eq!(output[i], input[i] + 1, "mismatch at index {}", i);
        }
    }

    #[test]
    fn test_passthrough_max_values() {
        let ctx = create_context();
        let input = vec![u32::MAX, u32::MAX - 1, 0];
        let output = ctx.run_passthrough(&input).unwrap();
        // u32 wraps around in WGSL just like Rust wrapping_add
        assert_eq!(output, vec![0, u32::MAX, 1]);
    }

    #[test]
    fn test_passthrough_single_element() {
        let ctx = create_context();
        let output = ctx.run_passthrough(&[42]).unwrap();
        assert_eq!(output, vec![43]);
    }

    // --- Entropy tests ---

    #[test]
    fn test_entropy_all_zeros() {
        let ctx = create_context();
        let data = vec![0u8; 256];
        let result = ctx.compute_entropy(&data, 256).unwrap();
        assert_eq!(result.len(), 1);
        assert!(
            result[0].abs() < 0.01,
            "all-zeros entropy should be ~0.0, got {}",
            result[0]
        );
    }

    #[test]
    fn test_entropy_uniform_distribution() {
        let ctx = create_context();
        // 256 bytes, each value appearing exactly once → max entropy = 8.0
        let data: Vec<u8> = (0..=255).collect();
        let result = ctx.compute_entropy(&data, 256).unwrap();
        assert_eq!(result.len(), 1);
        assert!(
            (result[0] - 8.0).abs() < 0.01,
            "uniform entropy should be ~8.0, got {}",
            result[0]
        );
    }

    #[test]
    fn test_entropy_two_blocks() {
        let ctx = create_context();
        // Block 0: all zeros → entropy ~0
        // Block 1: uniform → entropy ~8
        let mut data = vec![0u8; 256];
        data.extend(0..=255u8);
        let result = ctx.compute_entropy(&data, 256).unwrap();
        assert_eq!(result.len(), 2);
        assert!(result[0].abs() < 0.01, "block0: expected ~0, got {}", result[0]);
        assert!((result[1] - 8.0).abs() < 0.01, "block1: expected ~8, got {}", result[1]);
    }

    #[test]
    fn test_entropy_gpu_matches_cpu() {
        let ctx = create_context();
        // Generate pseudo-random data with known seed
        let mut data = Vec::with_capacity(1024);
        let mut state: u64 = 0xCAFE_BABE;
        for _ in 0..1024 {
            state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
            data.push((state >> 33) as u8);
        }

        let gpu_result = ctx.compute_entropy(&data, 256).unwrap();
        let cpu_result = tv_core::entropy::compute_entropy_cpu(&data, 256);

        assert_eq!(gpu_result.len(), cpu_result.len());
        for (i, (gpu, cpu)) in gpu_result.iter().zip(cpu_result.iter()).enumerate() {
            assert!(
                (gpu - cpu).abs() < 0.05,
                "block {}: GPU={} vs CPU={}, diff={}",
                i, gpu, cpu, (gpu - cpu).abs()
            );
        }
    }

    #[test]
    fn test_entropy_larger_block_size() {
        let ctx = create_context();
        // 1024 bytes, block_size=512 → 2 blocks
        // Each 512-byte block has values 0-255 each appearing twice → uniform → entropy = 8.0
        let data: Vec<u8> = (0..1024).map(|i| (i % 256) as u8).collect();
        let result = ctx.compute_entropy(&data, 512).unwrap();
        assert_eq!(result.len(), 2);
        for (i, &e) in result.iter().enumerate() {
            assert!(
                (e - 8.0).abs() < 0.1,
                "block {}: expected ~8.0, got {}",
                i, e
            );
        }
    }

    // --- Pattern scan tests ---

    #[test]
    fn test_scan_pattern_found() {
        let ctx = create_context();
        // Embed "DEAD" at known offsets
        let mut data = vec![0u8; 1024];
        data[100] = 0xDE;
        data[101] = 0xAD;
        data[500] = 0xDE;
        data[501] = 0xAD;

        let offsets = ctx.scan_pattern(&data, &[0xDE, 0xAD]).unwrap();
        assert_eq!(offsets, vec![100, 500]);
    }

    #[test]
    fn test_scan_pattern_not_found() {
        let ctx = create_context();
        let data = vec![0u8; 512];
        let offsets = ctx.scan_pattern(&data, &[0xFF, 0xFE]).unwrap();
        assert!(offsets.is_empty());
    }

    #[test]
    fn test_scan_pattern_at_start_and_end() {
        let ctx = create_context();
        let mut data = vec![0xCCu8; 256];
        // Pattern at offset 0
        data[0] = 0xAB;
        data[1] = 0xCD;
        // Pattern at last possible position
        data[254] = 0xAB;
        data[255] = 0xCD;

        let offsets = ctx.scan_pattern(&data, &[0xAB, 0xCD]).unwrap();
        assert_eq!(offsets, vec![0, 254]);
    }

    #[test]
    fn test_scan_single_byte_pattern() {
        let ctx = create_context();
        let mut data = vec![0u8; 64];
        data[10] = 0xFF;
        data[20] = 0xFF;
        data[30] = 0xFF;

        let offsets = ctx.scan_pattern(&data, &[0xFF]).unwrap();
        assert_eq!(offsets, vec![10, 20, 30]);
    }

    // --- Classification tests ---

    #[test]
    fn test_classify_all_zeros() {
        let ctx = create_context();
        let data = vec![0u8; 256];
        let result = ctx.compute_classification(&data, 256).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0], 0, "expected Zeros(0), got {}", result[0]);
    }

    #[test]
    fn test_classify_ascii() {
        let ctx = create_context();
        let data: Vec<u8> = std::iter::repeat(b'A').take(256).collect();
        let result = ctx.compute_classification(&data, 256).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0], 1, "expected Ascii(1), got {}", result[0]);
    }

    #[test]
    fn test_classify_high_entropy() {
        let ctx = create_context();
        // All 256 byte values → max entropy = 8.0
        let data: Vec<u8> = (0..=255).collect();
        let result = ctx.compute_classification(&data, 256).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0], 4, "expected HighEntropy(4), got {}", result[0]);
    }

    #[test]
    fn test_classify_multiple_blocks() {
        let ctx = create_context();
        let mut data = Vec::new();
        // Block 0: all zeros
        data.extend(vec![0u8; 256]);
        // Block 1: ASCII text
        data.extend(std::iter::repeat(b'X').take(256));
        // Block 2: high entropy (all values)
        data.extend(0..=255u8);

        let result = ctx.compute_classification(&data, 256).unwrap();
        assert_eq!(result.len(), 3);
        assert_eq!(result[0], 0, "block 0: expected Zeros(0), got {}", result[0]);
        assert_eq!(result[1], 1, "block 1: expected Ascii(1), got {}", result[1]);
        assert_eq!(result[2], 4, "block 2: expected HighEntropy(4), got {}", result[2]);
    }

    #[test]
    fn test_classify_gpu_matches_cpu() {
        let ctx = create_context();
        let mut data = Vec::new();
        // Block 0: zeros
        data.extend(vec![0u8; 256]);
        // Block 1: ASCII
        data.extend(std::iter::repeat(b'A').take(256));
        // Block 2: high entropy
        data.extend(0..=255u8);
        // Block 3: more zeros
        data.extend(vec![0u8; 256]);

        let gpu_result = ctx.compute_classification(&data, 256).unwrap();
        let cpu_result = tv_core::classify::classify_blocks_cpu(&data, 256);

        assert_eq!(gpu_result.len(), cpu_result.len());
        for (i, (gpu, cpu)) in gpu_result.iter().zip(cpu_result.iter()).enumerate() {
            assert_eq!(
                *gpu,
                *cpu as u8,
                "block {}: GPU={} vs CPU={:?}",
                i, gpu, cpu
            );
        }
    }

    #[test]
    fn test_scan_elf_magic() {
        let ctx = create_context();
        let workspace_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent().unwrap()
            .parent().unwrap();
        let fixture = workspace_root.join("test-fixtures/embedded_magic.bin");
        if fixture.exists() {
            let file = tv_core::MappedFile::open(&fixture).unwrap();
            let data = file.slice(tv_core::FileRegion::new(0, file.len()));

            // ELF magic at offset 0
            let offsets = ctx.scan_pattern(data, &[0x7F, 0x45, 0x4C, 0x46]).unwrap();
            assert_eq!(offsets, vec![0]);

            // JPEG magic at offset 0x100
            let offsets = ctx.scan_pattern(data, &[0xFF, 0xD8, 0xFF, 0xE0]).unwrap();
            assert_eq!(offsets, vec![0x100]);

            // PNG magic at offset 0x200
            let offsets = ctx.scan_pattern(data, &[0x89, 0x50, 0x4E, 0x47]).unwrap();
            assert_eq!(offsets, vec![0x200]);
        }
    }

    // --- Multi-pattern scan tests ---

    #[test]
    fn test_multi_pattern_empty_patterns() {
        let ctx = create_context();
        let data = vec![0u8; 256];
        let matches = ctx.scan_multi_pattern(&data, &[]).unwrap();
        assert!(matches.is_empty());
    }

    #[test]
    fn test_multi_pattern_empty_data() {
        let ctx = create_context();
        let patterns: Vec<&[u8]> = vec![b"test"];
        let matches = ctx.scan_multi_pattern(&[], &patterns).unwrap();
        assert!(matches.is_empty());
    }

    #[test]
    fn test_multi_pattern_single_pattern() {
        let ctx = create_context();
        let mut data = vec![0u8; 512];
        data[100..104].copy_from_slice(&[0xDE, 0xAD, 0xBE, 0xEF]);
        data[300..304].copy_from_slice(&[0xDE, 0xAD, 0xBE, 0xEF]);

        let patterns: Vec<&[u8]> = vec![&[0xDE, 0xAD, 0xBE, 0xEF]];
        let matches = ctx.scan_multi_pattern(&data, &patterns).unwrap();

        assert_eq!(matches.len(), 2);
        assert_eq!(matches[0], MultiPatternMatch { pattern_idx: 0, offset: 100 });
        assert_eq!(matches[1], MultiPatternMatch { pattern_idx: 0, offset: 300 });
    }

    #[test]
    fn test_multi_pattern_multiple_patterns() {
        let ctx = create_context();
        let mut data = vec![0u8; 512];
        // Pattern 0: DEAD at offset 50
        data[50..52].copy_from_slice(&[0xDE, 0xAD]);
        // Pattern 1: BEEF at offset 100
        data[100..102].copy_from_slice(&[0xBE, 0xEF]);
        // Pattern 2: CAFE at offset 200
        data[200..202].copy_from_slice(&[0xCA, 0xFE]);

        let patterns: Vec<&[u8]> = vec![
            &[0xDE, 0xAD],
            &[0xBE, 0xEF],
            &[0xCA, 0xFE],
        ];
        let matches = ctx.scan_multi_pattern(&data, &patterns).unwrap();

        assert_eq!(matches.len(), 3);
        // Should be sorted by offset
        assert_eq!(matches[0], MultiPatternMatch { pattern_idx: 0, offset: 50 });
        assert_eq!(matches[1], MultiPatternMatch { pattern_idx: 1, offset: 100 });
        assert_eq!(matches[2], MultiPatternMatch { pattern_idx: 2, offset: 200 });
    }

    #[test]
    fn test_multi_pattern_overlapping_matches() {
        let ctx = create_context();
        // Data: AB BC CD
        let data = vec![0xAB, 0xBC, 0xCD];
        let patterns: Vec<&[u8]> = vec![
            &[0xAB, 0xBC],  // Pattern 0 at offset 0
            &[0xBC, 0xCD],  // Pattern 1 at offset 1
        ];
        let matches = ctx.scan_multi_pattern(&data, &patterns).unwrap();

        assert_eq!(matches.len(), 2);
        assert_eq!(matches[0], MultiPatternMatch { pattern_idx: 0, offset: 0 });
        assert_eq!(matches[1], MultiPatternMatch { pattern_idx: 1, offset: 1 });
    }

    #[test]
    fn test_multi_pattern_file_signatures() {
        let ctx = create_context();
        let mut data = vec![0u8; 1024];
        // Embed various "signatures" at known offsets
        // ELF at 0
        data[0..4].copy_from_slice(&[0x7F, 0x45, 0x4C, 0x46]);
        // JPEG at 256
        data[256..260].copy_from_slice(&[0xFF, 0xD8, 0xFF, 0xE0]);
        // PNG at 512
        data[512..516].copy_from_slice(&[0x89, 0x50, 0x4E, 0x47]);
        // PDF at 768
        data[768..773].copy_from_slice(&[0x25, 0x50, 0x44, 0x46, 0x2D]);

        let patterns: Vec<&[u8]> = vec![
            &[0x7F, 0x45, 0x4C, 0x46],      // ELF
            &[0xFF, 0xD8, 0xFF, 0xE0],      // JPEG
            &[0x89, 0x50, 0x4E, 0x47],      // PNG
            &[0x25, 0x50, 0x44, 0x46, 0x2D], // PDF
        ];

        let matches = ctx.scan_multi_pattern(&data, &patterns).unwrap();

        assert_eq!(matches.len(), 4);
        assert_eq!(matches[0], MultiPatternMatch { pattern_idx: 0, offset: 0 });
        assert_eq!(matches[1], MultiPatternMatch { pattern_idx: 1, offset: 256 });
        assert_eq!(matches[2], MultiPatternMatch { pattern_idx: 2, offset: 512 });
        assert_eq!(matches[3], MultiPatternMatch { pattern_idx: 3, offset: 768 });
    }

    #[test]
    fn test_multi_pattern_no_matches() {
        let ctx = create_context();
        let data = vec![0u8; 256];
        let patterns: Vec<&[u8]> = vec![
            &[0xDE, 0xAD],
            &[0xBE, 0xEF],
        ];
        let matches = ctx.scan_multi_pattern(&data, &patterns).unwrap();
        assert!(matches.is_empty());
    }
}
