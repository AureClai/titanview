"""Generate a 1 GB benchmark file with realistic mixed content.

Sections:
  - ASCII log-like text (low entropy)
  - Repeated structured records (medium entropy)
  - Pseudo-random data (high entropy)
  - Zero-filled gaps
  - Embedded magic bytes (ELF, PNG, PDF, ZIP headers) at known offsets

The file is deterministic (seeded RNG) so benchmarks are reproducible.
"""

import struct
import os
import random
import time

OUTPUT = os.path.join(os.path.dirname(__file__), "..", "test-fixtures", "bench_1gb.bin")
TARGET_SIZE = 1 * 1024 * 1024 * 1024  # 1 GB

# Known magic bytes we'll plant at specific offsets
MAGICS = {
    "ELF":  b"\x7fELF\x02\x01\x01\x00" + b"\x00" * 8,   # 16 bytes
    "PNG":  b"\x89PNG\r\n\x1a\n" + b"\x00" * 8,           # 16 bytes
    "PDF":  b"%PDF-1.7\n" + b"\x00" * 6,                   # 16 bytes
    "ZIP":  b"PK\x03\x04" + b"\x00" * 12,                  # 16 bytes
    "JPEG": b"\xff\xd8\xff\xe0" + b"\x00" * 12,            # 16 bytes
}

# Plant magics at these offsets (spread across the file)
MAGIC_OFFSETS = [
    (0,                     "ELF"),
    (1024,                  "PNG"),
    (50 * 1024 * 1024,      "ELF"),    # 50 MB
    (100 * 1024 * 1024,     "PDF"),    # 100 MB
    (200 * 1024 * 1024,     "ZIP"),    # 200 MB
    (333 * 1024 * 1024,     "JPEG"),   # 333 MB
    (500 * 1024 * 1024,     "ELF"),    # 500 MB
    (512 * 1024 * 1024,     "PNG"),    # 512 MB
    (750 * 1024 * 1024,     "PDF"),    # 750 MB
    (900 * 1024 * 1024,     "ZIP"),    # 900 MB
    (TARGET_SIZE - 1024,    "ELF"),    # near end
]


def gen_ascii_log(rng, size):
    """Generate fake log lines (low entropy)."""
    levels = ["INFO", "WARN", "ERROR", "DEBUG", "TRACE"]
    modules = ["server", "db", "auth", "cache", "net", "io", "parser", "render"]
    messages = [
        "request processed successfully",
        "connection established",
        "cache miss for key",
        "timeout waiting for response",
        "retrying operation",
        "invalid input received",
        "shutting down gracefully",
        "loaded configuration from disk",
        "spawned worker thread",
        "checksum verification passed",
    ]
    buf = bytearray()
    ts = 1700000000
    while len(buf) < size:
        ts += rng.randint(1, 5000)
        level = rng.choice(levels)
        mod = rng.choice(modules)
        msg = rng.choice(messages)
        line = f"[{ts}] {level:5s} {mod:8s} | {msg}\n"
        buf.extend(line.encode("ascii"))
    return bytes(buf[:size])


def gen_structured_records(rng, size):
    """Generate packed binary records (medium entropy).
    Each record: 4-byte id, 8-byte timestamp, 4-byte value, 16-byte payload = 32 bytes.
    """
    buf = bytearray()
    rec_id = 0
    while len(buf) < size:
        ts = 1700000000 + rec_id * 100 + rng.randint(0, 50)
        val = rng.randint(0, 1_000_000)
        payload = bytes(rng.getrandbits(8) for _ in range(16))
        buf.extend(struct.pack("<IqI", rec_id, ts, val))
        buf.extend(payload)
        rec_id += 1
    return bytes(buf[:size])


def gen_random_data(rng, size):
    """Generate pseudo-random bytes (high entropy)."""
    return rng.randbytes(size)


def gen_zeros(size):
    """Generate zero-filled data (zero entropy)."""
    return b"\x00" * size


