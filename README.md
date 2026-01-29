# TitanView

**A high-performance forensic hex viewer and binary analysis tool built in Rust.**

TitanView is designed for security researchers, reverse engineers, and forensic analysts who need to explore large binary files with advanced visualization and analysis capabilities.

![TitanView](https://img.shields.io/badge/version-0.1.0-blue)
![Rust](https://img.shields.io/badge/rust-1.70+-orange)
![License](https://img.shields.io/badge/license-MIT-green)

## Features

### Core Capabilities
- **GPU-Accelerated Analysis** - Entropy and block classification computed on GPU via wgpu
- **Memory-Mapped Files** - Handle multi-gigabyte files without loading them entirely into RAM
- **Real-time Minimap** - Visual overview with entropy heatmap and content classification

### Visualization
- **Hex View** - Traditional hex editor with syntax highlighting and edit support
- **Hilbert Curve** - Space-filling curve visualization for pattern detection
- **Byte Histogram** - Statistical distribution analysis
- **Entropy Heatmap** - Identify encrypted, compressed, or structured regions

### Analysis Tools
- **Disassembler** - Multi-architecture support (x86, x64, ARM, ARM64, MIPS, PowerPC, RISC-V)
- **Structure Inspector** - Parse binary structures with JSON templates
- **Signature Scanner** - Detect file formats, magic bytes, and patterns
- **Cross-References** - Track jumps, calls, and data references
- **Binary Diff** - Compare two files side-by-side

### Productivity
- **Workspaces** - Pre-configured analysis environments:
  - Generic Analysis
  - Malware Forensics
  - Data Carving
  - Firmware Analysis
  - Crypto Analysis
- **Session Persistence** - Save and restore your analysis state (`.titan-session`)
- **Bookmarks & Labels** - Annotate interesting locations
- **Script Console** - Automate tasks with Rhai scripting language
- **JSON Templates** - Define custom binary structures

## Screenshots

*Coming soon*

## Installation

### From Source

```bash
# Clone the repository
git clone https://github.com/YOUR_USERNAME/titanview.git
cd titanview

# Build in release mode
cargo build --release

# Run
./target/release/tv-app
```

### Requirements
- Rust 1.70 or later
- GPU with Vulkan, Metal, or DX12 support (for GPU acceleration)
- Windows, macOS, or Linux

## Usage

### Quick Start

1. **Open a file**: `File > Open` or drag & drop
2. **Navigate**: Mouse wheel, Page Up/Down, or click minimap
3. **Search**: `Ctrl+F` for hex/text search
4. **Analyze**: Use F-keys to toggle analysis windows

### Keyboard Shortcuts

| Key | Action |
|-----|--------|
| `Ctrl+O` | Open file |
| `Ctrl+S` | Save session |
| `Ctrl+F` | Search |
| `F1` | File Info |
| `F2` | Signatures |
| `F3` | Performance |
| `F4` | Hilbert Curve |
| `F5` | Disassembly |
| `F6` | Binary Diff |
| `F7` | Structure Inspector |
| `F8` | Histogram |
| `F9` | Cross-References |
| `F10` | Bookmarks |
| `F11` | Script Console |
| `Ctrl+1-5` | Switch Workspace |
| `Escape` | Close windows |

### Scripting

TitanView includes a Rhai scripting console for automation:

```javascript
// Find all MZ headers
let results = search([0x4D, 0x5A]);
print(`Found ${results.len()} PE files`);

// XOR decode a region
for i in range(0x100, 0x200) {
    let b = read_byte(i);
    write_byte(i, b ^ 0x42);
}

// Navigate to offset
goto(0x1000);
```

### Custom Templates

Define binary structures in JSON:

```json
{
  "name": "PE DOS Header",
  "fields": [
    { "name": "e_magic", "type": { "type": "magic", "value": [77, 90] } },
    { "name": "e_cblp", "type": { "type": "u16" } },
    { "name": "e_lfanew", "type": { "type": "u32" } }
  ]
}
```

## Architecture

```
titanview/
├── crates/
│   ├── tv-core/      # Core data structures and file handling
│   ├── tv-gpu/       # GPU compute shaders (wgpu)
│   ├── tv-ui/        # UI components (egui)
│   └── tv-app/       # Main application
├── shaders/          # WGSL compute shaders
└── examples/         # Sample files and templates
```

## Contributing

Contributions are welcome! Please feel free to submit issues and pull requests.

1. Fork the repository
2. Create your feature branch (`git checkout -b feature/amazing-feature`)
3. Commit your changes (`git commit -m 'Add amazing feature'`)
4. Push to the branch (`git push origin feature/amazing-feature`)
5. Open a Pull Request

## License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.

## Acknowledgments

- [egui](https://github.com/emilk/egui) - Immediate mode GUI library
- [wgpu](https://github.com/gfx-rs/wgpu) - Cross-platform GPU API
- [Rhai](https://github.com/rhaiscript/rhai) - Embedded scripting language
- [Capstone](https://github.com/capstone-engine/capstone) - Disassembly framework

## Roadmap

- [ ] Network packet analysis workspace
- [ ] YARA rule integration
- [ ] Plugin system
- [ ] Collaborative analysis
- [ ] More file format parsers
