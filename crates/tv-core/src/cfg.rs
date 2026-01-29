//! Control Flow Graph (CFG) analysis for disassembled code.
//!
//! Builds a graph of basic blocks from disassembled instructions,
//! detecting control flow edges from jumps, calls, and fall-through.

use std::collections::{HashMap, HashSet, VecDeque};

/// A single disassembled instruction for CFG analysis.
#[derive(Debug, Clone)]
pub struct CfgInstruction {
    /// Address of the instruction.
    pub address: u64,
    /// Size in bytes.
    pub size: u8,
    /// Mnemonic (e.g., "jmp", "call", "mov").
    pub mnemonic: String,
    /// Operands string.
    pub operands: String,
    /// Raw bytes.
    pub bytes: Vec<u8>,
}

impl CfgInstruction {
    /// Check if this instruction is a control flow instruction.
    pub fn is_control_flow(&self) -> bool {
        self.is_jump() || self.is_call() || self.is_return()
    }

    /// Check if this is any kind of jump.
    pub fn is_jump(&self) -> bool {
        let m = self.mnemonic.to_lowercase();
        m == "jmp"
            || (m.starts_with("j") && m.len() <= 4)  // jne, je, jz, jnz, jle, jge, etc.
            || m == "loop" || m == "loope" || m == "loopne"
    }

    /// Check if this is an unconditional jump.
    pub fn is_unconditional_jump(&self) -> bool {
        self.mnemonic.to_lowercase() == "jmp"
    }

    /// Check if this is a conditional jump.
    pub fn is_conditional_jump(&self) -> bool {
        self.is_jump() && !self.is_unconditional_jump()
    }

    /// Check if this is a call instruction.
    pub fn is_call(&self) -> bool {
        self.mnemonic.to_lowercase() == "call"
    }

    /// Check if this is a return instruction.
    pub fn is_return(&self) -> bool {
        let m = self.mnemonic.to_lowercase();
        m == "ret" || m == "retn" || m == "retf" || m == "iret" || m == "iretd" || m == "iretq"
    }

    /// Try to extract the target address from jump/call operands.
    pub fn target_address(&self) -> Option<u64> {
        let ops = self.operands.trim();
        if ops.is_empty() {
            return None;
        }

        // Try to parse as hex (0x...) or decimal
        // Capstone can output addresses in various formats:
        // - "0x7f" (hex with prefix)
        // - "7f" (hex without prefix)
        // - "127" (decimal)
        // - For indirect jumps: "rax", "[rip + 0x100]", etc. (ignore these)

        // First, check for indirect addressing patterns (brackets, registers)
        if ops.contains('[') || ops.contains(']') {
            return None; // Indirect jump - can't determine static target
        }

        // Check if it's a register name (common x86 registers)
        let lower = ops.to_lowercase();
        if matches!(lower.as_str(),
            "rax" | "rbx" | "rcx" | "rdx" | "rsi" | "rdi" | "rbp" | "rsp" |
            "r8" | "r9" | "r10" | "r11" | "r12" | "r13" | "r14" | "r15" |
            "eax" | "ebx" | "ecx" | "edx" | "esi" | "edi" | "ebp" | "esp" |
            "ax" | "bx" | "cx" | "dx" | "si" | "di" | "bp" | "sp"
        ) {
            return None; // Register indirect - can't determine static target
        }

        // Try 0x prefix (case insensitive)
        if let Some(hex) = ops.strip_prefix("0x").or_else(|| ops.strip_prefix("0X")) {
            return u64::from_str_radix(hex, 16).ok();
        }

        // If the string contains any hex letters, try parsing as hex
        if ops.chars().any(|c| matches!(c.to_ascii_lowercase(), 'a'..='f')) {
            return u64::from_str_radix(ops, 16).ok();
        }

        // Otherwise try decimal
        ops.parse().ok()
    }
}

