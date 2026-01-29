use anyhow::Result;
use capstone::prelude::*;

/// Supported CPU architectures for disassembly.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Architecture {
    #[default]
    X86_64,
    X86_32,
    Arm64,
    Arm32,
    Mips32,
    Mips64,
    Riscv32,
    Riscv64,
}

impl Architecture {
    pub fn label(&self) -> &'static str {
        match self {
            Architecture::X86_64 => "x86-64",
            Architecture::X86_32 => "x86 (32-bit)",
            Architecture::Arm64 => "ARM64 (AArch64)",
            Architecture::Arm32 => "ARM (32-bit)",
            Architecture::Mips32 => "MIPS (32-bit)",
            Architecture::Mips64 => "MIPS (64-bit)",
            Architecture::Riscv32 => "RISC-V (32-bit)",
            Architecture::Riscv64 => "RISC-V (64-bit)",
        }
    }

    pub fn all() -> &'static [Architecture] {
        &[
            Architecture::X86_64,
            Architecture::X86_32,
            Architecture::Arm64,
            Architecture::Arm32,
            Architecture::Mips32,
            Architecture::Mips64,
            Architecture::Riscv32,
            Architecture::Riscv64,
        ]
    }
}

/// A single disassembled instruction.
#[derive(Debug, Clone)]
pub struct Instruction {
    /// Address/offset of the instruction.
    pub address: u64,
    /// Raw bytes of the instruction.
    pub bytes: Vec<u8>,
    /// Mnemonic (e.g., "mov", "push", "call").
    pub mnemonic: String,
    /// Operands (e.g., "rax, rbx").
    pub operands: String,
}

impl Instruction {
    /// Format bytes as hex string.
    pub fn bytes_hex(&self) -> String {
        self.bytes.iter().map(|b| format!("{:02X}", b)).collect::<Vec<_>>().join(" ")
    }

    /// Full instruction text (mnemonic + operands).
    pub fn text(&self) -> String {
        if self.operands.is_empty() {
            self.mnemonic.clone()
        } else {
            format!("{} {}", self.mnemonic, self.operands)
        }
    }
}

/// Result of disassembly operation.
#[derive(Debug, Clone)]
pub struct DisassemblyResult {
    /// Architecture used for disassembly.
    pub arch: Architecture,
    /// Base address (offset in file).
    pub base_address: u64,
    /// Disassembled instructions.
    pub instructions: Vec<Instruction>,
    /// Number of bytes consumed.
    pub bytes_consumed: usize,
    /// Any error message (partial disassembly).
    pub error: Option<String>,
}

/// Disassemble bytes at a given offset.
pub fn disassemble(
    data: &[u8],
    base_address: u64,
    arch: Architecture,
    max_instructions: usize,
) -> Result<DisassemblyResult> {
    let cs = create_capstone(arch)?;

    let insns = cs.disasm_count(data, base_address, max_instructions)
        .map_err(|e| anyhow::anyhow!("Disassembly failed: {}", e))?;

    let mut instructions = Vec::with_capacity(insns.len());
    let mut bytes_consumed = 0;

    for insn in insns.iter() {
        let bytes = insn.bytes().to_vec();
        bytes_consumed += bytes.len();

        instructions.push(Instruction {
            address: insn.address(),
            bytes,
            mnemonic: insn.mnemonic().unwrap_or("???").to_string(),
            operands: insn.op_str().unwrap_or("").to_string(),
        });
    }

    Ok(DisassemblyResult {
        arch,
        base_address,
        instructions,
        bytes_consumed,
        error: None,
    })
}

