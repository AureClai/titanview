<div align="center">

# TitanView

### The Forensic Hex Viewer Built for the Modern Era

**Explore gigabyte-scale binaries at 60 FPS with GPU-accelerated analysis**

[![Rust](https://img.shields.io/badge/Rust-1.70+-orange?logo=rust)](https://www.rust-lang.org/)
[![License](https://img.shields.io/badge/License-MIT-green.svg)](LICENSE)
[![Platform](https://img.shields.io/badge/Platform-Windows%20%7C%20macOS%20%7C%20Linux-blue)]()

[Features](#-features) · [Quick Start](#-quick-start) · [Screenshots](#-screenshots) · [Documentation](#-documentation)

</div>

---

## Why TitanView?

Traditional hex editors choke on large files. They load everything into RAM, freeze on searches, and offer limited analysis capabilities. **TitanView is different.**

| Challenge | TitanView Solution |
|-----------|-------------------|
| 4GB firmware dump? | Memory-mapped I/O — only loads what you see |
| Finding patterns in noise? | GPU-computed entropy heatmap in milliseconds |
| Identifying file types? | 200+ built-in signatures with deep scan |
| Repetitive analysis tasks? | Script console with full file access |
| Context switching? | Workspaces remember your tool layout |

Built from the ground up in **Rust** with **wgpu** for GPU compute and **egui** for a buttery-smooth interface.

---

## ✨ Features

### 🔬 Deep Analysis Tools

<table>
<tr>
<td width="50%">

**Entropy Visualization**
- Real-time heatmap computed on GPU
- Instantly spot encrypted, compressed, or structured regions
- 256-byte block resolution

**Block Classification**
- Automatic detection: ASCII, UTF-8, Binary, High-entropy, Zeros
- Color-coded minimap overview
- Filter and navigate by content type

</td>
<td width="50%">

**Multi-Architecture Disassembler**
- x86, x86-64, ARM, ARM64, MIPS, PowerPC, RISC-V
- Cross-reference tracking (jumps, calls, data refs)
- Control Flow Graph visualization

**Structure Inspector**
- Parse any binary format with JSON templates
- Built-in templates for PE, ELF, ZIP, PNG, and more
- Create and share custom templates

</td>
</tr>
</table>

### 🎯 Productivity Features

- **Hilbert Curve View** — See your entire file as a 2D space-filling curve. Patterns that are invisible in linear view become obvious.

- **Binary Diff** — Compare two files byte-by-byte with synchronized scrolling and highlighted differences.

- **Smart Search** — Hex patterns, text strings, regex. Results highlighted in both hex view and minimap.

- **Bookmarks & Labels** — Annotate interesting offsets. Export your findings.

- **Session Persistence** — Save your complete analysis state. Pick up exactly where you left off.

### 🚀 Performance

```
Benchmark: 1GB random binary file
─────────────────────────────────────────
Open file:           < 100ms (memory-mapped)
Compute entropy:     ~800ms (GPU, RTX 3060)
Full-file search:    ~1.2s (parallel SIMD)
Scroll/render:       60 FPS constant
Memory usage:        ~50MB (regardless of file size)
```

### 🎨 Workspaces

Pre-configured analysis environments that set up the right tools for the job:

| Workspace | Purpose | Key Tools |
|-----------|---------|-----------|
| 🔍 **Generic** | General exploration | Hex view, minimap, search |
| 🦠 **Malware** | Reverse engineering | Disasm, CFG, signatures, entropy |
| ⛏️ **Carving** | Data recovery | Classification, deep scan, bookmarks |
| 🔧 **Firmware** | Embedded analysis | Multi-arch disasm, structure inspector |
| 🔐 **Crypto** | Encryption analysis | Entropy focus, histogram, XOR scripts |

Switch instantly with `Ctrl+1` through `Ctrl+5`.

### 📜 Scripting

Automate repetitive tasks with the built-in **Rhai** script console:

```javascript
// Find and decode XOR-encoded strings
let results = search([0x4D, 0x5A]);  // Find MZ headers
print(`Found ${results.len()} PE files`);

for offset in results {
    goto(offset);
    print(`PE at ${hex(offset)}`);
}

// XOR decode a region
for i in range(0x1000, 0x2000) {
    let b = read_byte(i);
    write_byte(i, b ^ 0x42);
}
```

Full syntax highlighting, history, and example scripts included.

---

## 🚀 Quick Start

### Installation

```bash
# Clone the repository
git clone https://github.com/AureClai/titanview.git
cd titanview

# Build in release mode (important for performance!)
cargo build --release

# Run
./target/release/tv-app      # Linux/macOS
.\target\release\tv-app.exe  # Windows
```

### Requirements

- **Rust** 1.70 or later
- **GPU** with Vulkan, Metal, or DX12 support
- ~100MB disk space

### First Steps

1. **Open a file** — Drag & drop or `File > Open`
2. **Explore** — Scroll with mouse wheel, click minimap to jump
3. **Analyze** — Press `F2` for signatures, `F4` for Hilbert view
4. **Search** — `Ctrl+F` for hex/text patterns
5. **Script** — `F11` opens the console

---

## 📸 Screenshots

*Coming soon — contributions welcome!*

---

## ⌨️ Keyboard Shortcuts

<table>
<tr><td>

| Navigation | |
|------------|--|
| `Scroll` | Mouse wheel / Page Up/Down |
| `Jump` | Click minimap |
| `Goto` | `Ctrl+G` |

| File | |
|------|--|
| `Open` | `Ctrl+O` |
| `Save Session` | `Ctrl+S` |
| `Close` | `File > Close Session` |

</td><td>

| Windows | |
|---------|--|
| `File Info` | `F1` |
| `Signatures` | `F2` |
| `Hilbert` | `F4` |
| `Disassembly` | `F5` |
| `Diff` | `F6` |
| `Inspector` | `F7` |
| `Histogram` | `F8` |
| `Bookmarks` | `F10` |
| `Scripts` | `F11` |
| `Close All` | `Escape` |

</td><td>

| Workspaces | |
|------------|--|
| Generic | `Ctrl+1` |
| Malware | `Ctrl+2` |
| Carving | `Ctrl+3` |
| Firmware | `Ctrl+4` |
| Crypto | `Ctrl+5` |

</td></tr>
</table>

---

## 📖 Documentation

### Custom Templates

Define binary structures in JSON for the Structure Inspector:

```json
{
  "name": "PNG Chunk",
  "fields": [
    { "name": "length", "type": { "type": "u32_be" } },
    { "name": "type", "type": { "type": "ascii", "value": 4 } },
    { "name": "crc", "type": { "type": "u32_be" } }
  ]
}
```

### Project Structure

```
titanview/
├── crates/
│   ├── tv-core/     # File handling, entropy, signatures, disasm
│   ├── tv-gpu/      # wgpu compute shaders
│   ├── tv-ui/       # egui interface components
│   └── tv-app/      # Main application
├── shaders/         # WGSL compute shaders
└── examples/        # Sample templates and test files
```

---

## 🤝 Contributing

Contributions are welcome! Whether it's:

- 🐛 Bug reports and fixes
- ✨ New features and analysis tools
- 📝 Documentation improvements
- 🎨 UI/UX enhancements

Please feel free to open issues and pull requests.

```bash
# Run tests
cargo test --workspace

# Run with logging
RUST_LOG=debug cargo run --release -p tv-app
```

---

## 📜 License

MIT License — See [LICENSE](LICENSE) for details.

---

## 🙏 Acknowledgments

Built with amazing open-source projects:

- [egui](https://github.com/emilk/egui) — Immediate mode GUI
- [wgpu](https://github.com/gfx-rs/wgpu) — Cross-platform GPU API
- [Rhai](https://github.com/rhaiscript/rhai) — Embedded scripting
- [Capstone](https://github.com/capstone-engine/capstone) — Disassembly framework

---

<div align="center">

**[⬆ Back to top](#titanview)**

Made with ☕ and Rust

</div>