def main():
    os.makedirs(os.path.dirname(OUTPUT), exist_ok=True)
    rng = random.Random(42)  # deterministic seed

    print(f"Generating {TARGET_SIZE / (1024**3):.0f} GB benchmark file...")
    print(f"Output: {os.path.abspath(OUTPUT)}")
    start = time.time()

    # Plan sections (offset, size, type)
    # 0-256 MB:   ASCII logs (low entropy)
    # 256-512 MB: structured records (medium entropy)
    # 512-768 MB: random data (high entropy)
    # 768 MB-1 GB: mixed (alternating 1 MB blocks of zeros and random)
    SECTION_SIZE = 256 * 1024 * 1024  # 256 MB each

    # Pre-compute magic offset lookup
    magic_map = {}
    for offset, name in MAGIC_OFFSETS:
        magic_map[offset] = MAGICS[name]

    written = 0
    CHUNK = 4 * 1024 * 1024  # write in 4 MB chunks

    with open(OUTPUT, "wb") as f:
        # Section 1: ASCII logs (0 - 256 MB)
        print("  [0-256 MB] ASCII log data...")
        section_written = 0
        while section_written < SECTION_SIZE:
            chunk_size = min(CHUNK, SECTION_SIZE - section_written)
            chunk = bytearray(gen_ascii_log(rng, chunk_size))
            # Plant any magics in this range
            for mo, mdata in magic_map.items():
                rel = mo - written
                if 0 <= rel < len(chunk) and rel + len(mdata) <= len(chunk):
                    chunk[rel:rel + len(mdata)] = mdata
            f.write(chunk)
            written += chunk_size
            section_written += chunk_size

        # Section 2: Structured records (256 - 512 MB)
        print("  [256-512 MB] Structured records...")
        section_written = 0
        while section_written < SECTION_SIZE:
            chunk_size = min(CHUNK, SECTION_SIZE - section_written)
            chunk = bytearray(gen_structured_records(rng, chunk_size))
            for mo, mdata in magic_map.items():
                rel = mo - written
                if 0 <= rel < len(chunk) and rel + len(mdata) <= len(chunk):
                    chunk[rel:rel + len(mdata)] = mdata
            f.write(chunk)
            written += chunk_size
            section_written += chunk_size

        # Section 3: Random data (512 - 768 MB)
        print("  [512-768 MB] Random data...")
        section_written = 0
        while section_written < SECTION_SIZE:
            chunk_size = min(CHUNK, SECTION_SIZE - section_written)
            chunk = bytearray(gen_random_data(rng, chunk_size))
            for mo, mdata in magic_map.items():
                rel = mo - written
                if 0 <= rel < len(chunk) and rel + len(mdata) <= len(chunk):
                    chunk[rel:rel + len(mdata)] = mdata
            f.write(chunk)
            written += chunk_size
            section_written += chunk_size

        # Section 4: Mixed zeros/random (768 MB - 1 GB)
        print("  [768 MB-1 GB] Mixed zeros/random blocks...")
        remaining = TARGET_SIZE - written
        block_size = 1 * 1024 * 1024  # 1 MB alternating blocks
        block_idx = 0
        section_written = 0
        while section_written < remaining:
            chunk_size = min(block_size, remaining - section_written)
            if block_idx % 2 == 0:
                chunk = bytearray(gen_zeros(chunk_size))
            else:
                chunk = bytearray(gen_random_data(rng, chunk_size))
            for mo, mdata in magic_map.items():
                rel = mo - written
                if 0 <= rel < len(chunk) and rel + len(mdata) <= len(chunk):
                    chunk[rel:rel + len(mdata)] = mdata
            f.write(chunk)
            written += chunk_size
            section_written += chunk_size
            block_idx += 1

    elapsed = time.time() - start
    actual_size = os.path.getsize(OUTPUT)
    print(f"\nDone in {elapsed:.1f}s")
    print(f"File size: {actual_size:,} bytes ({actual_size / (1024**3):.2f} GB)")
    print(f"Write speed: {actual_size / elapsed / (1024**2):.0f} MB/s")
    print(f"\nPlanted {len(MAGIC_OFFSETS)} magic markers:")
    for offset, name in sorted(MAGIC_OFFSETS):
        print(f"  0x{offset:012X} ({offset / (1024**2):>8.1f} MB) = {name}")


if __name__ == "__main__":
    main()