/// Create a Capstone instance for the given architecture.
fn create_capstone(arch: Architecture) -> Result<Capstone> {
    let cs = match arch {
        Architecture::X86_64 => {
            Capstone::new()
                .x86()
                .mode(arch::x86::ArchMode::Mode64)
                .syntax(arch::x86::ArchSyntax::Intel)
                .detail(true)
                .build()
        }
        Architecture::X86_32 => {
            Capstone::new()
                .x86()
                .mode(arch::x86::ArchMode::Mode32)
                .syntax(arch::x86::ArchSyntax::Intel)
                .detail(true)
                .build()
        }
        Architecture::Arm64 => {
            Capstone::new()
                .arm64()
                .mode(arch::arm64::ArchMode::Arm)
                .detail(true)
                .build()
        }
        Architecture::Arm32 => {
            Capstone::new()
                .arm()
                .mode(arch::arm::ArchMode::Arm)
                .detail(true)
                .build()
        }
        Architecture::Mips32 => {
            Capstone::new()
                .mips()
                .mode(arch::mips::ArchMode::Mips32)
                .detail(true)
                .build()
        }
        Architecture::Mips64 => {
            Capstone::new()
                .mips()
                .mode(arch::mips::ArchMode::Mips64)
                .detail(true)
                .build()
        }
        Architecture::Riscv32 => {
            Capstone::new()
                .riscv()
                .mode(arch::riscv::ArchMode::RiscV32)
                .detail(true)
                .build()
        }
        Architecture::Riscv64 => {
            Capstone::new()
                .riscv()
                .mode(arch::riscv::ArchMode::RiscV64)
                .detail(true)
                .build()
        }
    };

    cs.map_err(|e| anyhow::anyhow!("Failed to create Capstone: {}", e))
}