/// A basic block: a sequence of instructions with single entry and exit.
#[derive(Debug, Clone)]
pub struct BasicBlock {
    /// Starting address of the block.
    pub start_addr: u64,
    /// Ending address (address of last instruction).
    pub end_addr: u64,
    /// Instructions in this block.
    pub instructions: Vec<CfgInstruction>,
    /// Addresses of successor blocks (outgoing edges).
    pub successors: Vec<u64>,
    /// Addresses of predecessor blocks (incoming edges).
    pub predecessors: Vec<u64>,
    /// Layout position (computed by layout algorithm).
    pub layout_x: f32,
    pub layout_y: f32,
    /// Layer in hierarchical layout.
    pub layer: usize,
}

impl BasicBlock {
    /// Create a new empty basic block starting at the given address.
    pub fn new(start_addr: u64) -> Self {
        Self {
            start_addr,
            end_addr: start_addr,
            instructions: Vec::new(),
            successors: Vec::new(),
            predecessors: Vec::new(),
            layout_x: 0.0,
            layout_y: 0.0,
            layer: 0,
        }
    }

    /// Add an instruction to this block.
    pub fn add_instruction(&mut self, instr: CfgInstruction) {
        self.end_addr = instr.address;
        self.instructions.push(instr);
    }

    /// Get the last instruction in the block.
    pub fn last_instruction(&self) -> Option<&CfgInstruction> {
        self.instructions.last()
    }

    /// Check if this block ends with a return.
    pub fn ends_with_return(&self) -> bool {
        self.last_instruction().is_some_and(|i| i.is_return())
    }

    /// Check if this block ends with an unconditional jump.
    pub fn ends_with_unconditional_jump(&self) -> bool {
        self.last_instruction().is_some_and(|i| i.is_unconditional_jump())
    }

    /// Get the computed height for rendering.
    pub fn render_height(&self) -> f32 {
        let header_height = 20.0;
        let instr_height = 14.0;
        header_height + (self.instructions.len() as f32 * instr_height) + 10.0
    }

    /// Get the computed width for rendering.
    pub fn render_width(&self) -> f32 {
        // Estimate based on longest instruction
        let max_len = self.instructions.iter()
            .map(|i| format!("{:08X}  {} {}", i.address, i.mnemonic, i.operands).len())
            .max()
            .unwrap_or(20);
        (max_len as f32 * 7.0).max(150.0).min(400.0)
    }
}

/// Edge type in the CFG.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EdgeType {
    /// Unconditional jump or fall-through.
    Unconditional,
    /// Conditional branch taken.
    ConditionalTrue,
    /// Conditional branch not taken (fall-through).
    ConditionalFalse,
    /// Function call (and return).
    Call,
}

/// An edge in the control flow graph.
#[derive(Debug, Clone)]
pub struct CfgEdge {
    pub from: u64,
    pub to: u64,
    pub edge_type: EdgeType,
}

/// The complete Control Flow Graph.
#[derive(Debug, Clone)]
pub struct ControlFlowGraph {
    /// Entry point address.
    pub entry: u64,
    /// Basic blocks indexed by start address.
    pub blocks: HashMap<u64, BasicBlock>,
    /// All edges in the graph.
    pub edges: Vec<CfgEdge>,
    /// Computed layers for layout.
    pub layers: Vec<Vec<u64>>,
}

impl ControlFlowGraph {
    /// Build a CFG from a list of disassembled instructions.
    pub fn build(instructions: &[CfgInstruction], entry: u64) -> Self {
        let mut cfg = Self {
            entry,
            blocks: HashMap::new(),
            edges: Vec::new(),
            layers: Vec::new(),
        };

        if instructions.is_empty() {
            return cfg;
        }

        // Debug: count control flow instructions
        #[cfg(debug_assertions)]
        {
            let jumps: Vec<_> = instructions.iter()
                .filter(|i| i.is_jump())
                .map(|i| format!("{:X}: {} {} -> {:?}", i.address, i.mnemonic, i.operands, i.target_address()))
                .collect();
            if !jumps.is_empty() {
                eprintln!("[CFG] Found {} jump instructions:", jumps.len());
                for j in &jumps {
                    eprintln!("  {}", j);
                }
            }
        }

        // Step 1: Identify block boundaries (leaders)
        let leaders = Self::find_leaders(instructions, entry);

        #[cfg(debug_assertions)]
        eprintln!("[CFG] Leaders: {:X?}", leaders.iter().collect::<Vec<_>>());

        // Step 2: Build basic blocks
        cfg.build_blocks(instructions, &leaders);

        #[cfg(debug_assertions)]
        eprintln!("[CFG] Built {} blocks at: {:X?}", cfg.blocks.len(), cfg.blocks.keys().collect::<Vec<_>>());

        // Step 3: Connect blocks with edges
        cfg.build_edges();

        #[cfg(debug_assertions)]
        eprintln!("[CFG] Built {} edges", cfg.edges.len());

        // Step 4: Compute layout
        cfg.compute_layout();

        cfg
    }

