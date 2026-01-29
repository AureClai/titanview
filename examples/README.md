# TitanView Examples

This directory contains example files for testing and demonstrating TitanView features.

## Files

### `sample_file.tvts`
A test binary file with a custom "TitanView Test Sample" format. Contains:
- 64-byte header with magic bytes, version, flags, etc.
- 256 bytes of structured data (16 entries x 16 bytes)
- ASCII text section
- High-entropy (pseudo-random) section
- Zero-filled section

### `sample_header.template.json`
JSON template that matches the `sample_file.tvts` header format. Demonstrates:
- Magic bytes validation
- Primitive types (u8, u16, u32, u64)
- Flags with named bits
- Enum with named values
- Byte arrays
- Fixed-length strings

### `simple_record.template.json`
A minimal template example showing basic field types.

## Usage

1. Open TitanView and load `sample_file.tvts`
2. Press **F7** to open the Structure Inspector
3. Click **Load JSON...** and select `sample_header.template.json`
4. Click **Apply** to parse the header

Expected output:
- `magic`: "TVTS" (bytes 54 56 54 53) - should show checkmark
- `version_major`: 1
- `version_minor`: 5
- `flags`: 0x0011 [COMPRESSED, HAS_CHECKSUM]
- `file_type`: DATA (0x03)
- `data_offset`: 64
- `data_size`: 256
- `entry_count`: 16
- `checksum`: 0xABCD
- `timestamp`: 1706500000
- `name`: "example_data_file"

## Creating Custom Templates

Template JSON format:
```json
{
  "name": "Template Name",
  "description": "Description",
  "little_endian": true,
  "fields": [
    {
      "name": "field_name",
      "field_type": {"type": "primitive", "value": "u32"},
      "offset": 0,
      "description": "Field description"
    }
  ],
  "size": 4
}
```

### Field Types

| Type | Value Format | Description |
|------|--------------|-------------|
| `primitive` | `"u8"`, `"u16"`, `"u32"`, `"u64"`, `"i8"`, `"i16"`, `"i32"`, `"i64"`, `"f32"`, `"f64"` | Basic numeric types |
| `byte_array` | `<size>` (number) | Fixed-size byte array |
| `string` | `<size>` (number) | Fixed-length ASCII string |
| `c_string` | `<max_size>` (number) | Null-terminated string |
| `magic` | `[<bytes>]` (array of numbers) | Expected byte sequence |
| `enum` | `{"base": "<type>", "values": {"<value>": "<name>", ...}}` | Named values |
| `flags` | `{"base": "<type>", "bits": {"<bit>": "<name>", ...}}` | Named bit flags |

### Regenerating the Test File

```bash
# Using Python
cd examples
python generate_sample.py

# Using Rust
rustc generate_sample.rs && ./generate_sample
```
