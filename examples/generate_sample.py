#!/usr/bin/env python3
"""Generate sample_file.tvts for testing TitanView templates."""

import struct

def main():
    with open("sample_file.tvts", "wb") as f:
        # === Header (64 bytes) ===

        # Magic: TVTS (4 bytes)
        f.write(b"TVTS")

        # Version: 1.5 (2 bytes)
        f.write(struct.pack("<BB", 1, 5))

        # Flags: COMPRESSED | HAS_CHECKSUM = 0x0011 (2 bytes, u16 LE)
        f.write(struct.pack("<H", 0x0011))

        # File type: DATA = 3 (1 byte)
        f.write(struct.pack("<B", 3))

        # Reserved (3 bytes)
        f.write(b"\x00\x00\x00")

        # Data offset: 64 (4 bytes, u32 LE)
        f.write(struct.pack("<I", 64))

        # Data size: 256 (4 bytes, u32 LE)
        f.write(struct.pack("<I", 256))

        # Entry count: 16 (2 bytes, u16 LE)
        f.write(struct.pack("<H", 16))

        # Checksum: 0xABCD (2 bytes, u16 LE)
        f.write(struct.pack("<H", 0xABCD))

        # Timestamp: 1706500000 (8 bytes, u64 LE)
        f.write(struct.pack("<Q", 1706500000))

        # Name: "example_data_file" padded to 32 bytes
        name = b"example_data_file"
        f.write(name + b"\x00" * (32 - len(name)))

        # === Data section (256 bytes) ===
        # 16 entries of 16 bytes each
        for i in range(16):
            entry = bytes([
                i,           # Entry ID
                i * 2,       # Pattern 1
                i * 3,       # Pattern 2
                i * 4,       # Pattern 3
            ] + [(i + j) ^ 0x55 for j in range(4, 16)])  # Fill
            f.write(entry)

        # === ASCII text section ===
        text = (
            b"This is sample text data in the TitanView test file format.\n"
            b"It demonstrates various data types and patterns.\n"
            b"You can use the Structure Inspector to parse the header.\n"
        )
        f.write(text)

        # === High entropy section (128 bytes) ===
        seed = 0xDEADBEEF
        random_data = []
        for _ in range(128):
            seed = (seed * 1103515245 + 12345) & 0xFFFFFFFF
            random_data.append((seed >> 16) & 0xFF)
        f.write(bytes(random_data))

        # === Zero section (64 bytes) ===
        f.write(b"\x00" * 64)

        total = 64 + 256 + len(text) + 128 + 64
        print(f"Created sample_file.tvts ({total} bytes)")

if __name__ == "__main__":
    main()
