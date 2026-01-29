//! Built-in structure templates for common file formats.

use crate::templates::{StructTemplate, FieldType, PrimitiveType};
use std::collections::HashMap;

/// Get all built-in templates.
pub fn builtin_templates() -> Vec<StructTemplate> {
    vec![
        elf64_header(),
        elf32_header(),
        pe_dos_header(),
        pe_file_header(),
        png_header(),
        jpeg_header(),
        zip_local_header(),
        bmp_header(),
        gif_header(),
        wav_header(),
        tar_header(),
    ]
}

/// Get a template by name.
pub fn get_template(name: &str) -> Option<StructTemplate> {
    builtin_templates().into_iter().find(|t| t.name == name)
}

/// ELF64 file header.
pub fn elf64_header() -> StructTemplate {
    let mut e_type_values = HashMap::new();
    e_type_values.insert(0, "ET_NONE".to_string());
    e_type_values.insert(1, "ET_REL".to_string());
    e_type_values.insert(2, "ET_EXEC".to_string());
    e_type_values.insert(3, "ET_DYN".to_string());
    e_type_values.insert(4, "ET_CORE".to_string());

    let mut e_machine_values = HashMap::new();
    e_machine_values.insert(0, "EM_NONE".to_string());
    e_machine_values.insert(3, "EM_386".to_string());
    e_machine_values.insert(40, "EM_ARM".to_string());
    e_machine_values.insert(62, "EM_X86_64".to_string());
    e_machine_values.insert(183, "EM_AARCH64".to_string());
    e_machine_values.insert(243, "EM_RISCV".to_string());

    let mut class_values = HashMap::new();
    class_values.insert(1, "32-bit".to_string());
    class_values.insert(2, "64-bit".to_string());

    let mut endian_values = HashMap::new();
    endian_values.insert(1, "Little".to_string());
    endian_values.insert(2, "Big".to_string());

    let mut osabi_values = HashMap::new();
    osabi_values.insert(0, "UNIX System V".to_string());
    osabi_values.insert(3, "Linux".to_string());
    osabi_values.insert(9, "FreeBSD".to_string());

    StructTemplate::builder("ELF64 Header")
        .description("64-bit ELF executable header")
        .field("e_ident_magic", FieldType::Magic(vec![0x7F, b'E', b'L', b'F']))
        .field("e_ident_class", FieldType::Enum { base: PrimitiveType::U8, values: class_values })
        .field("e_ident_data", FieldType::Enum { base: PrimitiveType::U8, values: endian_values })
        .field("e_ident_version", FieldType::Primitive(PrimitiveType::U8))
        .field("e_ident_osabi", FieldType::Enum { base: PrimitiveType::U8, values: osabi_values })
        .field("e_ident_pad", FieldType::ByteArray(8))
        .field("e_type", FieldType::Enum { base: PrimitiveType::U16, values: e_type_values })
        .field("e_machine", FieldType::Enum { base: PrimitiveType::U16, values: e_machine_values })
        .field("e_version", FieldType::Primitive(PrimitiveType::U32))
        .field_desc("e_entry", FieldType::Primitive(PrimitiveType::U64), "Entry point virtual address")
        .field_desc("e_phoff", FieldType::Primitive(PrimitiveType::U64), "Program header table offset")
        .field_desc("e_shoff", FieldType::Primitive(PrimitiveType::U64), "Section header table offset")
        .field("e_flags", FieldType::Primitive(PrimitiveType::U32))
        .field_desc("e_ehsize", FieldType::Primitive(PrimitiveType::U16), "ELF header size")
        .field_desc("e_phentsize", FieldType::Primitive(PrimitiveType::U16), "Program header entry size")
        .field_desc("e_phnum", FieldType::Primitive(PrimitiveType::U16), "Number of program headers")
        .field_desc("e_shentsize", FieldType::Primitive(PrimitiveType::U16), "Section header entry size")
        .field_desc("e_shnum", FieldType::Primitive(PrimitiveType::U16), "Number of section headers")
        .field_desc("e_shstrndx", FieldType::Primitive(PrimitiveType::U16), "Section name string table index")
        .build()
}

