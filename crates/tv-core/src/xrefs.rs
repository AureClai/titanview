//! Cross-Reference (XRef) analysis for binary code.
//!
//! Tracks where addresses are referenced from, allowing navigation
//! from a target address back to all locations that reference it.

use std::collections::HashMap;

/// Type of cross-reference.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum XRefType {
    /// Call instruction targeting this address.
    Call,
    /// Jump instruction targeting this address.
    Jump,
    /// Data reference (lea, mov with address, etc.).
    Data,
    /// Read operation from this address.
    Read,
    /// Write operation to this address.
    Write,
}

impl XRefType {
    pub fn label(&self) -> &'static str {
        match self {
            XRefType::Call => "CALL",
            XRefType::Jump => "JUMP",
            XRefType::Data => "DATA",
            XRefType::Read => "READ",
            XRefType::Write => "WRITE",
        }
    }

    pub fn short_label(&self) -> &'static str {
        match self {
            XRefType::Call => "c",
            XRefType::Jump => "j",
            XRefType::Data => "d",
            XRefType::Read => "r",
            XRefType::Write => "w",
        }
    }
}

/// A single cross-reference entry.
#[derive(Debug, Clone)]
pub struct XRef {
    /// Address where the reference originates (the instruction doing the referencing).
    pub from: u64,
    /// Address being referenced (the target).
    pub to: u64,
    /// Type of reference.
    pub xref_type: XRefType,
    /// Instruction mnemonic that creates this reference.
    pub mnemonic: String,
}

/// Collection of cross-references for a binary.
#[derive(Debug, Clone, Default)]
pub struct XRefTable {
    /// References TO an address (key = target address, value = list of sources).
    pub refs_to: HashMap<u64, Vec<XRef>>,
    /// References FROM an address (key = source address, value = list of targets).
    pub refs_from: HashMap<u64, Vec<XRef>>,
    /// Total number of references.
    pub total_refs: usize,
}

impl XRefTable {
    /// Create a new empty XRef table.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a cross-reference.
    pub fn add(&mut self, xref: XRef) {
        let to = xref.to;
        let from = xref.from;

        self.refs_to
            .entry(to)
            .or_insert_with(Vec::new)
            .push(xref.clone());

        self.refs_from
            .entry(from)
            .or_insert_with(Vec::new)
            .push(xref);

        self.total_refs += 1;
    }

    /// Get all references TO an address.
    pub fn get_refs_to(&self, addr: u64) -> Option<&Vec<XRef>> {
        self.refs_to.get(&addr)
    }

    /// Get all references FROM an address.
    pub fn get_refs_from(&self, addr: u64) -> Option<&Vec<XRef>> {
        self.refs_from.get(&addr)
    }

    /// Get count of references to an address.
    pub fn count_refs_to(&self, addr: u64) -> usize {
        self.refs_to.get(&addr).map_or(0, |v| v.len())
    }

    /// Get all addresses that have incoming references.
    pub fn referenced_addresses(&self) -> Vec<u64> {
        let mut addrs: Vec<u64> = self.refs_to.keys().copied().collect();
        addrs.sort();
        addrs
    }

    /// Clear all references.
    pub fn clear(&mut self) {
        self.refs_to.clear();
        self.refs_from.clear();
        self.total_refs = 0;
    }

    /// Build XRef table from disassembled instructions.
    pub fn from_instructions(instructions: &[crate::Instruction]) -> Self {
        let mut table = Self::new();

        for instr in instructions {
            let mnemonic_lower = instr.mnemonic.to_lowercase();

            // Determine XRef type based on mnemonic
            let xref_type = if mnemonic_lower == "call" {
                Some(XRefType::Call)
            } else if mnemonic_lower.starts_with("j") && mnemonic_lower.len() <= 4 {
                // Jump instructions: jmp, je, jne, jz, jnz, etc.
                Some(XRefType::Jump)
            } else if mnemonic_lower == "jmp" {
                Some(XRefType::Jump)
            } else if mnemonic_lower == "lea" {
                Some(XRefType::Data)
            } else if mnemonic_lower.starts_with("mov") && instr.operands.contains('[') {
                // Memory access - determine read/write
                // If destination is memory, it's a write; if source is memory, it's a read
                let ops = &instr.operands;
                if let Some(comma_pos) = ops.find(',') {
                    let dest = &ops[..comma_pos];
                    if dest.contains('[') {
                        Some(XRefType::Write)
                    } else {
                        Some(XRefType::Read)
                    }
                } else {
                    Some(XRefType::Data)
                }
            } else {
                None
            };

            if let Some(xtype) = xref_type {
                // Try to extract target address from operands
                if let Some(target) = parse_address_from_operands(&instr.operands) {
                    table.add(XRef {
                        from: instr.address,
                        to: target,
                        xref_type: xtype,
                        mnemonic: instr.mnemonic.clone(),
                    });
                }
            }
        }

        table
    }
}