    /// Find leader instructions (block start addresses).
    fn find_leaders(instructions: &[CfgInstruction], entry: u64) -> HashSet<u64> {
        let mut leaders = HashSet::new();

        // Entry point is always a leader
        leaders.insert(entry);

        // First instruction is a leader
        if let Some(first) = instructions.first() {
            leaders.insert(first.address);
        }

        // Build address set for quick lookup
        let addr_set: HashSet<u64> = instructions.iter().map(|i| i.address).collect();

        for (i, instr) in instructions.iter().enumerate() {
            if instr.is_jump() || instr.is_call() {
                // Target of jump/call is a leader (if in our range)
                if let Some(target) = instr.target_address() {
                    if addr_set.contains(&target) {
                        leaders.insert(target);
                    }
                }

                // Instruction after jump/call is a leader (fall-through)
                if i + 1 < instructions.len() {
                    leaders.insert(instructions[i + 1].address);
                }
            }

            if instr.is_return() {
                // Instruction after return is a leader
                if i + 1 < instructions.len() {
                    leaders.insert(instructions[i + 1].address);
                }
            }
        }

        leaders
    }

    /// Build basic blocks from instructions and leaders.
    fn build_blocks(&mut self, instructions: &[CfgInstruction], leaders: &HashSet<u64>) {
        let mut current_block: Option<BasicBlock> = None;

        for instr in instructions {
            if leaders.contains(&instr.address) {
                // Start a new block
                if let Some(block) = current_block.take() {
                    self.blocks.insert(block.start_addr, block);
                }
                current_block = Some(BasicBlock::new(instr.address));
            }

            if let Some(ref mut block) = current_block {
                block.add_instruction(instr.clone());
            }
        }

        // Don't forget the last block
        if let Some(block) = current_block {
            self.blocks.insert(block.start_addr, block);
        }
    }