/// ELF32 file header.
pub fn elf32_header() -> StructTemplate {
    let mut e_type_values = HashMap::new();
    e_type_values.insert(0, "ET_NONE".to_string());
    e_type_values.insert(1, "ET_REL".to_string());
    e_type_values.insert(2, "ET_EXEC".to_string());
    e_type_values.insert(3, "ET_DYN".to_string());
    e_type_values.insert(4, "ET_CORE".to_string());

    let mut e_machine_values = HashMap::new();
    e_machine_values.insert(3, "EM_386".to_string());
    e_machine_values.insert(40, "EM_ARM".to_string());

    StructTemplate::builder("ELF32 Header")
        .description("32-bit ELF executable header")
        .field("e_ident_magic", FieldType::Magic(vec![0x7F, b'E', b'L', b'F']))
        .field("e_ident_class", FieldType::Primitive(PrimitiveType::U8))
        .field("e_ident_data", FieldType::Primitive(PrimitiveType::U8))
        .field("e_ident_version", FieldType::Primitive(PrimitiveType::U8))
        .field("e_ident_osabi", FieldType::Primitive(PrimitiveType::U8))
        .field("e_ident_pad", FieldType::ByteArray(8))
        .field("e_type", FieldType::Enum { base: PrimitiveType::U16, values: e_type_values })
        .field("e_machine", FieldType::Enum { base: PrimitiveType::U16, values: e_machine_values })
        .field("e_version", FieldType::Primitive(PrimitiveType::U32))
        .field_desc("e_entry", FieldType::Primitive(PrimitiveType::U32), "Entry point")
        .field_desc("e_phoff", FieldType::Primitive(PrimitiveType::U32), "Program header offset")
        .field_desc("e_shoff", FieldType::Primitive(PrimitiveType::U32), "Section header offset")
        .field("e_flags", FieldType::Primitive(PrimitiveType::U32))
        .field("e_ehsize", FieldType::Primitive(PrimitiveType::U16))
        .field("e_phentsize", FieldType::Primitive(PrimitiveType::U16))
        .field("e_phnum", FieldType::Primitive(PrimitiveType::U16))
        .field("e_shentsize", FieldType::Primitive(PrimitiveType::U16))
        .field("e_shnum", FieldType::Primitive(PrimitiveType::U16))
        .field("e_shstrndx", FieldType::Primitive(PrimitiveType::U16))
        .build()
}

/// DOS/MZ header (start of PE files).
pub fn pe_dos_header() -> StructTemplate {
    StructTemplate::builder("DOS Header (MZ)")
        .description("DOS MZ executable header (PE files start with this)")
        .field("e_magic", FieldType::Magic(vec![b'M', b'Z']))
        .field_desc("e_cblp", FieldType::Primitive(PrimitiveType::U16), "Bytes on last page")
        .field_desc("e_cp", FieldType::Primitive(PrimitiveType::U16), "Pages in file")
        .field("e_crlc", FieldType::Primitive(PrimitiveType::U16))
        .field("e_cparhdr", FieldType::Primitive(PrimitiveType::U16))
        .field("e_minalloc", FieldType::Primitive(PrimitiveType::U16))
        .field("e_maxalloc", FieldType::Primitive(PrimitiveType::U16))
        .field("e_ss", FieldType::Primitive(PrimitiveType::U16))
        .field("e_sp", FieldType::Primitive(PrimitiveType::U16))
        .field("e_csum", FieldType::Primitive(PrimitiveType::U16))
        .field("e_ip", FieldType::Primitive(PrimitiveType::U16))
        .field("e_cs", FieldType::Primitive(PrimitiveType::U16))
        .field("e_lfarlc", FieldType::Primitive(PrimitiveType::U16))
        .field("e_ovno", FieldType::Primitive(PrimitiveType::U16))
        .field("e_res", FieldType::ByteArray(8))
        .field("e_oemid", FieldType::Primitive(PrimitiveType::U16))
        .field("e_oeminfo", FieldType::Primitive(PrimitiveType::U16))
        .field("e_res2", FieldType::ByteArray(20))
        .field_desc("e_lfanew", FieldType::Primitive(PrimitiveType::U32), "Offset to PE header")
        .build()
}