/// Parse an address from instruction operands.
/// Handles formats like "0x1234", "1234h", "[0x1234]", "rip + 0x100", etc.
fn parse_address_from_operands(operands: &str) -> Option<u64> {
    let ops = operands.trim();
    if ops.is_empty() {
        return None;
    }

    // Skip if it's a register-only operand
    let lower = ops.to_lowercase();
    if is_register(&lower) {
        return None;
    }

    // Try to find hex address with 0x prefix
    if let Some(hex_start) = ops.find("0x").or_else(|| ops.find("0X")) {
        let hex_part = &ops[hex_start + 2..];
        let hex_end = hex_part
            .find(|c: char| !c.is_ascii_hexdigit())
            .unwrap_or(hex_part.len());
        if hex_end > 0 {
            if let Ok(addr) = u64::from_str_radix(&hex_part[..hex_end], 16) {
                return Some(addr);
            }
        }
    }

    // Try to parse as pure hex (if contains a-f characters)
    let clean = ops
        .trim_start_matches('[')
        .trim_end_matches(']')
        .trim();

    if clean.chars().any(|c| matches!(c.to_ascii_lowercase(), 'a'..='f'))
        && clean.chars().all(|c| c.is_ascii_hexdigit())
    {
        if let Ok(addr) = u64::from_str_radix(clean, 16) {
            return Some(addr);
        }
    }

    None
}

/// Check if a string is a common x86 register name.
fn is_register(s: &str) -> bool {
    matches!(
        s,
        "rax" | "rbx" | "rcx" | "rdx" | "rsi" | "rdi" | "rbp" | "rsp" |
        "r8" | "r9" | "r10" | "r11" | "r12" | "r13" | "r14" | "r15" |
        "eax" | "ebx" | "ecx" | "edx" | "esi" | "edi" | "ebp" | "esp" |
        "ax" | "bx" | "cx" | "dx" | "si" | "di" | "bp" | "sp" |
        "al" | "bl" | "cl" | "dl" | "ah" | "bh" | "ch" | "dh" |
        "rip" | "eip" | "ip"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Instruction;

    fn make_instr(addr: u64, mnemonic: &str, operands: &str) -> Instruction {
        Instruction {
            address: addr,
            bytes: vec![],
            mnemonic: mnemonic.to_string(),
            operands: operands.to_string(),
        }
    }

    #[test]
    fn test_xref_from_call() {
        let instructions = vec![
            make_instr(0x100, "call", "0x200"),
            make_instr(0x105, "mov", "eax, ebx"),
            make_instr(0x108, "call", "0x200"),
        ];

        let table = XRefTable::from_instructions(&instructions);

        // Should have 2 references to 0x200
        assert_eq!(table.count_refs_to(0x200), 2);

        let refs = table.get_refs_to(0x200).unwrap();
        assert_eq!(refs[0].from, 0x100);
        assert_eq!(refs[1].from, 0x108);
        assert_eq!(refs[0].xref_type, XRefType::Call);
    }

    #[test]
    fn test_xref_from_jump() {
        let instructions = vec![
            make_instr(0x100, "jne", "0x110"),
            make_instr(0x102, "jmp", "0x120"),
        ];

        let table = XRefTable::from_instructions(&instructions);

        assert_eq!(table.count_refs_to(0x110), 1);
        assert_eq!(table.count_refs_to(0x120), 1);

        let refs = table.get_refs_to(0x110).unwrap();
        assert_eq!(refs[0].xref_type, XRefType::Jump);
    }

    #[test]
    fn test_parse_address() {
        assert_eq!(parse_address_from_operands("0x1234"), Some(0x1234));
        assert_eq!(parse_address_from_operands("0X1234"), Some(0x1234));
        assert_eq!(parse_address_from_operands("ABCD"), Some(0xABCD));
        assert_eq!(parse_address_from_operands("[0x1000]"), Some(0x1000));
        assert_eq!(parse_address_from_operands("rax"), None);
        assert_eq!(parse_address_from_operands("eax, ebx"), None);
    }
}