    /// Build edges between blocks based on control flow.
    fn build_edges(&mut self) {
        let block_addrs: Vec<u64> = self.blocks.keys().copied().collect();

        // Build a map of address -> block start for fall-through detection
        let mut addr_to_block: HashMap<u64, u64> = HashMap::new();
        for block in self.blocks.values() {
            for instr in &block.instructions {
                addr_to_block.insert(instr.address, block.start_addr);
            }
        }

        // Also add block start addresses to the map
        for &addr in &block_addrs {
            addr_to_block.insert(addr, addr);
        }

        // Helper to find containing block
        let find_block = |addr: u64| -> Option<u64> {
            addr_to_block.get(&addr).copied()
        };

        // Collect all edges and successor updates first
        let mut all_edges: Vec<CfgEdge> = Vec::new();
        let mut successor_updates: HashMap<u64, Vec<u64>> = HashMap::new();

        for &block_addr in &block_addrs {
            let block = &self.blocks[&block_addr];
            let last_instr = match block.last_instruction() {
                Some(i) => i.clone(),
                None => continue,
            };

            let mut successors = Vec::new();

            // Handle jumps
            if last_instr.is_jump() {
                if let Some(target) = last_instr.target_address() {
                    // Find the block containing this target
                    if let Some(target_block) = find_block(target) {
                        if last_instr.is_conditional_jump() {
                            // Conditional: true branch
                            all_edges.push(CfgEdge {
                                from: block_addr,
                                to: target_block,
                                edge_type: EdgeType::ConditionalTrue,
                            });
                            successors.push(target_block);

                            // Fall-through: false branch
                            let fall_through = last_instr.address + last_instr.size as u64;
                            if let Some(next_block) = find_block(fall_through) {
                                all_edges.push(CfgEdge {
                                    from: block_addr,
                                    to: next_block,
                                    edge_type: EdgeType::ConditionalFalse,
                                });
                                if !successors.contains(&next_block) {
                                    successors.push(next_block);
                                }
                            }
                        } else {
                            // Unconditional jump
                            all_edges.push(CfgEdge {
                                from: block_addr,
                                to: target_block,
                                edge_type: EdgeType::Unconditional,
                            });
                            successors.push(target_block);
                        }
                    } else if last_instr.is_conditional_jump() {
                        // Target not found, but still add fall-through for conditional
                        let fall_through = last_instr.address + last_instr.size as u64;
                        if let Some(next_block) = find_block(fall_through) {
                            all_edges.push(CfgEdge {
                                from: block_addr,
                                to: next_block,
                                edge_type: EdgeType::ConditionalFalse,
                            });
                            successors.push(next_block);
                        }
                    }
                }
            } else if last_instr.is_call() {
                // Call: fall-through to next instruction
                let fall_through = last_instr.address + last_instr.size as u64;
                if let Some(next_block) = find_block(fall_through) {
                    all_edges.push(CfgEdge {
                        from: block_addr,
                        to: next_block,
                        edge_type: EdgeType::Call,
                    });
                    successors.push(next_block);
                }
            } else if !last_instr.is_return() {
                // Normal instruction: fall-through
                let fall_through = last_instr.address + last_instr.size as u64;
                if let Some(next_block) = find_block(fall_through) {
                    all_edges.push(CfgEdge {
                        from: block_addr,
                        to: next_block,
                        edge_type: EdgeType::Unconditional,
                    });
                    successors.push(next_block);
                }
            }

            successor_updates.insert(block_addr, successors);
        }

        // Apply updates
        for (addr, successors) in successor_updates {
            if let Some(block) = self.blocks.get_mut(&addr) {
                block.successors = successors;
            }
        }

        self.edges = all_edges;

        // Build predecessors from edges
        for edge in &self.edges {
            if let Some(block) = self.blocks.get_mut(&edge.to) {
                if !block.predecessors.contains(&edge.from) {
                    block.predecessors.push(edge.from);
                }
            }
        }
    }

    /// Compute hierarchical layout for the graph.
    pub fn compute_layout(&mut self) {
        if self.blocks.is_empty() {
            return;
        }

        // Step 1: Assign layers using BFS from entry
        self.assign_layers();

        // Step 2: Order nodes within layers
        self.order_within_layers();

        // Step 3: Assign X/Y coordinates
        self.assign_coordinates();
    }

    /// Assign layers using BFS (Coffman-Graham style).
    fn assign_layers(&mut self) {
        let mut visited: HashSet<u64> = HashSet::new();
        let mut queue: VecDeque<(u64, usize)> = VecDeque::new();
        let mut max_layer: HashMap<u64, usize> = HashMap::new();

        // Start from entry
        queue.push_back((self.entry, 0));

        while let Some((addr, layer)) = queue.pop_front() {
            if !self.blocks.contains_key(&addr) {
                continue;
            }

            // Track maximum layer for each node (handles back edges)
            let current_max = max_layer.entry(addr).or_insert(0);
            if layer > *current_max {
                *current_max = layer;
            }

            if visited.contains(&addr) {
                continue;
            }
            visited.insert(addr);

            // Add successors
            if let Some(block) = self.blocks.get(&addr) {
                for &succ in &block.successors {
                    queue.push_back((succ, layer + 1));
                }
            }
        }

        // Also process unreachable blocks
        for &addr in self.blocks.keys() {
            if !visited.contains(&addr) {
                max_layer.insert(addr, 0);
            }
        }

        // Apply layers to blocks
        for (&addr, &layer) in &max_layer {
            if let Some(block) = self.blocks.get_mut(&addr) {
                block.layer = layer;
            }
        }

        // Build layer list
        let max_layer_num = max_layer.values().copied().max().unwrap_or(0);
        self.layers = vec![Vec::new(); max_layer_num + 1];
        for (&addr, &layer) in &max_layer {
            self.layers[layer].push(addr);
        }
    }