/// Try to detect architecture from file content (PE/ELF headers).
pub fn detect_architecture(data: &[u8]) -> Option<Architecture> {
    if data.len() < 64 {
        return None;
    }

    // Check for ELF magic
    if data.starts_with(b"\x7FELF") {
        let class = data.get(4)?;
        let machine = u16::from_le_bytes([*data.get(18)?, *data.get(19)?]);

        return match (class, machine) {
            (2, 0x3E) => Some(Architecture::X86_64),    // ELF64, x86-64
            (1, 0x03) => Some(Architecture::X86_32),    // ELF32, x86
            (2, 0xB7) => Some(Architecture::Arm64),     // ELF64, AArch64
            (1, 0x28) => Some(Architecture::Arm32),     // ELF32, ARM
            (1, 0x08) => Some(Architecture::Mips32),    // ELF32, MIPS
            (2, 0x08) => Some(Architecture::Mips64),    // ELF64, MIPS
            (1, 0xF3) => Some(Architecture::Riscv32),   // ELF32, RISC-V
            (2, 0xF3) => Some(Architecture::Riscv64),   // ELF64, RISC-V
            _ => None,
        };
    }

    // Check for PE magic (MZ header)
    if data.starts_with(b"MZ") {
        let pe_offset = u32::from_le_bytes([
            *data.get(0x3C)?,
            *data.get(0x3D)?,
            *data.get(0x3E)?,
            *data.get(0x3F)?,
        ]) as usize;

        if pe_offset + 6 > data.len() {
            return None;
        }

        // Check PE signature
        if data.get(pe_offset..pe_offset + 4)? != b"PE\x00\x00" {
            return None;
        }

        let machine = u16::from_le_bytes([
            *data.get(pe_offset + 4)?,
            *data.get(pe_offset + 5)?,
        ]);

        return match machine {
            0x8664 => Some(Architecture::X86_64),  // AMD64
            0x014C => Some(Architecture::X86_32),  // i386
            0xAA64 => Some(Architecture::Arm64),   // ARM64
            0x01C0 | 0x01C4 => Some(Architecture::Arm32), // ARM
            _ => None,
        };
    }

    // Check for Mach-O magic
    if data.len() >= 8 {
        let magic = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
        let cputype = u32::from_le_bytes([data[4], data[5], data[6], data[7]]);

        match magic {
            0xFEEDFACE | 0xCEFAEDFE => { // 32-bit Mach-O
                match cputype & 0xFF {
                    7 => return Some(Architecture::X86_32),
                    12 => return Some(Architecture::Arm32),
                    _ => {}
                }
            }
            0xFEEDFACF | 0xCFFAEDFE => { // 64-bit Mach-O
                match cputype & 0xFF {
                    7 => return Some(Architecture::X86_64),
                    12 => return Some(Architecture::Arm64),
                    _ => {}
                }
            }
            _ => {}
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_disasm_x86_64() {
        // mov rax, rbx; ret
        let code = [0x48, 0x89, 0xD8, 0xC3];
        let result = disassemble(&code, 0x1000, Architecture::X86_64, 10).unwrap();

        assert_eq!(result.instructions.len(), 2);
        assert_eq!(result.instructions[0].mnemonic, "mov");
        assert_eq!(result.instructions[1].mnemonic, "ret");
    }

    #[test]
    fn test_disasm_x86_32() {
        // push ebp; mov ebp, esp; ret
        let code = [0x55, 0x89, 0xE5, 0xC3];
        let result = disassemble(&code, 0x1000, Architecture::X86_32, 10).unwrap();

        assert_eq!(result.instructions.len(), 3);
        assert_eq!(result.instructions[0].mnemonic, "push");
        assert_eq!(result.instructions[1].mnemonic, "mov");
        assert_eq!(result.instructions[2].mnemonic, "ret");
    }

    #[test]
    fn test_detect_elf_x86_64() {
        let mut elf = vec![0u8; 64];
        elf[0..4].copy_from_slice(b"\x7FELF");
        elf[4] = 2;  // ELF64
        elf[18] = 0x3E;  // x86-64
        elf[19] = 0x00;

        assert_eq!(detect_architecture(&elf), Some(Architecture::X86_64));
    }

    #[test]
    fn test_detect_pe_x86_64() {
        let mut pe = vec![0u8; 0x100];
        pe[0..2].copy_from_slice(b"MZ");
        pe[0x3C] = 0x80;  // PE offset
        pe[0x80..0x84].copy_from_slice(b"PE\x00\x00");
        pe[0x84] = 0x64;  // AMD64
        pe[0x85] = 0x86;

        assert_eq!(detect_architecture(&pe), Some(Architecture::X86_64));
    }

    #[test]
    fn test_instruction_format() {
        let insn = Instruction {
            address: 0x1000,
            bytes: vec![0x48, 0x89, 0xD8],
            mnemonic: "mov".to_string(),
            operands: "rax, rbx".to_string(),
        };

        assert_eq!(insn.bytes_hex(), "48 89 D8");
        assert_eq!(insn.text(), "mov rax, rbx");
    }

    #[test]
    fn test_jump_operand_format() {
        // Short conditional jump: jne +5 (relative offset of 5)
        // JNE rel8 at address 0x100, will jump to 0x107 (0x100 + 2 + 5)
        let code = [0x75, 0x05]; // jne $+7 (skip 5 bytes after the 2-byte instruction)
        let result = disassemble(&code, 0x100, Architecture::X86_64, 1).unwrap();
        assert_eq!(result.instructions.len(), 1);
        assert_eq!(result.instructions[0].mnemonic, "jne");
        // Capstone should output the absolute target address
        let operands = &result.instructions[0].operands;
        eprintln!("JNE operands: '{}'", operands);
        // The target is 0x100 + 2 (instruction size) + 5 (offset) = 0x107
        assert!(operands.contains("107") || operands.contains("0x107"),
            "Expected target address 0x107, got: {}", operands);

        // Near unconditional jump: jmp +0x10
        let code2 = [0xEB, 0x10]; // jmp short +0x12 (0x100 + 2 + 0x10 = 0x112)
        let result2 = disassemble(&code2, 0x100, Architecture::X86_64, 1).unwrap();
        assert_eq!(result2.instructions[0].mnemonic, "jmp");
        let operands2 = &result2.instructions[0].operands;
        eprintln!("JMP operands: '{}'", operands2);
        // Target = 0x100 + 2 + 0x10 = 0x112
        assert!(operands2.contains("112") || operands2.contains("0x112"),
            "Expected target address 0x112, got: {}", operands2);
    }
}