/// PE COFF file header.
pub fn pe_file_header() -> StructTemplate {
    let mut machine_values = HashMap::new();
    machine_values.insert(0x14c, "i386".to_string());
    machine_values.insert(0x8664, "AMD64".to_string());
    machine_values.insert(0xAA64, "ARM64".to_string());

    let mut characteristics = HashMap::new();
    characteristics.insert(0x0001, "RELOCS_STRIPPED".to_string());
    characteristics.insert(0x0002, "EXECUTABLE_IMAGE".to_string());
    characteristics.insert(0x0020, "LARGE_ADDRESS_AWARE".to_string());
    characteristics.insert(0x0100, "32BIT_MACHINE".to_string());
    characteristics.insert(0x2000, "DLL".to_string());

    StructTemplate::builder("PE File Header")
        .description("PE COFF file header (after PE signature)")
        .field("Signature", FieldType::Magic(vec![b'P', b'E', 0, 0]))
        .field("Machine", FieldType::Enum { base: PrimitiveType::U16, values: machine_values })
        .field_desc("NumberOfSections", FieldType::Primitive(PrimitiveType::U16), "Number of sections")
        .field_desc("TimeDateStamp", FieldType::Primitive(PrimitiveType::U32), "Unix timestamp")
        .field("PointerToSymbolTable", FieldType::Primitive(PrimitiveType::U32))
        .field("NumberOfSymbols", FieldType::Primitive(PrimitiveType::U32))
        .field_desc("SizeOfOptionalHeader", FieldType::Primitive(PrimitiveType::U16), "Size of optional header")
        .field("Characteristics", FieldType::Flags { base: PrimitiveType::U16, bits: characteristics })
        .build()
}

/// PNG file header.
pub fn png_header() -> StructTemplate {
    StructTemplate::builder("PNG Header")
        .description("PNG image file header and IHDR chunk")
        .field("signature", FieldType::Magic(vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]))
        .field_desc("ihdr_length", FieldType::Primitive(PrimitiveType::U32), "IHDR chunk length (big-endian)")
        .field("ihdr_type", FieldType::Magic(vec![b'I', b'H', b'D', b'R']))
        .little_endian(false)  // PNG uses big-endian
        .field_desc("width", FieldType::Primitive(PrimitiveType::U32), "Image width in pixels")
        .field_desc("height", FieldType::Primitive(PrimitiveType::U32), "Image height in pixels")
        .field_desc("bit_depth", FieldType::Primitive(PrimitiveType::U8), "Bits per channel")
        .field_desc("color_type", FieldType::Primitive(PrimitiveType::U8), "0=gray,2=RGB,3=indexed,4=gray+alpha,6=RGBA")
        .field("compression", FieldType::Primitive(PrimitiveType::U8))
        .field("filter", FieldType::Primitive(PrimitiveType::U8))
        .field_desc("interlace", FieldType::Primitive(PrimitiveType::U8), "0=none, 1=Adam7")
        .build()
}

/// JPEG file header (SOI + APP0).
pub fn jpeg_header() -> StructTemplate {
    StructTemplate::builder("JPEG Header")
        .description("JPEG image header (JFIF)")
        .field("soi", FieldType::Magic(vec![0xFF, 0xD8]))
        .field("app0_marker", FieldType::Magic(vec![0xFF, 0xE0]))
        .field_desc("app0_length", FieldType::Primitive(PrimitiveType::U16), "APP0 segment length")
        .little_endian(false)  // JPEG uses big-endian
        .field("identifier", FieldType::String(5))
        .field_desc("version_major", FieldType::Primitive(PrimitiveType::U8), "JFIF major version")
        .field_desc("version_minor", FieldType::Primitive(PrimitiveType::U8), "JFIF minor version")
        .field_desc("density_units", FieldType::Primitive(PrimitiveType::U8), "0=none,1=dpi,2=dpcm")
        .field_desc("x_density", FieldType::Primitive(PrimitiveType::U16), "Horizontal density")
        .field_desc("y_density", FieldType::Primitive(PrimitiveType::U16), "Vertical density")
        .build()
}