    /// Order nodes within each layer to minimize edge crossings.
    fn order_within_layers(&mut self) {
        // Simple heuristic: sort by predecessor positions
        for layer_idx in 1..self.layers.len() {
            let prev_layer = &self.layers[layer_idx - 1];
            let prev_positions: HashMap<u64, usize> = prev_layer
                .iter()
                .enumerate()
                .map(|(i, &addr)| (addr, i))
                .collect();

            // Sort current layer by average predecessor position
            let mut layer_with_scores: Vec<(u64, f32)> = self.layers[layer_idx]
                .iter()
                .map(|&addr| {
                    let block = &self.blocks[&addr];
                    let avg_pos = if block.predecessors.is_empty() {
                        0.0
                    } else {
                        let sum: usize = block.predecessors.iter()
                            .filter_map(|&p| prev_positions.get(&p))
                            .sum();
                        let count = block.predecessors.iter()
                            .filter(|&p| prev_positions.contains_key(p))
                            .count();
                        if count > 0 { sum as f32 / count as f32 } else { 0.0 }
                    };
                    (addr, avg_pos)
                })
                .collect();

            layer_with_scores.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());
            self.layers[layer_idx] = layer_with_scores.into_iter().map(|(addr, _)| addr).collect();
        }
    }

    /// Assign X/Y coordinates based on layers.
    fn assign_coordinates(&mut self) {
        let layer_spacing = 150.0;
        let node_spacing = 50.0;

        for (layer_idx, layer) in self.layers.iter().enumerate() {
            let y = layer_idx as f32 * layer_spacing;

            // Calculate total width of this layer
            let total_width: f32 = layer.iter()
                .map(|&addr| self.blocks[&addr].render_width() + node_spacing)
                .sum();

            // Center the layer
            let mut x = -total_width / 2.0;

            for &addr in layer {
                if let Some(block) = self.blocks.get_mut(&addr) {
                    block.layout_x = x;
                    block.layout_y = y;
                    x += block.render_width() + node_spacing;
                }
            }
        }
    }

    /// Get the bounding box of the graph.
    pub fn bounds(&self) -> (f32, f32, f32, f32) {
        if self.blocks.is_empty() {
            return (0.0, 0.0, 100.0, 100.0);
        }

        let mut min_x = f32::MAX;
        let mut min_y = f32::MAX;
        let mut max_x = f32::MIN;
        let mut max_y = f32::MIN;

        for block in self.blocks.values() {
            min_x = min_x.min(block.layout_x);
            min_y = min_y.min(block.layout_y);
            max_x = max_x.max(block.layout_x + block.render_width());
            max_y = max_y.max(block.layout_y + block.render_height());
        }

        (min_x, min_y, max_x, max_y)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_instr(addr: u64, size: u8, mnemonic: &str, operands: &str) -> CfgInstruction {
        CfgInstruction {
            address: addr,
            size,
            mnemonic: mnemonic.to_string(),
            operands: operands.to_string(),
            bytes: vec![0; size as usize],
        }
    }

    #[test]
    fn test_instruction_classification() {
        let jmp = make_instr(0, 5, "jmp", "0x100");
        assert!(jmp.is_jump());
        assert!(jmp.is_unconditional_jump());
        assert!(!jmp.is_conditional_jump());

        let jne = make_instr(0, 2, "jne", "0x50");
        assert!(jne.is_jump());
        assert!(!jne.is_unconditional_jump());
        assert!(jne.is_conditional_jump());

        let call = make_instr(0, 5, "call", "0x200");
        assert!(call.is_call());
        assert!(!call.is_jump());

        let ret = make_instr(0, 1, "ret", "");
        assert!(ret.is_return());
    }

    #[test]
    fn test_target_address_parsing() {
        let instr = make_instr(0, 5, "jmp", "0x1234");
        assert_eq!(instr.target_address(), Some(0x1234));

        let instr2 = make_instr(0, 5, "jmp", "ABCD");
        assert_eq!(instr2.target_address(), Some(0xABCD));
    }

    #[test]
    fn test_simple_cfg() {
        // Simple linear code with one conditional branch
        let instructions = vec![
            make_instr(0x00, 3, "mov", "eax, 1"),
            make_instr(0x03, 3, "cmp", "eax, 0"),
            make_instr(0x06, 2, "jne", "0x10"),  // Jump to 0x10 if not equal
            make_instr(0x08, 3, "mov", "ebx, 1"),
            make_instr(0x0B, 1, "ret", ""),
            make_instr(0x10, 3, "mov", "ebx, 2"),
            make_instr(0x13, 1, "ret", ""),
        ];

        let cfg = ControlFlowGraph::build(&instructions, 0x00);

        // Should have 4 blocks: entry, fall-through, branch target, and the block after jne
        assert!(cfg.blocks.len() >= 3);
        assert!(cfg.blocks.contains_key(&0x00));
        assert!(cfg.blocks.contains_key(&0x10));
    }

    #[test]
    fn test_empty_cfg() {
        let cfg = ControlFlowGraph::build(&[], 0);
        assert!(cfg.blocks.is_empty());
        assert!(cfg.edges.is_empty());
    }

    #[test]
    fn test_cfg_with_edges() {
        // Simulate real Capstone output format
        let instructions = vec![
            make_instr(0x100, 1, "push", "rbp"),
            make_instr(0x101, 3, "mov", "rbp, rsp"),
            make_instr(0x104, 3, "cmp", "rdi, 0"),
            make_instr(0x107, 2, "je", "0x110"),   // Conditional jump to 0x110
            make_instr(0x109, 5, "mov", "eax, 1"),
            make_instr(0x10E, 1, "ret", ""),
            make_instr(0x110, 5, "mov", "eax, 0"), // Target of je
            make_instr(0x115, 1, "ret", ""),
        ];

        let cfg = ControlFlowGraph::build(&instructions, 0x100);

        eprintln!("Blocks: {:?}", cfg.blocks.keys().collect::<Vec<_>>());
        eprintln!("Edges: {:?}", cfg.edges);

        // Should have blocks at: 0x100 (entry), 0x109 (after je), 0x110 (target)
        assert!(cfg.blocks.len() >= 3, "Expected at least 3 blocks, got {}", cfg.blocks.len());

        // Should have at least 2 edges from the conditional jump
        assert!(cfg.edges.len() >= 2, "Expected at least 2 edges, got {}: {:?}", cfg.edges.len(), cfg.edges);

        // Verify we have both true and false branches
        let has_true = cfg.edges.iter().any(|e| e.edge_type == EdgeType::ConditionalTrue);
        let has_false = cfg.edges.iter().any(|e| e.edge_type == EdgeType::ConditionalFalse);
        assert!(has_true, "Missing ConditionalTrue edge");
        assert!(has_false, "Missing ConditionalFalse edge");
    }

    #[test]
    fn test_target_parsing_formats() {
        // Test different operand formats that Capstone might output
        let instr1 = make_instr(0, 2, "jne", "0x7f");
        assert_eq!(instr1.target_address(), Some(0x7f));

        let instr2 = make_instr(0, 2, "jne", "0x107");
        assert_eq!(instr2.target_address(), Some(0x107));

        let instr3 = make_instr(0, 2, "jne", "7f");
        assert_eq!(instr3.target_address(), Some(0x7f));

        let instr4 = make_instr(0, 2, "jne", "0X7F");
        assert_eq!(instr4.target_address(), Some(0x7f));

        // Indirect jumps should return None
        let instr5 = make_instr(0, 2, "jmp", "rax");
        assert_eq!(instr5.target_address(), None);

        let instr6 = make_instr(0, 6, "jmp", "[rip + 0x100]");
        assert_eq!(instr6.target_address(), None);
    }
}