/// ZIP local file header.
pub fn zip_local_header() -> StructTemplate {
    let mut compression_values = HashMap::new();
    compression_values.insert(0, "Store".to_string());
    compression_values.insert(8, "Deflate".to_string());
    compression_values.insert(9, "Deflate64".to_string());
    compression_values.insert(12, "BZIP2".to_string());
    compression_values.insert(14, "LZMA".to_string());

    StructTemplate::builder("ZIP Local Header")
        .description("ZIP local file header")
        .field("signature", FieldType::Magic(vec![0x50, 0x4B, 0x03, 0x04]))
        .field_desc("version_needed", FieldType::Primitive(PrimitiveType::U16), "Version needed to extract")
        .field("flags", FieldType::Primitive(PrimitiveType::U16))
        .field("compression", FieldType::Enum { base: PrimitiveType::U16, values: compression_values })
        .field("mod_time", FieldType::Primitive(PrimitiveType::U16))
        .field("mod_date", FieldType::Primitive(PrimitiveType::U16))
        .field_desc("crc32", FieldType::Primitive(PrimitiveType::U32), "CRC-32 checksum")
        .field_desc("compressed_size", FieldType::Primitive(PrimitiveType::U32), "Compressed size")
        .field_desc("uncompressed_size", FieldType::Primitive(PrimitiveType::U32), "Uncompressed size")
        .field_desc("filename_len", FieldType::Primitive(PrimitiveType::U16), "Filename length")
        .field_desc("extra_len", FieldType::Primitive(PrimitiveType::U16), "Extra field length")
        .build()
}

/// BMP file header.
pub fn bmp_header() -> StructTemplate {
    StructTemplate::builder("BMP Header")
        .description("Windows bitmap file header + DIB header")
        .field("signature", FieldType::Magic(vec![b'B', b'M']))
        .field_desc("file_size", FieldType::Primitive(PrimitiveType::U32), "Total file size")
        .field("reserved1", FieldType::Primitive(PrimitiveType::U16))
        .field("reserved2", FieldType::Primitive(PrimitiveType::U16))
        .field_desc("data_offset", FieldType::Primitive(PrimitiveType::U32), "Offset to pixel data")
        // DIB header (BITMAPINFOHEADER)
        .field_desc("dib_size", FieldType::Primitive(PrimitiveType::U32), "DIB header size (40 for BITMAPINFOHEADER)")
        .field_desc("width", FieldType::Primitive(PrimitiveType::I32), "Image width")
        .field_desc("height", FieldType::Primitive(PrimitiveType::I32), "Image height (negative = top-down)")
        .field_desc("planes", FieldType::Primitive(PrimitiveType::U16), "Color planes (always 1)")
        .field_desc("bpp", FieldType::Primitive(PrimitiveType::U16), "Bits per pixel")
        .field_desc("compression", FieldType::Primitive(PrimitiveType::U32), "Compression method")
        .field("image_size", FieldType::Primitive(PrimitiveType::U32))
        .field("x_ppm", FieldType::Primitive(PrimitiveType::I32))
        .field("y_ppm", FieldType::Primitive(PrimitiveType::I32))
        .field("colors_used", FieldType::Primitive(PrimitiveType::U32))
        .field("colors_important", FieldType::Primitive(PrimitiveType::U32))
        .build()
}

/// GIF header.
pub fn gif_header() -> StructTemplate {
    StructTemplate::builder("GIF Header")
        .description("GIF image header")
        .field("signature", FieldType::String(6))  // GIF87a or GIF89a
        .field_desc("width", FieldType::Primitive(PrimitiveType::U16), "Logical screen width")
        .field_desc("height", FieldType::Primitive(PrimitiveType::U16), "Logical screen height")
        .field_desc("packed", FieldType::Primitive(PrimitiveType::U8), "Packed fields (GCT flag, color resolution, sort, GCT size)")
        .field_desc("bg_color", FieldType::Primitive(PrimitiveType::U8), "Background color index")
        .field_desc("aspect_ratio", FieldType::Primitive(PrimitiveType::U8), "Pixel aspect ratio")
        .build()
}

/// WAV/RIFF header.
pub fn wav_header() -> StructTemplate {
    let mut format_values = HashMap::new();
    format_values.insert(1, "PCM".to_string());
    format_values.insert(3, "IEEE Float".to_string());
    format_values.insert(6, "A-law".to_string());
    format_values.insert(7, "Mu-law".to_string());

    StructTemplate::builder("WAV Header")
        .description("WAV/RIFF audio file header")
        .field("riff_magic", FieldType::Magic(vec![b'R', b'I', b'F', b'F']))
        .field_desc("file_size", FieldType::Primitive(PrimitiveType::U32), "File size - 8")
        .field("wave_magic", FieldType::Magic(vec![b'W', b'A', b'V', b'E']))
        .field("fmt_magic", FieldType::Magic(vec![b'f', b'm', b't', b' ']))
        .field_desc("fmt_size", FieldType::Primitive(PrimitiveType::U32), "Format chunk size (16 for PCM)")
        .field("audio_format", FieldType::Enum { base: PrimitiveType::U16, values: format_values })
        .field_desc("num_channels", FieldType::Primitive(PrimitiveType::U16), "Number of channels")
        .field_desc("sample_rate", FieldType::Primitive(PrimitiveType::U32), "Sample rate (Hz)")
        .field_desc("byte_rate", FieldType::Primitive(PrimitiveType::U32), "Bytes per second")
        .field_desc("block_align", FieldType::Primitive(PrimitiveType::U16), "Bytes per sample frame")
        .field_desc("bits_per_sample", FieldType::Primitive(PrimitiveType::U16), "Bits per sample")
        .build()
}

/// TAR header (USTAR format).
pub fn tar_header() -> StructTemplate {
    StructTemplate::builder("TAR Header")
        .description("TAR archive entry header (USTAR)")
        .field_desc("name", FieldType::String(100), "File name")
        .field_desc("mode", FieldType::String(8), "File mode (octal)")
        .field_desc("uid", FieldType::String(8), "Owner UID (octal)")
        .field_desc("gid", FieldType::String(8), "Owner GID (octal)")
        .field_desc("size", FieldType::String(12), "File size (octal)")
        .field_desc("mtime", FieldType::String(12), "Modification time (octal)")
        .field_desc("checksum", FieldType::String(8), "Header checksum")
        .field_desc("typeflag", FieldType::Primitive(PrimitiveType::U8), "Entry type")
        .field_desc("linkname", FieldType::String(100), "Link target name")
        .field("magic", FieldType::String(6))  // "ustar\0" or "ustar "
        .field("version", FieldType::String(2))
        .field("uname", FieldType::String(32))
        .field("gname", FieldType::String(32))
        .field("devmajor", FieldType::String(8))
        .field("devminor", FieldType::String(8))
        .field_desc("prefix", FieldType::String(155), "Filename prefix")
        .build()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_builtin_count() {
        let templates = builtin_templates();
        assert!(templates.len() >= 10);
    }

    #[test]
    fn test_elf64_header_size() {
        let elf = elf64_header();
        assert_eq!(elf.size, 64); // ELF64 header is exactly 64 bytes
    }

    #[test]
    fn test_get_template() {
        let template = get_template("ELF64 Header");
        assert!(template.is_some());
        assert_eq!(template.unwrap().name, "ELF64 Header");
    }
}
